//! Wake-driven Hub I/O owner.
//!
//! One `HubIo` owns every blocking wait the TUI performs:
//!
//! - the Crossterm `EventStream` input thread,
//! - one socket reader thread and one socket writer thread per Hub connection,
//! - the absolute-deadline table for outstanding host-control requests.
//!
//! The application thread never blocks on a socket, the filesystem, or a timer
//! that is not an absolute deadline. It waits on exactly one channel with
//! `recv_timeout` to the earliest deadline and applies the `AppWake` it gets.
//!
//! Every frame of one connection rides that connection: correlated responses,
//! unsolicited events, entity subscription frames, and routed terminal frames.
//!
//! Bounds (section 6 of the implementation contract):
//!
//! - 32 outstanding host-control requests per connection; a 33rd `submit`
//!   completes immediately with `DaemonRequestError::TooManyOutstandingRequests`.
//! - 256 items / 8 MiB of pending wakes, shared by every route and by
//!   retained frames. Terminal output is credited per route (S13): the Hub
//!   sends a frame only against credit this client granted for it, after the
//!   Hub demanded that whole frame, so the reader never waits and never sheds
//!   a slow route, and control frames never wait behind a flooding route.
//!   Grants are carved from this budget first come first served; a route's
//!   pool stays charged until its frames are dequeued, returned, or its
//!   generation closes (`credit.rs`). Only a frame that does not decode faults
//!   its route (`RouteFault`); the application detaches and re-attaches that
//!   route only. A frame no grant covered, a second unserved demand, or an
//!   over-return is a protocol violation that closes the connection. Control
//!   frames are never shed because their producers are already bounded (32
//!   responses, Hub-shed events with `EventGap`, entity snapshots per
//!   subscription).

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    future::Future,
    io::{self, BufReader, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender},
    },
    task::{Context, Poll, Waker},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use botster_hub_client::{
    ClientFrame, DaemonCompatibilityRequirement, DaemonEndpoint, DaemonEntityFrame, DaemonEvent,
    DaemonHelloAck, DaemonRequest, DaemonRequestError, DaemonResponse, DaemonResponseKind,
    DaemonTransportError, DaemonUnixCreditFrame, DaemonUnixFrameReader, DaemonUnixMuxFrame,
    DaemonUnixTerminalFrame, MAX_OUTSTANDING_REQUESTS, RequestIdSequence, ServerFrame,
    TerminalCompatibilityRequirement, connect_and_hello_with_terminal_requirement,
    encode_client_frame, encode_request_id, encode_unix_credit_frame, encode_unix_terminal_frame,
    parse_request_id,
};
use botster_terminal_protocol_client::{RouteId, RoutedTerminalFrame, TerminalFrame};
use crossterm::event::{Event, EventStream};
use futures_lite::{StreamExt, future};

use crate::credit::{CreditLedger, CreditViolation, Release};

/// Pending wake items before a terminal route is shed.
pub const MAX_PENDING_WAKE_ITEMS: usize = 256;
/// Pending wake bytes before a terminal route is shed.
pub const MAX_PENDING_WAKE_BYTES: usize = 8 * 1024 * 1024;

// Every terminal frame fits the byte bound once the queue drains, so a
// waiting reader always makes progress.
const _: () =
    assert!(botster_terminal_protocol_client::MAX_ROUTE_EGRESS_BYTES < MAX_PENDING_WAKE_BYTES);
/// Client-to-Hub input containers carry a reserved epoch of 0; Hub validates
/// only the route and the fixed attachment generation.
const INPUT_CONTAINER_EPOCH: u32 = 0;

/// One wake delivered to the application loop.
#[derive(Debug)]
pub enum AppWake {
    /// One Crossterm terminal event from the input thread.
    Input(Event),
    /// One routed terminal frame decoded with the Core scheme 2 codec.
    Terminal(RoutedTerminalFrame),
    /// One host-control request completed, expired, cancelled, or lost.
    Completed {
        request_id: u64,
        result: Result<Box<DaemonResponse>, DaemonRequestError>,
    },
    /// One unsolicited host event on the current connection.
    Event(DaemonEvent),
    /// One entity subscription frame on the current connection.
    Entity(DaemonEntityFrame),
    /// The connection with this generation completed its Hello.
    Connected {
        generation: u64,
        ack: Box<DaemonHelloAck>,
    },
    /// The connection with this generation closed or failed to open.
    Disconnected {
        generation: u64,
        error: DaemonTransportError,
    },
    /// A terminal route lost bytes in the client queue or failed to decode.
    ///
    /// Byte continuity for that route is gone. The application must detach and
    /// re-attach the route; sibling routes and host requests are unaffected.
    RouteFault {
        route: RouteId,
        generation: u64,
        reason: String,
    },
    /// The application-supplied deadline passed with nothing else to deliver.
    Deadline,
    /// The input stream ended or failed; the application should exit.
    Shutdown,
}

/// Messages from I/O threads to the owner. Mapped to `AppWake` by `next_wake`.
enum IoMessage {
    Input(Event),
    InputEnded,
    Terminal {
        generation: u64,
        frame: RoutedTerminalFrame,
    },
    Response {
        generation: u64,
        request_id: u64,
        response: Box<DaemonResponse>,
    },
    Event {
        generation: u64,
        event: DaemonEvent,
    },
    Entity {
        generation: u64,
        frame: DaemonEntityFrame,
    },
    Connected {
        generation: u64,
        ack: Box<DaemonHelloAck>,
        link: Box<LinkHandles>,
    },
    Disconnected {
        generation: u64,
        error: DaemonTransportError,
    },
    RouteFault {
        generation: u64,
        route: RouteId,
        route_generation: u64,
        reason: String,
    },
    Credit {
        generation: u64,
        frame: DaemonUnixCreditFrame,
    },
}

/// Handles the connect thread hands to the owner once Hello succeeded.
struct LinkHandles {
    stream: UnixStream,
    writer: Sender<WriteCommand>,
    reader_stopped: Receiver<()>,
    writer_stopped: Receiver<()>,
}

enum WriteCommand {
    Bytes(Vec<u8>),
    Stop,
}

