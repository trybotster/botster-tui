//! Actual-client live tests: the built `botster-tui` binary under a real PTY
//! against an isolated Hub daemon from a recorded candidate set.
//!
//! The tests are `#[ignore]`d so `script/test` needs no external setup; the
//! `script/test-live-tui` selects them with `--ignored --exact`
//! and they fail closed, never skip, when any input is missing:
//!
//! - `BOTSTER_HUB_BIN`, `BOTSTER_SESSION_WORKER_BIN`: dev-profile prebuilt
//!   executables from the candidate set.
//! - `BOTSTER_CANDIDATE_MANIFEST`: the prebuild manifest whose sha256 entries
//!   the isolated Hub verifies before it starts.
//!
//! The Hub producer creates all three inputs with
//! `script/build-dev-artifacts --out-dir <candidate-dir>`. The caller exports
//! the two binary paths and `<candidate-dir>/install-manifest.json` before it
//! runs `script/test-live-tui`.
//!
//! The TUI binary is the one Cargo built for this test run
//! (`CARGO_BIN_EXE_botster-tui`). Every wait has an absolute deadline and a
//! failure prints one line with the agreed fields: layer, step, session_id,
//! subscription_id, generation, stream_epoch, deadline_ms, elapsed_ms,
//! last_pane_titles, cause. The TUI cannot see route generations or stream
//! epochs on its screen; those fields come from the Hub's attach occupancy
//! when known. An unavailable stream epoch is reported as `unavailable`.

use std::{
    collections::VecDeque,
    fmt, fs,
    io::{Read, Write},
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError},
    },
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
use portable_pty::{Child, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system};

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
/// One absolute budget shared by every wait in one exact smoke test.
const TEST_DEADLINE: Duration = Duration::from_secs(120);
const LAST_PANE_TITLES: usize = 16;
const STREAM_EPOCH_UNAVAILABLE: &str = "unavailable";
static SHORT_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    last_pane_titles: Vec<String>,
    cause: String,
}

impl fmt::Display for StepFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "layer={LAYER} step={} session_id={} subscription_id={} generation={} stream_epoch={} deadline_ms={} elapsed_ms={} last_pane_titles={:?} cause={}",
            self.step,
            self.session_id,
            self.subscription_id,
            self.generation,
            self.stream_epoch,
            self.deadline_ms,
            self.elapsed_ms,
            self.last_pane_titles,
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
    /// Fails closed with every missing input named; a live test never skips.
    fn from_env() -> Self {
        let mut missing = Vec::new();
        let mut read = |name: &'static str| {
            let value = std::env::var(name).unwrap_or_default();
            if value.is_empty() {
                missing.push(name);
            }
            value
        };
        let hub_bin = read("BOTSTER_HUB_BIN");
        let worker_bin = read("BOTSTER_SESSION_WORKER_BIN");
        let manifest = read("BOTSTER_CANDIDATE_MANIFEST");
        assert!(
            missing.is_empty(),
            "layer={LAYER} step=candidate_inputs cause=missing environment {missing:?}; the candidate smoke needs the prebuilt Hub, worker, and manifest"
        );
        for (name, path) in [
            ("BOTSTER_HUB_BIN", &hub_bin),
            ("BOTSTER_SESSION_WORKER_BIN", &worker_bin),
            ("BOTSTER_CANDIDATE_MANIFEST", &manifest),
        ] {
            assert!(
                std::path::Path::new(path).is_file(),
                "layer={LAYER} step=candidate_inputs cause={name} is not a file: {path}"
            );
        }
        Self {
            hub_bin,
            worker_bin,
            manifest,
        }
    }
}

/// Owns the isolated Hub for the whole test. Normal completion runs the
/// explicit shutdown and fails the test if it errors; an assertion panic
/// leaves teardown to `IsolatedHub`'s own panicking cleanup.
struct HubGuard {
    hub: Option<IsolatedHub>,
    _short_root: ShortTempRoot,
}

impl HubGuard {
    fn hub(&self) -> &IsolatedHub {
        self.hub
            .as_ref()
            .expect("hub is alive until the guard drops")
    }
}

