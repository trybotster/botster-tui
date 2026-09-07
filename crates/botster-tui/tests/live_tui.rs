//! Actual-client live tests: the built `botster-tui` binary under a real PTY
//! against an isolated Hub daemon from a recorded candidate set.
//!
//! Inputs (all required; the tests fail closed when any is missing while the
//! candidate manifest is set, and report a skip when the manifest is unset):
//!
//! - `BOTSTER_HUB_BIN`, `BOTSTER_SESSION_WORKER_BIN`: dev-profile prebuilt
//!   executables from the candidate set.
//! - `BOTSTER_CANDIDATE_MANIFEST`: the prebuild manifest whose sha256 entries
//!   the isolated Hub verifies before it starts.
//!
//! The TUI binary is the one Cargo built for this test run
//! (`CARGO_BIN_EXE_botster-tui`). Every wait has an absolute deadline and a
//! failure prints one line with the agreed fields: layer, step, session_id,
//! subscription_id, generation, stream_epoch, deadline_ms, elapsed_ms,
//! last_kinds, cause. The TUI cannot see route generations or stream epochs
//! on its screen; those fields come from the Hub's attach occupancy when
//! known and stay empty otherwise.

use std::{
    collections::VecDeque,
    fmt,
    io::{Read, Write},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use botster_core::contract::terminal_screen::TerminalScreenSize;
use botster_hub_client::{
    DaemonAttachOccupancy, DaemonEndpoint, DaemonRequest, DaemonResponse, DaemonResponseKind,
    request,
};
use botster_hub_test_support::{IsolatedHub, IsolatedHubBuilder};
use botster_terminal_ghostty::GhosttyClientProjection;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

const LAYER: &str = "tui";
const SCREEN_ROWS: u16 = 40;
const SCREEN_COLS: u16 = 140;
const SESSION_READY_MARKER: &str = "live-ready";
const SHELL_COMMAND: &str =
    "printf 'live-ready\\n'; while IFS= read -r line; do printf 'echo:%s\\n' \"$line\"; done";
const MARKER_ONE: &str = "tui-live-marker-one";
const MARKER_TWO: &str = "tui-live-marker-two";
const SESSION_RUNNING_DEADLINE: Duration = Duration::from_secs(20);
const SCREEN_DEADLINE: Duration = Duration::from_secs(20);
const EXIT_DEADLINE: Duration = Duration::from_secs(10);
const LAST_KINDS: usize = 16;

/// One bounded-failure record printed on one line.
#[derive(Debug, Clone)]
struct StepFailure {
    step: &'static str,
    session_id: String,
    subscription_id: String,
    generation: String,
    stream_epoch: String,
    deadline_ms: u128,
    elapsed_ms: u128,
    last_kinds: Vec<String>,
    cause: String,
}

impl fmt::Display for StepFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "layer={LAYER} step={} session_id={} subscription_id={} generation={} stream_epoch={} deadline_ms={} elapsed_ms={} last_kinds={:?} cause={}",
            self.step,
            self.session_id,
            self.subscription_id,
            self.generation,
            self.stream_epoch,
            self.deadline_ms,
            self.elapsed_ms,
            self.last_kinds,
            self.cause
        )
    }
}

/// Candidate-set inputs for one run.
struct Candidate {
    hub_bin: String,
    worker_bin: String,
    manifest: String,
}

impl Candidate {
    /// `None` when no candidate manifest is configured (the run reports a
    /// skip); a set manifest with a missing binary path fails closed.
    fn from_env() -> Option<Self> {
        let manifest = std::env::var("BOTSTER_CANDIDATE_MANIFEST").ok()?;
        let hub_bin = std::env::var("BOTSTER_HUB_BIN")
            .expect("BOTSTER_HUB_BIN is required with BOTSTER_CANDIDATE_MANIFEST");
        let worker_bin = std::env::var("BOTSTER_SESSION_WORKER_BIN")
            .expect("BOTSTER_SESSION_WORKER_BIN is required with BOTSTER_CANDIDATE_MANIFEST");
        Some(Self {
            hub_bin,
            worker_bin,
            manifest,
        })
    }
}

