use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use botster_core::ui::{UiChild, UiFormValues, UiNode, UiNodeId, UiNodeKind};
use botster_hub_client::{
    DaemonConnection, DaemonEndpoint, DaemonEvent, DaemonRequest, DaemonResponse,
    DaemonResponseKind, DaemonTransportError, DaemonTransportResult,
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
    client: Option<DaemonConnection>,
    status: String,
    error: Option<String>,
    sessions: Vec<String>,
    selected_session: Option<String>,
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
            error: None,
            sessions: Vec::new(),
            selected_session: None,
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

        let Some(session_id) = self.selected_session.clone() else {
            return;
        };
        match self.request(DaemonRequest::Drain { session_id }) {
            Ok(response) => self.apply_response(response),
            Err(error) => self.disconnect(error),
        }
    }

    fn handle_dispatch(&mut self, dispatch: InputDispatch) {
        match dispatch {
            InputDispatch::Action(request) => {
                self.handle_action(request.action_id.0, request.values, request.payload);
            }
            InputDispatch::TerminalForward { bytes, .. } => {
                let Some(session_id) = self.selected_session.clone() else {
                    self.error = Some("attach a session before sending terminal input".to_string());
                    return;
                };
                match String::from_utf8(bytes) {
                    Ok(data) => {
                        self.request_and_apply(DaemonRequest::SendInput { session_id, data })
                    }
                    Err(error) => {
                        self.error = Some(format!("terminal input was not UTF-8: {error}"))
                    }
                }
            }
            InputDispatch::TerminalResize { rows, cols, .. } => {
                if let Some(session_id) = self.selected_session.clone() {
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
            "botster.tui.refresh" => self.refresh_sessions(),
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
            self.status = "configure --hub-socket or BOTSTER_HUB_SOCKET".to_string();
            return;
        };
        match DaemonConnection::connect(endpoint) {
            Ok(client) => {
                self.client = Some(client);
                self.status = "connected".to_string();
                self.error = None;
                self.restore_after_connect();
            }
            Err(error) => {
                self.client = None;
                self.status = "reconnecting".to_string();
                self.error = Some(error.to_string());
            }
        }
    }

    fn restore_after_connect(&mut self) {
        self.refresh_sessions();
        if self.selected_session.is_some() {
            self.attach_selected_or_first();
        }
    }

    fn refresh_sessions(&mut self) {
        self.request_and_apply(DaemonRequest::ListSessions);
    }

    fn spawn_session(&mut self) {
        if self.command.trim().is_empty() {
            self.error = Some("command is required".to_string());
            return;
        }
        let session_id = format!("btui-{}", short_suffix());
        let command = self.command.clone();
        match self.request(DaemonRequest::Spawn {
            session_id: session_id.clone(),
            command,
        }) {
            Ok(response) => self.apply_response(response),
            Err(error) => {
                self.disconnect(error);
                return;
            }
        }
        self.selected_session = Some(session_id.clone());
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
        self.selected_session = Some(session_id.clone());
        self.request_and_apply(DaemonRequest::Attach {
            session_id,
            subscription_id: self.subscription_id.clone(),
        });
    }

    fn request_and_apply(&mut self, request: DaemonRequest) {
        #[cfg(test)]
        self.record_request(&request);
        match self.request(request) {
            Ok(response) => self.apply_response(response),
            Err(error) => self.disconnect(error),
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
            DaemonRequest::ListSessions => {
                self.observed_requests.push(ObservedRequest::ListSessions)
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

    fn disconnect(&mut self, error: DaemonTransportError) {
        self.client = None;
        self.status = "reconnecting".to_string();
        self.error = Some(error.to_string());
    }

    fn apply_response(&mut self, response: DaemonResponse) {
        if let Some(error) = response.error {
            self.error = Some(error.message);
            return;
        }

        self.error = None;
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
                }
                DaemonEvent::AttachState { state, .. } => {
                    self.status = format!("attach {state}");
                }
                _ => {}
            }
        }
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
        if let Some(error) = &self.error {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        panel.children = children;
        panel
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
                        json!({ "text": session_id }),
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
        panel.children = vec![child(list)];
        panel
    }

    fn terminal_panel(&self) -> UiNode {
        let mut terminal = node(
            UiNodeKind::TerminalView,
            "dogfood-terminal",
            json!({
                "title": "terminal",
                "session_id": self.selected_session.clone().unwrap_or_else(|| "not attached".to_string())
            }),
        );
        terminal.children = vec![child(node(
            UiNodeKind::Text,
            "dogfood-terminal-output",
            json!({ "text": self.terminal_output }),
        ))];
        terminal
    }
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum ObservedRequest {
    ListSessions,
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
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 100, 24);
        let rendered = lines.join("\n");
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

        let (lines, _) = renderer::render_to_lines(&app.surface(), 100, 24);
        assert!(lines.join("\n").contains("printf draft"));
    }

    #[test]
    fn blank_command_validation_renders_visible_error_state() {
        let mut app = DogfoodApp::new(None);
        app.command = " \t\n".to_string();

        app.spawn_session();

        assert_eq!(app.error.as_deref(), Some("command is required"));
        let (lines, _) = renderer::render_to_lines(&app.surface(), 100, 24);
        assert!(lines.join("\n").contains("error: command is required"));
    }

    #[test]
    fn terminal_view_carries_output_bytes() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output = "hello terminal".to_string();

        let (lines, _) = renderer::render_to_lines(&app.surface(), 100, 24);
        assert!(lines.join("\n").contains("hello terminal"));
    }

    #[test]
    fn terminal_output_renders_as_terminal_primitive_content() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output = "primitive terminal bytes".to_string();

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 100, 24);

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
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 100, 24);
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
    fn reconnect_restore_pulls_session_read_model_before_reattaching_selected_session() {
        let mut app = DogfoodApp::new(None);
        app.observed_requests.clear();
        app.sessions = vec!["session-alpha".to_string()];
        app.selected_session = Some("session-alpha".to_string());
        let subscription_id = app.subscription_id.clone();

        app.restore_after_connect();

        assert_eq!(
            app.observed_requests,
            vec![
                ObservedRequest::ListSessions,
                ObservedRequest::Attach {
                    session_id: "session-alpha".to_string(),
                    subscription_id,
                },
            ]
        );
    }

    #[test]
    fn tui_hub_boundary_uses_public_client_without_private_protocol_plumbing() {
        let source = source_without_line_comments();

        assert!(source.contains("use botster_hub_client"));
        for required in [
            "DaemonConnection",
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
            concat!("Unix", "Stream"),
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
        DaemonResponse {
            kind: DaemonResponseKind::Sessions,
            status: None,
            sessions: session_ids
                .into_iter()
                .map(|session_id| botster_hub_client::DaemonSession {
                    session_id: session_id.to_string(),
                    lifecycle: "running".to_string(),
                })
                .collect(),
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
        }
    }
}