/// Owns one short absolute root for the Hub's macOS Unix socket.
///
/// Atomic directory creation prevents collisions. Successful tests remove the
/// root. Failed tests preserve it with the Hub's diagnostic state.
struct ShortTempRoot {
    path: PathBuf,
}

impl ShortTempRoot {
    fn create() -> Self {
        loop {
            let sequence = SHORT_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = PathBuf::from(format!("/tmp/btui-{}-{sequence}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            match builder.create(&path) {
                Ok(()) => return Self { path },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!(
                    "layer={LAYER} step=short_temp_root cause=failed to create {}: {error}",
                    path.display()
                ),
            }
        }
    }
}

impl Drop for ShortTempRoot {
    fn drop(&mut self) {
        if thread::panicking() {
            return;
        }
        if let Err(error) = fs::remove_dir_all(&self.path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            panic!(
                "layer={LAYER} step=short_temp_root_cleanup cause=failed to remove {}: {error}",
                self.path.display()
            );
        }
    }
}

impl Drop for HubGuard {
    fn drop(&mut self) {
        if thread::panicking() {
            return;
        }
        if let Some(hub) = self.hub.take()
            && let Err(error) = hub.shutdown()
        {
            panic!("layer={LAYER} step=hub_shutdown cause={error}");
        }
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

    fn wait_exit(&mut self, deadline: Instant) -> Result<ExitStatus, String> {
        loop {
            match self.child.try_wait() {
                Ok(Some(status)) => return Ok(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
                Ok(None) => {
                    return Err("deadline expired while the TUI was still running".to_string());
                }
                Err(error) => return Err(format!("failed to read the TUI exit status: {error}")),
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
    last_pane_titles: VecDeque<String>,
    last_title: String,
    test_deadline: Instant,
}

struct ScreenRow {
    text: String,
    cell_starts: Vec<usize>,
}

impl Screen {
    fn attach(child: &TuiChild, test_deadline: Instant) -> Self {
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
            last_pane_titles: VecDeque::new(),
            last_title: String::new(),
            test_deadline,
        }
    }

    fn remaining(&self, requested: Duration) -> Duration {
        requested.min(self.test_deadline.saturating_duration_since(Instant::now()))
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

    fn screen_rows(&mut self) -> Vec<ScreenRow> {
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
                let mut text = String::new();
                let mut cell_starts = Vec::with_capacity(row.len());
                for cell in row {
                    cell_starts.push(text.len());
                    if cell.grapheme.is_empty() {
                        text.push(' ');
                    } else {
                        text.push_str(&cell.grapheme);
                    }
                }
                ScreenRow { text, cell_starts }
            })
            .collect()
    }

    fn rows(&mut self) -> Vec<String> {
        self.screen_rows().into_iter().map(|row| row.text).collect()
    }

    /// Track terminal pane title changes for failure diagnostics.
    fn note_title(&mut self) {
        let title = self
            .rows()
            .into_iter()
            .find(|row| row.contains("Terminal"))
            .map(|row| row.trim().to_string())
            .unwrap_or_default();
        if !title.is_empty() && title != self.last_title {
            self.last_title = title.clone();
            if self.last_pane_titles.len() == LAST_PANE_TITLES {
                self.last_pane_titles.pop_front();
            }
            self.last_pane_titles.push_back(title);
        }
    }

    /// Zero-based (col, row) of the first cell of `needle` on screen.
    fn locate(&mut self, needle: &str) -> Option<(u16, u16)> {
        self.screen_rows()
            .into_iter()
            .enumerate()
            .find_map(|(row_index, row)| {
                let byte = row.text.find(needle)?;
                let col = row.cell_starts.iter().position(|start| *start == byte)?;
                Some((col as u16, row_index as u16))
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
        let deadline = self.remaining(deadline);
        let until = started + deadline;
        loop {
            if let Some(at) = self.locate(needle) {
                return Ok(at);
            }
            if Instant::now() >= until {
                let tail_rows = self.tail_rows();
                return Err(self.failure(
                    step,
                    identity,
                    deadline,
                    started,
                    format!("{needle:?} not visible; last rows: {tail_rows:?}"),
                ));
            }
            self.pump((Instant::now() + Duration::from_millis(100)).min(until));
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

    fn head_rows(&mut self) -> Vec<String> {
        self.rows()
            .into_iter()
            .map(|row| row.trim_end().to_string())
            .filter(|row| !row.is_empty())
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
            stream_epoch: STREAM_EPOCH_UNAVAILABLE.to_string(),
            deadline_ms: deadline.as_millis(),
            elapsed_ms: started.elapsed().as_millis(),
            last_pane_titles: self.last_pane_titles.iter().cloned().collect(),
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
            stream_epoch: STREAM_EPOCH_UNAVAILABLE.to_string(),
            deadline_ms: 0,
            elapsed_ms: 0,
            last_pane_titles: Vec::new(),
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
fn spawn_session(endpoint: &DaemonEndpoint, identity: &mut Identity, test_deadline: Instant) {
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
    let deadline = SESSION_RUNNING_DEADLINE.min(test_deadline.saturating_duration_since(started));
    let until = started + deadline;
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
                stream_epoch: STREAM_EPOCH_UNAVAILABLE.to_string(),
                deadline_ms: deadline.as_millis(),
                elapsed_ms: started.elapsed().as_millis(),
                last_pane_titles: Vec::new(),
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
        thread::sleep(
            Duration::from_millis(200).min(until.saturating_duration_since(Instant::now())),
        );
    }
}

/// Current Hub-side attach occupancy rows for the session.
fn occupancies(endpoint: &DaemonEndpoint, identity: &Identity) -> Vec<DaemonAttachOccupancy> {
    let response = expect_ok(
        "status",
        identity,
        request(endpoint, DaemonRequest::Status).expect("status transport"),
    );
    response
        .status
        .map(|status| {
            status
                .live_attach_occupancy
                .into_iter()
                .filter(|row| row.session_id == identity.session_id)
                .collect()
        })
        .unwrap_or_default()
}

fn wait_for_occupancy(
    endpoint: &DaemonEndpoint,
    screen: &mut Screen,
    identity: &Identity,
    previous: Option<&DaemonAttachOccupancy>,
) -> DaemonAttachOccupancy {
    let started = Instant::now();
    let deadline = screen.remaining(SCREEN_DEADLINE);
    let until = started + deadline;
    let mut attached_control_seen = false;
    loop {
        let actual = occupancies(endpoint, identity);
        if let Some(occupancy) = actual
            .iter()
            .find(|occupancy| {
                previous.is_none_or(|previous| {
                    occupancy.subscription_id != previous.subscription_id
                        || occupancy.generation != previous.generation
                })
            })
            .cloned()
        {
            return occupancy;
        }
        if Instant::now() >= until {
            let head_rows = screen.head_rows();
            let tail_rows = screen.tail_rows();
            let failure = screen.failure(
                "attach_occupancy",
                identity,
                deadline,
                started,
                format!(
                    "hub did not publish new live attach occupancy; attached_control_seen={attached_control_seen} actual_occupancies={actual:?}; first_rows={head_rows:?}; last_rows={tail_rows:?}"
                ),
            );
            panic!("{failure}");
        }
        let poll_until = (Instant::now() + Duration::from_millis(100)).min(until);
        screen.pump(poll_until);
        attached_control_seen |= screen.contains("Detach");
        thread::sleep(poll_until.saturating_duration_since(Instant::now()));
    }
}

fn start_hub(candidate: &Candidate) -> HubGuard {
    println!(
        "provenance: manifest={} hub_bin={} worker_bin={} tui_bin={} tui_version={}",
        candidate.manifest,
        candidate.hub_bin,
        candidate.worker_bin,
        env!("CARGO_BIN_EXE_botster-tui"),
        env!("CARGO_PKG_VERSION")
    );
    let short_root = ShortTempRoot::create();
    let hub = IsolatedHubBuilder::new()
        .hub_bin(&candidate.hub_bin)
        .session_worker_bin(&candidate.worker_bin)
        .manifest(&candidate.manifest)
        .root(&short_root.path)
        .name("live-tui")
        .start()
        .expect("isolated hub starts from the verified candidate set");
    HubGuard {
        hub: Some(hub),
        _short_root: short_root,
    }
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
    previous_occupancy: Option<&DaemonAttachOccupancy>,
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
    let (ready, occupancy) = if previous_occupancy.is_some() {
        let occupancy = wait_for_occupancy(endpoint, screen, identity, previous_occupancy);
        identity.adopt(&occupancy);
        screen
            .wait_for(
                "reattach_control_visible",
                "Detach",
                SCREEN_DEADLINE,
                identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let ready = screen
            .wait_for(
                "session_ready_visible",
                SESSION_READY_MARKER,
                SCREEN_DEADLINE,
                identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        (ready, occupancy)
    } else {
        // The initial projection is empty, so this marker cannot be retained
        // from an earlier attachment.
        let ready = screen
            .wait_for(
                "session_ready_visible",
                SESSION_READY_MARKER,
                SCREEN_DEADLINE,
                identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let occupancy = wait_for_occupancy(endpoint, screen, identity, previous_occupancy);
        identity.adopt(&occupancy);
        (ready, occupancy)
    };
    // Focus the terminal pane by clicking a cell inside it, then type.
    tui.click(ready.0, ready.1);
    let focus_until = Instant::now() + screen.remaining(Duration::from_millis(200));
    screen.pump(focus_until);
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
    let (col, row) = screen
        .wait_for(
            "detach_control_visible_before_quit",
            "Detach",
            SCREEN_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    tui.click(col, row);
    screen
        .wait_for(
            "detached_visible_before_quit",
            "detached",
            SCREEN_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    // 'q' quits only when the terminal pane is not focused; the detach click
    // moved focus to the toolbar button.
    tui.write_all(b"q");
    let started = Instant::now();
    let deadline = screen.remaining(EXIT_DEADLINE);
    let status = tui.wait_exit(started + deadline).unwrap_or_else(|cause| {
        let failure = screen.failure("tui_exit_after_detach", identity, deadline, started, cause);
        panic!("{failure}");
    });
    if !status.success() {
        let failure = screen.failure(
            "tui_exit_after_detach",
            identity,
            deadline,
            started,
            format!(
                "TUI exited unsuccessfully: exit_code={} signal={:?}",
                status.exit_code(),
                status.signal()
            ),
        );
        panic!("{failure}");
    }
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s1_connect_select_session_and_see_echo() {
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity, test_deadline);
    {
        let mut tui = TuiChild::spawn(&hub);
        let mut screen = Screen::attach(&tui, test_deadline);
        let session_row = format!("{} · running", identity.session_id);
        screen
            .wait_for(
                "session_row_visible",
                &session_row,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let occupancy = attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut identity,
            &session_row,
            MARKER_ONE,
            None,
        );
        println!(
            "t_s1: attached session={} subscription={} generation={} echo visible",
            occupancy.session_id, occupancy.subscription_id, occupancy.generation
        );
        detach_and_quit(&mut tui, &mut screen, &identity);
    }
    drop(guard);
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s2_detach_and_reattach_keeps_echo_visible_with_a_new_generation() {
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity, test_deadline);
    {
        let mut tui = TuiChild::spawn(&hub);
        let mut screen = Screen::attach(&tui, test_deadline);
        let session_row = format!("{} · running", identity.session_id);
        screen
            .wait_for(
                "session_row_visible",
                &session_row,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let first = attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut identity,
            &session_row,
            MARKER_ONE,
            None,
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
        let detach_deadline = screen.remaining(SCREEN_DEADLINE);
        let detach_until = detach_started + detach_deadline;
        while occupancies(hub.endpoint(), &identity)
            .iter()
            .any(|occupancy| {
                occupancy.subscription_id == first.subscription_id
                    && occupancy.generation == first.generation
            })
        {
            if Instant::now() >= detach_until {
                let failure = screen.failure(
                    "detach_occupancy_released",
                    &identity,
                    detach_deadline,
                    detach_started,
                    "hub still reports the attachment after detach".to_string(),
                );
                panic!("{failure}");
            }
            thread::sleep(
                Duration::from_millis(100)
                    .min(detach_until.saturating_duration_since(Instant::now())),
            );
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
            Some(&first),
        );
        assert!(
            screen.contains(&format!("echo:{MARKER_ONE}")),
            "first echo must survive re-attach through the restored snapshot"
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
    drop(guard);
}