struct HubLink {
    generation: u64,
    stream: UnixStream,
    writer: Sender<WriteCommand>,
    reader_stopped: Receiver<()>,
    writer_stopped: Receiver<()>,
}

impl HubLink {
    /// Close without waiting: stop the writer and shut the socket so the
    /// reader unblocks. Used once the connection has already failed.
    fn close_now(self) {
        let _ = self.writer.send(WriteCommand::Stop);
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    /// Close within `bound`: let the writer drain queued frames (for example a
    /// final Detach) first, then shut the socket so the reader unblocks.
    /// Returns whether both threads confirmed their stop within the bound.
    fn close_within(self, bound: Duration) -> bool {
        // timer: deadline — link threads confirm their stop within the close bound; expiry returns an incomplete close
        let deadline = Instant::now() + bound;
        let _ = self.writer.send(WriteCommand::Stop);
        let left = deadline.saturating_duration_since(Instant::now());
        // timer: deadline — the writer drains and confirms within the close bound; expiry returns an incomplete close
        let writer_stopped = self.writer_stopped.recv_timeout(left).is_ok();
        let _ = self.stream.shutdown(Shutdown::Both);
        let left = deadline.saturating_duration_since(Instant::now());
        // timer: deadline — the reader confirms after the socket shutdown within the close bound; expiry returns an incomplete close
        let reader_stopped = self.reader_stopped.recv_timeout(left).is_ok();
        writer_stopped && reader_stopped
    }
}

struct PendingRequest {
    deadline: Instant,
}

/// Shared pending-wake accounting between the reader thread and the owner.
struct WakeBudget {
    items: AtomicUsize,
    bytes: AtomicUsize,
    /// Routes (with route generation) already reported as faulted.
    faulted_routes: Mutex<BTreeSet<(String, u64)>>,
}

impl WakeBudget {
    fn new() -> Self {
        Self {
            items: AtomicUsize::new(0),
            bytes: AtomicUsize::new(0),
            faulted_routes: Mutex::new(BTreeSet::new()),
        }
    }

    fn is_faulted(&self, route: &str, generation: u64) -> bool {
        self.faulted_routes
            .lock()
            .map(|set| set.contains(&(route.to_string(), generation)))
            .unwrap_or(true)
    }

    /// Mark a route faulted. Returns false when it was already marked.
    fn mark_faulted(&self, route: &str, generation: u64) -> bool {
        self.faulted_routes
            .lock()
            .map(|mut set| set.insert((route.to_string(), generation)))
            .unwrap_or(false)
    }

    fn forget_route(&self, route: &str) {
        if let Ok(mut set) = self.faulted_routes.lock() {
            set.retain(|(candidate, _)| candidate != route);
        }
    }

    /// Reserve one terminal item of `bytes`. Returns false at the bound.
    fn try_reserve_terminal(&self, bytes: usize) -> bool {
        let items = self.items.load(Ordering::Acquire);
        let pending_bytes = self.bytes.load(Ordering::Acquire);
        if items.saturating_add(1) > MAX_PENDING_WAKE_ITEMS
            || pending_bytes.saturating_add(bytes) > MAX_PENDING_WAKE_BYTES
        {
            return false;
        }
        self.items.fetch_add(1, Ordering::AcqRel);
        self.bytes.fetch_add(bytes, Ordering::AcqRel);
        true
    }

    fn reserve_control(&self) {
        self.items.fetch_add(1, Ordering::AcqRel);
    }

    fn release_control(&self) {
        self.items.fetch_sub(1, Ordering::AcqRel);
    }

    fn release_terminal(&self, bytes: usize) {
        self.items.fetch_sub(1, Ordering::AcqRel);
        self.bytes.fetch_sub(bytes, Ordering::AcqRel);
    }

    /// Give back credit that left a route's pool.
    fn release(&self, release: Release) {
        let items = usize::try_from(release.items).unwrap_or(usize::MAX);
        let bytes = usize::try_from(release.bytes).unwrap_or(usize::MAX);
        self.items.fetch_sub(items, Ordering::AcqRel);
        self.bytes.fetch_sub(bytes, Ordering::AcqRel);
    }
}

/// Stop signal for the input thread: a flag plus the waker of its executor.
struct StopSignal {
    stopped: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl StopSignal {
    fn new() -> Self {
        Self {
            stopped: AtomicBool::new(false),
            waker: Mutex::new(None),
        }
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut slot) = self.waker.lock()
            && let Some(waker) = slot.take()
        {
            waker.wake();
        }
    }
}

/// Future that resolves to `None` once the stop signal fires.
struct StopWait<'a>(&'a StopSignal);

impl Future for StopWait<'_> {
    type Output = Option<io::Result<Event>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0.stopped.load(Ordering::SeqCst) {
            return Poll::Ready(None);
        }
        if let Ok(mut slot) = self.0.waker.lock() {
            *slot = Some(cx.waker().clone());
        }
        if self.0.stopped.load(Ordering::SeqCst) {
            return Poll::Ready(None);
        }
        Poll::Pending
    }
}

struct InputPump {
    stop: Arc<StopSignal>,
    stopped: Receiver<()>,
    handle: Option<JoinHandle<()>>,
}

/// The single I/O owner for one TUI process.
pub struct HubIo {
    wakes: Receiver<IoMessage>,
    wake_tx: Sender<IoMessage>,
    ready: VecDeque<AppWake>,
    input: Option<InputPump>,
    link: Option<HubLink>,
    /// Generation of the newest connection attempt (connecting or connected).
    generation: u64,
    request_ids: RequestIdSequence,
    pending: BTreeMap<u64, PendingRequest>,
    budget: Arc<WakeBudget>,
    /// Output credit per route of the current connection (S13).
    credit: CreditLedger,
    /// Test seam: when set, terminal input frames are recorded instead of
    /// written, so a test without a Hub can observe the write path.
    #[cfg(test)]
    captured_terminal: Option<Vec<(String, u64, Vec<u8>)>>,
    /// Test seam: when set, credit frames are recorded instead of written.
    #[cfg(test)]
    captured_credit: Option<Vec<DaemonUnixCreditFrame>>,
}