/// The TUI child under its PTY. Dropped before the isolated Hub: kill and
/// reap the child so no test-owned process outlives the run.
struct TuiChild {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

impl TuiChild {
    fn spawn(hub: &IsolatedHub) -> Self {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: SCREEN_ROWS,
                cols: SCREEN_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pty for the tui");
        let socket = hub.endpoint().socket_path.display().to_string();
        let connection = format!(
            "{{\"transport\":{{\"type\":\"unix_socket\",\"path\":{}}}}}",
            serde_json::to_string(&socket).expect("socket path json")
        );
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_botster-tui"));
        command.env("BOTSTER_HUB_CONNECTION", connection);
        command.env("BOTSTER_HUB_DATA_DIR", hub.data_dir().display().to_string());
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.cwd(hub.working_directory().display().to_string());
        let child = pair
            .slave
            .spawn_command(command)
            .expect("spawn botster-tui under the pty");
        drop(pair.slave);
        let writer = pair.master.take_writer().expect("pty writer");
        Self {
            child,
            master: pair.master,
            writer,
        }
    }

    fn reader(&self) -> Box<dyn Read + Send> {
        self.master.try_clone_reader().expect("pty reader")
    }

    fn write_all(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).expect("write to the tui pty");
        self.writer.flush().expect("flush the tui pty");
    }

    /// SGR mouse press and release at a zero-based screen cell.
    fn click(&mut self, col: u16, row: u16) {
        let press = format!("\x1b[<0;{};{}M", col + 1, row + 1);
        let release = format!("\x1b[<0;{};{}m", col + 1, row + 1);
        self.write_all(press.as_bytes());
        self.write_all(release.as_bytes());
    }

    fn type_line(&mut self, text: &str) {
        self.write_all(text.as_bytes());
        self.write_all(b"\r");
    }

    fn wait_exit(&mut self, deadline: Instant) -> bool {
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => return true,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                _ => return false,
            }
        }
    }
}

