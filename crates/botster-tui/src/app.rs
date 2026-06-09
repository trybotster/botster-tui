use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    path::PathBuf,
    time::Duration,
};

use botster_core::ui::{UiActionRequest, UiChild, UiNode, UiNodeId, UiNodeKind};
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
use serde_json::{Map, Value, json};

use crate::renderer::{self, HitMap, InputDispatch, InputRouter};
use crate::socket_client::{ClientReadModel, HubSocketClient};

pub const SMOKE_MESSAGE: &str = "botster-tui smoke ok";
const DEFAULT_BRANCH: &str = "main";
const DEFAULT_TERMINAL_ROWS: u16 = 24;
const DEFAULT_TERMINAL_COLS: u16 = 80;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppArgs {
    pub smoke: bool,
    pub hub_socket: Option<PathBuf>,
}

impl AppArgs {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        let mut parsed = Self::default();
        let mut args = args.into_iter();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--smoke" => parsed.smoke = true,
                "--hub-socket" => {
                    if let Some(path) = args.next() {
                        parsed.hub_socket = Some(PathBuf::from(path));
                    }
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
    let mut terminal = setup_terminal()?;
    let mut app = DogfoodApp::new(args.hub_socket);
    let run_result = run_loop(&mut terminal, &mut app);
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

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut DogfoodApp,
) -> io::Result<()> {
    let mut router = InputRouter::new();
    loop {
        app.poll_hub();

        let mut hit_map = HitMap::default();
        terminal.draw(|frame| draw(frame, &mut hit_map, app))?;

        if event::poll(Duration::from_millis(250))? {
            let event = event::read()?;
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press && should_quit(key) => break,
                _ => {
                    let dispatch = router.dispatch_event(event, &hit_map);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionState {
    Disconnected,
    Connected,
}

#[derive(Debug)]
pub struct DogfoodApp {
    hub_socket: Option<PathBuf>,
    connection_state: ConnectionState,
    client: Option<HubSocketClient>,
    read_model: ClientReadModel,
    branch: String,
    prompt: String,
    validation_error: Option<String>,
    action_error: Option<String>,
    pending_request_id: Option<String>,
    request_seq: u64,
}

impl DogfoodApp {
    pub fn new(hub_socket: Option<PathBuf>) -> Self {
        let mut app = Self {
            hub_socket,
            connection_state: ConnectionState::Disconnected,
            client: None,
            read_model: ClientReadModel::default(),
            branch: DEFAULT_BRANCH.to_string(),
            prompt: String::new(),
            validation_error: None,
            action_error: None,
            pending_request_id: None,
            request_seq: 0,
        };
        app.try_connect();
        app
    }

    #[cfg(test)]
    fn disconnected() -> Self {
        Self {
            hub_socket: None,
            connection_state: ConnectionState::Disconnected,
            client: None,
            read_model: ClientReadModel::default(),
            branch: DEFAULT_BRANCH.to_string(),
            prompt: String::new(),
            validation_error: None,
            action_error: None,
            pending_request_id: None,
            request_seq: 0,
        }
    }

    pub fn surface(&self) -> UiNode {
        let mut root = node(
            UiNodeKind::Stack,
            "dogfood-root",
            json!({ "direction": "vertical" }),
        );
        root.children = vec![
            child(status_panel(self)),
            child(spawn_form(self)),
            child(session_list(self)),
            child(terminal_panel(self)),
        ];
        root.validate()
            .expect("dogfood UiNode should satisfy the core UI contract");
        renderer::tui_capabilities()
            .validate_node(&root)
            .expect("dogfood UiNode should fit TUI renderer capabilities");
        root
    }

    pub fn handle_dispatch(&mut self, dispatch: InputDispatch) {
        match dispatch {
            InputDispatch::Action(request) => self.handle_action(request),
            InputDispatch::TerminalForward { bytes, .. } => self.forward_terminal(bytes),
            InputDispatch::TerminalResize { rows, cols, .. } => self.resize_terminal(rows, cols),
            InputDispatch::Hover { .. }
            | InputDispatch::Focus { .. }
            | InputDispatch::Scroll { .. }
            | InputDispatch::Ignored => {}
        }
    }

    pub fn poll_hub(&mut self) {
        let Some(client) = &mut self.client else {
            return;
        };

        match client.read_available() {
            Ok(frames) => {
                for frame in frames {
                    if let Err(error) = self.read_model.apply_frame(frame) {
                        self.action_error = Some(error.to_string());
                    }
                }
                if let Some(error) = self.read_model.last_command_error.take() {
                    self.pending_request_id = None;
                    self.action_error = Some(error);
                }
                if self.read_model.attached_session_uuid.is_some() {
                    self.pending_request_id = None;
                }
            }
            Err(error) => {
                self.connection_state = ConnectionState::Disconnected;
                self.client = None;
                self.action_error = Some(format!("hub disconnected: {error}"));
            }
        }
    }

    fn handle_action(&mut self, request: UiActionRequest) {
        match request.action_id.0.as_str() {
            "botster.session.spawn" => self.submit_spawn(request.values.map(|values| values.0)),
            "botster.form.reset" => self.reset_form(),
            "botster.terminal.focus" => self.attach_selected_terminal(),
            _ => {}
        }
    }

    fn submit_spawn(&mut self, values: Option<Map<String, Value>>) {
        if let Some(values) = values {
            if let Some(branch) = values.get("branch").and_then(Value::as_str) {
                self.branch = branch.to_string();
            }
            if let Some(prompt) = values.get("prompt").and_then(Value::as_str) {
                self.prompt = prompt.to_string();
            }
        }

        self.validation_error = validate_spawn(&self.branch, &self.prompt).err();
        if self.validation_error.is_some() {
            return;
        }

        let Some(client) = &mut self.client else {
            self.action_error =
                Some("connect to a local hub before spawning a session".to_string());
            return;
        };

        self.request_seq += 1;
        let request_id = format!("tui-spawn-{}", self.request_seq);
        match client.send_create_agent(&request_id, &self.branch, &self.prompt) {
            Ok(()) => {
                self.pending_request_id = Some(request_id);
                self.action_error = None;
            }
            Err(error) => self.action_error = Some(format!("spawn request failed: {error}")),
        }
    }

    fn reset_form(&mut self) {
        self.branch = DEFAULT_BRANCH.to_string();
        self.prompt.clear();
        self.validation_error = None;
        self.action_error = None;
    }

    fn attach_selected_terminal(&mut self) {
        let Some(session_uuid) = self.selected_session_uuid() else {
            self.action_error = Some("no session is available to attach".to_string());
            return;
        };
        let Some(client) = &mut self.client else {
            self.action_error =
                Some("connect to a local hub before attaching terminal".to_string());
            return;
        };

        match client.subscribe_terminal(&session_uuid, DEFAULT_TERMINAL_ROWS, DEFAULT_TERMINAL_COLS)
        {
            Ok(()) => {
                self.read_model.attached_session_uuid = Some(session_uuid);
                self.action_error = None;
            }
            Err(error) => self.action_error = Some(format!("terminal attach failed: {error}")),
        }
    }

    fn forward_terminal(&mut self, bytes: Vec<u8>) {
        let Some(session_uuid) = self.read_model.attached_session_uuid.clone() else {
            self.action_error =
                Some("terminal input ignored until a session is attached".to_string());
            return;
        };
        if let Some(client) = &mut self.client
            && let Err(error) = client.send_terminal_input(&session_uuid, bytes)
        {
            self.action_error = Some(format!("terminal input failed: {error}"));
        }
    }

    fn resize_terminal(&mut self, rows: u16, cols: u16) {
        let Some(session_uuid) = self.read_model.attached_session_uuid.clone() else {
            return;
        };
        if let Some(client) = &mut self.client
            && let Err(error) = client.send_resize(&session_uuid, rows, cols)
        {
            self.action_error = Some(format!("terminal resize failed: {error}"));
        }
    }

    fn try_connect(&mut self) {
        let Some(path) = self.hub_socket.as_deref() else {
            return;
        };

        match HubSocketClient::connect(path) {
            Ok(client) => {
                self.client = Some(client);
                self.connection_state = ConnectionState::Connected;
                self.action_error = None;
            }
            Err(error) => {
                self.connection_state = ConnectionState::Disconnected;
                self.action_error = Some(format!("hub socket unavailable: {error}"));
            }
        }
    }

    fn selected_session_uuid(&self) -> Option<String> {
        self.read_model.attached_session_uuid.clone().or_else(|| {
            self.read_model.sessions().into_iter().find_map(|session| {
                session
                    .get("session_uuid")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
            })
        })
    }
}

fn validate_spawn(branch: &str, prompt: &str) -> Result<(), String> {
    if branch.trim().is_empty() {
        return Err("Branch or issue is required.".to_string());
    }
    if prompt.trim().is_empty() {
        return Err("Prompt is required before spawning a session.".to_string());
    }
    Ok(())
}

fn status_panel(app: &DogfoodApp) -> UiNode {
    let connection_label = match app.connection_state {
        ConnectionState::Connected => "Connected",
        ConnectionState::Disconnected => "Disconnected",
    };
    let mut panel = node(
        UiNodeKind::Panel,
        "dogfood-status",
        json!({ "title": "Local hub" }),
    );
    panel.children = vec![
        child(node(
            UiNodeKind::Badge,
            "dogfood-connection-badge",
            json!({
                "label": connection_label,
                "tone": if app.connection_state == ConnectionState::Connected { "success" } else { "warning" },
            }),
        )),
        child(node(
            UiNodeKind::Text,
            "dogfood-socket-path",
            json!({ "text": hub_socket_label(app) }),
        )),
        child(node(
            UiNodeKind::Text,
            "dogfood-error",
            json!({ "text": app.action_error.clone().unwrap_or_default() }),
        )),
    ];
    panel
}

fn spawn_form(app: &DogfoodApp) -> UiNode {
    let mut form = node(
        UiNodeKind::Panel,
        "dogfood-spawn-panel",
        json!({ "title": "Spawn session" }),
    );
    form.children = vec![
        child(node(
            UiNodeKind::TextInput,
            "dogfood-branch",
            json!({
                "name": "branch",
                "label": "Branch or issue",
                "value": app.branch,
            }),
        )),
        child(node(
            UiNodeKind::TextInput,
            "dogfood-prompt",
            json!({
                "name": "prompt",
                "label": "Prompt",
                "value": app.prompt,
                "error": app.validation_error,
            }),
        )),
        child(node(
            UiNodeKind::Button,
            "dogfood-submit",
            json!({
                "label": if app.pending_request_id.is_some() { "Spawning" } else { "Spawn and attach" },
                "action": { "id": "botster.session.spawn" },
            }),
        )),
        child(node(
            UiNodeKind::Button,
            "dogfood-reset",
            json!({
                "label": "Reset",
                "action": { "id": "botster.form.reset" },
            }),
        )),
    ];
    form
}

fn session_list(app: &DogfoodApp) -> UiNode {
    let sessions = app.read_model.sessions();
    if sessions.is_empty() {
        return node(
            UiNodeKind::EmptyState,
            "dogfood-empty-sessions",
            json!({
                "title": "No session entity snapshots yet",
                "description": "Connect to a local hub; the TUI requests hub:entities after subscribe.",
            }),
        );
    }

    let mut panel = node(
        UiNodeKind::Panel,
        "dogfood-sessions-panel",
        json!({ "title": "Sessions" }),
    );
    let mut list = node(
        UiNodeKind::List,
        "dogfood-session-list",
        json!({ "aria_label": "Sessions" }),
    );
    list.children = sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            let session_uuid = session
                .get("session_uuid")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("session-{index}"));
            let title = session
                .get("display_name")
                .or_else(|| session.get("label"))
                .or_else(|| session.get("name"))
                .and_then(Value::as_str)
                .unwrap_or(session_uuid.as_str());
            let mut item = node(
                UiNodeKind::ListItem,
                &format!("dogfood-session-{session_uuid}"),
                json!({ "value": session }),
            );
            item.slots.insert(
                "title".to_string(),
                vec![child(node(
                    UiNodeKind::Text,
                    &format!("dogfood-session-title-{session_uuid}"),
                    json!({ "text": title }),
                ))],
            );
            child(item)
        })
        .collect();
    panel.children = vec![
        child(list),
        child(node(
            UiNodeKind::Button,
            "dogfood-attach-first-session",
            json!({
                "label": "Attach first session",
                "action": { "id": "botster.terminal.focus" },
            }),
        )),
    ];
    panel
}

fn terminal_panel(app: &DogfoodApp) -> UiNode {
    match app.read_model.attached_session_uuid.as_deref() {
        Some(session_uuid) => {
            let mut panel = node(
                UiNodeKind::Panel,
                "dogfood-terminal-panel",
                json!({ "title": "Attached terminal" }),
            );
            panel.children = vec![
                child(node(
                    UiNodeKind::TerminalView,
                    "dogfood-terminal",
                    json!({
                        "session_id": session_uuid,
                        "title": "Attached terminal",
                    }),
                )),
                child(node(
                    UiNodeKind::Text,
                    "dogfood-terminal-output",
                    json!({ "text": terminal_content(app) }),
                )),
            ];
            panel
        }
        None => node(
            UiNodeKind::EmptyState,
            "dogfood-terminal-empty",
            json!({
                "title": "Terminal unavailable",
                "description": "Spawn or select a session to attach terminal output.",
            }),
        ),
    }
}

fn terminal_content(app: &DogfoodApp) -> String {
    if app.read_model.terminal_output.is_empty() {
        "waiting for terminal snapshot or output".to_string()
    } else {
        app.read_model.terminal_output.clone()
    }
}

fn hub_socket_label(app: &DogfoodApp) -> String {
    app.hub_socket
        .as_ref()
        .map(|path| format!("socket: {}", path.display()))
        .unwrap_or_else(|| "set --hub-socket or BOTSTER_HUB_SOCKET".to_string())
}

fn node(kind: UiNodeKind, id: &str, props: Value) -> UiNode {
    UiNode {
        kind,
        id: Some(UiNodeId(id.to_string())),
        props: props.as_object().cloned().unwrap_or_else(Map::new),
        children: Vec::new(),
        slots: BTreeMap::new(),
    }
}

fn child(node: UiNode) -> UiChild {
    UiChild::Node(Box::new(node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{HitRole, render_to_lines};
    use crate::socket_client::SocketFrame;

    #[test]
    fn smoke_message_names_the_scaffold() {
        assert_eq!(smoke_message(), "botster-tui smoke ok");
    }

    #[test]
    fn args_parse_socket_flag_and_env_fallback() {
        let args = AppArgs::parse([
            "--hub-socket".to_string(),
            "/tmp/botster.sock".to_string(),
            "--smoke".to_string(),
        ]);
        assert!(args.smoke);
        assert_eq!(args.hub_socket, Some(PathBuf::from("/tmp/botster.sock")));
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
    fn production_surface_is_dogfood_session_spawn_not_demo_fixture() {
        let app = DogfoodApp::disconnected();
        let (lines, _hit_map) = render_to_lines(&app.surface(), 100, 24);
        let frame = lines.join("\n");

        assert!(frame.contains("Spawn session"));
        assert!(frame.contains("Branch or issue"));
        assert!(frame.contains("Terminal unavailable"));
        assert!(!frame.contains("Core UiNode renderer scaffold"));
    }

    #[test]
    fn invalid_spawn_prompt_sets_validation_error_without_hub() {
        let mut app = DogfoodApp::disconnected();
        app.submit_spawn(Some(Map::from_iter([
            ("branch".to_string(), Value::String("main".to_string())),
            ("prompt".to_string(), Value::String(String::new())),
        ])));

        assert_eq!(
            app.validation_error.as_deref(),
            Some("Prompt is required before spawning a session.")
        );
        assert!(app.pending_request_id.is_none());
    }

    #[test]
    fn entity_snapshot_surface_renders_sessions_and_terminal_output() {
        let mut app = DogfoodApp::disconnected();
        app.read_model
            .apply_frame(SocketFrame::Json(json!({
                "type": "entity_snapshot",
                "entity_type": "session",
                "snapshot_seq": 1,
                "items": [{
                    "session_uuid": "sess-dogfood",
                    "display_name": "Dogfood session",
                    "status": "running"
                }]
            })))
            .unwrap();
        app.read_model
            .apply_frame(SocketFrame::PtyOutput {
                session_uuid: "sess-dogfood".to_string(),
                data: b"hello from hub\r\n".to_vec(),
            })
            .unwrap();

        let (lines, hit_map) = render_to_lines(&app.surface(), 100, 24);
        let frame = lines.join("\n");

        assert!(frame.contains("Dogfood session"));
        assert!(frame.contains("hello from hub"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.role == HitRole::TerminalView
                    && region.node_id == "dogfood-terminal")
        );
    }

    #[test]
    fn terminal_dispatch_without_attachment_reports_error() {
        let mut app = DogfoodApp::disconnected();
        app.handle_dispatch(InputDispatch::TerminalForward {
            node_id: "dogfood-terminal".to_string(),
            bytes: b"x".to_vec(),
        });

        assert_eq!(
            app.action_error.as_deref(),
            Some("terminal input ignored until a session is attached")
        );
    }
}
