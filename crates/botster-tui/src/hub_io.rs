//! Wake-driven Hub I/O owner.
//!
//! One `HubIo` owns every blocking wait the TUI performs:
//!
//! - the Crossterm `EventStream` input thread,
//! - one socket reader thread and one socket writer thread per Hub connection,
//! - one reader thread per dedicated entity-stream connection (Hub converts a
//!   connection into an entity stream on `SubscribeEntities`),
//! - the absolute-deadline table for outstanding host-control requests.
//!
//! The application thread never blocks on a socket, the filesystem, or a timer
//! that is not an absolute deadline. It waits on exactly one channel with
//! `recv_timeout` to the earliest deadline and applies the `AppWake` it gets.
//!
//! Bounds (section 6 of the implementation contract):
//!
//! - 32 outstanding host-control requests per connection; a 33rd `submit`
//!   completes immediately with `DaemonRequestError::TooManyOutstandingRequests`.
//! - 256 items / 8 MiB of pending wakes. A terminal frame that would exceed
//!   the bound is dropped and its route is reported once as `RouteFault`; the
//!   application detaches and re-attaches that route only. Control frames are
//!   never shed here because their producers are already bounded (32 responses,
//!   Hub-shed events with `EventGap`, entity snapshots per subscription).

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
    DaemonHelloAck, DaemonRequest, DaemonRequestError, DaemonResponse,
    DaemonResponseKind as ServerResponseKind, DaemonTransportError, DaemonUnixFrameReader,
    DaemonUnixMuxFrame, DaemonUnixTerminalFrame, MAX_OUTSTANDING_REQUESTS, RequestIdSequence,
    ServerFrame, TerminalCompatibilityRequirement, connect_and_hello_with_terminal_requirement,
    encode_client_frame, encode_request_id, encode_unix_terminal_frame, parse_request_id,
};
use botster_terminal_protocol_client::{RouteId, RoutedTerminalFrame, TerminalFrame};
use crossterm::event::{Event, EventStream};
use futures_lite::{StreamExt, future};

/// Pending wake items before a terminal route is shed.
pub const MAX_PENDING_WAKE_ITEMS: usize = 256;
/// Pending wake bytes before a terminal route is shed.
pub const MAX_PENDING_WAKE_BYTES: usize = 8 * 1024 * 1024;

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
        result: Result<DaemonResponse, DaemonRequestError>,
    },
    /// One unsolicited host event on the current connection.
    Event(DaemonEvent),
    /// One frame from an entity-stream connection.
    Entity {
        subscription_id: String,
        frame: DaemonEntityFrame,
    },
    /// One entity-stream connection completed its SubscribeEntities handshake.
    EntitySubscribed {
        subscription_id: String,
        result: Result<(), String>,
    },
    /// One entity-stream connection ended before `unsubscribe_entities`.
    EntityClosed {
        subscription_id: String,
        error: DaemonTransportError,
    },
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
        subscription_id: String,
        frame: DaemonEntityFrame,
    },
    EntityOpened {
        subscription_id: String,
        stream: UnixStream,
        stopped: Receiver<()>,
    },
    EntitySubscribed {
        subscription_id: String,
        result: Result<(), String>,
    },
    EntityClosed {
        subscription_id: String,
        error: DaemonTransportError,
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
    fn close(self, bound: Duration) {
        let _ = self.writer.send(WriteCommand::Stop);
        let _ = self.stream.shutdown(Shutdown::Both);
        let deadline = Instant::now() + bound;
        let _ = self
            .writer_stopped
            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
        let _ = self
            .reader_stopped
            .recv_timeout(deadline.saturating_duration_since(Instant::now()));
    }
}

struct PendingRequest {
    deadline: Instant,
}