impl HubIo {
    /// Create the owner without an input thread. Harness drivers that own no
    /// terminal use this constructor.
    pub fn new() -> Self {
        let (wake_tx, wakes) = mpsc::channel();
        Self {
            wakes,
            wake_tx,
            ready: VecDeque::new(),
            input: None,
            link: None,
            generation: 0,
            request_ids: RequestIdSequence::new(),
            pending: BTreeMap::new(),
            budget: Arc::new(WakeBudget::new()),
            credit: CreditLedger::default(),
            #[cfg(test)]
            captured_terminal: None,
            #[cfg(test)]
            captured_credit: None,
        }
    }

    /// Record terminal input frames instead of writing them. Test only.
    #[cfg(test)]
    pub fn capture_terminal_frames(&mut self) {
        self.captured_terminal = Some(Vec::new());
    }

    /// Frames recorded since the last call: (route, generation, body). Test only.
    #[cfg(test)]
    pub fn take_captured_terminal_frames(&mut self) -> Vec<(String, u64, Vec<u8>)> {
        self.captured_terminal
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    /// Create the owner and start the Crossterm `EventStream` input thread.
    pub fn with_terminal_input() -> io::Result<Self> {
        let mut io = Self::new();
        io.start_input_thread()?;
        Ok(io)
    }

    fn start_input_thread(&mut self) -> io::Result<()> {
        let stop = Arc::new(StopSignal::new());
        let (stopped_tx, stopped_rx) = mpsc::channel();
        let sender = self.wake_tx.clone();
        let thread_stop = Arc::clone(&stop);
        let handle = thread::Builder::new()
            .name("botster-tui-input".to_string())
            .spawn(move || {
                run_input_thread(&sender, &thread_stop);
                let _ = stopped_tx.send(());
            })?;
        self.input = Some(InputPump {
            stop,
            stopped: stopped_rx,
            handle: Some(handle),
        });
        Ok(())
    }

    /// Begin a connection attempt. Hello runs on a connect thread; the result
    /// arrives as `AppWake::Connected` or `AppWake::Disconnected` with the
    /// returned generation.
    pub fn connect(
        &mut self,
        endpoint: DaemonEndpoint,
        host_requirement: DaemonCompatibilityRequirement,
        terminal_requirement: TerminalCompatibilityRequirement,
    ) -> u64 {
        self.disconnect_now();
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        let sender = self.wake_tx.clone();
        let budget = Arc::clone(&self.budget);
        let spawned = thread::Builder::new()
            .name(format!("botster-tui-hub-connect-{generation}"))
            .spawn(move || {
                run_connect_thread(
                    generation,
                    &endpoint,
                    &host_requirement,
                    &terminal_requirement,
                    &sender,
                    budget,
                );
            });
        if let Err(error) = spawned {
            let _ = self.wake_tx.send(IoMessage::Disconnected {
                generation,
                error: DaemonTransportError::Io(error),
            });
        }
        generation
    }

    /// Generation of the current connection attempt.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether a Hello-complete connection is installed.
    pub fn is_connected(&self) -> bool {
        self.link.is_some()
    }

    /// Close the current connection, if any, and fail every pending request.
    ///
    /// Waits at most `bound` for the reader and writer threads to stop.
    /// Close the link within `bound` and fail its pending requests. Returns
    /// whether the link threads confirmed their stop; true when no link.
    pub fn disconnect(&mut self, bound: Duration) -> bool {
        let closed = self.link.take().is_none_or(|link| link.close_within(bound));
        self.reset_credit();
        self.fail_pending();
        closed
    }

    /// Close the link without waiting and fail its pending requests.
    pub fn disconnect_now(&mut self) {
        if let Some(link) = self.link.take() {
            link.close_now();
        }
        self.reset_credit();
        self.fail_pending();
    }

    /// The connection ended: every route's credit returns to the budget.
    /// Frames of that connection still queued were in those pools, so they
    /// release nothing when they are dequeued and discarded.
    fn reset_credit(&mut self) {
        let release = self.credit.reset();
        self.budget.release(release);
    }

    /// Grant queued demands while the budget covers each whole frame.
    fn serve_credit(&mut self) {
        let budget = &self.budget;
        let grants = self.credit.serve(|bytes| {
            budget.try_reserve_terminal(usize::try_from(bytes).unwrap_or(usize::MAX))
        });
        for grant in grants {
            let frame = DaemonUnixCreditFrame::Grant {
                route: grant.route,
                generation: grant.generation,
                items: grant.items,
                bytes: grant.bytes,
            };
            #[cfg(test)]
            if let Some(captured) = self.captured_credit.as_mut() {
                captured.push(frame);
                continue;
            }
            let Some(bytes) = encode_unix_credit_frame(&frame) else {
                continue;
            };
            if let Some(link) = self.link.as_ref() {
                let _ = link.writer.send(WriteCommand::Bytes(bytes));
            }
        }
    }

    /// A credit rule was broken: the connection ends typed.
    fn credit_violation(&mut self, violation: &CreditViolation) {
        let reason = match violation {
            CreditViolation::SecondDemand(_) => {
                "a second credit demand before the first was served"
            }
            CreditViolation::UncoveredFrame(_) => "a terminal frame without credit",
            CreditViolation::ReturnExceedsPool(_) => "a credit return larger than the route's pool",
        };
        self.end_link(DaemonTransportError::Protocol(reason));
    }

    fn fail_pending(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        for request_id in pending.into_keys() {
            self.ready.push_back(AppWake::Completed {
                request_id,
                result: Err(DaemonRequestError::ConnectionClosed),
            });
        }
    }

    /// Close the current link from the owner side and queue `Disconnected`.
    fn end_link(&mut self, error: DaemonTransportError) {
        let generation = self.generation;
        if let Some(link) = self.link.take() {
            link.close_now();
        }
        self.reset_credit();
        self.fail_pending();
        self.ready
            .push_back(AppWake::Disconnected { generation, error });
    }

    /// Submit one host-control request with an absolute deadline.
    ///
    /// Returns the request id. Completion arrives as `AppWake::Completed`.
    /// A submit without a connection, or beyond the 32 outstanding bound,
    /// completes immediately through the wake queue.
    pub fn submit(&mut self, request: &DaemonRequest, deadline: Instant) -> u64 {
        let request_id = self.request_ids.next();
        let Some(link) = self.link.as_ref() else {
            self.ready.push_back(AppWake::Completed {
                request_id,
                result: Err(DaemonRequestError::ConnectionClosed),
            });
            return request_id;
        };
        if self.pending.len() >= MAX_OUTSTANDING_REQUESTS {
            self.ready.push_back(AppWake::Completed {
                request_id,
                result: Err(DaemonRequestError::TooManyOutstandingRequests),
            });
            return request_id;
        }
        let frame = ClientFrame::Request {
            request_id: encode_request_id(request_id),
            request: request.clone(),
        };
        let bytes = match encode_client_frame(&frame) {
            Ok(bytes) => bytes,
            Err(error) => {
                // An unencodable request is a client defect. End the connection
                // deterministically instead of leaving the request half-sent.
                self.pending.insert(request_id, PendingRequest { deadline });
                self.end_link(error);
                return request_id;
            }
        };
        if link.writer.send(WriteCommand::Bytes(bytes)).is_err() {
            self.ready.push_back(AppWake::Completed {
                request_id,
                result: Err(DaemonRequestError::ConnectionClosed),
            });
            return request_id;
        }
        self.pending.insert(request_id, PendingRequest { deadline });
        request_id
    }

    /// Drop the local completion for one request. A late response for the id
    /// is discarded. Returns whether the request was still pending.
    ///
    /// Part of the owner contract (section 4.4); the application currently
    /// lets deadlines expire instead of cancelling.
    #[allow(dead_code)]
    pub fn cancel(&mut self, request_id: u64) -> bool {
        self.pending.remove(&request_id).is_some()
    }

    /// Number of outstanding host-control requests.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn outstanding(&self) -> usize {
        self.pending.len()
    }

