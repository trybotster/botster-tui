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
    path::{Path, PathBuf},
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
use portable_pty::{
    ChildKiller, CommandBuilder, ExitStatus, MasterPty, PtySize, native_pty_system,
};

const LAYER: &str = "tui";
const SCREEN_ROWS: u16 = 40;
const SCREEN_COLS: u16 = 140;
const SESSION_READY_MARKER: &str = "live-ready";
const SHELL_COMMAND: &str =
    "printf 'live-ready\\n'; while IFS= read -r line; do printf 'echo:%s\\n' \"$line\"; done";
/// Reports its PTY size on every SIGWINCH (`winch:<rows> <cols>`), so a resize
/// is observable without any further input, and on request (`size:r:c`).
const RESIZE_SHELL_COMMAND: &str = "trap 'printf \"winch:%s\\n\" \"$(stty size)\"' WINCH; printf 'live-ready\\n'; while IFS= read -r line; do if [ \"$line\" = tui-report-size ]; then set -- $(stty size); printf 'size:%s:%s\\n' \"$1\" \"$2\"; else printf 'echo:%s\\n' \"$line\"; fi; done";
const LAUNCH_TARGET_ID: &str = "tui-launch";
const LAUNCH_TARGET_LABEL: &str = "TUI launch";
const LAUNCH_SESSION_TYPE: &str = "shell";
const LAUNCH_SESSION_TYPE_LABEL: &str = "TUI launch shell";
const MARKER_ONE: &str = "tui-live-marker-one";
const MARKER_TWO: &str = "tui-live-marker-two";
const MARKER_THREE: &str = "tui-live-marker-three";
const CONSENT_NORMAL_MARKER: &str = "tui-consent-normal-input";
const CONSENT_OUTPUT_MARKER: &str = "tui-consent-output-while-dialog-open";
const CONSENT_FIRST_ACCEPTED: &str = "tui-consent-first-line-accepted";
const CONSENT_SECOND_ACCEPTED: &str = "tui-consent-second-line-accepted";
const RAW_CONTROL_TOKEN: &str = "TUI_RAW_CONTROL_PAYLOAD";
const RAW_SECOND_TOKEN: &str = "TUI_PRIVATE_SECOND_LINE";
const UNSAFE_PASTE: &str = "\x1b[31mTUI_RAW_CONTROL_PAYLOAD\x1b[0m\nTUI_PRIVATE_SECOND_LINE\n";
const SESSION_RUNNING_DEADLINE: Duration = Duration::from_secs(20);
const SCREEN_DEADLINE: Duration = Duration::from_secs(20);
const CONTROL_RECONNECT_DEADLINE: Duration = Duration::from_secs(30);
const EXIT_DEADLINE: Duration = Duration::from_secs(10);
/// One absolute budget shared by every wait in one exact smoke test.
const TEST_DEADLINE: Duration = Duration::from_secs(120);
const LAST_PANE_TITLES: usize = 16;
const STREAM_EPOCH_UNAVAILABLE: &str = "unavailable";
static SHORT_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static SESSION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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

    fn restart(&mut self) {
        let hub = self.hub.take().expect("hub is alive before restart");
        self.hub = Some(
            hub.restart()
                .expect("isolated hub restarts at its existing endpoint"),
        );
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
    killer: Box<dyn ChildKiller + Send + Sync>,
    /// One message: the child's exit status, sent by a thread blocked in `wait`.
    exited: Receiver<std::io::Result<ExitStatus>>,
    exit: Option<ExitStatus>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

impl TuiChild {
    fn spawn(hub: &IsolatedHub) -> Self {
        Self::spawn_at(hub.endpoint(), hub.data_dir(), hub.working_directory())
    }

    fn spawn_at(endpoint: &DaemonEndpoint, data_dir: &Path, working_directory: &Path) -> Self {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: SCREEN_ROWS,
                cols: SCREEN_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("open a pty for the tui");
        let socket = endpoint.socket_path.display().to_string();
        let connection = format!(
            "{{\"transport\":{{\"type\":\"unix_socket\",\"path\":{}}}}}",
            serde_json::to_string(&socket).expect("socket path json")
        );
        let mut command = CommandBuilder::new(env!("CARGO_BIN_EXE_botster-tui"));
        command.env("BOTSTER_HUB_CONNECTION", connection);
        command.env("BOTSTER_HUB_DATA_DIR", data_dir.display().to_string());
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.cwd(working_directory.display().to_string());
        let mut child = pair
            .slave
            .spawn_command(command)
            .expect("spawn botster-tui under the pty");
        drop(pair.slave);
        let killer = child.clone_killer();
        let (exit_tx, exited) = mpsc::channel();
        thread::spawn(move || {
            let _ = exit_tx.send(child.wait());
        });
        let writer = pair.master.take_writer().expect("pty writer");
        Self {
            killer,
            exited,
            exit: None,
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

    /// Send one real Crossterm bracketed-paste event through the outer PTY.
    fn paste(&mut self, text: &str) {
        self.write_all(b"\x1b[200~");
        self.write_all(text.as_bytes());
        self.write_all(b"\x1b[201~");
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("resize the tui pty");
    }

    fn wait_exit(&mut self, deadline: Instant) -> Result<ExitStatus, String> {
        if let Some(status) = &self.exit {
            return Ok(status.clone());
        }
        match self
            .exited
            // timer: deadline — the TUI exits within the step budget; expiry fails the step
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            Ok(Ok(status)) => {
                self.exit = Some(status.clone());
                Ok(status)
            }
            Ok(Err(error)) => Err(format!("failed to read the TUI exit status: {error}")),
            Err(RecvTimeoutError::Timeout) => {
                Err("deadline expired while the TUI was still running".to_string())
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err("the TUI exit waiter stopped without a status".to_string())
            }
        }
    }
}

impl Drop for TuiChild {
    fn drop(&mut self) {
        if self.exit.is_none() {
            let _ = self.killer.kill();
            // timer: deadline — the killed TUI is reaped within the exit budget; expiry leaves the waiter thread
            let _ = self.exited.recv_timeout(EXIT_DEADLINE);
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

    /// Block for the next TUI output until `until`, then apply everything
    /// already queued. Returns false when the deadline passed (or the TUI
    /// closed its output) with nothing new.
    fn pump_next(&mut self, until: Instant) -> bool {
        let now = Instant::now();
        if now >= until {
            return false;
        }
        // timer: deadline — bounded wait for TUI output; expiry fails the waiting step
        let Ok(first) = self.bytes.recv_timeout(until - now) else {
            return false;
        };
        self.projection.apply_terminal_output(&first);
        // Drain what is queued, but never past the deadline.
        while Instant::now() < until
            && let Ok(chunk) = self.bytes.try_recv()
        {
            self.projection.apply_terminal_output(&chunk);
        }
        self.note_title();
        true
    }

    /// The single bounded wait of these tests: re-check `ready` after each
    /// batch of TUI output until it yields a value or the deadline passes.
    fn wait_until<T>(
        &mut self,
        step: &'static str,
        deadline: Duration,
        identity: &Identity,
        mut ready: impl FnMut(&mut Self) -> Option<T>,
        cause: impl FnOnce(&mut Self) -> String,
    ) -> Result<T, Box<StepFailure>> {
        let started = Instant::now();
        let deadline = self.remaining(deadline);
        let until = started + deadline;
        loop {
            if let Some(value) = ready(self) {
                return Ok(value);
            }
            if !self.pump_next(until) {
                if let Some(value) = ready(self) {
                    return Ok(value);
                }
                let cause = cause(self);
                return Err(Box::new(
                    self.failure(step, identity, deadline, started, cause),
                ));
            }
        }
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

    /// Inner (rows, cols) of the drawn Terminal pane, measured from its border.
    fn terminal_pane_inner_size(&mut self) -> Option<(u16, u16)> {
        let grid: Vec<Vec<char>> = self
            .rows()
            .iter()
            .map(|row| row.chars().collect())
            .collect();
        let (top, left) = grid.iter().enumerate().find_map(|(y, row)| {
            let text: String = row.iter().collect();
            let byte = text.find("┌Terminal")?;
            Some((y, text[..byte].chars().count()))
        })?;
        let right = left
            + 1
            + grid[top]
                .iter()
                .skip(left + 1)
                .position(|cell| *cell == '┐')?;
        let bottom = top
            + 1
            + grid
                .iter()
                .skip(top + 1)
                .position(|row| row.get(left) == Some(&'└'))?;
        Some(((bottom - top - 1) as u16, (right - left - 1) as u16))
    }

    /// Wait until the drawn Terminal pane is measurable and differs from `previous`.
    fn wait_for_pane(
        &mut self,
        step: &'static str,
        previous: Option<(u16, u16)>,
        identity: &Identity,
    ) -> (u16, u16) {
        self.wait_until(
            step,
            SCREEN_DEADLINE,
            identity,
            |screen| {
                screen
                    .terminal_pane_inner_size()
                    .filter(|size| Some(*size) != previous)
            },
            |_| format!("terminal pane not measurable or unchanged from {previous:?}"),
        )
        .unwrap_or_else(|failure| panic!("{failure}"))
    }

    fn contains(&mut self, needle: &str) -> bool {
        self.rows().iter().any(|row| row.contains(needle))
    }

    fn resize(&mut self, rows: u16, cols: u16) {
        self.projection
            .resize(TerminalScreenSize::new(rows, cols))
            .expect("resize the tui screen projection");
    }

    /// Wait until `needle` is visible. Bounded; failure carries the record.
    fn wait_for(
        &mut self,
        step: &'static str,
        needle: &str,
        deadline: Duration,
        identity: &Identity,
    ) -> Result<(u16, u16), Box<StepFailure>> {
        self.wait_until(
            step,
            deadline,
            identity,
            |screen| screen.locate(needle),
            |screen| {
                format!(
                    "{needle:?} not visible; last rows: {:?}",
                    screen.tail_rows()
                )
            },
        )
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

    /// Wait until one screen row contains every needle.
    fn wait_for_row_with_all(
        &mut self,
        step: &'static str,
        needles: &[&str],
        deadline: Duration,
        identity: &Identity,
    ) {
        self.wait_until(
            step,
            deadline,
            identity,
            |screen| {
                screen
                    .rows()
                    .iter()
                    .any(|row| needles.iter().all(|needle| row.contains(needle)))
                    .then_some(())
            },
            |screen| {
                format!(
                    "no row contains all of {needles:?}; first rows: {:?}",
                    screen.head_rows()
                )
            },
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
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

/// Spawn one shell session through the Hub.
///
/// Callers wait for its "running" row on the TUI screen, which the TUI renders
/// from the Hub's session entity events, so this does not poll the Hub.
fn spawn_session(endpoint: &DaemonEndpoint, identity: &mut Identity, command: &str) {
    identity.session_id = format!(
        "live-tui-{}-{}",
        std::process::id(),
        SESSION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let response = request(
        endpoint,
        DaemonRequest::Spawn {
            session_id: identity.session_id.clone(),
            command: command.to_string(),
        },
    )
    .expect("spawn request transport");
    expect_ok("spawn", identity, response);
}

/// Build the consent test shell with one test-owned output trigger.
///
/// The shell emits neutral acceptance markers. It does not write the raw paste
/// payload to the terminal. The client screen can therefore check for leaks.
fn consent_shell_command(output_trigger: &Path, output_written: &Path) -> String {
    let trigger = output_trigger.display().to_string();
    let written = output_written.display().to_string();
    for path in [&trigger, &written] {
        assert!(
            path.bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"/_-.".contains(&byte)),
            "consent FIFO path must be safe for the fixed test shell: {path}"
        );
    }
    // The background reader blocks on the trigger FIFO, prints the marker,
    // then reports on the second FIFO that the marker was written.
    format!(
        "stty -echo; \
         (read _ < '{trigger}'; printf '{CONSENT_OUTPUT_MARKER}\\n'; echo written > '{written}') & \
         printf '{SESSION_READY_MARKER}\\n'; \
         while IFS= read -r line; do \
           case \"$line\" in \
             *{RAW_CONTROL_TOKEN}*) printf '{CONSENT_FIRST_ACCEPTED}\\n' ;; \
             {RAW_SECOND_TOKEN}) printf '{CONSENT_SECOND_ACCEPTED}\\n' ;; \
             *) printf 'echo:%s\\n' \"$line\" ;; \
           esac; \
         done"
    )
}

/// Create one FIFO for a test shell to block on or report through.
fn make_fifo(path: &Path) {
    let status = std::process::Command::new("mkfifo")
        .arg(path)
        .status()
        .expect("run mkfifo");
    assert!(
        status.success(),
        "mkfifo {} failed: {status:?}",
        path.display()
    );
}

/// Write one release into a FIFO the test shell blocks on, bounded by
/// `deadline`: opening a FIFO for writing blocks until a reader opens it.
fn write_fifo(fifo: &Path, bytes: &'static [u8], deadline: Duration) -> Result<(), String> {
    let (done_tx, done_rx) = mpsc::channel();
    let fifo = fifo.to_path_buf();
    thread::spawn(move || {
        let _ = done_tx.send(fs::write(&fifo, bytes));
    });
    // timer: deadline — the shell's reader opens the FIFO within the step budget; expiry fails the step
    match done_rx.recv_timeout(deadline) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(format!("fifo write failed: {error}")),
        Err(_) => Err("no reader opened the FIFO before the deadline".to_string()),
    }
}

/// Read one report that a test shell writes into `fifo`.
fn wait_for_fifo_report(fifo: &Path, deadline: Duration) -> Result<String, String> {
    let (report_tx, report_rx) = mpsc::channel();
    let fifo = fifo.to_path_buf();
    thread::spawn(move || {
        let _ = report_tx.send(fs::read_to_string(&fifo));
    });
    // timer: deadline — the shell reports within the step budget; expiry fails the step
    match report_rx.recv_timeout(deadline) {
        Ok(Ok(report)) => Ok(report),
        Ok(Err(error)) => Err(format!("fifo read failed: {error}")),
        Err(_) => Err("the shell did not report before the deadline".to_string()),
    }
}

fn read_session_screen(endpoint: &DaemonEndpoint, identity: &Identity) -> String {
    let response = expect_ok(
        "read_screen",
        identity,
        request(
            endpoint,
            DaemonRequest::ReadScreen {
                session_id: identity.session_id.clone(),
            },
        )
        .expect("read screen transport"),
    );
    assert_eq!(response.kind, DaemonResponseKind::ReadScreen);
    let readback = response
        .read_screen
        .expect("read screen response includes its payload");
    assert_eq!(readback.session_id, identity.session_id);
    assert!(
        readback.unavailable.is_none(),
        "read screen is available during the consent test: {:?}",
        readback.unavailable
    );
    readback.text
}

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

/// The Hub occupancy of an attachment known to be complete.
///
/// Call only after output typed on that attachment is visible: typed input
/// reaches a session only on a live attachment, and the Hub records the route
/// before it answers the Attach, so one read is enough.
fn attached_occupancy(
    endpoint: &DaemonEndpoint,
    identity: &Identity,
    previous: Option<&DaemonAttachOccupancy>,
) -> DaemonAttachOccupancy {
    let rows = occupancies(endpoint, identity);
    rows.iter()
        .find(|occupancy| {
            previous.is_none_or(|previous| {
                occupancy.subscription_id != previous.subscription_id
                    || occupancy.generation != previous.generation
            })
        })
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "layer={LAYER} step=attach_occupancy session_id={} cause=no new live attach occupancy after a completed attach: {rows:?}",
                identity.session_id
            )
        })
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
    if previous_occupancy.is_some() {
        // The detached projection stays readable, so wait until the new
        // attachment owns the pane before clicking into it.
        screen
            .wait_for(
                "reattach_control_visible",
                "[ Detach ]",
                SCREEN_DEADLINE,
                identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
    }
    let ready = screen
        .wait_for(
            "session_ready_visible",
            SESSION_READY_MARKER,
            SCREEN_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    // Focus the pane, then type. The TUI reads its input in order, so the
    // click is applied before the keys.
    tui.click(ready.0, ready.1);
    tui.type_line(marker);
    screen
        .wait_for(
            "echo_visible",
            &format!("echo:{marker}"),
            SCREEN_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| {
            // An echo on the Hub's screen proves the input reached the session
            // and the loss is on the output path to this client. No echo
            // there does not by itself prove an input-route failure.
            let hub_screen = read_session_screen(endpoint, identity);
            let reached = hub_screen.contains(&format!("echo:{marker}"));
            panic!("{failure} hub_screen_has_echo={reached} hub_screen={hub_screen:?}")
        });
    let occupancy = attached_occupancy(endpoint, identity, previous_occupancy);
    identity.adopt(&occupancy);
    occupancy
}

fn detach_and_quit(tui: &mut TuiChild, screen: &mut Screen, identity: &Identity) {
    let (col, row) = screen
        .wait_for(
            "detach_control_visible_before_quit",
            "[ Detach ]",
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
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity, SHELL_COMMAND);
    {
        let mut tui = TuiChild::spawn(hub);
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
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity, SHELL_COMMAND);
    {
        let mut tui = TuiChild::spawn(hub);
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
                "[ Detach ]",
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        tui.click(col, row);
        screen
            .wait_for(
                "detach_confirmed",
                &format!("{} · detached", identity.session_id),
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        // The pane shows "detached" only after the Hub's successful Detach
        // response, which the Hub sends after retiring the route and Core has
        // detached that generation: the release barrier for a fresh Status.
        let after_detach = occupancies(hub.endpoint(), &identity);
        assert!(
            !after_detach.iter().any(|occupancy| {
                occupancy.subscription_id == first.subscription_id
                    && occupancy.generation == first.generation
            }),
            "the confirmed Detach must release the first attachment: {after_detach:?}"
        );
        // Re-attach through the toolbar: the restored snapshot shows the first
        // echo, live input still works, and the Hub minted a new attachment.
        let second = attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut identity,
            "[ Attach ]",
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

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s3_unsafe_paste_requires_explicit_consent_and_retries_once() {
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let output_trigger = hub.data_dir().join("consent-output.trigger");
    let output_written = hub.data_dir().join("consent-output.written");
    make_fifo(&output_trigger);
    make_fifo(&output_written);
    let command = consent_shell_command(&output_trigger, &output_written);
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity, &command);
    {
        let mut tui = TuiChild::spawn(hub);
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

        // The first attempt opens the dialog. The first Enter arms the primary
        // control. The next Enter activates Cancel.
        tui.paste(UNSAFE_PASTE);
        screen
            .wait_for(
                "unsafe_paste_dialog_visible",
                "Unsafe paste blocked",
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        assert!(screen.contains("Review paste"));
        assert!(!screen.contains("Paste anyway"));
        assert!(!screen.contains(RAW_CONTROL_TOKEN));
        assert!(!screen.contains(RAW_SECOND_TOKEN));

        // The session writes output while the dialog is open; the report
        // proves the write happened before the dialog was dismissed. The pane
        // shows it after cancel (screen-applied evidence below).
        write_fifo(
            &output_trigger,
            b"emit\n",
            screen.remaining(SCREEN_DEADLINE),
        )
        .unwrap_or_else(|cause| panic!("layer={LAYER} step=release_dialog_output cause={cause}"));
        wait_for_fifo_report(&output_written, screen.remaining(SCREEN_DEADLINE)).unwrap_or_else(
            |cause| panic!("layer={LAYER} step=dialog_output_written cause={cause}"),
        );
        assert!(screen.contains("Unsafe paste blocked"));

        tui.write_all(b"\r");
        screen
            .wait_for(
                "unsafe_paste_armed",
                "Paste anyway",
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        tui.write_all(b"\r");
        let terminal = screen
            .wait_for(
                "unsafe_paste_cancelled",
                SESSION_READY_MARKER,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        screen
            .wait_for(
                "dialog_output_visible_after_cancel",
                CONSENT_OUTPUT_MARKER,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));

        // Cancellation restores terminal input. The second unsafe paste opens
        // Review. Tab selects Paste anyway. Enter sends a fresh retry.
        tui.click(terminal.0, terminal.1);
        tui.type_line(CONSENT_NORMAL_MARKER);
        screen
            .wait_for(
                "normal_input_after_cancel",
                &format!("echo:{CONSENT_NORMAL_MARKER}"),
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        // Ordered barrier: the shell handles lines in order, so a paste that
        // leaked before consent would have printed before this echo.
        assert!(!screen.contains(CONSENT_FIRST_ACCEPTED));
        assert!(!screen.contains(CONSENT_SECOND_ACCEPTED));
        tui.paste(UNSAFE_PASTE);
        screen
            .wait_for(
                "second_unsafe_paste_dialog_visible",
                "Unsafe paste blocked",
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        assert!(!screen.contains(RAW_CONTROL_TOKEN));
        assert!(!screen.contains(RAW_SECOND_TOKEN));
        tui.write_all(b"\r");
        screen
            .wait_for(
                "second_unsafe_paste_armed",
                "Paste anyway",
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        tui.write_all(b"\t");
        tui.write_all(b"\r");
        screen
            .wait_for(
                "unsafe_paste_first_line_retried",
                CONSENT_FIRST_ACCEPTED,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        screen
            .wait_for(
                "unsafe_paste_second_line_retried",
                CONSENT_SECOND_ACCEPTED,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        assert!(!screen.contains(RAW_CONTROL_TOKEN));
        assert!(!screen.contains(RAW_SECOND_TOKEN));
        // Arming alone never sends: the consented retry is the only delivery.
        for accepted in [CONSENT_FIRST_ACCEPTED, CONSENT_SECOND_ACCEPTED] {
            let count = screen
                .rows()
                .iter()
                .filter(|row| row.contains(accepted))
                .count();
            assert_eq!(count, 1, "{accepted} must be delivered exactly once");
        }

        println!(
            "t_s3: consent session={} subscription={} generation={} zero-before-consent cancel-safe output-preserved retry-accepted raw-payload-hidden",
            occupancy.session_id, occupancy.subscription_id, occupancy.generation
        );
        detach_and_quit(&mut tui, &mut screen, &identity);
    }
    drop(guard);
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s4_outer_pty_resize_reaches_the_attached_session() {
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity, RESIZE_SHELL_COMMAND);
    {
        let mut tui = TuiChild::spawn(hub);
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
        // The session prints its PTY size on every SIGWINCH, so the fit to the
        // drawn pane is observed without any further input.
        let initial_size = screen.wait_for_pane("attached_pane_measured", None, &identity);
        screen
            .wait_for(
                "session_reports_attached_pty",
                &format!("winch:{} {}", initial_size.0, initial_size.1),
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));

        // One outer resize, with no follow-up input, must reach the session.
        const RESIZED_ROWS: u16 = 30;
        const RESIZED_COLS: u16 = 100;
        tui.resize(RESIZED_ROWS, RESIZED_COLS);
        screen.resize(RESIZED_ROWS, RESIZED_COLS);
        let resized = screen.wait_for_pane("resized_pane_measured", Some(initial_size), &identity);
        screen
            .wait_for(
                "session_reports_resized_pty",
                &format!("winch:{} {}", resized.0, resized.1),
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));

        println!(
            "t_s4: resized session={} subscription={} generation={} initial={:?} final={:?}",
            occupancy.session_id,
            occupancy.subscription_id,
            occupancy.generation,
            initial_size,
            resized
        );
        detach_and_quit(&mut tui, &mut screen, &identity);
    }
    drop(guard);
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s5_control_reconnect_clears_stale_attachment_and_attaches_a_fresh_session() {
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let mut guard = start_hub(&candidate);
    let mut stale_identity = Identity::default();
    spawn_session(guard.hub().endpoint(), &mut stale_identity, SHELL_COMMAND);
    {
        let mut tui = TuiChild::spawn(guard.hub());
        let mut screen = Screen::attach(&tui, test_deadline);
        let stale_session_row = format!("{} · running", stale_identity.session_id);
        screen
            .wait_for(
                "stale_session_row_visible",
                &stale_session_row,
                SCREEN_DEADLINE,
                &stale_identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let stale_occupancy = attach_and_echo(
            guard.hub().endpoint(),
            &mut tui,
            &mut screen,
            &mut stale_identity,
            &stale_session_row,
            MARKER_ONE,
            None,
        );

        guard.restart();

        // The reconnect must consume a fresh empty session snapshot. The old
        // attachment, projection, and toolbar control must not survive it.
        screen
            .wait_for(
                "control_reconnect_connected",
                "Hub: connected (running) · 0 sessions",
                CONTROL_RECONNECT_DEADLINE,
                &stale_identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        screen
            .wait_for(
                "control_reconnect_fresh_snapshot",
                "No sessions yet",
                SCREEN_DEADLINE,
                &stale_identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        assert!(!screen.contains(&stale_session_row));
        assert!(!screen.contains(SESSION_READY_MARKER));
        assert!(!screen.contains(&format!("echo:{MARKER_ONE}")));
        assert!(!screen.contains("[ Detach ]"));
        assert!(
            occupancies(guard.hub().endpoint(), &stale_identity).is_empty(),
            "the fresh hub must not retain the stale attachment: {stale_occupancy:?}"
        );

        let mut fresh_identity = Identity::default();
        spawn_session(guard.hub().endpoint(), &mut fresh_identity, SHELL_COMMAND);
        assert_ne!(fresh_identity.session_id, stale_identity.session_id);
        let fresh_session_row = format!("{} · running", fresh_identity.session_id);
        screen
            .wait_for(
                "fresh_session_row_visible",
                &fresh_session_row,
                SCREEN_DEADLINE,
                &fresh_identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let fresh_occupancy = attach_and_echo(
            guard.hub().endpoint(),
            &mut tui,
            &mut screen,
            &mut fresh_identity,
            &fresh_session_row,
            MARKER_TWO,
            None,
        );
        assert!(!screen.contains(&stale_session_row));
        assert!(!screen.contains(&format!("echo:{MARKER_ONE}")));
        // After the explicit attach completes, the Hub holds exactly one
        // occupancy for the session. The no-automatic-attach invariant is
        // proven at the request boundary by a unit test in app.rs.
        assert_eq!(
            occupancies(guard.hub().endpoint(), &fresh_identity),
            vec![fresh_occupancy.clone()],
            "the fresh session is attached only by explicit activation"
        );
        println!(
            "t_s5: reconnected stale_session={} stale_subscription={} fresh_session={} fresh_subscription={} input-output-visible",
            stale_identity.session_id,
            stale_occupancy.subscription_id,
            fresh_identity.session_id,
            fresh_occupancy.subscription_id
        );
        detach_and_quit(&mut tui, &mut screen, &fresh_identity);
    }
    drop(guard);
}

/// Admit a spawn target whose repo file defines one shell session type.
fn admit_launch_target(endpoint: &DaemonEndpoint, root: &Path, identity: &Identity) {
    let botster_dir = root.join(".botster");
    fs::create_dir_all(&botster_dir).expect("create the launch target .botster dir");
    let definition = serde_json::json!({
        "session_types": [{
            "id": LAUNCH_SESSION_TYPE,
            "label": LAUNCH_SESSION_TYPE_LABEL,
            "role": "botster.shell",
            "interaction": "interactive",
            "traits": [],
            "lifecycle": "task",
            "execution": { "mode": "shell_command" },
            "command": SHELL_COMMAND,
            "working_directory": { "policy": "package_root" }
        }]
    });
    fs::write(
        botster_dir.join("session-types.json"),
        serde_json::to_vec_pretty(&definition).expect("session types json"),
    )
    .expect("write the launch target session types");
    let response = request(
        endpoint,
        DaemonRequest::CreateSpawnTarget {
            target_id: Some(LAUNCH_TARGET_ID.to_string()),
            label: Some(LAUNCH_TARGET_LABEL.to_string()),
            root: root.to_path_buf(),
            enabled: true,
            kind: Some("directory".to_string()),
            base_ref: None,
            metadata: std::collections::BTreeMap::new(),
        },
    )
    .expect("create spawn target transport");
    expect_ok("create_spawn_target", identity, response);
}

/// Adopt the id of the one session the launch dialog started.
///
/// The TUI renders its running row from the Hub's session entity events, so
/// once that row is visible one Hub listing names it.
fn adopt_launched_session(endpoint: &DaemonEndpoint, screen: &mut Screen, identity: &mut Identity) {
    screen
        .wait_for(
            "launched_session_running",
            " · running",
            SESSION_RUNNING_DEADLINE,
            identity,
        )
        .unwrap_or_else(|failure| panic!("{failure}"));
    let response = expect_ok(
        "list_sessions",
        identity,
        request(endpoint, DaemonRequest::ListSessions).expect("list sessions transport"),
    );
    let running = response
        .sessions
        .iter()
        .filter(|session| session.lifecycle == "running")
        .map(|session| session.session_id.clone())
        .collect::<Vec<_>>();
    let [session_id] = running.as_slice() else {
        panic!(
            "layer={LAYER} step=launch_dialog_session_running cause=expected one running session, got {running:?}"
        );
    };
    identity.session_id = session_id.clone();
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s6_launch_dialog_spawns_a_session_type_at_an_admitted_target() {
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let target_root = ShortTempRoot::create();
    let mut identity = Identity::default();
    admit_launch_target(hub.endpoint(), &target_root.path, &identity);
    {
        let mut tui = TuiChild::spawn(hub);
        let mut screen = Screen::attach(&tui, test_deadline);
        let mut click = |screen: &mut Screen, step: &'static str, needle: &str| {
            let (col, row) = screen
                .wait_for(step, needle, SCREEN_DEADLINE, &identity)
                .unwrap_or_else(|failure| panic!("{failure}"));
            tui.click(col, row);
        };
        click(&mut screen, "spawn_control_visible", "[ Spawn ]");
        click(
            &mut screen,
            "launch_target_visible",
            &format!("[ {LAUNCH_TARGET_LABEL} ({LAUNCH_TARGET_ID}) ]"),
        );
        click(
            &mut screen,
            "launch_session_type_visible",
            &format!("[ {LAUNCH_SESSION_TYPE_LABEL} · "),
        );
        adopt_launched_session(hub.endpoint(), &mut screen, &mut identity);
        let session_row = format!("{} · running", identity.session_id);
        screen
            .wait_for(
                "launched_session_row_visible",
                &session_row,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        assert!(
            screen.contains("type="),
            "the launched row carries its session type classification"
        );
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
            "t_s6: launched session={} via target={} subscription={} generation={} echo visible",
            occupancy.session_id, LAUNCH_TARGET_ID, occupancy.subscription_id, occupancy.generation
        );
        detach_and_quit(&mut tui, &mut screen, &identity);
    }
    drop(guard);
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s7_host_keys_switch_sessions_and_leave_the_terminal() {
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let candidate = Candidate::from_env();
    let guard = start_hub(&candidate);
    let hub = guard.hub();
    let mut first = Identity::default();
    spawn_session(hub.endpoint(), &mut first, SHELL_COMMAND);
    let mut second = Identity::default();
    spawn_session(hub.endpoint(), &mut second, SHELL_COMMAND);
    assert_ne!(first.session_id, second.session_id);
    {
        let mut tui = TuiChild::spawn(hub);
        let mut screen = Screen::attach(&tui, test_deadline);
        let first_row = format!("{} · running", first.session_id);
        let second_row = format!("{} · running", second.session_id);
        for (step, row) in [
            ("first_row_visible", &first_row),
            ("second_row_visible", &second_row),
        ] {
            screen
                .wait_for(step, row, SCREEN_DEADLINE, &first)
                .unwrap_or_else(|failure| panic!("{failure}"));
        }
        // Leaves the first session's terminal pane focused.
        attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut first,
            &first_row,
            MARKER_ONE,
            None,
        );

        // Ctrl+J (0x0A) would be a newline to the shell; it must select the
        // other session instead, and Enter on the focused row attaches it.
        // The TUI reads its input in order, so neither key needs a settle.
        tui.write_all(b"\x0a");
        tui.write_all(b"\r");
        screen
            .wait_for(
                "second_session_attached",
                &format!("Terminal · {}", second.session_id),
                SCREEN_DEADLINE,
                &second,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let ready = screen
            .wait_for(
                "second_session_ready",
                SESSION_READY_MARKER,
                SCREEN_DEADLINE,
                &second,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        tui.click(ready.0, ready.1);
        tui.type_line(MARKER_TWO);
        screen
            .wait_for(
                "second_echo_visible",
                &format!("echo:{MARKER_TWO}"),
                SCREEN_DEADLINE,
                &second,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let occupancy = attached_occupancy(hub.endpoint(), &second, None);
        second.adopt(&occupancy);

        // Ordered barrier for "Ctrl+J and Enter never reached the first
        // session": return to it and type. Its shell handles input in order,
        // so a stray newline would already show as an empty echo line.
        tui.write_all(b"\x0b");
        tui.write_all(b"\r");
        screen
            .wait_for(
                "first_session_reattached",
                &format!("Terminal · {}", first.session_id),
                SCREEN_DEADLINE,
                &first,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let ready = screen
            .wait_for(
                "first_session_history_visible",
                &format!("echo:{MARKER_ONE}"),
                SCREEN_DEADLINE,
                &first,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        tui.click(ready.0, ready.1);
        tui.type_line(MARKER_THREE);
        screen
            .wait_for(
                "first_barrier_echo_visible",
                &format!("echo:{MARKER_THREE}"),
                SCREEN_DEADLINE,
                &first,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let echo_rows = screen
            .rows()
            .iter()
            .filter(|row| row.contains("echo:"))
            .count();
        assert_eq!(
            echo_rows,
            2,
            "only the two typed markers may echo in the first session: {:?}",
            screen.rows()
        );

        // Ctrl+P (0x10) moves focus to the toolbar, so q quits the TUI; a q
        // delivered to the shell would leave the TUI running instead.
        tui.write_all(b"\x10");
        tui.write_all(b"q");
        let started = Instant::now();
        let deadline = screen.remaining(EXIT_DEADLINE);
        let status = tui.wait_exit(started + deadline).unwrap_or_else(|cause| {
            let failure = screen.failure("tui_exit_after_ctrl_p", &first, deadline, started, cause);
            panic!("{failure}");
        });
        assert!(status.success(), "TUI exited unsuccessfully: {status:?}");
        println!(
            "t_s7: ctrl+j switched {} -> {} without stray input; ctrl+p left the terminal; q quit",
            first.session_id, second.session_id
        );
    }
    drop(guard);
}

/// One candidate Hub whose data directory survives a stop, for restart recovery.
///
/// `IsolatedHub::restart` recreates the data directory, so it cannot show what
/// happens to an existing session. This helper verifies the manifest hashes,
/// runs `botster-hub start` under a short owned root, and stops the Hub either
/// with `botster-hub shutdown` or with SIGKILL to the Hub process only, so the
/// session workers can outlive a crash. Each Hub start runs in a new process
/// group that the test records; `finish` stops the Hub, kills those groups,
/// and asserts that no member survives. Drop repeats the group kill as a
/// fallback after a failure.
struct PersistentHub {
    candidate: Candidate,
    root: ShortTempRoot,
    endpoint: DaemonEndpoint,
    running: Option<RunningHub>,
    starts: u32,
    /// Process groups this test created: one per Hub start. Session workers
    /// run in the group of the Hub start that spawned them.
    owned_groups: Vec<u32>,
}

/// One running Hub start: its pid and the exit status sent by a thread that
/// owns the child and blocks in `wait`.
struct RunningHub {
    pid: u32,
    exited: Receiver<std::io::Result<std::process::ExitStatus>>,
}

#[derive(Clone, Copy, Debug)]
enum HubStop {
    Graceful,
    Kill,
}

impl PersistentHub {
    fn start(candidate: Candidate) -> Self {
        verify_candidate_hashes(&candidate);
        let root = ShortTempRoot::create();
        fs::create_dir_all(root.path.join("data")).expect("create hub data dir");
        fs::create_dir_all(root.path.join("tmp")).expect("create hub tmp dir");
        let endpoint = DaemonEndpoint::new(root.path.join("data").join("botster-hub.sock"));
        let mut hub = Self {
            candidate,
            root,
            endpoint,
            running: None,
            starts: 0,
            owned_groups: Vec::new(),
        };
        hub.launch();
        hub
    }

    fn endpoint(&self) -> &DaemonEndpoint {
        &self.endpoint
    }

    fn data_dir(&self) -> PathBuf {
        self.root.path.join("data")
    }

    fn launch(&mut self) {
        use std::os::unix::process::CommandExt;
        self.starts += 1;
        let log = fs::File::create(self.root.path.join(format!("hub-{}.log", self.starts)))
            .expect("create hub log");
        let worker_dir = Path::new(&self.candidate.worker_bin)
            .parent()
            .expect("worker bin has a parent directory");
        let path = format!(
            "{}:{}",
            worker_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        // The Hub writes one readiness line on this pipe once its socket is
        // bound and its owner loop accepts requests, then closes it.
        let (ready_reader, ready_writer) = std::io::pipe().expect("readiness pipe");
        let ready_fd = std::os::fd::AsRawFd::as_raw_fd(&ready_writer);
        let mut command = std::process::Command::new(&self.candidate.hub_bin);
        command
            .args(["start", "--data-dir"])
            .arg(self.data_dir())
            .arg("--session-worker-bin")
            .arg(&self.candidate.worker_bin)
            .arg("--ready-fd")
            .arg(ready_fd.to_string())
            .current_dir(&self.root.path)
            .env("BOTSTER_ENV", "test")
            .env("TMPDIR", self.root.path.join("tmp"))
            .env("PATH", path)
            .stdout(log.try_clone().expect("clone hub log"))
            .stderr(log)
            .process_group(0);
        // SAFETY: fcntl(F_SETFD) is async-signal-safe. It clears close-on-exec
        // on the write end in the child only, so the Hub inherits exactly it.
        unsafe {
            command.pre_exec(move || {
                let fd = std::os::fd::BorrowedFd::borrow_raw(ready_fd);
                rustix::io::fcntl_setfd(fd, rustix::io::FdFlags::empty())
                    .map_err(std::io::Error::from)
            });
        }
        let child = command.spawn().expect("spawn candidate hub");
        // Only the child may hold the write end, or EOF never arrives.
        drop(ready_writer);
        let pid = child.id();
        self.owned_groups.push(pid);
        let (exit_tx, exited) = mpsc::channel();
        thread::spawn(move || {
            let mut child = child;
            let _ = exit_tx.send(child.wait());
        });
        self.running = Some(RunningHub { pid, exited });
        let (line_tx, line_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let read =
                std::io::BufRead::read_line(&mut std::io::BufReader::new(ready_reader), &mut line);
            let _ = line_tx.send(read.map(|_| line));
        });
        // timer: deadline — the Hub reports readiness within the start budget; expiry fails the start
        let line = match line_rx.recv_timeout(SESSION_RUNNING_DEADLINE) {
            Ok(Ok(line)) => line,
            Ok(Err(error)) => panic!(
                "layer={LAYER} step=persistent_hub_ready cause=hub start {} readiness read failed: {error}",
                self.starts
            ),
            Err(_) => {
                kill_groups(&[pid]);
                panic!(
                    "layer={LAYER} step=persistent_hub_ready cause=hub start {} reported no readiness before the deadline",
                    self.starts
                );
            }
        };
        let Some((protocol_version, build_revision)) = parse_ready_line(&line) else {
            // EOF (an empty read) or a malformed line: the start failed.
            panic!(
                "layer={LAYER} step=persistent_hub_ready cause=hub start {} failed before readiness: line={line:?}; see {}",
                self.starts,
                self.root
                    .path
                    .join(format!("hub-{}.log", self.starts))
                    .display()
            );
        };
        assert_eq!(
            protocol_version,
            botster_hub_client::PROTOCOL_VERSION,
            "the candidate Hub must speak the pinned protocol"
        );
        println!(
            "hub start {}: ready protocol_version={protocol_version} build_revision={build_revision}",
            self.starts
        );
    }

    fn stop(&mut self, how: HubStop) {
        let running = self.running.as_ref().expect("hub is running before stop");
        match how {
            HubStop::Graceful => {
                let status = std::process::Command::new(&self.candidate.hub_bin)
                    .args(["shutdown", "--data-dir"])
                    .arg(self.data_dir())
                    .env("BOTSTER_ENV", "test")
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .expect("run hub shutdown");
                assert!(status.success(), "hub shutdown failed: {status:?}");
            }
            HubStop::Kill => kill_pid(running.pid),
        }
        // timer: deadline — the stopped Hub exits within the exit budget; expiry fails the stop
        match running.exited.recv_timeout(EXIT_DEADLINE) {
            Ok(Ok(_status)) => self.running = None,
            Ok(Err(error)) => panic!(
                "layer={LAYER} step=persistent_hub_stop cause=waiting for hub pid {} failed after {how:?}: {error}",
                running.pid
            ),
            Err(_) => panic!(
                "layer={LAYER} step=persistent_hub_stop cause=hub pid {} did not exit after {how:?}",
                running.pid
            ),
        }
    }

    /// Stop the Hub, kill every group this test created, and assert that no
    /// member of those groups survives.
    fn finish(mut self) {
        if self.running.is_some() {
            self.stop(HubStop::Graceful);
        }
        kill_groups(&self.owned_groups);
        let members = group_members(&self.owned_groups);
        let pids = members.iter().map(|(pid, _, _)| *pid).collect::<Vec<_>>();
        wait_for_exits(&pids, EXIT_DEADLINE);
        let survivors = group_members(&self.owned_groups);
        assert!(
            survivors.is_empty(),
            "layer={LAYER} step=cleanup cause=processes survive in owned groups: {survivors:?}"
        );
        println!("cleanup: owned_groups={:?} survivors=0", self.owned_groups);
    }

    fn restart_after(&mut self, how: HubStop) {
        self.stop(how);
        self.launch();
    }
}

impl Drop for PersistentHub {
    fn drop(&mut self) {
        if let Some(running) = self.running.take() {
            kill_pid(running.pid);
            // timer: deadline — the killed Hub is reaped within the exit budget; expiry leaves the waiter thread
            let _ = running.exited.recv_timeout(EXIT_DEADLINE);
        }
        kill_groups(&self.owned_groups);
    }
}

/// Parse the Hub's readiness line: exactly `ready <protocol_version> <build_revision>\n`,
/// where the revision is printable ASCII without spaces.
fn parse_ready_line(line: &str) -> Option<(u16, String)> {
    let body = line.strip_suffix('\n')?;
    let mut fields = body.split(' ');
    let (Some("ready"), Some(protocol), Some(revision), None) =
        (fields.next(), fields.next(), fields.next(), fields.next())
    else {
        return None;
    };
    let protocol = protocol.parse().ok()?;
    let printable = !revision.is_empty() && revision.bytes().all(|byte| byte.is_ascii_graphic());
    printable.then(|| (protocol, revision.to_string()))
}

#[test]
fn ready_line_parser_accepts_only_the_exact_contract() {
    assert_eq!(
        parse_ready_line("ready 10 e3dacd99\n"),
        Some((10, "e3dacd99".to_string()))
    );
    assert_eq!(
        parse_ready_line("ready 10 unknown\n"),
        Some((10, "unknown".to_string()))
    );
    for bad in [
        "",
        "ready 10 abc",
        "ready 10\n",
        "ready x abc\n",
        "ready 10 a b\n",
        "ready 10  abc\n",
        "ready 70000 abc\n",
        "readyx 10 abc\n",
        "ready 10 ab\tc\n",
    ] {
        assert_eq!(parse_ready_line(bad), None, "{bad:?} must be rejected");
    }
}

fn kill_pid(pid: u32) {
    if let Some(pid) = i32::try_from(pid)
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process(pid, rustix::process::Signal::KILL);
    }
}

/// A `Timespec` for the time left until `until` (zero once it has passed).
fn timespec_until(until: Instant) -> rustix::event::Timespec {
    let left = until.saturating_duration_since(Instant::now());
    rustix::event::Timespec {
        tv_sec: left
            .as_secs()
            .try_into()
            .unwrap_or(rustix::event::Secs::MAX),
        tv_nsec: left.subsec_nanos().into(),
    }
}

/// Wait until every pid has exited, bounded by `deadline`. Both platforms
/// report exits of processes that are not our children: kqueue EVFILT_PROC
/// NOTE_EXIT on macOS, pidfd on Linux (both through rustix).
#[cfg(target_os = "macos")]
fn wait_for_exits(pids: &[u32], deadline: Duration) {
    use rustix::event::kqueue::{
        Event, EventFilter, EventFlags, ProcessEvents, kevent_timespec, kqueue,
    };
    let queue = kqueue().expect("kqueue");
    let mut pending = 0;
    for pid in pids {
        let Some(pid) = i32::try_from(*pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        else {
            continue;
        };
        let change = Event::new(
            EventFilter::Proc {
                pid,
                flags: ProcessEvents::EXIT,
            },
            EventFlags::ADD | EventFlags::ONESHOT,
            std::ptr::null_mut(),
        );
        let mut none: Vec<Event> = Vec::new();
        // SAFETY: the change names a pid, not a file descriptor; udata is null.
        match unsafe { kevent_timespec(&queue, &[change], &mut none, None) } {
            Ok(_) => pending += 1,
            // ESRCH: the pid already exited, so there is nothing to wait for.
            Err(rustix::io::Errno::SRCH) => {}
            Err(error) => panic!("kqueue registration for pid {pid:?} failed: {error}"),
        }
    }
    // timer: deadline — killed processes exit within the cleanup budget; expiry fails the survivor check
    let until = Instant::now() + deadline;
    let mut events: Vec<Event> = Vec::with_capacity(pending.max(1));
    while pending > 0 {
        events.clear();
        let timeout = timespec_until(until);
        // SAFETY: no changes; the kernel only fills `events`.
        match unsafe { kevent_timespec(&queue, &[], &mut events, Some(&timeout)) } {
            Ok(0) => break,
            Ok(ready) => pending = pending.saturating_sub(ready),
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => panic!("kqueue wait failed: {error}"),
        }
    }
}

#[cfg(target_os = "linux")]
fn wait_for_exits(pids: &[u32], deadline: Duration) {
    use rustix::event::{PollFd, PollFlags, poll};
    use rustix::process::{PidfdFlags, pidfd_open};
    let mut fds = Vec::new();
    for pid in pids {
        let Some(pid) = i32::try_from(*pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        else {
            continue;
        };
        match pidfd_open(pid, PidfdFlags::empty()) {
            Ok(fd) => fds.push(fd),
            // ESRCH: the pid already exited, so there is nothing to wait for.
            Err(rustix::io::Errno::SRCH) => {}
            Err(error) => panic!("pidfd_open for pid {pid:?} failed: {error}"),
        }
    }
    // timer: deadline — killed processes exit within the cleanup budget; expiry fails the survivor check
    let until = Instant::now() + deadline;
    while !fds.is_empty() {
        let timeout = timespec_until(until);
        let mut polled = fds
            .iter()
            .map(|fd| PollFd::new(fd, PollFlags::IN))
            .collect::<Vec<_>>();
        let ready = match poll(&mut polled, Some(&timeout)) {
            Ok(ready) => ready,
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => panic!("pidfd poll failed: {error}"),
        };
        if ready == 0 {
            break;
        }
        // IN or HUP on a pidfd means the process exited; ERR, NVAL, or any
        // other flag is a failed wait, not exit evidence.
        let exit_flags = PollFlags::IN | PollFlags::HUP;
        let exited = polled
            .iter()
            .map(|entry| {
                let flags = entry.revents();
                assert!(
                    exit_flags.contains(flags),
                    "pidfd poll reported non-exit flags {flags:?}"
                );
                !flags.is_empty()
            })
            .collect::<Vec<_>>();
        drop(polled);
        let mut index = 0;
        fds.retain(|_| {
            let keep = !exited[index];
            index += 1;
            keep
        });
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("the live tests wait for process exits with kqueue (macOS) or pidfd (Linux)");

/// SIGKILL every process group in `groups`; each was created by this test.
fn kill_groups(groups: &[u32]) {
    for group in groups {
        let Some(pgid) = i32::try_from(*group)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        else {
            continue;
        };
        // ESRCH means the group is already empty.
        let result = rustix::process::kill_process_group(pgid, rustix::process::Signal::KILL);
        println!("cleanup: kill -9 -{group} -> {result:?}");
    }
}

/// Fail closed unless both candidate binaries match the manifest sha256 values.
fn verify_candidate_hashes(candidate: &Candidate) {
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&candidate.manifest).expect("read candidate manifest"))
            .expect("candidate manifest json");
    for (name, path) in [
        ("botster-hub", &candidate.hub_bin),
        ("botster-session-worker", &candidate.worker_bin),
    ] {
        let expected = manifest["artifacts"]
            .as_array()
            .and_then(|artifacts| artifacts.iter().find(|artifact| artifact["name"] == name))
            .and_then(|artifact| artifact["sha256"].as_str())
            .unwrap_or_else(|| panic!("manifest has no sha256 for {name}"))
            .to_string();
        let output = std::process::Command::new("shasum")
            .args(["-a", "256", path])
            .output()
            .expect("run shasum");
        let actual = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        assert_eq!(
            actual, expected,
            "layer={LAYER} step=candidate_hash name={name}"
        );
        println!("provenance: {name}={path} sha256={actual} (manifest match)");
    }
    println!(
        "provenance: manifest={} tui_bin={} tui_version={}",
        candidate.manifest,
        env!("CARGO_BIN_EXE_botster-tui"),
        env!("CARGO_PKG_VERSION")
    );
}

/// Live processes in the given process groups, as (pid, pgid, command).
fn group_members(groups: &[u32]) -> Vec<(u32, u32, String)> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,pgid=,stat=,command="])
        .output()
        .expect("run ps");
    // An empty listing from a failed ps must not read as "no survivors".
    assert!(output.status.success(), "ps failed: {:?}", output.status);
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let pgid = fields.next()?.parse().ok()?;
            // An exited process awaiting its reaper is not a survivor.
            if fields.next()?.starts_with('Z') {
                return None;
            }
            let command = fields.collect::<Vec<_>>().join(" ");
            groups.contains(&pgid).then_some((pid, pgid, command))
        })
        .collect()
}

/// The session workers this test's Hub starts spawned, by process group.
fn owned_session_workers(groups: &[u32]) -> Vec<(u32, u32, String)> {
    group_members(groups)
        .into_iter()
        .filter(|(_, _, command)| {
            command
                .split_whitespace()
                .next()
                .is_some_and(|program| program.ends_with("/botster-session-worker"))
        })
        .collect()
}

fn listed_lifecycle(endpoint: &DaemonEndpoint, identity: &Identity) -> Option<String> {
    expect_ok(
        "list_sessions_after_restart",
        identity,
        request(endpoint, DaemonRequest::ListSessions).expect("list sessions transport"),
    )
    .sessions
    .into_iter()
    .find(|session| session.session_id == identity.session_id)
    .map(|session| session.lifecycle)
}

/// Counts input lines in the session process, so output after a Hub restart
/// shows whether the same process survived (`seq:2`) or a new one runs (`seq:1`).
const COUNTING_SHELL_COMMAND: &str = "printf 'live-ready\\n'; n=0; while IFS= read -r line; do n=$((n+1)); printf 'echo:%s\\nseq:%s\\n' \"$line\" \"$n\"; done";

/// Spawn and attach one session, stop the Hub, start it on the same data
/// directory, and record what the reconnected TUI shows for that session.
///
/// Client coherence is asserted: the TUI reconnects, its row agrees with the
/// Hub's listing, and a session the Hub lists as running attaches on a fresh
/// route and echoes. The outcome is printed, and the recovery gate then
/// requires that the same session process survived.
fn restart_recovery(how: HubStop, label: &str) {
    // timer: deadline — whole-test budget shared by every wait; expiry fails the test
    let test_deadline = Instant::now() + TEST_DEADLINE;
    let mut hub = PersistentHub::start(Candidate::from_env());
    let mut identity = Identity::default();
    spawn_session(hub.endpoint(), &mut identity, COUNTING_SHELL_COMMAND);
    let workers = owned_session_workers(&hub.owned_groups);
    assert!(
        !workers.is_empty(),
        "layer={LAYER} step=worker_group cause=no session worker in owned groups {:?}",
        hub.owned_groups
    );
    println!("{label}: workers_before_stop={workers:?}");
    {
        let mut tui = TuiChild::spawn_at(hub.endpoint(), &hub.data_dir(), &hub.root.path);
        let mut screen = Screen::attach(&tui, test_deadline);
        let running_row = format!("{} · running", identity.session_id);
        screen
            .wait_for(
                "session_row_visible",
                &running_row,
                SCREEN_DEADLINE,
                &identity,
            )
            .unwrap_or_else(|failure| panic!("{failure}"));
        let before = attach_and_echo(
            hub.endpoint(),
            &mut tui,
            &mut screen,
            &mut identity,
            &running_row,
            MARKER_ONE,
            None,
        );
        screen
            .wait_for("first_line_counted", "seq:1", SCREEN_DEADLINE, &identity)
            .unwrap_or_else(|failure| panic!("{failure}"));

        hub.restart_after(how);
        // Before the stop the status row said "Attached: <session>". A row with
        // both needles can only come from the new connection.
        screen.wait_for_row_with_all(
            "tui_reconnected",
            &["connected (running)", "Attached: none"],
            CONTROL_RECONNECT_DEADLINE,
            &identity,
        );
        // The recovered session's row comes from the new connection's entity
        // snapshot. A session that is not running shows another state and is
        // classified from the Hub listing below.
        let _ = screen.wait_for(
            "recovered_row_running",
            &running_row,
            SCREEN_DEADLINE,
            &identity,
        );
        let lifecycle = listed_lifecycle(hub.endpoint(), &identity);
        println!(
            "{label}: owned_groups={:?} workers_after_restart={:?}",
            hub.owned_groups,
            owned_session_workers(&hub.owned_groups)
        );
        let rows = screen.head_rows();

        let outcome = match lifecycle.as_deref() {
            Some("running") => {
                let occupancy = attach_and_echo(
                    hub.endpoint(),
                    &mut tui,
                    &mut screen,
                    &mut identity,
                    &running_row,
                    MARKER_TWO,
                    None,
                );
                assert!(
                    occupancy.subscription_id != before.subscription_id
                        || occupancy.generation != before.generation,
                    "recovery must use a fresh terminal route"
                );
                // New output on the fresh route decides process continuity:
                // the counter line printed after the marker's echo.
                let counted = screen
                    .wait_until(
                        "post_recovery_count",
                        SCREEN_DEADLINE,
                        &identity,
                        |screen| counter_after_echo(&screen.rows(), MARKER_TWO),
                        |screen| {
                            format!("no count after the recovery echo: {:?}", screen.tail_rows())
                        },
                    )
                    .unwrap_or_else(|failure| panic!("{failure}"));
                let continuity = if counted == 2 {
                    "same process (seq:2)"
                } else {
                    "new process (seq restarted)"
                };
                let history = screen.contains(&format!("echo:{MARKER_ONE}"));
                detach_and_quit(&mut tui, &mut screen, &identity);
                format!(
                    "running_after_restart continuity={continuity} fresh_subscription={} generation={} history_visible={history}",
                    occupancy.subscription_id, occupancy.generation
                )
            }
            Some(other) => {
                let row = format!("{} · {other}", identity.session_id);
                screen
                    .wait_for(
                        "unresolved_row_matches_hub",
                        &row,
                        SCREEN_DEADLINE,
                        &identity,
                    )
                    .unwrap_or_else(|failure| panic!("{failure}"));
                quit_with_ctrl_p(&mut tui, &mut screen, &identity);
                format!("reported lifecycle={other}")
            }
            None => {
                screen
                    .wait_until(
                        "gone_row_removed",
                        SCREEN_DEADLINE,
                        &identity,
                        |screen| (!screen.contains(&identity.session_id)).then_some(()),
                        |_| {
                            format!(
                                "TUI still shows {} after the Hub dropped it",
                                identity.session_id
                            )
                        },
                    )
                    .unwrap_or_else(|failure| panic!("{failure}"));
                quit_with_ctrl_p(&mut tui, &mut screen, &identity);
                "gone (hub no longer lists the session)".to_string()
            }
        };
        println!(
            "{label}: stop={how:?} session={} outcome={outcome} rows_after_reconnect={rows:?}",
            identity.session_id
        );
        // The recovery gate: the same session process survives the Hub stop.
        assert!(
            outcome.contains("continuity=same process"),
            "layer={LAYER} step=recovery_gate cause={label} did not recover the existing session: {outcome}"
        );
    }
    hub.finish();
}

/// The `seq:<n>` count printed right after `echo:<marker>`, if visible.
fn counter_after_echo(rows: &[String], marker: &str) -> Option<u64> {
    let echo = format!("echo:{marker}");
    let at = rows.iter().position(|row| row.contains(&echo))?;
    rows[at + 1..].iter().find_map(|row| {
        let (_, count) = row.split_once("seq:")?;
        count
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .ok()
    })
}

/// Ctrl+P moves focus off the terminal, then q quits; the exit must succeed.
fn quit_with_ctrl_p(tui: &mut TuiChild, screen: &mut Screen, identity: &Identity) {
    // The TUI reads its input in order: Ctrl+P is applied before q.
    tui.write_all(b"\x10");
    tui.write_all(b"q");
    let started = Instant::now();
    let deadline = screen.remaining(EXIT_DEADLINE);
    let status = tui.wait_exit(started + deadline).unwrap_or_else(|cause| {
        let failure = screen.failure("tui_exit_after_ctrl_p", identity, deadline, started, cause);
        panic!("{failure}");
    });
    assert!(status.success(), "TUI exited unsuccessfully: {status:?}");
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s8a_graceful_hub_restart_reports_the_existing_session() {
    restart_recovery(HubStop::Graceful, "t_s8a");
}

#[test]
#[ignore = "candidate smoke: needs BOTSTER_HUB_BIN, BOTSTER_SESSION_WORKER_BIN, BOTSTER_CANDIDATE_MANIFEST; run with --ignored --exact"]
fn t_s8b_hub_kill_restart_reports_the_existing_session() {
    restart_recovery(HubStop::Kill, "t_s8b");
}
