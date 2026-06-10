use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use botster_core::ui::{UiChild, UiFormValues, UiNode, UiNodeId, UiNodeKind};
use botster_hub_client::{
    DaemonCompatibility, DaemonCompatibilityRequirement, DaemonDiagnostic, DaemonDiagnosticKind,
    DaemonEndpoint, DaemonEvent, DaemonPackage, DaemonRequest, DaemonResponse, DaemonResponseKind,
    DaemonTransportError, DaemonTransportResult, FEATURE_RESIZE, FEATURE_SESSIONS,
    FEATURE_TERMINAL_STREAMING, PROTOCOL, connect_and_hello_with_requirement,
    read_frame_from_reader, write_frame,
};
use crossterm::{
    cursor::Show,
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};
use serde_json::{Value, json};

use crate::renderer::{self, HitMap, InputDispatch, InputRouter};

const DEFAULT_COMMAND: &str = "printf 'botster-tui-ready\\n'; while IFS= read -r line; do printf 'echo:%s\\n' \"$line\"; done";
const HEADLESS_INPUT: &str = "botster-tui-headless\n";
const HEADLESS_OUTPUT: &str = "echo:botster-tui-headless";
const SMOKE_MESSAGE: &str = "botster-tui smoke ok";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppArgs {
    pub smoke: bool,
    pub hub_socket: Option<PathBuf>,
    pub headless_dogfood: bool,
}

impl AppArgs {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut parsed = Self::default();
        let mut iter = args.into_iter();
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--smoke" => parsed.smoke = true,
                "--headless-dogfood" => parsed.headless_dogfood = true,
                "--hub-socket" => {
                    parsed.hub_socket = iter.next().map(PathBuf::from);
                }
                _ => {}
            }
        }
        if parsed.hub_socket.is_none() {
            parsed.hub_socket = std::env::var_os("BOTSTER_HUB_SOCKET").map(PathBuf::from);
        }
        parsed
    }
}

pub fn smoke_message() -> &'static str {
    SMOKE_MESSAGE
}

pub fn run(args: AppArgs) -> io::Result<()> {
    if args.headless_dogfood {
        return run_headless_dogfood(args)
            .map_err(|error| io::Error::other(format!("headless dogfood failed: {error}")));
    }

    let mut terminal = setup_terminal()?;
    let run_result = run_loop(&mut terminal, args);
    let restore_result = restore_terminal(&mut terminal);

    match (run_result, restore_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen, EnableMouseCapture) {
        let _ = disable_raw_mode();
        return Err(error);
    }

    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let mut stdout = io::stdout();
            let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen, Show);
            let _ = disable_raw_mode();
            Err(error)
        }
    }
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let leave_result = execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        Show
    );
    let raw_result = disable_raw_mode();
    let cursor_result = terminal.show_cursor();

    leave_result?;
    raw_result?;
    cursor_result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, args: AppArgs) -> io::Result<()> {
    let mut app = DogfoodApp::new(args.hub_socket);
    let mut router = InputRouter::new();
    loop {
        app.poll_hub();
        app.set_drafts(router.draft_values());

        let mut hit_map = HitMap::default();
        terminal.draw(|frame| draw(frame, &mut hit_map, &app))?;

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press && should_quit(key) => break,
                _ => {
                    let dispatch = router.dispatch_event(event, &hit_map);
                    app.sync_focused_session(router.selected_row_value("dogfood-session-list"));
                    app.handle_dispatch(dispatch);
                }
            }
        }
    }

    Ok(())
}

fn draw(frame: &mut Frame<'_>, hit_map: &mut HitMap, app: &DogfoodApp) {
    let node = app.surface();
    renderer::render_node(frame, frame.area(), &node, hit_map);
}