    /// Send one encoded terminal input frame on the terminal plane.
    ///
    /// Returns false when no connection is installed or the container cannot
    /// carry the route and body.
    pub fn send_terminal(&mut self, route: &str, generation: u64, body: &[u8]) -> bool {
        #[cfg(test)]
        if let Some(captured) = self.captured_terminal.as_mut() {
            captured.push((route.to_string(), generation, body.to_vec()));
            return true;
        }
        if self.link.is_none() {
            return false;
        }
        let Some(bytes) =
            encode_unix_terminal_frame(route, generation, INPUT_CONTAINER_EPOCH, body)
        else {
            return false;
        };
        // Input is credited per route: a frame beyond the route's credit waits
        // in order until the Hub returns credit. A route with no attachment
        // (closed or never attached) takes no input.
        let Some(ready) = self.credit.send_input(route, generation, bytes) else {
            return false;
        };
        self.write_input(ready)
    }

    /// Write input frames that input credit released, in order.
    fn write_input(&mut self, frames: Vec<Vec<u8>>) -> bool {
        let Some(link) = self.link.as_ref() else {
            return false;
        };
        frames
            .into_iter()
            .all(|bytes| link.writer.send(WriteCommand::Bytes(bytes)).is_ok())
    }

    /// Forget fault state for a retired route so a re-attach with the same
    /// route id is admitted again.
    pub fn forget_route(&mut self, route: &str) {
        self.budget.forget_route(route);
    }

    /// Charge one frame the application retains after dequeue (for example a
    /// terminal frame parked until its Attach response) against the same
    /// 256-item / 8 MiB pending budget as queued wakes. Returns false at the
    /// bound; the caller then fails only the affected route.
    pub fn try_retain(&self, bytes: usize) -> bool {
        self.budget.try_reserve_terminal(bytes)
    }

    /// Release one retained frame charged with `try_retain`.
    pub fn release_retained(&mut self, bytes: usize) {
        self.budget.release_terminal(bytes);
        self.serve_credit();
    }