/// One dedicated entity-stream connection.
///
/// Hub converts a connection into an entity stream on `SubscribeEntities`;
/// a mux connection with bound routes refuses the request. The owner keeps
/// the stream so it can end the reader thread within a bound.
enum EntityLink {
    /// Connect thread running; no stream yet.
    Connecting,
    /// Stream open; the reader thread signals `stopped` when it exits.
    Open {
        stream: UnixStream,
        stopped: Receiver<()>,
    },
    /// `unsubscribe_entities` ran before the stream arrived.
    Closing,
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
    /// Dedicated entity-stream connections by subscription id.
    entity_links: BTreeMap<String, EntityLink>,
    /// Generation of the newest connection attempt (connecting or connected).
    generation: u64,
    request_ids: RequestIdSequence,
    pending: BTreeMap<u64, PendingRequest>,
    budget: Arc<WakeBudget>,
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
            entity_links: BTreeMap::new(),
            generation: 0,
            request_ids: RequestIdSequence::new(),
            pending: BTreeMap::new(),
            budget: Arc::new(WakeBudget::new()),
        }
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
        self.disconnect(Duration::ZERO);
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

    /// Close the current connection and every entity stream, and fail every
    /// pending request.
    ///
    /// Waits at most `bound` for the reader and writer threads to stop.
    pub fn disconnect(&mut self, bound: Duration) {
        let deadline = Instant::now() + bound;
        if let Some(link) = self.link.take() {
            link.close(bound);
        }
        let ids: Vec<String> = self.entity_links.keys().cloned().collect();
        for subscription_id in ids {
            self.unsubscribe_entities(
                &subscription_id,
                deadline.saturating_duration_since(Instant::now()),
            );
        }
        self.fail_pending(DaemonRequestError::ConnectionClosed);
    }

    /// Open one dedicated entity-stream connection and subscribe.
    ///
    /// The handshake runs on its own thread. `AppWake::EntitySubscribed`
    /// reports admission; frames follow as `AppWake::Entity`; the stream ends
    /// with `AppWake::EntityClosed` unless `unsubscribe_entities` closed it.
    pub fn subscribe_entities(
        &mut self,
        endpoint: DaemonEndpoint,
        host_requirement: DaemonCompatibilityRequirement,
        entity_type: &str,
        subscription_id: &str,
    ) {
        if self.entity_links.contains_key(subscription_id) {
            return;
        }
        self.entity_links
            .insert(subscription_id.to_string(), EntityLink::Connecting);
        let sender = self.wake_tx.clone();
        let budget = Arc::clone(&self.budget);
        let entity_type = entity_type.to_string();
        let id = subscription_id.to_string();
        let spawned = thread::Builder::new()
            .name(format!("botster-tui-entity-{entity_type}"))
            .spawn(move || {
                run_entity_thread(
                    &endpoint,
                    &host_requirement,
                    &entity_type,
                    &id,
                    &sender,
                    &budget,
                );
            });
        if let Err(error) = spawned {
            self.entity_links.remove(subscription_id);
            self.ready.push_back(AppWake::EntityClosed {
                subscription_id: subscription_id.to_string(),
                error: DaemonTransportError::Io(error),
            });
        }
    }

    /// End one entity stream within `bound`. Hub drops the subscription on EOF.
    pub fn unsubscribe_entities(&mut self, subscription_id: &str, bound: Duration) {
        match self.entity_links.remove(subscription_id) {
            None => {}
            Some(EntityLink::Connecting) | Some(EntityLink::Closing) => {
                self.entity_links
                    .insert(subscription_id.to_string(), EntityLink::Closing);
            }
            Some(EntityLink::Open { stream, stopped }) => {
                let _ = stream.shutdown(Shutdown::Both);
                let _ = stopped.recv_timeout(bound);
            }
        }
    }

    /// Whether an entity stream is connecting or open for this id.
    pub fn has_entity_stream(&self, subscription_id: &str) -> bool {
        matches!(
            self.entity_links.get(subscription_id),
            Some(EntityLink::Connecting | EntityLink::Open { .. })
        )
    }

    fn fail_pending(&mut self, error: DaemonRequestError) {
        let pending = std::mem::take(&mut self.pending);
        for request_id in pending.into_keys() {
            self.ready.push_back(AppWake::Completed {
                request_id,
                result: Err(error.clone()),
            });
        }
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

    /// Close the current link from the owner side and queue `Disconnected`.
    fn end_link(&mut self, error: DaemonTransportError) {
        let generation = self.generation;
        if let Some(link) = self.link.take() {
            link.close(Duration::ZERO);
        }
        self.fail_pending(DaemonRequestError::ConnectionClosed);
        self.ready
            .push_back(AppWake::Disconnected { generation, error });
    }

    /// Drop the local completion for one request. A late response for the id
    /// is discarded. Returns whether the request was still pending.
    pub fn cancel(&mut self, request_id: u64) -> bool {
        self.pending.remove(&request_id).is_some()
    }

    /// Number of outstanding host-control requests.
    pub fn outstanding(&self) -> usize {
        self.pending.len()
    }

    /// Send one encoded terminal input frame on the terminal plane.
    ///
    /// Returns false when no connection is installed or the container cannot
    /// carry the route and body.
    pub fn send_terminal(&mut self, route: &str, generation: u64, body: &[u8]) -> bool {
        let Some(link) = self.link.as_ref() else {
            return false;
        };
        let Some(bytes) = encode_unix_terminal_frame(route, generation, body) else {
            return false;
        };
        link.writer.send(WriteCommand::Bytes(bytes)).is_ok()
    }

    /// Forget fault state for a retired route so a re-attach with the same
    /// route id is admitted again.
    pub fn forget_route(&mut self, route: &str) {
        self.budget.forget_route(route);
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

    /// Ids of outstanding requests in submission order. Test observation only.
    #[cfg(test)]
    pub fn pending_request_ids(&self) -> Vec<u64> {
        self.pending.keys().copied().collect()
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

    fn map_message(&mut self, message: IoMessage) -> Option<AppWake> {
        match message {
            IoMessage::Input(event) => Some(AppWake::Input(event)),
            IoMessage::InputEnded => Some(AppWake::Shutdown),
            IoMessage::Terminal { generation, frame } => {
                self.budget.release_terminal(frame.frame.len());
                self.current(generation).then_some(AppWake::Terminal(frame))
            }
            IoMessage::Response {
                generation,
                request_id,
                response,
            } => {
                self.budget.release_control();
                if !self.current(generation) {
                    return None;
                }
                // A response for an unknown, cancelled, or expired id is discarded
                // for that key only.
                self.pending.remove(&request_id)?;
                Some(AppWake::Completed {
                    request_id,
                    result: Ok(*response),
                })
            }
            IoMessage::Event { generation, event } => {
                self.budget.release_control();
                self.current(generation).then_some(AppWake::Event(event))
            }
            IoMessage::Entity {
                subscription_id,
                frame,
            } => {
                self.budget.release_control();
                self.has_entity_stream(&subscription_id)
                    .then_some(AppWake::Entity {
                        subscription_id,
                        frame,
                    })
            }
            IoMessage::EntityOpened {
                subscription_id,
                stream,
                stopped,
            } => {
                match self.entity_links.get(&subscription_id) {
                    Some(EntityLink::Connecting) => {
                        self.entity_links
                            .insert(subscription_id, EntityLink::Open { stream, stopped });
                    }
                    _ => {
                        // Unsubscribed before the stream arrived, or unknown.
                        let _ = stream.shutdown(Shutdown::Both);
                        self.entity_links.remove(&subscription_id);
                    }
                }
                None
            }
            IoMessage::EntitySubscribed {
                subscription_id,
                result,
            } => self
                .has_entity_stream(&subscription_id)
                .then_some(AppWake::EntitySubscribed {
                    subscription_id,
                    result,
                }),
            IoMessage::EntityClosed {
                subscription_id,
                error,
            } => {
                let live = self.has_entity_stream(&subscription_id);
                self.entity_links.remove(&subscription_id);
                live.then_some(AppWake::EntityClosed {
                    subscription_id,
                    error,
                })
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
                    link.close(Duration::ZERO);
                }
                self.fail_pending(DaemonRequestError::ConnectionClosed);
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
    pub fn shutdown(mut self, bound: Duration) {
        let deadline = Instant::now() + bound;
        self.disconnect(deadline.saturating_duration_since(Instant::now()));
        let Some(mut input) = self.input.take() else {
            return;
        };
        input.stop.stop();
        let stopped = input
            .stopped
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .is_ok();
        if stopped && let Some(handle) = input.handle.take() {
            let _ = handle.join();
        }
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

/// Connect, Hello, SubscribeEntities, then read entity frames until the
/// stream ends. The first Response must be EntitySubscribed.
fn run_entity_thread(
    endpoint: &DaemonEndpoint,
    host_requirement: &DaemonCompatibilityRequirement,
    entity_type: &str,
    subscription_id: &str,
    sender: &Sender<IoMessage>,
    budget: &WakeBudget,
) {
    let closed = |error: DaemonTransportError| IoMessage::EntityClosed {
        subscription_id: subscription_id.to_string(),
        error,
    };
    let (mut stream, _ack) =
        match connect_and_hello_with_terminal_requirement(endpoint, host_requirement, None) {
            Ok(connected) => connected,
            Err(error) => {
                let _ = sender.send(closed(error));
                return;
            }
        };
    let owner_stream = match stream.try_clone() {
        Ok(owner_stream) => owner_stream,
        Err(error) => {
            let _ = sender.send(closed(DaemonTransportError::Io(error)));
            return;
        }
    };
    let (stopped_tx, stopped_rx) = mpsc::channel();
    if sender
        .send(IoMessage::EntityOpened {
            subscription_id: subscription_id.to_string(),
            stream: owner_stream,
            stopped: stopped_rx,
        })
        .is_err()
    {
        return;
    }
    let frame = ClientFrame::Request {
        request_id: encode_request_id(1),
        request: DaemonRequest::SubscribeEntities {
            entity_type: entity_type.to_string(),
            subscription_id: subscription_id.to_string(),
        },
    };
    let bytes = match encode_client_frame(&frame) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = sender.send(closed(error));
            let _ = stopped_tx.send(());
            return;
        }
    };
    if let Err(error) = stream.write_all(&bytes) {
        let _ = sender.send(closed(DaemonTransportError::Io(error)));
        let _ = stopped_tx.send(());
        return;
    }
    let mut reader = BufReader::new(stream);
    let mut decoder = DaemonUnixFrameReader::new();
    let mut subscribed = false;
    loop {
        let frame = match decoder.read_frame(&mut reader) {
            Ok(frame) => frame,
            Err(error) => {
                let _ = sender.send(closed(error));
                break;
            }
        };
        let message = match frame {
            DaemonUnixMuxFrame::Server(ServerFrame::Response { response, .. }) if !subscribed => {
                subscribed = true;
                let result = if response.kind == ServerResponseKind::EntitySubscribed
                    && response.error.is_none()
                {
                    Ok(())
                } else {
                    Err(response
                        .error
                        .map(|error| error.message)
                        .unwrap_or_else(|| format!("{:?}", response.kind)))
                };
                let admitted = result.is_ok();
                let _ = sender.send(IoMessage::EntitySubscribed {
                    subscription_id: subscription_id.to_string(),
                    result,
                });
                if !admitted {
                    break;
                }
                continue;
            }
            DaemonUnixMuxFrame::Server(ServerFrame::Entity { entity }) if subscribed => {
                budget.reserve_control();
                IoMessage::Entity {
                    subscription_id: subscription_id.to_string(),
                    frame: entity,
                }
            }
            DaemonUnixMuxFrame::Server(ServerFrame::Close { reason }) => {
                let _ = sender.send(closed(DaemonTransportError::ClosedByHub(reason)));
                break;
            }
            _ => {
                let _ = sender.send(closed(DaemonTransportError::Protocol(
                    "unexpected frame on an entity stream",
                )));
                break;
            }
        };
        if sender.send(message).is_err() {
            break;
        }
    }
    let _ = stopped_tx.send(());
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
            DaemonUnixMuxFrame::Server(ServerFrame::Entity { .. }) => {
                // Entity frames travel only on dedicated entity-stream connections.
                let _ = sender.send(IoMessage::Disconnected {
                    generation,
                    error: DaemonTransportError::Protocol(
                        "entity frame on the host-control connection",
                    ),
                });
                return;
            }
            DaemonUnixMuxFrame::Server(ServerFrame::Close { reason }) => {
                let _ = sender.send(IoMessage::Disconnected {
                    generation,
                    error: DaemonTransportError::ClosedByHub(reason),
                });
                return;
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

/// Decode one Hub terminal container into a routed frame under the wake budget.
///
/// `Ok(None)` means the frame was dropped for a route already reported faulted.
/// `Err(Route)` carries the first fault for a route; `Err(Protocol)` means the
/// connection itself violated the contract.
fn admit_terminal_frame(
    terminal: DaemonUnixTerminalFrame,
    budget: &WakeBudget,
) -> Result<Option<RoutedTerminalFrame>, TerminalAdmitError> {
    let DaemonUnixTerminalFrame {
        route,
        generation,
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
    if !budget.try_reserve_terminal(frame.len()) {
        if budget.mark_faulted(&route, generation) {
            return Err(TerminalAdmitError::Route {
                route: route_id,
                route_generation: generation,
                reason: format!(
                    "client pending wake bound exceeded ({MAX_PENDING_WAKE_ITEMS} items / {MAX_PENDING_WAKE_BYTES} bytes)"
                ),
            });
        }
        return Ok(None);
    }
    Ok(Some(RoutedTerminalFrame {
        route: route_id,
        generation,
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
            body: encode_output(body)
                .expect("encode output")
                .as_bytes()
                .to_vec(),
        }
    }

    #[test]
    fn admit_reserves_budget_and_faults_a_route_once() {
        let budget = WakeBudget::new();
        let admitted = admit_terminal_frame(terminal_container("r", 1, b"hi"), &budget)
            .ok()
            .flatten()
            .expect("first frame is admitted");
        assert_eq!(admitted.route.as_str(), "r");
        assert_eq!(admitted.generation, 1);
        assert_eq!(admitted.frame.body(), b"hi");
        assert_eq!(budget.items.load(Ordering::Acquire), 1);
        for _ in 1..MAX_PENDING_WAKE_ITEMS {
            assert!(budget.try_reserve_terminal(0));
        }
        match admit_terminal_frame(terminal_container("r", 1, b"x"), &budget) {
            Err(TerminalAdmitError::Route {
                route,
                route_generation,
                reason,
            }) => {
                assert_eq!(route.as_str(), "r");
                assert_eq!(route_generation, 1);
                assert!(reason.contains("pending wake bound"));
            }
            other => panic!("expected a route fault, got {:?}", other.is_ok()),
        }
        assert!(matches!(
            admit_terminal_frame(terminal_container("r", 1, b"y"), &budget),
            Ok(None)
        ));
        budget.forget_route("r");
        assert!(!budget.is_faulted("r", 1));
    }

    #[test]
    fn decode_failure_faults_the_route_without_reserving_budget() {
        let budget = WakeBudget::new();
        let malformed = DaemonUnixTerminalFrame {
            route: "r".to_string(),
            generation: 2,
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
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            remote.stop();
        });
        let resolved = future::block_on(future::or(
            future::pending::<Option<io::Result<Event>>>(),
            StopWait(&stop),
        ));
        assert!(resolved.is_none());
        handle.join().expect("stop thread");
    }

    #[test]
    fn unsubscribe_before_the_stream_arrives_marks_the_link_closing() {
        let mut io = HubIo::new();
        io.entity_links
            .insert("e".to_string(), EntityLink::Connecting);
        assert!(io.has_entity_stream("e"));
        io.unsubscribe_entities("e", Duration::ZERO);
        assert!(matches!(
            io.entity_links.get("e"),
            Some(EntityLink::Closing)
        ));
        assert!(!io.has_entity_stream("e"));
        assert!(
            io.map_message(IoMessage::EntitySubscribed {
                subscription_id: "e".to_string(),
                result: Ok(()),
            })
            .is_none()
        );
    }
}