fn should_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || matches!(key.code, KeyCode::Char('q' | 'Q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

struct DogfoodApp {
    endpoint: Option<DaemonEndpoint>,
    client: Option<HubConnection>,
    status: String,
    connection_error: Option<String>,
    error: Option<String>,
    action_feedback: Option<String>,
    compatibility: Option<DaemonCompatibility>,
    diagnostics: Vec<DaemonDiagnostic>,
    package_count: usize,
    enabled_package_count: usize,
    packages: Vec<DaemonPackage>,
    sessions: Vec<String>,
    selected_session: Option<String>,
    attached_session: Option<String>,
    schema_version: Option<u16>,
    subscription_id: String,
    terminal_output: String,
    command: String,
    drafts: BTreeMap<String, Value>,
    last_reconnect_attempt: Option<Instant>,
    #[cfg(test)]
    observed_requests: Vec<ObservedRequest>,
}

impl DogfoodApp {
    fn new(hub_socket: Option<PathBuf>) -> Self {
        let endpoint = hub_socket.map(DaemonEndpoint::new);
        let mut app = Self {
            endpoint,
            client: None,
            status: "disconnected".to_string(),
            connection_error: None,
            error: None,
            action_feedback: None,
            compatibility: None,
            diagnostics: Vec::new(),
            package_count: 0,
            enabled_package_count: 0,
            packages: Vec::new(),
            sessions: Vec::new(),
            selected_session: None,
            attached_session: None,
            schema_version: None,
            subscription_id: format!("btui-sub-{}", short_suffix()),
            terminal_output: String::new(),
            command: DEFAULT_COMMAND.to_string(),
            drafts: BTreeMap::new(),
            last_reconnect_attempt: None,
            #[cfg(test)]
            observed_requests: Vec::new(),
        };
        app.try_connect();
        app
    }

    fn set_drafts(&mut self, drafts: BTreeMap<String, Value>) {
        self.drafts = drafts;
    }

    fn sync_focused_session(&mut self, selected_row: Option<&Value>) {
        let Some(session_id) = selected_row.and_then(Value::as_str) else {
            return;
        };
        if self
            .sessions
            .iter()
            .any(|candidate| candidate == session_id)
        {
            self.selected_session = Some(session_id.to_string());
        }
    }

    fn poll_hub(&mut self) {
        if self.client.is_none() {
            self.try_connect_throttled();
            return;
        }

        let Some(session_id) = self
            .attached_session
            .clone()
            .or_else(|| self.selected_session.clone())
        else {
            return;
        };
        match self.request(DaemonRequest::Drain { session_id }) {
            Ok(response) => self.apply_response(response),
            Err(error) => self.record_transport_error(error),
        }
    }

    fn handle_dispatch(&mut self, dispatch: InputDispatch) {
        match dispatch {
            InputDispatch::Action(request) => {
                self.handle_action(request.action_id.0, request.values, request.payload);
            }
            InputDispatch::TerminalForward { bytes, .. } => {
                let Some(session_id) = self.attached_session.clone() else {
                    self.error = Some(
                        "terminal stream unavailable: attach a session before sending terminal input"
                            .to_string(),
                    );
                    return;
                };
                match String::from_utf8(bytes) {
                    Ok(data) => {
                        self.error = None;
                        self.request_and_apply(DaemonRequest::SendInput { session_id, data })
                    }
                    Err(error) => {
                        self.error = Some(format!("terminal input was not UTF-8: {error}"))
                    }
                }
            }
            InputDispatch::TerminalResize { rows, cols, .. } => {
                if let Some(session_id) = self.attached_session.clone() {
                    self.request_and_apply(DaemonRequest::Resize {
                        session_id,
                        rows,
                        cols,
                    });
                }
            }
            _ => {}
        }
    }

    fn handle_action(
        &mut self,
        action_id: String,
        values: Option<UiFormValues>,
        payload: Option<Value>,
    ) {
        if let Some(values) = values
            && let Some(value) = values.0.get("command").and_then(Value::as_str)
        {
            self.command = value.to_string();
        }

        match action_id.as_str() {
            "botster.tui.connect" => self.force_reconnect(),
            "botster.tui.spawn" => self.spawn_session(),
            "botster.tui.attach" => {
                if let Some(session_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_id"))
                    .and_then(Value::as_str)
                {
                    self.selected_session = Some(session_id.to_string());
                }
                self.attach_selected_or_first();
            }
            "botster.tui.detach" => self.detach_attached(),
            "botster.tui.refresh" => self.refresh_read_models(),
            "botster.terminal.focus" => self.attach_selected_or_first(),
            _ => {}
        }
    }

    fn try_connect_throttled(&mut self) {
        let now = Instant::now();
        if self
            .last_reconnect_attempt
            .is_some_and(|attempt| now.duration_since(attempt) < Duration::from_millis(750))
        {
            return;
        }
        self.try_connect();
    }

    fn force_reconnect(&mut self) {
        self.client = None;
        self.try_connect();
    }

    fn try_connect(&mut self) {
        self.last_reconnect_attempt = Some(Instant::now());
        let Some(endpoint) = &self.endpoint else {
            self.status = "hub socket missing".to_string();
            self.connection_error =
                Some("configure --hub-socket or BOTSTER_HUB_SOCKET".to_string());
            return;
        };
        match HubConnection::connect(endpoint) {
            Ok(client) => {
                self.client = Some(client);
                self.status = "connected".to_string();
                self.connection_error = None;
                self.refresh_read_models();
                self.restore_after_connect();
            }
            Err(error) => {
                self.record_transport_error(error);
            }
        }
    }

    fn restore_after_connect(&mut self) {
        if self.selected_session.is_some() {
            self.attach_selected_or_first();
        }
    }

    fn refresh_read_models(&mut self) {
        self.refresh_status();
        self.refresh_sessions();
        self.refresh_packages();
    }

    fn refresh_status(&mut self) {
        self.request_and_apply(DaemonRequest::Status);
    }

    fn refresh_sessions(&mut self) {
        self.request_and_apply(DaemonRequest::ListSessions);
    }

    fn refresh_packages(&mut self) {
        self.request_and_apply(DaemonRequest::ListPackages);
    }

    fn spawn_session(&mut self) {
        if self.command.trim().is_empty() {
            self.error = Some("command is required".to_string());
            return;
        }
        self.error = None;
        let session_id = format!("btui-{}", short_suffix());
        let command = self.command.clone();
        match self.request(DaemonRequest::Spawn {
            session_id: session_id.clone(),
            command,
        }) {
            Ok(response) => self.apply_response(response),
            Err(error) => {
                self.record_transport_error(error);
                return;
            }
        }
        self.selected_session = Some(session_id.clone());
        self.action_feedback = Some(format!("spawned {session_id}; attach requested"));
        self.request_and_apply(DaemonRequest::Attach {
            session_id,
            subscription_id: self.subscription_id.clone(),
        });
    }

    fn attach_selected_or_first(&mut self) {
        let Some(session_id) = self
            .selected_session
            .clone()
            .or_else(|| self.sessions.first().cloned())
        else {
            self.error = Some("no session available to attach".to_string());
            return;
        };
        self.error = None;
        self.selected_session = Some(session_id.clone());
        self.action_feedback = Some(format!("attach requested: {session_id}"));
        self.request_and_apply(DaemonRequest::Attach {
            session_id,
            subscription_id: self.subscription_id.clone(),
        });
    }

    fn detach_attached(&mut self) {
        let Some(session_id) = self.attached_session.clone() else {
            self.error = Some("no attached terminal stream to detach".to_string());
            return;
        };
        self.error = None;
        self.action_feedback = Some(format!("detach requested: {session_id}"));
        self.request_and_apply(DaemonRequest::Detach {
            session_id,
            subscription_id: self.subscription_id.clone(),
        });
    }

    fn request_and_apply(&mut self, request: DaemonRequest) {
        #[cfg(test)]
        self.record_request(&request);
        match self.request(request) {
            Ok(response) => self.apply_response(response),
            Err(error) => self.record_transport_error(error),
        }
    }

    fn request(&mut self, request: DaemonRequest) -> DaemonTransportResult<DaemonResponse> {
        match &mut self.client {
            Some(client) => client.request(&request),
            None => Err(DaemonTransportError::NotRunning),
        }
    }

    #[cfg(test)]
    fn record_request(&mut self, request: &DaemonRequest) {
        match request {
            DaemonRequest::Status => self.observed_requests.push(ObservedRequest::Status),
            DaemonRequest::ListSessions => {
                self.observed_requests.push(ObservedRequest::ListSessions)
            }
            DaemonRequest::ListPackages => {
                self.observed_requests.push(ObservedRequest::ListPackages)
            }
            DaemonRequest::Attach {
                session_id,
                subscription_id,
            } => self.observed_requests.push(ObservedRequest::Attach {
                session_id: session_id.clone(),
                subscription_id: subscription_id.clone(),
            }),
            _ => {}
        }
    }

    fn record_transport_error(&mut self, error: DaemonTransportError) {
        self.client = None;
        self.attached_session = None;
        match error {
            // Defensive for malformed protocol frames outside the hello
            // compatibility path, which now surfaces as Compatibility below.
            DaemonTransportError::Protocol(message) => {
                self.status = "compatibility mismatch".to_string();
                self.connection_error = Some(format!(
                    "expected daemon protocol {PROTOCOL}; daemon protocol error: {message}"
                ));
                self.record_diagnostic(DaemonDiagnostic::compatibility_mismatch(message));
            }
            DaemonTransportError::Compatibility(error) => {
                self.status = "compatibility mismatch".to_string();
                self.connection_error = Some(error.diagnostic.clone());
                self.record_diagnostics(error.diagnostics);
            }
            DaemonTransportError::NotRunning => {
                self.status = "hub unavailable; reconnecting".to_string();
                self.connection_error = Some(error.to_string());
            }
            DaemonTransportError::ClientDisconnected => {
                self.status = "disconnected; reconnecting".to_string();
                self.connection_error = Some(error.to_string());
                self.record_diagnostic(DaemonDiagnostic::disconnected(error.to_string()));
            }
            other => {
                self.status = "reconnecting".to_string();
                self.connection_error = Some(other.to_string());
            }
        }
    }

    fn apply_response(&mut self, response: DaemonResponse) {
        self.record_diagnostics(response.diagnostics);

        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
            self.error = Some(error.message);
            return;
        }

        if let Some(status) = response.status {
            self.connection_error = None;
            self.clear_connection_diagnostics();
            self.schema_version = Some(status.schema_version);
            self.compatibility = Some(status.compatibility);
            self.record_diagnostics(status.diagnostics);
            self.status = format!("connected ({})", status.lifecycle_state);
            self.package_count = status.package_count;
            self.enabled_package_count = status.enabled_package_count;
        }

        if matches!(
            response.kind,
            DaemonResponseKind::Sessions | DaemonResponseKind::Spawned
        ) {
            self.sessions = response
                .sessions
                .into_iter()
                .map(|session| session.session_id)
                .collect();
            if self
                .selected_session
                .as_ref()
                .is_none_or(|selected| !self.sessions.contains(selected))
            {
                self.selected_session = self.sessions.first().cloned();
            }
        }

        if matches!(response.kind, DaemonResponseKind::Packages) {
            self.packages = response.packages;
        }

        for event in response.events {
            match event {
                DaemonEvent::TerminalOutput { data, .. } => {
                    self.terminal_output.push_str(&data);
                    if self.terminal_output.len() > 8_000 {
                        self.terminal_output = self
                            .terminal_output
                            .chars()
                            .rev()
                            .take(8_000)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                    }
                }
                DaemonEvent::ProcessExit { code, .. } => {
                    self.status = format!("process exited {}", code.unwrap_or_default());
                    self.attached_session = None;
                }
                DaemonEvent::AttachState {
                    session_id, state, ..
                } => {
                    self.action_feedback = Some(format!("attach {state}: {session_id}"));
                    if state == "attached" || state == "subscribed" || state == "ready" {
                        self.attached_session = Some(session_id);
                    } else if state == "detached" || state == "closed" || state == "unsubscribed" {
                        self.attached_session = None;
                    }
                }
                _ => {}
            }
        }
    }

    fn clear_connection_diagnostics(&mut self) {
        self.diagnostics.retain(|diagnostic| {
            !matches!(
                diagnostic.kind,
                DaemonDiagnosticKind::CompatibilityMismatch
                    | DaemonDiagnosticKind::UnsupportedFeature
                    | DaemonDiagnosticKind::Disconnected
                    | DaemonDiagnosticKind::DaemonStartupFailure
            )
        });
    }

    fn record_diagnostics(&mut self, diagnostics: Vec<DaemonDiagnostic>) {
        for diagnostic in diagnostics {
            self.record_diagnostic(diagnostic);
        }
    }

    fn record_diagnostic(&mut self, diagnostic: DaemonDiagnostic) {
        self.diagnostics.retain(|existing| {
            !(existing.kind == diagnostic.kind
                && existing.operation == diagnostic.operation
                && existing.feature == diagnostic.feature)
        });
        self.diagnostics.push(diagnostic);
    }

    fn surface(&self) -> UiNode {
        let mut root = node(
            UiNodeKind::Stack,
            "dogfood-root",
            json!({ "direction": "vertical" }),
        );
        root.children = vec![
            child(self.status_panel()),
            child(self.command_form()),
            child(self.sessions_panel()),
            child(self.terminal_panel()),
        ];
        root.validate()
            .expect("dogfood UiNode should satisfy the core UI contract");
        renderer::tui_capabilities()
            .validate_node(&root)
            .expect("dogfood UiNode should fit TUI renderer capabilities");
        root
    }

    fn status_panel(&self) -> UiNode {
        let mut panel = node(
            UiNodeKind::Panel,
            "dogfood-status-panel",
            json!({ "title": "hub" }),
        );
        let mut children = vec![
            child(node(
                UiNodeKind::Text,
                "dogfood-status",
                json!({ "text": self.status }),
            )),
            child(node(
                UiNodeKind::Text,
                "dogfood-compatibility",
                json!({ "text": self.compatibility_text() }),
            )),
            child(button(
                "dogfood-refresh",
                "Refresh",
                "botster.tui.refresh",
                json!({}),
            )),
            child(button(
                "dogfood-connect",
                "Reconnect",
                "botster.tui.connect",
                json!({}),
            )),
        ];
        if let Some(error) = &self.connection_error {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-connection-error",
                json!({ "text": format!("connection: {error}") }),
            )));
        }
        children.push(child(node(
            UiNodeKind::Text,
            "dogfood-package-summary",
            json!({ "text": self.package_summary_text() }),
        )));
        if self.packages.is_empty() {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-package-empty",
                json!({ "text": "packages: none reported" }),
            )));
        } else {
            for (index, package) in self.packages.iter().enumerate() {
                children.push(child(node(
                    UiNodeKind::Text,
                    &format!("dogfood-package-{index}"),
                    json!({ "text": format!("package: {}", package_text(package)) }),
                )));
                for (entrypoint_index, entrypoint) in
                    package.runnable_entrypoints.iter().enumerate()
                {
                    children.push(child(node(
                        UiNodeKind::Text,
                        &format!("dogfood-package-{index}-entrypoint-{entrypoint_index}"),
                        json!({
                            "text": format!(
                                "entrypoint: {} {}",
                                package.package_name,
                                entrypoint_text(entrypoint)
                            )
                        }),
                    )));
                }
            }
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            children.push(child(node(
                UiNodeKind::Text,
                &format!("dogfood-diagnostic-{index}"),
                json!({ "text": format!("diagnostic: {}", diagnostic_text(diagnostic)) }),
            )));
        }
        if let Some(feedback) = &self.action_feedback {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-action-feedback",
                json!({ "text": format!("action: {feedback}") }),
            )));
        }
        if let Some(error) = &self.error {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        children.push(child(node(
            UiNodeKind::Text,
            "dogfood-hints",
            json!({ "text": "hints: Tab focus | up/down select | Enter/Space activate | terminal focus forwards keys" }),
        )));
        panel.children = children;
        panel
    }

    fn package_summary_text(&self) -> String {
        format!(
            "packages: {} installed; {} enabled",
            self.package_count, self.enabled_package_count
        )
    }

    fn compatibility_text(&self) -> String {
        match &self.compatibility {
            Some(compatibility) => format!(
                "compatibility: protocol {} version {}; features {}; conformance {}; daemon schema {}",
                compatibility.protocol,
                compatibility.protocol_version,
                compatibility.features.join(","),
                compatibility.conformance_fixture_revision,
                self.schema_version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            None => format!(
                "compatibility: expected protocol {PROTOCOL}; daemon schema {}; descriptor unavailable",
                self.schema_version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        }
    }

    fn command_form(&self) -> UiNode {
        let command = self
            .drafts
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or(&self.command);
        let mut panel = node(
            UiNodeKind::Panel,
            "dogfood-command-panel",
            json!({ "title": "spawn" }),
        );
        panel.children = vec![
            child(node(
                UiNodeKind::TextInput,
                "dogfood-command",
                json!({
                    "name": "command",
                    "label": "command",
                    "value": command
                }),
            )),
            child(button(
                "dogfood-spawn",
                "Spawn and attach",
                "botster.tui.spawn",
                json!({}),
            )),
        ];
        panel
    }

    fn sessions_panel(&self) -> UiNode {
        let mut list = node(UiNodeKind::List, "dogfood-session-list", json!({}));
        list.children = self
            .sessions
            .iter()
            .map(|session_id| {
                let mut title = session_id.to_string();
                if self.attached_session.as_deref() == Some(session_id) {
                    title.push_str(" (attached)");
                } else if self.selected_session.as_deref() == Some(session_id) {
                    title.push_str(" (selected)");
                }
                let mut item = node(
                    UiNodeKind::ListItem,
                    &format!("dogfood-session-{session_id}"),
                    json!({
                        "value": session_id
                    }),
                );
                item.slots.insert(
                    "title".to_string(),
                    vec![child(node(
                        UiNodeKind::Text,
                        &format!("dogfood-session-{session_id}-title"),
                        json!({ "text": title }),
                    ))],
                );
                item.slots.insert(
                    "actions".to_string(),
                    vec![child(button(
                        &format!("dogfood-session-{session_id}-attach"),
                        "Attach",
                        "botster.tui.attach",
                        json!({ "session_id": session_id }),
                    ))],
                );
                child(item)
            })
            .collect();
        let mut panel = node(
            UiNodeKind::Panel,
            "dogfood-sessions-panel",
            json!({ "title": "sessions" }),
        );
        panel.children = vec![
            child(node(
                UiNodeKind::Text,
                "dogfood-selected-session",
                json!({ "text": format!("selected: {}", self.selected_session.as_deref().unwrap_or("none")) }),
            )),
            child(node(
                UiNodeKind::Text,
                "dogfood-attached-session",
                json!({ "text": format!("attached: {}", self.attached_session.as_deref().unwrap_or("none")) }),
            )),
            child(list),
            child(button(
                "dogfood-detach",
                "Detach",
                "botster.tui.detach",
                json!({}),
            )),
        ];
        panel
    }

    fn terminal_panel(&self) -> UiNode {
        let mut terminal = node(
            UiNodeKind::TerminalView,
            "dogfood-terminal",
            json!({
                "title": self.terminal_title(),
                "session_id": self.attached_session.clone().unwrap_or_else(|| "not attached".to_string())
            }),
        );
        terminal.children = vec![child(node(
            UiNodeKind::Text,
            "dogfood-terminal-output",
            json!({ "text": self.terminal_content() }),
        ))];
        terminal
    }

    fn terminal_title(&self) -> String {
        match (&self.attached_session, &self.selected_session) {
            (Some(attached), _) => format!("terminal attached: {attached}"),
            (None, Some(selected)) => format!("terminal stream unavailable: selected {selected}"),
            (None, None) => "terminal stream unavailable: no session selected".to_string(),
        }
    }

    fn terminal_content(&self) -> String {
        if !self.terminal_output.is_empty() {
            return self.terminal_output.clone();
        }
        match (&self.attached_session, &self.selected_session) {
            (Some(session_id), _) => format!("waiting for terminal output from {session_id}"),
            (None, Some(session_id)) => {
                format!("terminal stream unavailable: attach selected session {session_id}")
            }
            (None, None) => "terminal stream unavailable: no session selected".to_string(),
        }
    }
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum ObservedRequest {
    Status,
    ListSessions,
    ListPackages,
    Attach {
        session_id: String,
        subscription_id: String,
    },
}

fn run_headless_dogfood(args: AppArgs) -> DaemonTransportResult<()> {
    let Some(socket) = args.hub_socket else {
        return Err(DaemonTransportError::NotRunning);
    };
    let mut app = DogfoodApp::new(Some(socket));
    app.command = DEFAULT_COMMAND.to_string();
    app.spawn_session();
    if let Some(error) = &app.error {
        eprintln!("headless-dogfood-error: {error}");
        return Err(DaemonTransportError::Protocol("headless dogfood app error"));
    }
    let session_id = app
        .selected_session
        .clone()
        .ok_or(DaemonTransportError::Protocol(
            "headless session was not selected",
        ))?;

    wait_for_app_output(&mut app, "botster-tui-ready")?;
    app.request_and_apply(DaemonRequest::Resize {
        session_id: session_id.clone(),
        rows: 24,
        cols: 80,
    });
    app.request_and_apply(DaemonRequest::SendInput {
        session_id: session_id.clone(),
        data: HEADLESS_INPUT.to_string(),
    });
    wait_for_app_output(&mut app, HEADLESS_OUTPUT)?;
    #[cfg(test)]
    {
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");
        let compatibility = app
            .compatibility
            .as_ref()
            .expect("live hub status should include compatibility descriptor");
        assert_eq!(compatibility.protocol, PROTOCOL);
        assert!(compatibility.protocol_version > 0);
        assert!(!compatibility.features.is_empty());
        assert!(rendered.contains(&format!("protocol {}", compatibility.protocol)));
        assert!(rendered.contains(&format!("version {}", compatibility.protocol_version)));
        assert!(rendered.contains(&format!("features {}", compatibility.features.join(","))));
        assert!(rendered.contains(HEADLESS_OUTPUT));
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "dogfood-terminal-output")
        );
    }
    println!("terminal-output: {HEADLESS_OUTPUT}");
    app.request_and_apply(DaemonRequest::ShutdownSession { session_id });
    Ok(())
}