    /// Earliest absolute deadline among outstanding requests.
    pub fn earliest_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|pending| pending.deadline).min()
    }

    /// Complete every request whose deadline passed.
    fn expire(&mut self, now: Instant) {
        let expired: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(request_id, _)| *request_id)
            .collect();
        for request_id in expired {
            self.pending.remove(&request_id);
            self.ready.push_back(AppWake::Completed {
                request_id,
                result: Err(DaemonRequestError::DeadlineExpired),
            });
        }
    }

    /// Wait for the next wake, or until `until` when it is earlier.
    ///
    /// `AppWake::Deadline` is returned only when `until` passed and nothing
    /// else was ready. Expired requests complete before `Deadline`.
    pub fn next_wake(&mut self, until: Option<Instant>) -> AppWake {
        loop {
            if let Some(wake) = self.ready.pop_front() {
                return wake;
            }
            let now = Instant::now();
            self.expire(now);
            if !self.ready.is_empty() {
                continue;
            }
            let wait_until = match (until, self.earliest_deadline()) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            };
            let message = match wait_until {
                Some(deadline) => {
                    let timeout = deadline.saturating_duration_since(now);
                    // timer: deadline — earliest request or app deadline; expiry delivers AppWake::Deadline
                    match self.wakes.recv_timeout(timeout) {
                        Ok(message) => Some(message),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return AppWake::Shutdown,
                    }
                }
                None => match self.wakes.recv() {
                    Ok(message) => Some(message),
                    Err(_) => return AppWake::Shutdown,
                },
            };
            match message {
                Some(message) => {
                    if let Some(wake) = self.map_message(message) {
                        return wake;
                    }
                }
                None => {
                    let now = Instant::now();
                    self.expire(now);
                    if let Some(wake) = self.ready.pop_front() {
                        return wake;
                    }
                    if until.is_some_and(|deadline| deadline <= now) {
                        return AppWake::Deadline;
                    }
                }
            }
        }
    }

    /// Queue one wake as if an I/O thread produced it. Test injection only.
    #[cfg(test)]
    pub fn inject_wake(&mut self, wake: AppWake) {
        self.ready.push_back(wake);
    }

    /// Take one wake without waiting.
    pub fn try_next_wake(&mut self) -> Option<AppWake> {
        if let Some(wake) = self.ready.pop_front() {
            return Some(wake);
        }
        self.expire(Instant::now());
        if let Some(wake) = self.ready.pop_front() {
            return Some(wake);
        }
        loop {
            let message = self.wakes.try_recv().ok()?;
            if let Some(wake) = self.map_message(message) {
                return Some(wake);
            }
        }
    }

    /// Apply one credit frame from the Hub to the ledger.
    fn apply_credit(&mut self, frame: DaemonUnixCreditFrame) {
        let outcome = match frame {
            DaemonUnixCreditFrame::Demand {
                route,
                generation,
                bytes,
            } => self.credit.demand(&route, generation, bytes),
            DaemonUnixCreditFrame::Return {
                route,
                generation,
                items,
                bytes,
            } => self
                .credit
                .returned(&route, generation, u64::from(items), bytes)
                .map(|release| self.budget.release(release)),
            DaemonUnixCreditFrame::Closed {
                route, generation, ..
            } => {
                let release = self.credit.closed(&route, generation);
                self.budget.release(release);
                Ok(())
            }
            DaemonUnixCreditFrame::InputCredit {
                route,
                generation,
                items,
            } => {
                let ready = self.credit.input_returned(&route, generation, items);
                self.write_input(ready);
                Ok(())
            }
            DaemonUnixCreditFrame::Grant { .. } => {
                self.end_link(DaemonTransportError::Protocol(
                    "the Hub sent a client-only credit grant",
                ));
                Ok(())
            }
        };
        if let Err(violation) = outcome {
            self.credit_violation(&violation);
        }
    }

    fn map_message(&mut self, message: IoMessage) -> Option<AppWake> {
        match message {
            IoMessage::Input(event) => Some(AppWake::Input(event)),
            IoMessage::InputEnded => Some(AppWake::Shutdown),
            IoMessage::Terminal { generation, frame } => {
                if !self.current(generation) {
                    // Settled when that connection's credit was reset.
                    return None;
                }
                let bytes = u64::try_from(frame.frame.len()).unwrap_or(u64::MAX);
                match self
                    .credit
                    .dequeued(frame.route.as_str(), frame.generation, bytes)
                {
                    Ok(release) => {
                        self.budget.release(release);
                        self.serve_credit();
                        Some(AppWake::Terminal(frame))
                    }
                    Err(violation) => {
                        self.credit_violation(&violation);
                        None
                    }
                }
            }
            IoMessage::Credit { generation, frame } => {
                self.budget.release_control();
                if !self.current(generation) {
                    return None;
                }
                self.apply_credit(frame);
                self.serve_credit();
                None
            }
            IoMessage::Response {
                generation,
                request_id,
                response,
            } => {
                self.budget.release_control();
                self.serve_credit();
                if !self.current(generation) {
                    return None;
                }
                // The Hub demands credit for a route only after its Attach
                // response, so the route is known before its first demand.
                if response.kind == DaemonResponseKind::TerminalAttached
                    && let Some(attach) = response.terminal_attach.as_ref()
                {
                    self.credit.attach(
                        &attach.subscription_id,
                        attach.generation,
                        attach.input_credit_items,
                    );
                }
                // A response for an unknown, cancelled, or expired id is discarded
                // for that key only.
                self.pending.remove(&request_id)?;
                Some(AppWake::Completed {
                    request_id,
                    result: Ok(response),
                })
            }
            IoMessage::Event { generation, event } => {
                self.budget.release_control();
                self.serve_credit();
                self.current(generation).then_some(AppWake::Event(event))
            }
            IoMessage::Entity { generation, frame } => {
                self.budget.release_control();
                self.serve_credit();
                self.current(generation).then_some(AppWake::Entity(frame))
            }
            IoMessage::Connected {
                generation,
                ack,
                link,
            } => {
                if generation != self.generation {
                    let _ = link.stream.shutdown(Shutdown::Both);
                    let _ = link.writer.send(WriteCommand::Stop);
                    return None;
                }
                self.request_ids = RequestIdSequence::new();
                let link = *link;
                self.link = Some(HubLink {
                    generation,
                    stream: link.stream,
                    writer: link.writer,
                    reader_stopped: link.reader_stopped,
                    writer_stopped: link.writer_stopped,
                });
                Some(AppWake::Connected { generation, ack })
            }
            IoMessage::Disconnected { generation, error } => {
                if generation != self.generation {
                    return None;
                }
                if let Some(link) = self.link.take() {
                    link.close_now();
                }
                self.reset_credit();
                self.fail_pending();
                Some(AppWake::Disconnected { generation, error })
            }
            IoMessage::RouteFault {
                generation,
                route,
                route_generation,
                reason,
            } => self.current(generation).then_some(AppWake::RouteFault {
                route,
                generation: route_generation,
                reason,
            }),
        }
    }

    fn current(&self, generation: u64) -> bool {
        self.link
            .as_ref()
            .is_some_and(|link| link.generation == generation)
    }

    /// Stop every thread within `bound` and drop the owner.
    ///
    /// The input thread is signalled through its executor; dropping the
    /// `EventStream` inside that thread wakes Crossterm's internal wait thread
    /// (verified against crossterm 0.29.0 `event/stream.rs`: `Drop` sets the
    /// shutdown flag and wakes the mio poller).
    ///
    /// Returns whether every thread confirmed its stop; false is an
    /// incomplete shutdown that the caller reports.
    pub fn shutdown(mut self, bound: Duration) -> bool {
        // timer: deadline — link and input threads stop within the shutdown bound; expiry returns an incomplete shutdown
        let deadline = Instant::now() + bound;
        let link_closed = self.disconnect(deadline.saturating_duration_since(Instant::now()));
        let Some(mut input) = self.input.take() else {
            return link_closed;
        };
        input.stop.stop();
        let left = deadline.saturating_duration_since(Instant::now());
        // timer: deadline — the input thread confirms its stop within the shutdown bound; expiry returns an incomplete shutdown
        let input_stopped = input.stopped.recv_timeout(left).is_ok();
        if input_stopped && let Some(handle) = input.handle.take() {
            let _ = handle.join();
        }
        link_closed && input_stopped
    }
}

impl Default for HubIo {
    fn default() -> Self {
        Self::new()
    }
}