impl Drop for TuiChild {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// The TUI's own rendered screen, decoded from PTY bytes with the Ghostty
/// projection the TUI itself uses.
struct Screen {
    projection: GhosttyClientProjection,
    bytes: Receiver<Vec<u8>>,
    last_kinds: VecDeque<String>,
    last_title: String,
}

impl Screen {
    fn attach(child: &TuiChild) -> Self {
        let mut reader = child.reader();
        let (sender, bytes) = mpsc::channel();
        thread::Builder::new()
            .name("live-tui-pty-reader".to_string())
            .spawn(move || {
                let mut buffer = [0_u8; 16 * 1024];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) | Err(_) => return,
                        Ok(count) => {
                            if sender.send(buffer[..count].to_vec()).is_err() {
                                return;
                            }
                        }
                    }
                }
            })
            .expect("spawn the pty reader");
        let projection =
            GhosttyClientProjection::new(TerminalScreenSize::new(SCREEN_ROWS, SCREEN_COLS))
                .expect("ghostty projection for the tui screen");
        Self {
            projection,
            bytes,
            last_kinds: VecDeque::new(),
            last_title: String::new(),
        }
    }

    /// Apply PTY bytes until `until`; returns true when anything arrived.
    fn pump(&mut self, until: Instant) -> bool {
        let mut received = false;
        loop {
            let timeout = until.saturating_duration_since(Instant::now());
            match self.bytes.recv_timeout(timeout) {
                Ok(chunk) => {
                    self.projection.apply_terminal_output(&chunk);
                    received = true;
                    if timeout.is_zero() {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        if received {
            self.note_title();
        }
        received
    }

    fn rows(&mut self) -> Vec<String> {
        let viewport = match self.projection.project_viewport() {
            Ok(viewport) => viewport,
            Err(_) => return Vec::new(),
        };
        let cols = viewport.cols as usize;
        if cols == 0 {
            return Vec::new();
        }
        viewport
            .cells
            .chunks(cols)
            .map(|row| {
                row.iter()
                    .map(|cell| {
                        if cell.grapheme.is_empty() {
                            " "
                        } else {
                            cell.grapheme.as_str()
                        }
                    })
                    .collect::<String>()
            })
            .collect()
    }

    /// Track the terminal pane title as the TUI-visible "kind" trail.
    fn note_title(&mut self) {
        let title = self
            .rows()
            .into_iter()
            .find(|row| row.contains("Terminal"))
            .map(|row| row.trim().to_string())
            .unwrap_or_default();
        if !title.is_empty() && title != self.last_title {
            self.last_title = title.clone();
            if self.last_kinds.len() == LAST_KINDS {
                self.last_kinds.pop_front();
            }
            self.last_kinds.push_back(title);
        }
    }

    /// Zero-based (col, row) of the first cell of `needle` on screen.
    fn locate(&mut self, needle: &str) -> Option<(u16, u16)> {
        self.rows().iter().enumerate().find_map(|(row, text)| {
            text.find(needle)
                .map(|byte| (text[..byte].chars().count() as u16, row as u16))
        })
    }

    fn contains(&mut self, needle: &str) -> bool {
        self.rows().iter().any(|row| row.contains(needle))
    }

    /// Wait until `needle` is visible. Bounded; failure carries the record.
    fn wait_for(
        &mut self,
        step: &'static str,
        needle: &str,
        deadline: Duration,
        identity: &Identity,
    ) -> Result<(u16, u16), StepFailure> {
        let started = Instant::now();
        let until = started + deadline;
        loop {
            if let Some(at) = self.locate(needle) {
                return Ok(at);
            }
            if Instant::now() >= until {
                return Err(self.failure(
                    step,
                    identity,
                    deadline,
                    started,
                    format!("{needle:?} not visible; last rows: {:?}", self.tail_rows()),
                ));
            }
            self.pump(Instant::now() + Duration::from_millis(100));
        }
    }

    fn tail_rows(&mut self) -> Vec<String> {
        self.rows()
            .into_iter()
            .map(|row| row.trim_end().to_string())
            .filter(|row| !row.is_empty())
            .rev()
            .take(6)
            .collect()
    }

    fn failure(
        &mut self,
        step: &'static str,
        identity: &Identity,
        deadline: Duration,
        started: Instant,
        cause: String,
    ) -> StepFailure {
        StepFailure {
            step,
            session_id: identity.session_id.clone(),
            subscription_id: identity.subscription_id.clone(),
            generation: identity.generation.clone(),
            stream_epoch: String::new(),
            deadline_ms: deadline.as_millis(),
            elapsed_ms: started.elapsed().as_millis(),
            last_kinds: self.last_kinds.iter().cloned().collect(),
            cause,
        }
    }
}

/// What the test knows about the session and its current attachment.
#[derive(Debug, Clone, Default)]
struct Identity {
    session_id: String,
    subscription_id: String,
    generation: String,
}

impl Identity {
    fn adopt(&mut self, occupancy: &DaemonAttachOccupancy) {
        self.subscription_id = occupancy.subscription_id.clone();
        self.generation = occupancy.generation.to_string();
    }
}

fn expect_ok(step: &'static str, identity: &Identity, response: DaemonResponse) -> DaemonResponse {
    if let Some(error) = &response.error {
        let failure = StepFailure {
            step,
            session_id: identity.session_id.clone(),
            subscription_id: identity.subscription_id.clone(),
            generation: identity.generation.clone(),
            stream_epoch: String::new(),
            deadline_ms: 0,
            elapsed_ms: 0,
            last_kinds: Vec::new(),
            cause: format!(
                "hub rejected {}: {} (code={} operation={})",
                step, error.message, error.code, error.operation
            ),
        };
        panic!("{failure}");
    }
    response
}

/// Spawn the echo shell session through the Hub before the TUI starts.
fn spawn_session(endpoint: &DaemonEndpoint, identity: &mut Identity) {
    identity.session_id = format!(
        "live-tui-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default()
    );
    let response = request(
        endpoint,
        DaemonRequest::Spawn {
            session_id: identity.session_id.clone(),
            command: SHELL_COMMAND.to_string(),
        },
    )
    .expect("spawn request transport");
    expect_ok("spawn", identity, response);
    let started = Instant::now();
    let until = started + SESSION_RUNNING_DEADLINE;
    loop {
        let response = expect_ok(
            "list_sessions",
            identity,
            request(endpoint, DaemonRequest::ListSessions).expect("list sessions transport"),
        );
        assert_eq!(response.kind, DaemonResponseKind::Sessions);
        let running = response.sessions.iter().any(|session| {
            session.session_id == identity.session_id && session.lifecycle == "running"
        });
        if running {
            return;
        }
        if Instant::now() >= until {
            let failure = StepFailure {
                step: "session_running",
                session_id: identity.session_id.clone(),
                subscription_id: String::new(),
                generation: String::new(),
                stream_epoch: String::new(),
                deadline_ms: SESSION_RUNNING_DEADLINE.as_millis(),
                elapsed_ms: started.elapsed().as_millis(),
                last_kinds: Vec::new(),
                cause: format!(
                    "session never reached running: {:?}",
                    response
                        .sessions
                        .iter()
                        .map(|session| format!("{}={}", session.session_id, session.lifecycle))
                        .collect::<Vec<_>>()
                ),
            };
            panic!("{failure}");
        }
        thread::sleep(Duration::from_millis(200));
    }
}

/// Current Hub-side attach occupancy for the session, if any.
fn occupancy(endpoint: &DaemonEndpoint, identity: &Identity) -> Option<DaemonAttachOccupancy> {
    let response = expect_ok(
        "status",
        identity,
        request(endpoint, DaemonRequest::Status).expect("status transport"),
    );
    response.status.and_then(|status| {
        status
            .live_attach_occupancy
            .into_iter()
            .find(|row| row.session_id == identity.session_id)
    })
}

fn start_hub(candidate: &Candidate) -> IsolatedHub {
    println!(
        "provenance: manifest={} hub_bin={} worker_bin={} tui_bin={} tui_version={}",
        candidate.manifest,
        candidate.hub_bin,
        candidate.worker_bin,
        env!("CARGO_BIN_EXE_botster-tui"),
        env!("CARGO_PKG_VERSION")
    );
    IsolatedHubBuilder::new()
        .hub_bin(&candidate.hub_bin)
        .session_worker_bin(&candidate.worker_bin)
        .manifest(&candidate.manifest)
        .name("live-tui")
        .start()
        .expect("isolated hub starts from the verified candidate set")
}

/// Attach through the session row, focus the pane, type a marker, and wait
/// for its echo. Returns the Hub occupancy for the new attachment.
fn attach_and_echo(
    endpoint: &DaemonEndpoint,
    tui: &mut TuiChild,
    screen: &mut Screen,
    identity: &mut Identity,
    attach_via: &str,
    marker: &str,
) -> DaemonAttachOccupancy {
    let (col, row) = screen
        .wait_for(
            "attach_control_visible",
            attach_via,
            SCREEN_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    tui.click(col, row);
    let ready = screen
        .wait_for(
            "session_ready_visible",
            SESSION_READY_MARKER,
            SCREEN_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    let occupancy = occupancy(endpoint, identity).unwrap_or_else(|| {
        let failure = screen.failure(
            "attach_occupancy",
            identity,
            Duration::ZERO,
            Instant::now(),
            "hub reports no live attach occupancy after the pane rendered".to_string(),
        );
        panic!("{failure}");
    });
    identity.adopt(&occupancy);
    // Focus the terminal pane by clicking a cell inside it, then type.
    tui.click(ready.0, ready.1);
    screen.pump(Instant::now() + Duration::from_millis(200));
    tui.type_line(marker);
    screen
        .wait_for(
            "echo_visible",
            &format!("echo:{marker}"),
            SCREEN_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    occupancy
}

fn detach_and_quit(tui: &mut TuiChild, screen: &mut Screen, identity: &Identity) {
    if let Some((col, row)) = screen.locate("Detach") {
        tui.click(col, row);
        let _ = screen.wait_for("detached_visible", "detached", EXIT_DEADLINE, identity);
    }
    // 'q' quits only when the terminal pane is not focused; the detach click
    // moved focus to the toolbar button.
    tui.write_all(b"q");
    let exited = tui.wait_exit(Instant::now() + EXIT_DEADLINE);
    assert!(
        exited,
        "botster-tui did not exit within {EXIT_DEADLINE:?} after detach and q"
    );
}

#[test]
fn t_s1_connect_select_session_and_see_echo() {
    let Some(candidate) = Candidate::from_env() else {
        println!(
            "skipped: BOTSTER_CANDIDATE_MANIFEST is not set; the live TUI test needs the candidate set"
        );
        return;
    };
    let hub = start_hub(&candidate);
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity);
    {
        let mut tui = TuiChild::spawn(&hub);
        let mut screen = Screen::attach(&tui);
        screen
            .wait_for(
                "session_row_visible",
                &identity.session_id,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let session_id = identity.session_id.clone();
        let occupancy = attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut identity,
            &session_id,
            MARKER_ONE,
        );
        println!(
            "t_s1: attached session={} subscription={} generation={} echo visible",
            occupancy.session_id, occupancy.subscription_id, occupancy.generation
        );
        detach_and_quit(&mut tui, &mut screen, &identity);
    }
    hub.shutdown().expect("isolated hub shuts down cleanly");
}

#[test]
fn t_s2_detach_and_reattach_keeps_echo_visible_with_a_new_generation() {
    let Some(candidate) = Candidate::from_env() else {
        println!(
            "skipped: BOTSTER_CANDIDATE_MANIFEST is not set; the live TUI test needs the candidate set"
        );
        return;
    };
    let hub = start_hub(&candidate);
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity);
    {
        let mut tui = TuiChild::spawn(&hub);
        let mut screen = Screen::attach(&tui);
        screen
            .wait_for(
                "session_row_visible",
                &identity.session_id,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let session_id = identity.session_id.clone();
        let first = attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut identity,
            &session_id,
            MARKER_ONE,
        );

        // Detach: the pane title reports detached and the Hub drops the occupancy.
        let (col, row) = screen
            .wait_for(
                "detach_control_visible",
                "Detach",
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        tui.click(col, row);
        screen
            .wait_for("detached_visible", "detached", SCREEN_DEADLINE, &identity)
            .unwrap_or_else(|failure| panic!("{failure}"));
        let detach_started = Instant::now();
        while occupancy(hub.endpoint(), &identity).is_some() {
            if detach_started.elapsed() >= SCREEN_DEADLINE {
                let failure = screen.failure(
                    "detach_occupancy_released",
                    &identity,
                    SCREEN_DEADLINE,
                    detach_started,
                    "hub still reports the attachment after detach".to_string(),
                );
                panic!("{failure}");
            }
            thread::sleep(Duration::from_millis(100));
        }

        // Re-attach through the toolbar: the restored snapshot shows the first
        // echo, live input still works, and the Hub minted a new attachment.
        let second = attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut identity,
            "Attach",
            MARKER_TWO,
        );
        assert!(
            screen.contains(&format!("echo:{MARKER_ONE}")),
            "first echo must survive re-attach through the restored snapshot"
        );
        assert_ne!(
            second.subscription_id, first.subscription_id,
            "re-attach must mint a new route"
        );
        assert_ne!(
            second.generation, first.generation,
            "re-attach must carry a new attachment generation"
        );
        println!(
            "t_s2: reattached session={} first=({}, {}) second=({}, {})",
            first.session_id,
            first.subscription_id,
            first.generation,
            second.subscription_id,
            second.generation
        );
        detach_and_quit(&mut tui, &mut screen, &identity);
    }
    hub.shutdown().expect("isolated hub shuts down cleanly");
}