fn wait_for_app_output(app: &mut DogfoodApp, needle: &str) -> DaemonTransportResult<()> {
    if app.terminal_output.contains(needle) {
        return Ok(());
    }

    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        app.poll_hub();
        if app.terminal_output.contains(needle) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(25));
    }

    eprintln!("terminal-output-observed: {:?}", app.terminal_output);
    Err(DaemonTransportError::ClientDisconnected)
}

fn node(kind: UiNodeKind, id: &str, props: Value) -> UiNode {
    UiNode {
        kind,
        id: Some(UiNodeId(id.to_string())),
        props: props.as_object().cloned().unwrap_or_default(),
        children: Vec::new(),
        slots: BTreeMap::new(),
    }
}

fn child(node: UiNode) -> UiChild {
    UiChild::Node(Box::new(node))
}

fn button(id: &str, label: &str, action_id: &str, payload: Value) -> UiNode {
    node(
        UiNodeKind::Button,
        id,
        json!({
            "label": label,
            "action": {
                "id": action_id,
                "payload": payload
            }
        }),
    )
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn short_suffix() -> u64 {
    (unique_suffix() % 1_000_000_000_000) as u64
}

struct HubConnection {
    stream: std::os::unix::net::UnixStream,
    reader: std::io::BufReader<std::os::unix::net::UnixStream>,
}

impl HubConnection {
    fn connect(endpoint: &DaemonEndpoint) -> DaemonTransportResult<Self> {
        let stream =
            connect_and_hello_with_requirement(endpoint, &tui_compatibility_requirement())?;
        let reader = std::io::BufReader::new(stream.try_clone().map_err(DaemonTransportError::Io)?);
        Ok(Self { stream, reader })
    }

    fn request(&mut self, request: &DaemonRequest) -> DaemonTransportResult<DaemonResponse> {
        write_frame(&mut self.stream, request)?;
        read_frame_from_reader(&mut self.reader)
    }
}

fn tui_compatibility_requirement() -> DaemonCompatibilityRequirement {
    DaemonCompatibilityRequirement {
        protocol: PROTOCOL.to_string(),
        minimum_protocol_version: botster_hub_client::PROTOCOL_VERSION,
        required_features: vec![
            FEATURE_SESSIONS.to_string(),
            FEATURE_TERMINAL_STREAMING.to_string(),
            FEATURE_RESIZE.to_string(),
        ],
        minimum_conformance_fixture_revision: botster_hub_client::CONFORMANCE_FIXTURE_REVISION,
        client_name: "botster-tui".to_string(),
    }
}

fn diagnostic_text(diagnostic: &DaemonDiagnostic) -> String {
    let label = match diagnostic.kind {
        DaemonDiagnosticKind::Connected => "connected",
        DaemonDiagnosticKind::Disconnected => "disconnected",
        DaemonDiagnosticKind::CompatibilityMismatch => "compatibility_mismatch",
        DaemonDiagnosticKind::UnsupportedFeature => "unsupported_feature",
        DaemonDiagnosticKind::TerminalStreamUnavailable => "terminal_stream_unavailable",
        DaemonDiagnosticKind::ActionFailure => "action_failure",
        DaemonDiagnosticKind::DaemonStartupFailure => "daemon_startup_failure",
    };
    let mut parts = vec![label.to_string()];
    if let Some(operation) = &diagnostic.operation {
        parts.push(format!("operation={operation}"));
    }
    if let Some(feature) = &diagnostic.feature {
        parts.push(format!("feature={feature}"));
    }
    if let Some(message) = &diagnostic.message {
        parts.push(message.clone());
    }
    parts.join("; ")
}

fn package_text(package: &DaemonPackage) -> String {
    format!(
        "{} {} classification={} state={} capabilities={} provider_profile_admitted={}",
        package.package_name,
        package.version,
        package.classification,
        package.state,
        capability_text(&package.requested_capabilities),
        package.provider_profile_admitted
    )
}

fn entrypoint_text(entrypoint: &botster_hub_client::DaemonPackageRunnableEntrypoint) -> String {
    let process = &entrypoint.process;
    let mut parts = vec![
        format!("id={}", entrypoint.id),
        format!("kind={}", entrypoint.kind),
        format!("state={}", process.state),
    ];
    if !process.diagnostics.is_empty() {
        let diagnostics = process
            .diagnostics
            .iter()
            .map(|diagnostic| format!("{}:{}", diagnostic.kind, diagnostic.message))
            .collect::<Vec<_>>()
            .join(",");
        parts.push(format!("diagnostics={diagnostics}"));
    }
    if let Some(pid) = process.pid {
        parts.push(format!("pid={pid}"));
    }
    if let Some(started_at) = process.started_at {
        parts.push(format!("started_at={started_at}"));
    }
    if let Some(exited_at) = process.exited_at {
        parts.push(format!("exited_at={exited_at}"));
    }
    if let Some(exit_status) = &process.exit_status {
        parts.push(format!("exit_status={exit_status}"));
    }
    parts.join(",")
}

fn capability_text(capabilities: &[botster_hub_client::DaemonCapability]) -> String {
    if capabilities.is_empty() {
        return "none".to_string();
    }

    capabilities
        .iter()
        .map(|capability| match &capability.scope {
            Some(scope) => format!("{}:{scope}", capability.surface),
            None => capability.surface.clone(),
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_message_names_the_scaffold() {
        assert_eq!(smoke_message(), "botster-tui smoke ok");
    }

    #[test]
    fn quit_keys_match_documented_exit_path() {
        assert!(should_quit(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(should_quit(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        )));
        assert!(should_quit(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!should_quit(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE
        )));
    }

    #[test]
    fn parses_hub_socket_and_headless_mode() {
        let args = AppArgs::parse([
            "--hub-socket".to_string(),
            "target/hub.sock".to_string(),
            "--headless-dogfood".to_string(),
        ]);

        assert_eq!(args.hub_socket, Some(PathBuf::from("target/hub.sock")));
        assert!(args.headless_dogfood);
    }

    #[test]
    fn command_draft_is_rendered_before_submit() {
        let mut app = DogfoodApp::new(None);
        app.set_drafts(BTreeMap::from([(
            "command".to_string(),
            Value::String("printf draft\\n".to_string()),
        )]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        assert!(lines.join("\n").contains("printf draft"));
    }

    #[test]
    fn blank_command_validation_renders_visible_error_state() {
        let mut app = DogfoodApp::new(None);
        app.command = " \t\n".to_string();

        app.spawn_session();

        assert_eq!(app.error.as_deref(), Some("command is required"));
        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        assert!(lines.join("\n").contains("error: command is required"));
    }

    #[test]
    fn missing_hub_socket_renders_connection_diagnostic() {
        let app = DogfoodApp::new(None);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("hub socket missing"));
        assert!(rendered.contains("configure --hub-socket or BOTSTER_HUB_SOCKET"));
        assert!(rendered.contains(PROTOCOL));
    }

    #[test]
    fn compatibility_error_branch_renders_distinct_compatibility_diagnostic() {
        let mut app = DogfoodApp::new(None);
        let mut requirement = tui_compatibility_requirement();
        requirement
            .required_features
            .push("botster-tui-future-feature".to_string());
        let error =
            botster_hub_client::ensure_compatible(&requirement, &DaemonCompatibility::current())
                .expect_err("unsatisfied requirement should produce compatibility error");

        app.record_transport_error(DaemonTransportError::Compatibility(error));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("compatibility mismatch"));
        assert!(rendered.contains("unsupported_feature"));
        assert!(rendered.contains("botster-tui-future-feature"));
        assert!(!rendered.contains("hub unavailable; reconnecting"));
    }

    #[test]
    fn daemon_status_renders_compatibility_descriptor_from_public_status_response() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(status_response("running", 7));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("connected (running)"));
        assert!(rendered.contains("daemon schema 7"));
        assert!(rendered.contains("protocol botster-hub-daemon-v1 version 1"));
        assert!(rendered.contains("features sessions,terminal_streaming,resize"));
    }

    #[test]
    fn daemon_status_renders_package_counts_from_public_status_response() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(status_response_with_package_counts("running", 7, 3, 1));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("packages: 3 installed; 1 enabled"));
    }

    #[test]
    fn package_response_renders_installed_state_capabilities_and_provider_admission() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(status_response_with_package_counts("running", 7, 3, 1));
        app.apply_response(packages_response(vec![
            package(
                "local-alpha",
                "0.1.0",
                "local",
                "enabled",
                vec![
                    capability("mcp", Some("tools")),
                    capability("surface", None),
                ],
                true,
            ),
            package(
                "local-beta",
                "0.2.0",
                "local",
                "disabled",
                Vec::new(),
                false,
            ),
            package(
                "local-gamma",
                "0.3.0",
                "local",
                "pending-review",
                Vec::new(),
                false,
            ),
        ]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("packages: 3 installed; 1 enabled"));
        assert!(rendered.contains(
            "package: local-alpha 0.1.0 classification=local state=enabled capabilities=mcp:tools,surface provider_profile_admitted=true"
        ));
        assert!(rendered.contains(
            "package: local-beta 0.2.0 classification=local state=disabled capabilities=none provider_profile_admitted=false"
        ));
        assert!(rendered.contains("local-gamma 0.3.0 classification=local state=pending-review"));
    }

    #[test]
    fn package_response_preserves_zero_entrypoint_package_row() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(packages_response(vec![package(
            "local-alpha",
            "0.1.0",
            "local",
            "enabled",
            Vec::new(),
            true,
        )]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains(
            "package: local-alpha 0.1.0 classification=local state=enabled capabilities=none provider_profile_admitted=true"
        ));
        assert!(!rendered.contains("entrypoints="));
    }

    #[test]
    fn package_response_renders_running_entrypoint_process_state_without_url() {
        let mut app = DogfoodApp::new(None);

        let mut package = package(
            "workflow.plugin",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        package.runnable_entrypoints = vec![entrypoint("web", "web", process("running"))];

        app.apply_response(packages_response(vec![package]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 48);
        let rendered = lines.join("\n");

        assert!(
            rendered.contains("entrypoint: workflow.plugin id=web,kind=web,state=running,pid=1234")
        );
        assert!(rendered.contains("started_at=1781060000"));
        assert!(!rendered.contains("url="));
    }

    #[test]
    fn package_response_renders_failed_entrypoint_diagnostics() {
        let mut app = DogfoodApp::new(None);

        let mut failed = process("failed");
        failed.exit_status = Some("exit code 1".to_string());
        failed.diagnostics = vec![package_diagnostic("stderr", "server failed to bind")];
        let mut package = package(
            "workflow.plugin",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        package.runnable_entrypoints = vec![entrypoint("web", "web", failed)];

        app.apply_response(packages_response(vec![package]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("entrypoint: workflow.plugin id=web,kind=web,state=failed"));
        assert!(rendered.contains("exit_status=exit code 1"));
        assert!(rendered.contains("diagnostics=stderr:server failed to bind"));
    }

    #[test]
    fn package_response_renders_stopped_entrypoint_process_state() {
        let mut app = DogfoodApp::new(None);

        let mut stopped = process("stopped");
        stopped.pid = None;
        stopped.started_at = None;
        stopped.exited_at = Some(1781060300);
        let mut package = package(
            "workflow.plugin",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        package.runnable_entrypoints = vec![entrypoint("worker", "worker", stopped)];

        app.apply_response(packages_response(vec![package]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("id=worker,kind=worker,state=stopped"));
        assert!(rendered.contains("exited_at=1781060300"));
    }

    #[test]
    fn package_response_renders_multiple_entrypoint_process_states() {
        let mut app = DogfoodApp::new(None);

        let mut worker = process("starting");
        worker.pid = None;
        let mut package = package(
            "workflow.plugin",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        package.runnable_entrypoints = vec![
            entrypoint("web", "web", process("running")),
            entrypoint("worker", "worker", worker),
        ];

        app.apply_response(packages_response(vec![package]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("id=web,kind=web,state=running"));
        assert!(rendered.contains("id=worker,kind=worker,state=starting"));
    }

    #[test]
    fn package_diagnostics_render_through_existing_diagnostic_surface() {
        let mut app = DogfoodApp::new(None);
        app.apply_response(status_response_with_package_counts("running", 7, 1, 0));
        let mut response = packages_response(vec![package(
            "local-alpha",
            "0.1.0",
            "local",
            "disabled",
            Vec::new(),
            false,
        )]);
        response.diagnostics.push(DaemonDiagnostic {
            kind: DaemonDiagnosticKind::ActionFailure,
            operation: Some("list_packages".to_string()),
            feature: Some("package_registry".to_string()),
            message: Some("package manifest failed compatibility checks".to_string()),
        });

        app.apply_response(response);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("diagnostic: action_failure"));
        assert!(rendered.contains("operation=list_packages"));
        assert!(rendered.contains("feature=package_registry"));
        assert!(rendered.contains("package manifest failed compatibility checks"));
    }

    #[test]
    fn response_diagnostics_render_connected_state() {
        let mut app = DogfoodApp::new(None);
        let mut response = status_response("running", 7);
        response
            .diagnostics
            .push(DaemonDiagnostic::connected("status"));

        app.apply_response(response);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        assert!(
            lines
                .join("\n")
                .contains("diagnostic: connected; operation=status")
        );
    }

    #[test]
    fn healthy_status_clears_stale_connection_lifecycle_diagnostics() {
        let mut app = DogfoodApp::new(None);
        let mut requirement = tui_compatibility_requirement();
        requirement
            .required_features
            .push("botster-tui-future-feature".to_string());
        let error =
            botster_hub_client::ensure_compatible(&requirement, &DaemonCompatibility::current())
                .expect_err("unsatisfied requirement should produce compatibility error");
        let mut response = status_response("running", 7);
        response
            .diagnostics
            .push(DaemonDiagnostic::connected("status"));

        app.record_transport_error(DaemonTransportError::Compatibility(error));
        app.record_transport_error(DaemonTransportError::ClientDisconnected);
        app.apply_response(response);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("connected (running)"));
        assert!(rendered.contains("diagnostic: connected; operation=status"));
        assert!(!rendered.contains("compatibility_mismatch"));
        assert!(!rendered.contains("unsupported_feature"));
        assert!(!rendered.contains("disconnected"));
        assert!(!rendered.contains("botster-tui-future-feature"));
    }

    #[test]
    fn operator_diagnostics_render_terminal_stream_unavailable() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(operator_error_response_with_diagnostics(
            "attach failed",
            vec![DaemonDiagnostic::terminal_stream_unavailable(
                "attach",
                "no terminal stream",
            )],
        ));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("error: attach failed"));
        assert!(rendered.contains("terminal_stream_unavailable"));
        assert!(rendered.contains("feature=terminal_streaming"));
    }

    #[test]
    fn action_failure_survives_unrelated_successful_session_refresh() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(operator_error_response("spawn failed"));
        app.apply_response(sessions_response(["session-alpha"]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        assert!(lines.join("\n").contains("error: spawn failed"));
    }

    #[test]
    fn corrected_user_action_clears_stale_validation_error() {
        let mut app = DogfoodApp::new(None);
        app.command = " \t\n".to_string();
        app.spawn_session();
        assert_eq!(app.error.as_deref(), Some("command is required"));

        app.command = "printf fixed\\n".to_string();
        app.spawn_session();

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("error: command is required"));
        assert!(rendered.contains("hub unavailable; reconnecting"));
    }

    #[test]
    fn not_running_path_is_not_reported_as_compatibility_mismatch() {
        let mut app = DogfoodApp::new(None);

        app.record_transport_error(DaemonTransportError::NotRunning);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("hub unavailable; reconnecting"));
        assert!(!rendered.contains("compatibility mismatch"));
    }

    #[test]
    fn terminal_input_before_attach_renders_stream_unavailable_error() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec!["session-alpha".to_string()];
        app.selected_session = Some("session-alpha".to_string());

        app.handle_dispatch(InputDispatch::TerminalForward {
            node_id: "dogfood-terminal".to_string(),
            bytes: b"echo hello\n".to_vec(),
        });

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("terminal stream unavailable"));
        assert!(rendered.contains("attached: none"));
    }

    #[test]
    fn attach_state_tracks_attached_session_separately_from_selection() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec!["session-alpha".to_string(), "session-beta".to_string()];
        app.selected_session = Some("session-beta".to_string());

        app.apply_response(attach_state_response("session-beta", "attached"));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert_eq!(app.attached_session.as_deref(), Some("session-beta"));
        assert!(rendered.contains("attached: session-beta"));
        assert!(rendered.contains("session-beta (attached)"));
        assert!(rendered.contains("terminal attached: session-beta"));
    }

    #[test]
    fn terminal_view_carries_output_bytes() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output = "hello terminal".to_string();

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        assert!(lines.join("\n").contains("hello terminal"));
    }

    #[test]
    fn terminal_output_renders_as_terminal_primitive_content() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output = "primitive terminal bytes".to_string();

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);

        assert!(lines.join("\n").contains("primitive terminal bytes"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "dogfood-terminal")
        );
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "dogfood-terminal-output")
        );
    }

    #[test]
    fn focused_session_list_row_updates_attach_selection() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec!["session-alpha".to_string(), "session-beta".to_string()];
        app.selected_session = Some("session-alpha".to_string());
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let mut router = InputRouter::new();
        let first_row = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "dogfood-session-session-alpha")
            .expect("first session row should be focusable");

        router.dispatch_event(
            Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: first_row.rect.x,
                row: first_row.rect.y,
                modifiers: KeyModifiers::NONE,
            }),
            &hit_map,
        );
        router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            &hit_map,
        );
        app.sync_focused_session(router.selected_row_value("dogfood-session-list"));

        assert_eq!(app.selected_session.as_deref(), Some("session-beta"));
    }

    #[test]
    fn session_repull_preserves_selected_session_when_still_listed() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec!["session-alpha".to_string(), "session-beta".to_string()];
        app.selected_session = Some("session-beta".to_string());

        app.apply_response(sessions_response(["session-alpha", "session-beta"]));

        assert_eq!(app.sessions, vec!["session-alpha", "session-beta"]);
        assert_eq!(app.selected_session.as_deref(), Some("session-beta"));
    }

    #[test]
    fn session_repull_resets_stale_selected_session_to_first_listed_session() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec!["session-alpha".to_string(), "session-beta".to_string()];
        app.selected_session = Some("session-beta".to_string());

        app.apply_response(sessions_response(["session-gamma", "session-delta"]));

        assert_eq!(app.sessions, vec!["session-gamma", "session-delta"]);
        assert_eq!(app.selected_session.as_deref(), Some("session-gamma"));
    }

    #[test]
    fn refresh_read_models_pulls_status_sessions_and_packages() {
        let mut app = DogfoodApp::new(None);
        app.observed_requests.clear();

        app.refresh_read_models();

        assert_eq!(
            app.observed_requests,
            vec![
                ObservedRequest::Status,
                ObservedRequest::ListSessions,
                ObservedRequest::ListPackages,
            ]
        );
    }

    #[test]
    fn reconnect_restore_reattaches_selected_session_after_read_model_refresh() {
        let mut app = DogfoodApp::new(None);
        app.observed_requests.clear();
        app.sessions = vec!["session-alpha".to_string()];
        app.selected_session = Some("session-alpha".to_string());
        let subscription_id = app.subscription_id.clone();

        app.restore_after_connect();

        assert_eq!(
            app.observed_requests,
            vec![ObservedRequest::Attach {
                session_id: "session-alpha".to_string(),
                subscription_id,
            }]
        );
    }

    #[test]
    fn tui_hub_boundary_uses_public_client_without_private_protocol_plumbing() {
        let source = source_without_line_comments();

        assert!(source.contains("use botster_hub_client"));
        for required in [
            "connect_and_hello_with_requirement",
            "DaemonEndpoint",
            "DaemonRequest",
            "DaemonResponse",
        ] {
            assert!(
                source.contains(required),
                "botster-tui should keep using public botster-hub-client {required}"
            );
        }

        let forbidden_patterns = [
            concat!("FRA", "ME_"),
            concat!("SESSION", "_FRAME"),
            concat!("Daemon", "Frame"),
            concat!("Session", "Frame"),
            concat!("Hub", "Frame"),
            concat!("session", "_protocol"),
            concat!("read", "_line"),
            concat!("write", "_all"),
        ];
        for pattern in forbidden_patterns {
            assert!(
                !source.contains(pattern),
                "botster-tui source must not reintroduce private hub protocol plumbing: {pattern}"
            );
        }
    }

    #[test]
    fn headless_dogfood_runs_against_isolated_hub_when_binaries_are_available() {
        let Some(hub_bin) = std::env::var_os("BOTSTER_HUB_BIN") else {
            skip_or_panic("BOTSTER_HUB_BIN");
            return;
        };
        let Some(session_worker_bin) = std::env::var_os("BOTSTER_SESSION_WORKER_BIN") else {
            skip_or_panic("BOTSTER_SESSION_WORKER_BIN");
            return;
        };

        let root = PathBuf::from(format!("/tmp/bt{}", short_suffix() % 1_000_000));
        let hub = botster_hub_test_support::IsolatedHubBuilder::new()
            .hub_bin(hub_bin)
            .session_worker_bin(session_worker_bin)
            .root(&root)
            .name("botster-tui-headless-dogfood")
            .start()
            .expect("isolated hub starts");

        run_headless_dogfood(AppArgs {
            smoke: false,
            hub_socket: Some(hub.endpoint().socket_path.clone()),
            headless_dogfood: true,
        })
        .expect("headless dogfood surface completes a real hub round trip");

        let mut requirement = tui_compatibility_requirement();
        requirement
            .required_features
            .push("botster-tui-future-feature".to_string());
        let error = connect_and_hello_with_requirement(hub.endpoint(), &requirement)
            .expect_err("live hub should reject unsatisfied TUI compatibility requirement");
        let mut app = DogfoodApp::new(None);
        app.record_transport_error(error);
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("compatibility mismatch"));
        assert!(rendered.contains("unsupported_feature"));
        assert!(rendered.contains("botster-tui-future-feature"));

        hub.shutdown().expect("isolated hub shuts down cleanly");
    }

    fn skip_or_panic(variable: &'static str) {
        if std::env::var_os("BOTSTER_TUI_REQUIRE_HUB_TEST").is_some() {
            panic!("{variable} is required when BOTSTER_TUI_REQUIRE_HUB_TEST is set");
        }
        eprintln!("skipping isolated hub dogfood test; {variable} is not set");
    }

    fn source_without_line_comments() -> String {
        let src_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
        std::fs::read_dir(src_dir)
            .expect("botster-tui src directory is readable")
            .map(|entry| entry.expect("source entry is readable").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
            .map(|path| {
                std::fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("{} is readable: {error}", path.display()))
            })
            .flat_map(|contents| {
                contents
                    .lines()
                    .map(|line| {
                        line.split_once("//")
                            .map(|(before_comment, _)| before_comment)
                            .unwrap_or(line)
                            .to_string()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn sessions_response<const N: usize>(session_ids: [&str; N]) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::Sessions);
        response.sessions = session_ids
            .into_iter()
            .map(|session_id| botster_hub_client::DaemonSession {
                session_id: session_id.to_string(),
                lifecycle: "running".to_string(),
            })
            .collect();
        response
    }

    fn status_response(lifecycle_state: &str, schema_version: u16) -> DaemonResponse {
        status_response_with_package_counts(lifecycle_state, schema_version, 0, 0)
    }

    fn status_response_with_package_counts(
        lifecycle_state: &str,
        schema_version: u16,
        package_count: usize,
        enabled_package_count: usize,
    ) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::Status);
        response.status = Some(botster_hub_client::DaemonStatus {
            lifecycle_state: lifecycle_state.to_string(),
            compatibility: DaemonCompatibility {
                protocol: PROTOCOL.to_string(),
                protocol_version: 1,
                features: vec![
                    FEATURE_SESSIONS.to_string(),
                    FEATURE_TERMINAL_STREAMING.to_string(),
                    FEATURE_RESIZE.to_string(),
                ],
                conformance_fixture_revision: 1,
            },
            host_id: "test-host".to_string(),
            host_display_name: "test host".to_string(),
            schema_version,
            data_dir_configured: true,
            core_initialized: true,
            state_source: "test".to_string(),
            package_count,
            enabled_package_count,
            provider_count: 0,
            enabled_provider_count: 0,
            session_count: 0,
            recovered_sessions: Vec::new(),
            stale_sessions: Vec::new(),
            diagnostics: Vec::new(),
        });
        response
    }

    fn packages_response(packages: Vec<DaemonPackage>) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::Packages);
        response.packages = packages;
        response
    }

    fn package(
        package_name: &str,
        version: &str,
        classification: &str,
        state: &str,
        requested_capabilities: Vec<botster_hub_client::DaemonCapability>,
        provider_profile_admitted: bool,
    ) -> DaemonPackage {
        DaemonPackage {
            package_name: package_name.to_string(),
            version: version.to_string(),
            classification: classification.to_string(),
            state: state.to_string(),
            requested_capabilities,
            runnable_entrypoints: Vec::new(),
            provider_profile_admitted,
        }
    }

    fn entrypoint(
        id: &str,
        kind: &str,
        process: botster_hub_client::DaemonPackageProcess,
    ) -> botster_hub_client::DaemonPackageRunnableEntrypoint {
        botster_hub_client::DaemonPackageRunnableEntrypoint {
            id: id.to_string(),
            kind: kind.to_string(),
            command: "bin/run".to_string(),
            args: Vec::new(),
            working_directory: botster_hub_client::DaemonPackageWorkingDirectory {
                policy: "package_root".to_string(),
                path: None,
            },
            environment: Vec::new(),
            mode: "dev".to_string(),
            capabilities: Vec::new(),
            may_supervise: true,
            process,
        }
    }

    fn process(state: &str) -> botster_hub_client::DaemonPackageProcess {
        botster_hub_client::DaemonPackageProcess {
            state: state.to_string(),
            pid: Some(1234),
            started_at: Some(1781060000),
            exited_at: None,
            exit_status: None,
            diagnostics: Vec::new(),
        }
    }

    fn package_diagnostic(
        kind: &str,
        message: &str,
    ) -> botster_hub_client::DaemonPackageDiagnostic {
        botster_hub_client::DaemonPackageDiagnostic {
            kind: kind.to_string(),
            message: message.to_string(),
        }
    }

    fn capability(surface: &str, scope: Option<&str>) -> botster_hub_client::DaemonCapability {
        botster_hub_client::DaemonCapability {
            surface: surface.to_string(),
            scope: scope.map(str::to_string),
        }
    }

    fn operator_error_response(message: &str) -> DaemonResponse {
        operator_error_response_with_diagnostics(message, Vec::new())
    }

    fn operator_error_response_with_diagnostics(
        message: &str,
        diagnostics: Vec<DaemonDiagnostic>,
    ) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::OperatorError);
        response.error = Some(botster_hub_client::DaemonOperatorError {
            code: "test".to_string(),
            request_id: "request-test".to_string(),
            operation: "spawn".to_string(),
            message: message.to_string(),
            diagnostics,
        });
        response
    }

    fn attach_state_response(session_id: &str, state: &str) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::Events);
        response.events = vec![DaemonEvent::AttachState {
            session_id: session_id.to_string(),
            subscription_id: "sub-test".to_string(),
            state: state.to_string(),
        }];
        response
    }

    fn base_response(kind: DaemonResponseKind) -> DaemonResponse {
        DaemonResponse {
            kind,
            status: None,
            sessions: Vec::new(),
            packages: Vec::new(),
            package_decision: None,
            lifecycle: Vec::new(),
            plugin_tools: Vec::new(),
            plugin_tool_result: Value::Null,
            plugin_surface: None,
            plugin_action_result: None,
            events: Vec::new(),
            cleanup: None,
            coordination: None,
            error: None,
            diagnostics: Vec::new(),
        }
    }
}