fn run_input_thread(sender: &Sender<IoMessage>, stop: &StopSignal) {
    let mut stream = EventStream::new();
    future::block_on(async {
        loop {
            match future::or(stream.next(), StopWait(stop)).await {
                Some(Ok(event)) => {
                    if sender.send(IoMessage::Input(event)).is_err() {
                        return;
                    }
                }
                Some(Err(_)) | None => return,
            }
        }
    });
    // Drop wakes Crossterm's internal poll thread before this thread exits.
    drop(stream);
    if !stop.stopped.load(Ordering::SeqCst) {
        let _ = sender.send(IoMessage::InputEnded);
    }
}

fn run_connect_thread(
    generation: u64,
    endpoint: &DaemonEndpoint,
    host_requirement: &DaemonCompatibilityRequirement,
    terminal_requirement: &TerminalCompatibilityRequirement,
    sender: &Sender<IoMessage>,
    budget: Arc<WakeBudget>,
) {
    let disconnected = |error: DaemonTransportError| IoMessage::Disconnected { generation, error };
    let (stream, ack) = match connect_and_hello_with_terminal_requirement(
        endpoint,
        host_requirement,
        Some(terminal_requirement),
    ) {
        Ok(connected) => connected,
        Err(error) => {
            let _ = sender.send(disconnected(error));
            return;
        }
    };
    let (reader_stream, writer_stream) = match (stream.try_clone(), stream.try_clone()) {
        (Ok(reader_stream), Ok(writer_stream)) => (reader_stream, writer_stream),
        (Err(error), _) | (_, Err(error)) => {
            let _ = sender.send(disconnected(DaemonTransportError::Io(error)));
            return;
        }
    };
    let (writer_tx, writer_rx) = mpsc::channel();
    let (writer_stopped_tx, writer_stopped_rx) = mpsc::channel();
    let (reader_stopped_tx, reader_stopped_rx) = mpsc::channel();
    let writer_sender = sender.clone();
    let writer_spawn = thread::Builder::new()
        .name(format!("botster-tui-hub-writer-{generation}"))
        .spawn(move || {
            run_writer_thread(generation, writer_stream, &writer_rx, &writer_sender);
            let _ = writer_stopped_tx.send(());
        });
    if let Err(error) = writer_spawn {
        let _ = sender.send(disconnected(DaemonTransportError::Io(error)));
        return;
    }
    let reader_sender = sender.clone();
    let reader_spawn = thread::Builder::new()
        .name(format!("botster-tui-hub-reader-{generation}"))
        .spawn(move || {
            run_reader_thread(generation, reader_stream, &reader_sender, &budget);
            let _ = reader_stopped_tx.send(());
        });
    if let Err(error) = reader_spawn {
        let _ = writer_tx.send(WriteCommand::Stop);
        let _ = sender.send(disconnected(DaemonTransportError::Io(error)));
        return;
    }
    let _ = sender.send(IoMessage::Connected {
        generation,
        ack: Box::new(ack),
        link: Box::new(LinkHandles {
            stream,
            writer: writer_tx,
            reader_stopped: reader_stopped_rx,
            writer_stopped: writer_stopped_rx,
        }),
    });
}

fn run_writer_thread(
    generation: u64,
    mut stream: UnixStream,
    commands: &Receiver<WriteCommand>,
    sender: &Sender<IoMessage>,
) {
    while let Ok(command) = commands.recv() {
        match command {
            WriteCommand::Bytes(bytes) => {
                if let Err(error) = stream.write_all(&bytes) {
                    let _ = sender.send(IoMessage::Disconnected {
                        generation,
                        error: DaemonTransportError::Io(error),
                    });
                    return;
                }
            }
            WriteCommand::Stop => return,
        }
    }
}

fn run_reader_thread(
    generation: u64,
    stream: UnixStream,
    sender: &Sender<IoMessage>,
    budget: &WakeBudget,
) {
    let mut reader = BufReader::new(stream);
    let mut decoder = DaemonUnixFrameReader::new();
    loop {
        let frame = match decoder.read_frame(&mut reader) {
            Ok(frame) => frame,
            Err(error) => {
                let _ = sender.send(IoMessage::Disconnected { generation, error });
                return;
            }
        };
        let message = match frame {
            DaemonUnixMuxFrame::Server(ServerFrame::HelloAck { .. }) => {
                let _ = sender.send(IoMessage::Disconnected {
                    generation,
                    error: DaemonTransportError::Protocol("unexpected hello ack after handshake"),
                });
                return;
            }
            DaemonUnixMuxFrame::Server(ServerFrame::Response {
                request_id,
                response,
            }) => {
                let Some(request_id) = parse_request_id(&request_id) else {
                    let _ = sender.send(IoMessage::Disconnected {
                        generation,
                        error: DaemonTransportError::Protocol(
                            "response request_id is not a decimal u64",
                        ),
                    });
                    return;
                };
                budget.reserve_control();
                IoMessage::Response {
                    generation,
                    request_id,
                    response: Box::new(response),
                }
            }
            DaemonUnixMuxFrame::Server(ServerFrame::Event { event }) => {
                budget.reserve_control();
                IoMessage::Event { generation, event }
            }
            DaemonUnixMuxFrame::Server(ServerFrame::Entity { entity }) => {
                budget.reserve_control();
                IoMessage::Entity {
                    generation,
                    frame: entity,
                }
            }
            DaemonUnixMuxFrame::Server(ServerFrame::Close { reason }) => {
                let _ = sender.send(IoMessage::Disconnected {
                    generation,
                    error: DaemonTransportError::ClosedByHub(reason),
                });
                return;
            }
            DaemonUnixMuxFrame::Credit(frame) => {
                budget.reserve_control();
                IoMessage::Credit { generation, frame }
            }
            DaemonUnixMuxFrame::Terminal(terminal) => {
                match admit_terminal_frame(terminal, budget) {
                    Ok(Some(frame)) => IoMessage::Terminal { generation, frame },
                    Ok(None) => continue,
                    Err(TerminalAdmitError::Route {
                        route,
                        route_generation,
                        reason,
                    }) => IoMessage::RouteFault {
                        generation,
                        route,
                        route_generation,
                        reason,
                    },
                    Err(TerminalAdmitError::Protocol(reason)) => {
                        let _ = sender.send(IoMessage::Disconnected {
                            generation,
                            error: DaemonTransportError::Protocol(reason),
                        });
                        return;
                    }
                }
            }
        };
        if sender.send(message).is_err() {
            return;
        }
    }
}

enum TerminalAdmitError {
    Route {
        route: RouteId,
        route_generation: u64,
        reason: String,
    },
    Protocol(&'static str),
}

/// Decode one Hub terminal container into a routed frame.
///
/// The frame's budget was reserved when this client granted its credit, so
/// the reader never waits; the owner debits the route's pool on dequeue. A
/// frame dropped here (decode failure or a faulted route) stays charged to its
/// pool until the generation closes. `Ok(None)` means the frame was dropped
/// for a route already reported faulted. `Err(Route)` carries the first fault
/// for a route (a decode failure); `Err(Protocol)` means the connection itself
/// violated the contract.
fn admit_terminal_frame(
    terminal: DaemonUnixTerminalFrame,
    budget: &WakeBudget,
) -> Result<Option<RoutedTerminalFrame>, TerminalAdmitError> {
    let DaemonUnixTerminalFrame {
        route,
        generation,
        stream_epoch,
        body,
    } = terminal;
    if budget.is_faulted(&route, generation) {
        return Ok(None);
    }
    let route_id = RouteId::new(&route)
        .map_err(|_| TerminalAdmitError::Protocol("terminal container route id is invalid"))?;
    let frame = match TerminalFrame::from_bytes(&body) {
        Ok(frame) => frame,
        Err(error) => {
            budget.mark_faulted(&route, generation);
            return Err(TerminalAdmitError::Route {
                route: route_id,
                route_generation: generation,
                reason: format!("terminal frame decode failed: {error}"),
            });
        }
    };
    Ok(Some(RoutedTerminalFrame {
        route: route_id,
        generation,
        stream_epoch,
        frame,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use botster_terminal_protocol_client::encode_output;

    fn terminal_container(route: &str, generation: u64, body: &[u8]) -> DaemonUnixTerminalFrame {
        DaemonUnixTerminalFrame {
            route: route.to_string(),
            generation,
            stream_epoch: 0,
            body: encode_output(body)
                .expect("encode output")
                .as_bytes()
                .to_vec(),
        }
    }

    /// A HubIo with a current connection (generation 1) whose credit grants
    /// are captured instead of written.
    fn linked_io() -> HubIo {
        let mut io = HubIo::new();
        let (stream, _peer) = UnixStream::pair().expect("socket pair");
        let (writer, _writes) = mpsc::channel();
        let (_reader_stop, reader_stopped) = mpsc::channel();
        let (_writer_stop, writer_stopped) = mpsc::channel();
        io.generation = 1;
        io.link = Some(HubLink {
            generation: 1,
            stream,
            writer,
            reader_stopped,
            writer_stopped,
        });
        io.captured_credit = Some(Vec::new());
        io
    }

    fn attach_response(route: &str, generation: u64) -> Box<DaemonResponse> {
        let mut response = crate::app::tests::base_response(DaemonResponseKind::TerminalAttached);
        response.terminal_attach = Some(botster_hub_client::DaemonTerminalAttach::new(
            "s", route, generation, 64,
        ));
        Box::new(response)
    }

    fn credit(io: &mut HubIo, frame: DaemonUnixCreditFrame) -> Option<AppWake> {
        io.budget.reserve_control();
        io.map_message(IoMessage::Credit {
            generation: 1,
            frame,
        })
    }

    fn demand(route: &str, bytes: u64) -> DaemonUnixCreditFrame {
        DaemonUnixCreditFrame::Demand {
            route: route.to_string(),
            generation: 7,
            bytes,
        }
    }

    fn granted(io: &mut HubIo) -> Vec<(String, u64)> {
        io.captured_credit
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
            .into_iter()
            .map(|frame| match frame {
                DaemonUnixCreditFrame::Grant {
                    route,
                    items: 1,
                    bytes,
                    ..
                } => (route, bytes),
                other => panic!("expected a one-frame grant, got {other:?}"),
            })
            .collect()
    }

    /// Dequeue one terminal frame of the route through the owner.
    fn dequeue(io: &mut HubIo, route: &str) -> Option<AppWake> {
        let frame = admit_terminal_frame(terminal_container(route, 7, b"hi"), &io.budget)
            .ok()
            .flatten()
            .expect("frame decodes");
        io.map_message(IoMessage::Terminal {
            generation: 1,
            frame,
        })
    }

    fn frame_len() -> u64 {
        u64::try_from(
            encode_output(b"hi")
                .expect("encode output")
                .as_bytes()
                .len(),
        )
        .expect("small")
    }

    fn pending(io: &HubIo) -> (usize, usize) {
        (
            io.budget.items.load(Ordering::Acquire),
            io.budget.bytes.load(Ordering::Acquire),
        )
    }

    #[test]
    fn demands_are_granted_only_for_attached_routes() {
        let mut io = linked_io();
        credit(&mut io, demand("r", frame_len()));
        assert!(granted(&mut io).is_empty(), "stale before the attach");
        io.pending.insert(
            1,
            PendingRequest {
                // timer: deadline — test request expiry bound
                deadline: Instant::now() + Duration::from_secs(60),
            },
        );
        io.budget.reserve_control();
        io.map_message(IoMessage::Response {
            generation: 1,
            request_id: 1,
            response: attach_response("r", 7),
        });
        credit(&mut io, demand("r", frame_len()));
        assert_eq!(granted(&mut io), vec![("r".to_string(), frame_len())]);
        assert_eq!(
            pending(&io),
            (1, usize::try_from(frame_len()).expect("small"))
        );
        // The covered frame dequeues and its charge leaves the budget.
        assert!(matches!(dequeue(&mut io, "r"), Some(AppWake::Terminal(_))));
        assert_eq!(pending(&io), (0, 0));
    }

    #[test]
    fn a_frame_without_credit_closes_the_connection() {
        let mut io = linked_io();
        io.credit.attach("r", 7, 64);
        assert!(dequeue(&mut io, "r").is_none());
        assert!(io.link.is_none(), "the violation ends the link");
        assert!(matches!(
            io.ready.pop_front(),
            Some(AppWake::Disconnected {
                error: DaemonTransportError::Protocol("a terminal frame without credit"),
                ..
            })
        ));
    }

    #[test]
    fn a_waiting_head_demand_is_granted_when_budget_returns() {
        let mut io = linked_io();
        io.credit.attach("a", 7, 64);
        io.credit.attach("b", 7, 64);
        let half = u64::try_from(MAX_PENDING_WAKE_BYTES / 2).expect("fits");
        credit(&mut io, demand("a", half));
        credit(&mut io, demand("b", half));
        assert_eq!(granted(&mut io).len(), 2, "both halves fit");
        io.credit.attach("c", 7, 64);
        credit(&mut io, demand("c", 1));
        assert!(granted(&mut io).is_empty(), "the budget is full");
        // RETURN of a's credit frees room; the waiting demand is granted.
        credit(
            &mut io,
            DaemonUnixCreditFrame::Return {
                route: "a".to_string(),
                generation: 7,
                items: 1,
                bytes: half,
            },
        );
        assert_eq!(granted(&mut io), vec![("c".to_string(), 1)]);
    }

    #[test]
    fn closed_and_disconnect_release_the_pools() {
        let mut io = linked_io();
        io.credit.attach("a", 7, 64);
        io.credit.attach("b", 7, 64);
        credit(&mut io, demand("a", 100));
        credit(&mut io, demand("b", 200));
        assert_eq!(pending(&io), (2, 300));
        credit(
            &mut io,
            DaemonUnixCreditFrame::Closed {
                route: "a".to_string(),
                generation: 7,
                spent_items: 0,
                spent_bytes: 0,
            },
        );
        assert_eq!(pending(&io), (1, 200));
        // A demand for the closed generation is stale.
        credit(&mut io, demand("a", 10));
        granted(&mut io);
        assert_eq!(pending(&io), (1, 200));
        io.disconnect_now();
        assert_eq!(
            pending(&io),
            (0, 0),
            "a closed connection releases every pool"
        );
    }

    #[test]
    fn a_second_unserved_demand_or_an_over_return_closes_the_connection() {
        let mut io = linked_io();
        io.credit.attach("a", 7, 64);
        // One byte already pending, so the whole-budget demand stays unserved.
        assert!(io.budget.try_reserve_terminal(1));
        let full = u64::try_from(MAX_PENDING_WAKE_BYTES).expect("fits");
        credit(&mut io, demand("a", full));
        assert!(granted(&mut io).is_empty());
        credit(&mut io, demand("a", 1));
        assert!(
            io.link.is_none(),
            "a second demand while the first is unserved"
        );

        let mut io = linked_io();
        io.credit.attach("a", 7, 64);
        credit(
            &mut io,
            DaemonUnixCreditFrame::Return {
                route: "a".to_string(),
                generation: 7,
                items: 1,
                bytes: 1,
            },
        );
        assert!(io.link.is_none(), "a return larger than the pool");
    }

    #[test]
    fn decode_failure_faults_the_route_without_reserving_budget() {
        let budget = WakeBudget::new();
        let malformed = DaemonUnixTerminalFrame {
            route: "r".to_string(),
            generation: 2,
            stream_epoch: 0,
            body: vec![9, 9, 9],
        };
        match admit_terminal_frame(malformed, &budget) {
            Err(TerminalAdmitError::Route { reason, .. }) => {
                assert!(reason.contains("decode failed"));
            }
            _ => panic!("malformed body must fault the route"),
        }
        assert_eq!(budget.items.load(Ordering::Acquire), 0);
        assert!(budget.is_faulted("r", 2));
    }

    #[test]
    fn submit_without_a_link_completes_with_connection_closed() {
        let mut io = HubIo::new();
        let request_id = io.submit(
            &DaemonRequest::Status,
            // timer: deadline — test request expiry bound
            Instant::now() + Duration::from_secs(1),
        );
        match io.next_wake(Some(Instant::now())) {
            AppWake::Completed {
                request_id: completed,
                result: Err(DaemonRequestError::ConnectionClosed),
            } => assert_eq!(completed, request_id),
            _ => panic!("submit without a link must complete as connection closed"),
        }
        assert_eq!(io.outstanding(), 0);
    }

    #[test]
    fn deadline_wake_returns_only_after_the_deadline() {
        let mut io = HubIo::new();
        // timer: deadline — exercises the request-expiry path; expiry is the asserted outcome
        let until = Instant::now() + Duration::from_millis(20);
        assert!(matches!(io.next_wake(Some(until)), AppWake::Deadline));
        assert!(Instant::now() >= until);
    }

    #[test]
    fn injected_wakes_are_delivered_in_order() {
        let mut io = HubIo::new();
        io.inject_wake(AppWake::Deadline);
        io.inject_wake(AppWake::Shutdown);
        assert!(matches!(io.try_next_wake(), Some(AppWake::Deadline)));
        assert!(matches!(io.try_next_wake(), Some(AppWake::Shutdown)));
        assert!(io.try_next_wake().is_none());
    }

    #[test]
    fn stop_signal_resolves_the_input_wait_from_another_thread() {
        let stop = Arc::new(StopSignal::new());
        let remote = Arc::clone(&stop);
        let (registered_tx, registered_rx) = std::sync::mpsc::channel();
        let handle = thread::spawn(move || {
            // Stop only after the waiter registered its waker and went Pending.
            registered_rx.recv().expect("the waiter registers first");
            remote.stop();
        });
        let mut wait = StopWait(&stop);
        let mut registered = Some(registered_tx);
        let resolved = future::block_on(future::poll_fn(|cx| {
            let poll = Pin::new(&mut wait).poll(cx);
            if poll.is_pending()
                && let Some(registered) = registered.take()
            {
                let _ = registered.send(());
            }
            poll
        }));
        assert!(resolved.is_none());
        handle.join().expect("stop thread");
    }

    #[test]
    fn frames_from_a_previous_connection_generation_are_discarded() {
        let mut io = HubIo::new();
        assert!(
            io.map_message(IoMessage::Event {
                generation: 7,
                event: DaemonEvent::RuntimeObservation {
                    kind: "stale".to_string(),
                },
            })
            .is_none()
        );
        assert!(
            io.map_message(IoMessage::Entity {
                generation: 7,
                frame: DaemonEntityFrame::Remove {
                    subscription_id: "s".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 1,
                    id: "x".to_string(),
                },
            })
            .is_none()
        );
    }
}
