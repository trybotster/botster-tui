use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    path::PathBuf,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use botster_core::ui::{UiChild, UiFormValues, UiNode, UiNodeId, UiNodeKind};
use botster_hub_client::{
    DaemonApp, DaemonAvailablePackage, DaemonCaptureSnapshot, DaemonCompatibility,
    DaemonCompatibilityRequirement, DaemonDiagnostic, DaemonDiagnosticKind, DaemonEndpoint,
    DaemonEvent, DaemonPackage, DaemonPackageAvailabilityReason, DaemonPackageAvailabilityState,
    DaemonPackageInstallPlan, DaemonPackageNavigationEntry, DaemonPackagePin,
    DaemonPackageRouteDescriptor, DaemonPackageUpdateStatus, DaemonPluginSurface, DaemonRequest,
    DaemonResponse, DaemonResponseKind, DaemonTransportError, DaemonTransportResult,
    FEATURE_PACKAGE_NAVIGATION, FEATURE_RESIZE, FEATURE_SESSIONS, FEATURE_TERMINAL_READBACK,
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

const PACKAGE_CONFIG_FIELD_PREFIX: &str = "package-config";
const DEFAULT_COMMAND: &str = "printf 'botster-tui-ready\\n'; while IFS= read -r line; do printf 'echo:%s\\n' \"$line\"; done";
const HEADLESS_INPUT: &str = "botster-tui-headless\n";
const HEADLESS_OUTPUT: &str = "echo:botster-tui-headless";
const SMOKE_MESSAGE: &str = "botster-tui smoke ok";
const ATTACH_HYDRATION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppArgs {
    pub smoke: bool,
    pub hub_socket: Option<PathBuf>,
    pub hub_data_dir: Option<PathBuf>,
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
                "--data-dir" => {
                    parsed.hub_data_dir = iter.next().map(PathBuf::from);
                }
                _ => {}
            }
        }
        if parsed.hub_socket.is_none() {
            parsed.hub_socket = std::env::var_os("BOTSTER_HUB_SOCKET").map(PathBuf::from);
        }
        if parsed.hub_data_dir.is_none() {
            parsed.hub_data_dir = std::env::var_os("BOTSTER_HUB_DATA_DIR").map(PathBuf::from);
        }
        if std::env::var_os("BOTSTER_TUI_HEADLESS_DOGFOOD").is_some() {
            parsed.headless_dogfood = true;
        }
        parsed
    }
}

pub fn smoke_message() -> &'static str {
    SMOKE_MESSAGE
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionRow {
    session_id: String,
    lifecycle: String,
}

#[derive(Clone, Debug)]
struct AttachHydration {
    session_id: String,
    subscription_id: String,
    deadline: Instant,
}

#[derive(Default)]
struct HydrationEvidence {
    history_received: bool,
    lifecycle_ended: bool,
}

impl SessionRow {
    fn running(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            lifecycle: "running".to_string(),
        }
    }

    fn is_attachable(&self) -> bool {
        self.lifecycle == "running"
    }
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
    let mut router = InputRouter::new(renderer::action_request_context());
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
    apps: Vec<DaemonApp>,
    package_navigation: Vec<DaemonPackageNavigationEntry>,
    packages: Vec<DaemonPackage>,
    available_packages: Vec<DaemonAvailablePackage>,
    install_plan: Option<DaemonPackageInstallPlan>,
    update_status: Option<DaemonPackageUpdateStatus>,
    package_decision: Option<botster_hub_client::DaemonPackageDecision>,
    plugin_surface: Option<DaemonPluginSurface>,
    plugin_action_result: Option<Value>,
    sessions: Vec<SessionRow>,
    selected_session: Option<String>,
    attached_session: Option<String>,
    schema_version: Option<u16>,
    subscription_id: String,
    terminal_output: String,
    terminal_output_session_id: Option<String>,
    read_screen_fallback: Option<String>,
    snapshot_metadata: Option<DaemonCaptureSnapshot>,
    attach_hydration: Option<AttachHydration>,
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
            apps: Vec::new(),
            package_navigation: Vec::new(),
            packages: Vec::new(),
            available_packages: Vec::new(),
            install_plan: None,
            update_status: None,
            package_decision: None,
            plugin_surface: None,
            plugin_action_result: None,
            sessions: Vec::new(),
            selected_session: None,
            attached_session: None,
            schema_version: None,
            subscription_id: format!("btui-sub-{}", short_suffix()),
            terminal_output: String::new(),
            terminal_output_session_id: None,
            read_screen_fallback: None,
            snapshot_metadata: None,
            attach_hydration: None,
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
            .any(|candidate| candidate.session_id == session_id)
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
            .attach_hydration
            .as_ref()
            .map(|hydration| hydration.session_id.clone())
            .or_else(|| self.attached_session.clone())
            .or_else(|| self.selected_attachable_session_id_for_poll())
        else {
            return;
        };
        let request = DaemonRequest::Drain { session_id };
        #[cfg(test)]
        self.record_request(&request);
        match self.request(request) {
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
        if let Some(values) = values.as_ref()
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
            "botster.tui.navigation.open" => {
                if let Some((package_name, surface_id, route_id)) =
                    navigation_open_payload(&payload)
                {
                    self.open_package_navigation(package_name, surface_id, route_id);
                }
            }
            "botster.tui.package_config.submit" => {
                if let Some(package_name) = payload
                    .as_ref()
                    .and_then(|value| value.get("package_name"))
                    .and_then(Value::as_str)
                {
                    self.submit_package_configuration(package_name, values.as_ref());
                }
            }
            "botster.tui.package.enable" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("enable requested: {package_name}"));
                    self.request_and_apply(DaemonRequest::EnablePackage { package_name });
                }
            }
            "botster.tui.package.disable" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("disable requested: {package_name}"));
                    self.request_and_apply(DaemonRequest::DisablePackage { package_name });
                }
            }
            "botster.tui.package.remove" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("remove requested: {package_name}"));
                    self.request_and_apply(DaemonRequest::RemovePackage { package_name });
                }
            }
            "botster.tui.package.update_status" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("update status requested: {package_name}"));
                    self.request_and_apply(DaemonRequest::CheckPackageUpdate { package_name });
                }
            }
            "botster.tui.package.update_preview" => {
                if let Some((package_name, pin)) = package_name_and_pin_from_payload(&payload) {
                    self.action_feedback =
                        Some(format!("update preview requested: {package_name}"));
                    self.request_and_apply(DaemonRequest::PreviewPackageUpdate {
                        package_name,
                        pin,
                    });
                }
            }
            "botster.tui.package.update_apply" => {
                if let Some((package_name, pin)) = package_name_and_pin_from_payload(&payload) {
                    self.action_feedback = Some(format!("update apply requested: {package_name}"));
                    self.request_and_apply(DaemonRequest::ApplyPackageUpdate { package_name, pin });
                }
            }
            "botster.tui.entrypoint.start" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint start requested: {package_name}/{entrypoint_id}"
                    ));
                    self.request_and_apply(DaemonRequest::StartPackageEntrypoint {
                        package_name,
                        entrypoint_id,
                        environment_overrides: BTreeMap::new(),
                    });
                }
            }
            "botster.tui.entrypoint.stop" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint stop requested: {package_name}/{entrypoint_id}"
                    ));
                    self.request_and_apply(DaemonRequest::StopPackageEntrypoint {
                        package_name,
                        entrypoint_id,
                    });
                }
            }
            "botster.tui.entrypoint.restart" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint restart requested: {package_name}/{entrypoint_id}"
                    ));
                    self.request_and_apply(DaemonRequest::RestartPackageEntrypoint {
                        package_name,
                        entrypoint_id,
                    });
                }
            }
            "botster.tui.entrypoint.status" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint status requested: {package_name}/{entrypoint_id}"
                    ));
                    self.request_and_apply(DaemonRequest::PackageEntrypointStatus {
                        package_name,
                        entrypoint_id,
                    });
                }
            }
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
        self.attach_hydration = None;
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
        self.refresh_apps();
        self.refresh_package_navigation();
        self.refresh_packages();
    }

    fn refresh_status(&mut self) {
        self.request_and_apply(DaemonRequest::Status);
    }

    fn refresh_sessions(&mut self) {
        self.request_and_apply(DaemonRequest::ListSessions);
    }

    fn refresh_apps(&mut self) {
        self.request_and_apply(DaemonRequest::ListApps);
    }

    fn refresh_package_navigation(&mut self) {
        self.request_and_apply(DaemonRequest::ListPackageNavigation);
    }

    fn refresh_packages(&mut self) {
        self.request_and_apply(DaemonRequest::ListPackages);
    }

    fn open_package_navigation(
        &mut self,
        package_name: String,
        surface_id: String,
        route_id: String,
    ) {
        self.error = None;
        self.action_feedback = Some(format!(
            "navigation open requested: {package_name} {route_id}"
        ));
        self.request_and_apply(DaemonRequest::PluginSurfaceRender {
            package_name,
            surface_id,
            payload: json!({}),
        });
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
        if !self
            .sessions
            .iter()
            .any(|session| session.session_id == session_id)
        {
            self.sessions.push(SessionRow::running(session_id.clone()));
        }
        self.action_feedback = Some(format!("spawned {session_id}; attach requested"));
        self.attach_selected_or_first();
    }

    fn attach_selected_or_first(&mut self) {
        let Some(session_id) = self.selected_attachable_session_id() else {
            return;
        };
        self.error = None;
        self.selected_session = Some(session_id.clone());
        self.action_feedback = Some(format!("attach requested: {session_id}"));
        self.begin_attach_hydration(&session_id);
        self.request_and_apply(DaemonRequest::Attach {
            session_id,
            subscription_id: self.subscription_id.clone(),
        });
    }

    fn begin_attach_hydration(&mut self, session_id: &str) {
        // Every Attach creates a new subscription and replays full history, so
        // preserving the previous presentation would duplicate it on reconnect.
        self.terminal_output.clear();
        self.read_screen_fallback = None;
        self.snapshot_metadata = None;
        self.terminal_output_session_id = Some(session_id.to_string());
        self.attach_hydration = Some(AttachHydration {
            session_id: session_id.to_string(),
            subscription_id: self.subscription_id.clone(),
            deadline: Instant::now() + ATTACH_HYDRATION_TIMEOUT,
        });
    }

    fn selected_attachable_session_id(&mut self) -> Option<String> {
        let Some(session_id) = self.selected_session.clone().or_else(|| {
            self.sessions
                .first()
                .map(|session| session.session_id.clone())
        }) else {
            self.error = Some("no session available to attach".to_string());
            return None;
        };
        self.selected_session = Some(session_id.clone());

        let Some(session) = self
            .sessions
            .iter()
            .find(|candidate| candidate.session_id == session_id)
        else {
            self.error = Some(format!("{session_id} is not listed - cannot attach"));
            return None;
        };

        if session.is_attachable() {
            return Some(session_id);
        }

        self.error = Some(format!(
            "{} {} - cannot attach",
            session.session_id, session.lifecycle
        ));
        None
    }

    fn selected_attachable_session_id_for_poll(&self) -> Option<String> {
        let session_id = self.selected_session.as_ref()?;
        self.sessions
            .iter()
            .find(|session| session.session_id == *session_id && session.is_attachable())
            .map(|session| session.session_id.clone())
    }

    fn detach_attached(&mut self) {
        let Some(session_id) = self.attached_session.clone() else {
            self.error = Some("no attached terminal stream to detach".to_string());
            return;
        };
        self.error = None;
        self.action_feedback = Some(format!("detach requested: {session_id}"));
        self.attach_hydration = None;
        self.request_and_apply(DaemonRequest::Detach {
            session_id,
            subscription_id: self.subscription_id.clone(),
        });
    }

    fn submit_package_configuration(&mut self, package_name: &str, values: Option<&UiFormValues>) {
        let Some(values) = values else {
            self.error = Some("configuration form values were not submitted".to_string());
            return;
        };
        let Some(package) = self
            .packages
            .iter()
            .find(|package| package.package_name == package_name)
        else {
            self.error = Some(format!("package not found: {package_name}"));
            return;
        };

        let mut updates = BTreeMap::new();
        for field in package_configuration_fields(package) {
            let field_name = package_config_field_name(package_name, &field.key);
            let Some(draft) = values.0.get(&field_name) else {
                continue;
            };
            if let Some(value) = package_configuration_submit_value(&field, draft) {
                updates.insert(field.key, value);
            }
        }

        if updates.is_empty() {
            self.error = Some(format!("no configuration changes for {package_name}"));
            return;
        }

        self.error = None;
        self.action_feedback = Some(format!("configuration update requested: {package_name}"));
        self.request_and_apply(DaemonRequest::SetPackageConfiguration {
            package_name: package_name.to_string(),
            values: updates,
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
            DaemonRequest::ListApps => self.observed_requests.push(ObservedRequest::ListApps),
            DaemonRequest::ListPackageNavigation => self
                .observed_requests
                .push(ObservedRequest::ListPackageNavigation),
            DaemonRequest::ListPackages => {
                self.observed_requests.push(ObservedRequest::ListPackages)
            }
            DaemonRequest::SetPackageConfiguration {
                package_name,
                values,
            } => self
                .observed_requests
                .push(ObservedRequest::SetPackageConfiguration {
                    package_name: package_name.clone(),
                    values: values.clone(),
                }),
            DaemonRequest::EnablePackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::EnablePackage(package_name.clone())),
            DaemonRequest::DisablePackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::DisablePackage(package_name.clone())),
            DaemonRequest::RemovePackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::RemovePackage(package_name.clone())),
            DaemonRequest::CheckPackageUpdate { package_name } => self
                .observed_requests
                .push(ObservedRequest::CheckPackageUpdate(package_name.clone())),
            DaemonRequest::PreviewPackageUpdate { package_name, pin } => self
                .observed_requests
                .push(ObservedRequest::PreviewPackageUpdate {
                    package_name: package_name.clone(),
                    pin: pin.clone(),
                }),
            DaemonRequest::ApplyPackageUpdate { package_name, pin } => {
                self.observed_requests
                    .push(ObservedRequest::ApplyPackageUpdate {
                        package_name: package_name.clone(),
                        pin: pin.clone(),
                    })
            }
            DaemonRequest::StartPackageEntrypoint {
                package_name,
                entrypoint_id,
                ..
            } => self
                .observed_requests
                .push(ObservedRequest::StartPackageEntrypoint {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::StopPackageEntrypoint {
                package_name,
                entrypoint_id,
            } => self
                .observed_requests
                .push(ObservedRequest::StopPackageEntrypoint {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::RestartPackageEntrypoint {
                package_name,
                entrypoint_id,
            } => self
                .observed_requests
                .push(ObservedRequest::RestartPackageEntrypoint {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::PackageEntrypointStatus {
                package_name,
                entrypoint_id,
            } => self
                .observed_requests
                .push(ObservedRequest::PackageEntrypointStatus {
                    package_name: package_name.clone(),
                    entrypoint_id: entrypoint_id.clone(),
                }),
            DaemonRequest::PluginSurfaceRender {
                package_name,
                surface_id,
                ..
            } => self
                .observed_requests
                .push(ObservedRequest::PluginSurfaceRender {
                    package_name: package_name.clone(),
                    surface_id: surface_id.clone(),
                }),
            DaemonRequest::Attach {
                session_id,
                subscription_id,
            } => self.observed_requests.push(ObservedRequest::Attach {
                session_id: session_id.clone(),
                subscription_id: subscription_id.clone(),
            }),
            DaemonRequest::Drain { session_id } => self
                .observed_requests
                .push(ObservedRequest::Drain(session_id.clone())),
            DaemonRequest::ReadScreen { session_id } => self
                .observed_requests
                .push(ObservedRequest::ReadScreen(session_id.clone())),
            DaemonRequest::CaptureSnapshot { session_id } => self
                .observed_requests
                .push(ObservedRequest::CaptureSnapshot(session_id.clone())),
            _ => {}
        }
    }

    fn record_transport_error(&mut self, error: DaemonTransportError) {
        self.client = None;
        self.attached_session = None;
        self.attach_hydration = None;
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
        let evidence = self.apply_response_state(response);
        if evidence.lifecycle_ended {
            self.attach_hydration = None;
            return;
        }
        if evidence.history_received {
            self.complete_attach_hydration(false);
            return;
        }
        if self
            .attach_hydration
            .as_ref()
            .is_some_and(|hydration| Instant::now() >= hydration.deadline)
        {
            self.complete_attach_hydration(true);
        }
    }

    fn apply_response_state(&mut self, response: DaemonResponse) -> HydrationEvidence {
        let mut hydration_evidence = HydrationEvidence::default();
        self.record_diagnostics(response.diagnostics);

        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
            self.error = Some(format!(
                "{} (code={} operation={})",
                error.message, error.code, error.operation
            ));
            return hydration_evidence;
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
                .map(|session| SessionRow {
                    session_id: session.session_id,
                    lifecycle: session.lifecycle,
                })
                .collect();
            if self.selected_session.as_ref().is_none_or(|selected| {
                !self
                    .sessions
                    .iter()
                    .any(|session| session.session_id == *selected)
            }) {
                self.selected_session = self
                    .sessions
                    .first()
                    .map(|session| session.session_id.clone());
            }
        }

        if matches!(
            response.kind,
            DaemonResponseKind::Packages | DaemonResponseKind::PackageDecision
        ) {
            self.packages = response.packages;
        }
        if matches!(response.kind, DaemonResponseKind::Apps) {
            self.apps = response.apps;
        }
        if matches!(response.kind, DaemonResponseKind::PackageNavigation) {
            self.package_navigation = response.package_navigation;
        }
        if matches!(response.kind, DaemonResponseKind::AvailablePackages) {
            self.available_packages = response.available_packages;
        }
        if matches!(response.kind, DaemonResponseKind::PackageInstallPlan) {
            self.install_plan = response.install_plan;
        }
        if matches!(response.kind, DaemonResponseKind::PackageUpdateStatus) {
            self.update_status = response.update_status;
        }
        if matches!(response.kind, DaemonResponseKind::PackageDecision) {
            self.package_decision = response.package_decision;
        }
        if matches!(response.kind, DaemonResponseKind::PluginSurface) {
            self.plugin_surface = response.plugin_surface;
        }
        if matches!(response.kind, DaemonResponseKind::PluginActionResult) {
            self.plugin_action_result = response.plugin_action_result;
        }

        for event in response.events {
            match event {
                DaemonEvent::TerminalOutput { data, .. } => {
                    self.append_terminal_output(&data);
                }
                DaemonEvent::Snapshot {
                    session_id,
                    subscription_id,
                    data,
                    ..
                }
                | DaemonEvent::Scrollback {
                    session_id,
                    subscription_id,
                    data,
                    ..
                } => {
                    if !data.is_empty() && self.hydration_matches(&session_id, &subscription_id) {
                        hydration_evidence.history_received = true;
                    }
                    self.append_terminal_output(&data);
                }
                DaemonEvent::ProcessExit {
                    session_id,
                    subscription_id,
                    code,
                } => {
                    self.status = format!("process exited {}", code.unwrap_or_default());
                    self.attached_session = None;
                    self.clear_snapshot_metadata_for(&session_id);
                    if self.hydration_matches(&session_id, &subscription_id) {
                        hydration_evidence.lifecycle_ended = true;
                    }
                }
                DaemonEvent::AttachState {
                    session_id,
                    subscription_id,
                    state,
                } => {
                    self.action_feedback = Some(format!("attach {state}: {session_id}"));
                    if state == "attached" {
                        self.attached_session = Some(session_id);
                    } else if state == "detached" {
                        self.attached_session = None;
                        self.clear_snapshot_metadata_for(&session_id);
                        if self.hydration_matches(&session_id, &subscription_id) {
                            hydration_evidence.lifecycle_ended = true;
                        }
                    }
                }
                _ => {}
            }
        }
        hydration_evidence
    }

    fn hydration_matches(&self, session_id: &str, subscription_id: &str) -> bool {
        self.attach_hydration.as_ref().is_some_and(|hydration| {
            hydration.session_id == session_id && hydration.subscription_id == subscription_id
        })
    }

    fn clear_snapshot_metadata_for(&mut self, session_id: &str) {
        if self.terminal_output_session_id.as_deref() == Some(session_id) {
            self.snapshot_metadata = None;
        }
    }

    fn complete_attach_hydration(&mut self, deadline_expired: bool) {
        let Some(hydration) = self.attach_hydration.take() else {
            return;
        };
        if deadline_expired
            && self.terminal_output.is_empty()
            && self
                .read_screen_fallback
                .as_deref()
                .is_none_or(str::is_empty)
        {
            self.request_optional_readback(
                DaemonRequest::ReadScreen {
                    session_id: hydration.session_id.clone(),
                },
                "read_screen",
            );
        }
        self.request_optional_readback(
            DaemonRequest::CaptureSnapshot {
                session_id: hydration.session_id,
            },
            "capture_snapshot",
        );
    }

    fn request_optional_readback(&mut self, request: DaemonRequest, operation: &str) {
        if self.client.is_none() {
            return;
        }
        #[cfg(test)]
        self.record_request(&request);
        match self.request(request) {
            Ok(response) => self.apply_optional_readback_response(response, operation),
            Err(error) => {
                self.action_feedback = Some(format!("{operation} unavailable: {error}"));
                self.record_transport_error(error);
            }
        }
    }

    fn apply_optional_readback_response(&mut self, response: DaemonResponse, operation: &str) {
        self.record_diagnostics(response.diagnostics);
        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
            self.action_feedback = Some(format!("{operation} unavailable: {}", error.message));
            return;
        }
        match response.kind {
            DaemonResponseKind::ReadScreen => {
                if self.terminal_output.is_empty()
                    && let Some(screen) = response.read_screen
                    && !screen.text.is_empty()
                    && self.terminal_output_session_id.as_deref()
                        == Some(screen.session_id.as_str())
                {
                    self.read_screen_fallback = Some(screen.text);
                }
            }
            DaemonResponseKind::CaptureSnapshot => {
                if let Some(snapshot) = response.capture_snapshot
                    && self.terminal_output_session_id.as_deref()
                        == Some(snapshot.session_id.as_str())
                {
                    self.snapshot_metadata = Some(snapshot);
                }
            }
            _ => {}
        }
    }

    fn append_terminal_output(&mut self, data: &str) {
        if data.is_empty() {
            return;
        }
        self.read_screen_fallback = None;
        self.terminal_output.push_str(data);
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
        children.extend(self.package_navigation_nodes().into_iter().map(child));
        children.extend(self.app_nodes().into_iter().map(child));
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
                children.extend(
                    package_availability_nodes(package, index)
                        .into_iter()
                        .map(child),
                );
                children.extend(package_action_nodes(package, index).into_iter().map(child));
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
                    children.extend(
                        entrypoint_action_nodes(package, index, entrypoint, entrypoint_index)
                            .into_iter()
                            .map(child),
                    );
                }
                children.extend(
                    self.package_configuration_nodes(package, index)
                        .into_iter()
                        .map(child),
                );
            }
        }
        if !self.available_packages.is_empty() {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-marketplace-summary",
                json!({ "text": format!("marketplace: {} available", self.available_packages.len()) }),
            )));
            for (index, available_package) in self.available_packages.iter().enumerate() {
                children.push(child(node(
                    UiNodeKind::Text,
                    &format!("dogfood-available-package-{index}"),
                    json!({ "text": format!("available package: {}", available_package_text(available_package)) }),
                )));
            }
        }
        if let Some(install_plan) = &self.install_plan {
            children.extend(
                install_plan_nodes(install_plan)
                    .into_iter()
                    .enumerate()
                    .map(|(index, mut node)| {
                        node.id = Some(UiNodeId(format!("dogfood-install-plan-{index}")));
                        child(node)
                    }),
            );
        }
        if let Some(update_status) = &self.update_status {
            children.extend(
                update_status_nodes(update_status)
                    .into_iter()
                    .enumerate()
                    .map(|(index, mut node)| {
                        node.id = Some(UiNodeId(format!("dogfood-update-status-{index}")));
                        child(node)
                    }),
            );
        }
        if let Some(decision) = &self.package_decision {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-package-decision",
                json!({
                    "text": format!(
                        "package decision: package={} action={} state={} classification={}",
                        decision.package_name,
                        decision.action,
                        decision.state,
                        decision.classification
                    )
                }),
            )));
        }
        if let Some(surface) = &self.plugin_surface {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-plugin-surface",
                json!({ "text": format!("plugin surface: {}", plugin_surface_text(surface)) }),
            )));
            children.extend(plugin_surface_nodes(surface).into_iter().map(child));
        }
        if let Some(result) = &self.plugin_action_result {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-plugin-action-result",
                json!({ "text": format!("plugin action result: {}", plugin_action_result_text(result)) }),
            )));
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
        if let Some(snapshot) = &self.snapshot_metadata {
            children.push(child(node(
                UiNodeKind::Text,
                "dogfood-terminal-snapshot-metadata",
                json!({
                    "text": format!(
                        "terminal snapshot: session={} rows={} cols={} format={} payload_bytes={}",
                        snapshot.session_id,
                        snapshot.rows,
                        snapshot.cols,
                        snapshot.payload_format.as_deref().unwrap_or("none"),
                        snapshot.payload_bytes
                    )
                }),
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

    fn app_nodes(&self) -> Vec<UiNode> {
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "dogfood-app-summary",
            json!({ "text": format!("apps: {} installed", self.apps.len()) }),
        )];
        if self.apps.is_empty() {
            nodes.push(node(
                UiNodeKind::Text,
                "dogfood-app-empty",
                json!({ "text": "apps: none reported" }),
            ));
            return nodes;
        }

        for (app_index, app) in self.apps.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("dogfood-app-{app_index}"),
                json!({ "text": format!("app: {}", app_text(app)) }),
            ));
            nodes.push(node(
                UiNodeKind::Text,
                &format!("dogfood-app-{app_index}-launch-target"),
                json!({ "text": format!("launch target: {}", app_launch_target_text(app)) }),
            ));
            for (reason_index, reason) in app.blocked_reasons.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("dogfood-app-{app_index}-blocked-{reason_index}"),
                    json!({ "text": format!("app blocked: {reason}") }),
                ));
            }
            for (diagnostic_index, diagnostic) in app.diagnostics.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("dogfood-app-{app_index}-diagnostic-{diagnostic_index}"),
                    json!({ "text": format!("app diagnostic: {}", package_diagnostic_text(diagnostic)) }),
                ));
            }
            nodes.extend(
                action_state_nodes(
                    &app.actions,
                    "app action",
                    &format!("dogfood-app-{app_index}"),
                )
                .into_iter(),
            );
            if let Some(route) = &app.route {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("dogfood-app-{app_index}-route"),
                    json!({ "text": format!("app route: {}", route_text(route)) }),
                ));
            }
        }
        nodes
    }

    fn package_navigation_nodes(&self) -> Vec<UiNode> {
        if self.package_navigation.is_empty() {
            return Vec::new();
        }

        let mut nodes = vec![node(
            UiNodeKind::Text,
            "dogfood-package-navigation-summary",
            json!({ "text": format!("navigation: {} admitted entries", self.package_navigation.len()) }),
        )];

        for (index, entry) in self.package_navigation.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("dogfood-package-navigation-{index}"),
                json!({ "text": format!("navigation entry: {}", navigation_entry_text(entry)) }),
            ));
            for (diagnostic_index, diagnostic) in entry.diagnostics.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("dogfood-package-navigation-{index}-diagnostic-{diagnostic_index}"),
                    json!({ "text": format!("navigation diagnostic: {}", package_diagnostic_text(diagnostic)) }),
                ));
            }
            match navigation_open_payload_for_entry(entry) {
                Some(payload) if entry.enabled && !entry.blocked => {
                    nodes.push(button(
                        &format!("dogfood-package-navigation-{index}-open"),
                        "Open",
                        "botster.tui.navigation.open",
                        payload,
                    ));
                }
                Some(_) => nodes.push(node(
                    UiNodeKind::Text,
                    &format!("dogfood-package-navigation-{index}-blocked"),
                    json!({ "text": format!("navigation blocked: {}", navigation_blocked_text(entry)) }),
                )),
                None => nodes.push(node(
                    UiNodeKind::Text,
                    &format!("dogfood-package-navigation-{index}-unsupported"),
                    json!({ "text": format!("navigation unsupported: {}", navigation_unsupported_text(entry)) }),
                )),
            }
        }
        nodes
    }

    fn package_configuration_nodes(&self, package: &DaemonPackage, index: usize) -> Vec<UiNode> {
        let fields = package_configuration_fields(package);
        if fields.is_empty() && package.configuration.schema.is_none() {
            return Vec::new();
        }

        let mut nodes = vec![node(
            UiNodeKind::Text,
            &format!("dogfood-package-{index}-configuration-summary"),
            json!({
                "text": format!(
                    "configuration: schema={} values={} missing={} diagnostics={}",
                    if package.configuration.schema.is_some() { "yes" } else { "no" },
                    package.configuration.effective_values.len(),
                    package.configuration.missing_required.len(),
                    package.configuration.diagnostics.len()
                )
            }),
        )];

        for missing in &package.configuration.missing_required {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("dogfood-package-{index}-configuration-missing-{missing}"),
                json!({ "text": format!("configuration missing: {missing}") }),
            ));
        }

        for (diagnostic_index, diagnostic) in package.configuration.diagnostics.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("dogfood-package-{index}-configuration-diagnostic-{diagnostic_index}"),
                json!({
                    "text": format!(
                        "configuration diagnostic: {}",
                        package_configuration_diagnostic_text(diagnostic)
                    )
                }),
            ));
        }

        for field in fields {
            nodes.push(self.package_configuration_field_node(package, index, &field));
        }

        if !nodes.is_empty() {
            nodes.push(button(
                &format!("dogfood-package-{index}-configuration-submit"),
                "Update configuration",
                "botster.tui.package_config.submit",
                json!({ "package_name": package.package_name }),
            ));
        }

        nodes
    }

    fn package_configuration_field_node(
        &self,
        package: &DaemonPackage,
        index: usize,
        field: &PackageConfigurationField,
    ) -> UiNode {
        let field_name = package_config_field_name(&package.package_name, &field.key);
        let draft = self.drafts.get(&field_name);
        let effective = package.configuration.effective_values.get(&field.key);
        let error = package_configuration_field_error(package, &field.key);
        let mut props = json!({
            "name": field_name,
            "label": package_configuration_field_label(field),
        });
        if let Some(error) = error {
            props["error"] = Value::String(error);
        }

        match field.field_type.as_str() {
            "boolean" => {
                props["checked"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::Bool(configuration_value_bool(effective)));
                node(
                    UiNodeKind::Checkbox,
                    &format!("dogfood-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "select" => {
                props["selected"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                let mut select = node(
                    UiNodeKind::Select,
                    &format!("dogfood-package-{index}-configuration-{}", field.key),
                    props,
                );
                select.slots.insert(
                    "options".to_string(),
                    field
                        .options
                        .iter()
                        .enumerate()
                        .map(|(option_index, option)| {
                            child(node(
                                UiNodeKind::SelectOption,
                                &format!(
                                    "dogfood-package-{index}-configuration-{}-option-{option_index}",
                                    field.key
                                ),
                                json!({ "value": option.value, "label": option.label }),
                            ))
                        })
                        .collect(),
                );
                select
            }
            "multiline_text" => {
                props["value"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                node(
                    UiNodeKind::Textarea,
                    &format!("dogfood-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "secret" => {
                props["checked"] = draft.cloned().unwrap_or(Value::Bool(false));
                let state = configuration_secret_state(effective);
                props["label"] = Value::String(format!(
                    "{} secret ({state}; Space marks write-only update)",
                    field.label
                ));
                node(
                    UiNodeKind::Checkbox,
                    &format!("dogfood-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "string" | "path" | "url" => {
                props["value"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                node(
                    UiNodeKind::TextInput,
                    &format!("dogfood-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            other => node(
                UiNodeKind::Text,
                &format!("dogfood-package-{index}-configuration-{}", field.key),
                json!({
                    "text": format!(
                        "{}: unsupported configuration type {}",
                        package_configuration_field_label(field),
                        other
                    )
                }),
            ),
        }
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
            .map(|session| {
                let session_id = &session.session_id;
                let mut title = format!("{session_id} [{}]", session.lifecycle);
                if self.attached_session.as_deref() == Some(session_id.as_str()) {
                    title.push_str(" (attached)");
                } else if self.selected_session.as_deref() == Some(session_id.as_str()) {
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
        if let Some(fallback) = &self.read_screen_fallback {
            return fallback.clone();
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
    ListApps,
    ListPackageNavigation,
    ListPackages,
    SetPackageConfiguration {
        package_name: String,
        values: BTreeMap<String, Value>,
    },
    EnablePackage(String),
    DisablePackage(String),
    RemovePackage(String),
    CheckPackageUpdate(String),
    PreviewPackageUpdate {
        package_name: String,
        pin: DaemonPackagePin,
    },
    ApplyPackageUpdate {
        package_name: String,
        pin: DaemonPackagePin,
    },
    StartPackageEntrypoint {
        package_name: String,
        entrypoint_id: String,
    },
    StopPackageEntrypoint {
        package_name: String,
        entrypoint_id: String,
    },
    RestartPackageEntrypoint {
        package_name: String,
        entrypoint_id: String,
    },
    PackageEntrypointStatus {
        package_name: String,
        entrypoint_id: String,
    },
    PluginSurfaceRender {
        package_name: String,
        surface_id: String,
    },
    Attach {
        session_id: String,
        subscription_id: String,
    },
    Drain(String),
    ReadScreen(String),
    CaptureSnapshot(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PackageConfigurationField {
    key: String,
    field_type: String,
    label: String,
    required: bool,
    order: Option<i64>,
    options: Vec<PackageConfigurationOption>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PackageConfigurationOption {
    value: String,
    label: String,
}

fn package_configuration_fields(package: &DaemonPackage) -> Vec<PackageConfigurationField> {
    let Some(schema) = &package.configuration.schema else {
        return Vec::new();
    };
    let Some(fields) = schema.get("fields").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut parsed = fields
        .iter()
        .filter_map(package_configuration_field)
        .collect::<Vec<_>>();
    parsed.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then_with(|| left.key.cmp(&right.key))
    });
    parsed
}

fn package_configuration_field(value: &Value) -> Option<PackageConfigurationField> {
    let key = value.get("key").and_then(Value::as_str)?.to_string();
    let field_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unsupported")
        .to_string();
    let label = value
        .get("label")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| key.clone());
    let required = value
        .get("required")
        .and_then(Value::as_bool)
        .unwrap_or_default();
    let order = value.get("order").and_then(Value::as_i64);
    let options = value
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| {
            let value = option.get("value").and_then(Value::as_str)?.to_string();
            let label = option
                .get("label")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| value.clone());
            Some(PackageConfigurationOption { value, label })
        })
        .collect();

    Some(PackageConfigurationField {
        key,
        field_type,
        label,
        required,
        order,
        options,
    })
}

fn package_config_field_name(package_name: &str, key: &str) -> String {
    format!("{PACKAGE_CONFIG_FIELD_PREFIX}:{package_name}:{key}")
}

fn package_configuration_field_label(field: &PackageConfigurationField) -> String {
    if field.required {
        format!("{} *", field.label)
    } else {
        field.label.clone()
    }
}

fn package_configuration_field_error(package: &DaemonPackage, key: &str) -> Option<String> {
    if package
        .configuration
        .missing_required
        .iter()
        .any(|missing| missing == key)
    {
        return Some("required configuration value is missing".to_string());
    }
    None
}

fn package_configuration_diagnostic_text(
    diagnostic: &botster_hub_client::DaemonPackageDiagnostic,
) -> String {
    format!("{}:{}", diagnostic.kind, diagnostic.message)
}

fn package_configuration_submit_value(
    field: &PackageConfigurationField,
    draft: &Value,
) -> Option<Value> {
    match field.field_type.as_str() {
        "boolean" => Some(json!({
            "type": "boolean",
            "value": draft.as_bool().unwrap_or_default()
        })),
        "select" => Some(json!({
            "type": "select",
            "value": draft.as_str().unwrap_or_default()
        })),
        "multiline_text" => Some(json!({
            "type": "multiline_text",
            "value": draft.as_str().unwrap_or_default()
        })),
        "secret" => draft.as_bool().unwrap_or_default().then(|| {
            json!({
                "type": "secret",
                "state": "write_only"
            })
        }),
        "string" | "path" | "url" => Some(json!({
            "type": field.field_type,
            "value": draft.as_str().unwrap_or_default()
        })),
        _ => None,
    }
}

fn configuration_value_text(value: Option<&Value>) -> String {
    value
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn configuration_value_bool(value: Option<&Value>) -> bool {
    value
        .and_then(|value| value.get("value"))
        .and_then(Value::as_bool)
        .unwrap_or_default()
}

fn configuration_secret_state(value: Option<&Value>) -> &'static str {
    match value
        .and_then(|value| value.get("state"))
        .and_then(Value::as_str)
    {
        Some("redacted") => "redacted",
        Some("write_only") => "write-only",
        _ => "unset",
    }
}

fn run_headless_dogfood(args: AppArgs) -> DaemonTransportResult<()> {
    let Some(socket) = args.hub_socket else {
        return Err(DaemonTransportError::NotRunning);
    };
    if let Some(data_dir) = args.hub_data_dir.as_ref() {
        if !data_dir.is_dir() {
            return Err(DaemonTransportError::Protocol(
                "injected hub data dir is not a directory",
            ));
        }
        println!("hub-data-dir: configured");
    }
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
        for required_feature in [
            FEATURE_SESSIONS,
            FEATURE_TERMINAL_STREAMING,
            FEATURE_RESIZE,
            FEATURE_PACKAGE_NAVIGATION,
            FEATURE_TERMINAL_READBACK,
        ] {
            assert!(
                compatibility
                    .features
                    .iter()
                    .any(|feature| feature == required_feature)
            );
        }
        assert!(rendered.contains("features "));
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
            FEATURE_PACKAGE_NAVIGATION.to_string(),
            FEATURE_TERMINAL_READBACK.to_string(),
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
        DaemonDiagnosticKind::Backpressure => "backpressure",
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

fn package_diagnostic_text(diagnostic: &botster_hub_client::DaemonPackageDiagnostic) -> String {
    format!("{}:{}", diagnostic.kind, diagnostic.message)
}

fn package_name_from_payload(payload: &Option<Value>) -> Option<String> {
    payload
        .as_ref()
        .and_then(|value| value.get("package_name"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn package_entrypoint_from_payload(payload: &Option<Value>) -> Option<(String, String)> {
    let value = payload.as_ref()?;
    let package_name = value.get("package_name")?.as_str()?.to_string();
    let entrypoint_id = value.get("entrypoint_id")?.as_str()?.to_string();
    Some((package_name, entrypoint_id))
}

fn navigation_open_payload(payload: &Option<Value>) -> Option<(String, String, String)> {
    let value = payload.as_ref()?;
    let package_name = value.get("package_name")?.as_str()?.to_string();
    let surface_id = value.get("surface_id")?.as_str()?.to_string();
    let route_id = value.get("route_id")?.as_str()?.to_string();
    Some((package_name, surface_id, route_id))
}

fn package_name_and_pin_from_payload(
    payload: &Option<Value>,
) -> Option<(String, DaemonPackagePin)> {
    let value = payload.as_ref()?;
    let package_name = value.get("package_name")?.as_str()?.to_string();
    let pin = serde_json::from_value(value.get("pin")?.clone()).ok()?;
    Some((package_name, pin))
}

fn package_text(package: &DaemonPackage) -> String {
    format!(
        "{} {} classification={} state={} capabilities={} provider_profile_admitted={} availability={} surfaces={}",
        package.package_name,
        package.version,
        package.classification,
        package.state,
        capability_text(&package.requested_capabilities),
        package.provider_profile_admitted,
        availability_state_text(package.availability.state),
        package.surfaces.len()
    )
}

fn package_availability_nodes(package: &DaemonPackage, index: usize) -> Vec<UiNode> {
    let mut nodes = Vec::new();
    for (reason_index, reason) in package.availability.reasons.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("dogfood-package-{index}-availability-reason-{reason_index}"),
            json!({ "text": format!("package blocked: {}", availability_reason_text(reason)) }),
        ));
    }
    for (dependency_index, dependency) in package.dependency_availability.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("dogfood-package-{index}-dependency-{dependency_index}"),
            json!({
                "text": format!(
                    "dependency: id={} package={} state={}",
                    dependency.id,
                    dependency.package_name,
                    availability_state_text(dependency.state)
                )
            }),
        ));
        for (reason_index, reason) in dependency.reasons.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!(
                    "dogfood-package-{index}-dependency-{dependency_index}-reason-{reason_index}"
                ),
                json!({ "text": format!("dependency blocked: {}", availability_reason_text(reason)) }),
            ));
        }
    }
    for (feature_index, feature) in package.feature_availability.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("dogfood-package-{index}-feature-{feature_index}"),
            json!({
                "text": format!(
                    "feature: id={} state={}",
                    feature.id,
                    availability_state_text(feature.state)
                )
            }),
        ));
        for (reason_index, reason) in feature.reasons.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("dogfood-package-{index}-feature-{feature_index}-reason-{reason_index}"),
                json!({ "text": format!("feature blocked: {}", availability_reason_text(reason)) }),
            ));
        }
    }
    nodes
}

fn route_text(route: &DaemonPackageRouteDescriptor) -> String {
    let mut parts = vec![
        format!("package={}", route.package_name),
        format!("route_id={}", route.route_id),
        format!("path={}", route.route_path),
        format!("target={}", route.target.kind),
        format!("enabled={}", route.enabled),
        format!("blocked={}", route.blocked),
        format!("supports_settings={}", route.supports_settings),
    ];
    if let Some(surface_id) = &route.surface_id {
        parts.push(format!("surface_id={surface_id}"));
    }
    if let Some(target_surface_id) = &route.target.surface_id {
        parts.push(format!("target_surface_id={target_surface_id}"));
    }
    if let Some(app_id) = &route.app_id {
        parts.push(format!("app_id={app_id}"));
    }
    parts.join(" ")
}

fn package_action_nodes(package: &DaemonPackage, index: usize) -> Vec<UiNode> {
    vec![
        button(
            &format!("dogfood-package-{index}-enable"),
            "Enable",
            "botster.tui.package.enable",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("dogfood-package-{index}-disable"),
            "Disable",
            "botster.tui.package.disable",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("dogfood-package-{index}-remove"),
            "Remove",
            "botster.tui.package.remove",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("dogfood-package-{index}-update-status"),
            "Update status",
            "botster.tui.package.update_status",
            json!({ "package_name": package.package_name }),
        ),
    ]
}

fn entrypoint_action_nodes(
    package: &DaemonPackage,
    package_index: usize,
    entrypoint: &botster_hub_client::DaemonPackageRunnableEntrypoint,
    entrypoint_index: usize,
) -> Vec<UiNode> {
    let payload = json!({
        "package_name": package.package_name,
        "entrypoint_id": entrypoint.id,
    });
    vec![
        button(
            &format!("dogfood-package-{package_index}-entrypoint-{entrypoint_index}-start"),
            "Start",
            "botster.tui.entrypoint.start",
            payload.clone(),
        ),
        button(
            &format!("dogfood-package-{package_index}-entrypoint-{entrypoint_index}-stop"),
            "Stop",
            "botster.tui.entrypoint.stop",
            payload.clone(),
        ),
        button(
            &format!("dogfood-package-{package_index}-entrypoint-{entrypoint_index}-restart"),
            "Restart",
            "botster.tui.entrypoint.restart",
            payload.clone(),
        ),
        button(
            &format!("dogfood-package-{package_index}-entrypoint-{entrypoint_index}-status"),
            "Status",
            "botster.tui.entrypoint.status",
            payload,
        ),
    ]
}

fn available_package_text(package: &DaemonAvailablePackage) -> String {
    let mut parts = vec![
        format!("entry_id={}", package.entry_id),
        format!("package={}", package.package_name),
        format!("version={}", package.version),
        format!("classification={}", package.classification),
        format!("source_kind={}", package.source_kind),
        format!("source_label={}", package.source_label),
        format!("first_party={}", package.first_party),
        format!("state={}", package.state),
        format!(
            "capabilities={}",
            capability_text(&package.requested_capabilities)
        ),
        format!(
            "compatibility={}:{}",
            package.compatibility.result, package.compatibility.botster_requirement
        ),
    ];
    if !package.compatibility.diagnostics.is_empty() {
        parts.push(format!(
            "compatibility_diagnostics={}",
            package.compatibility.diagnostics.join(",")
        ));
    }
    if let Some(pin) = &package.pin {
        parts.push(format!("pin={}", pin_text(pin)));
    }
    parts.join(" ")
}

fn app_text(app: &DaemonApp) -> String {
    format!(
        "package={} app={} entrypoint={} kind={} launch_mode={} lifecycle={}",
        app.package_name,
        app.app_id,
        app.entrypoint_id,
        app.kind,
        app.launch_mode,
        app.lifecycle_state
    )
}

fn app_launch_target_text(app: &DaemonApp) -> String {
    let mut parts = vec![format!("kind={}", app.launch_target.kind)];
    match app.launch_target.local_url.as_deref() {
        Some(local_url) => {
            parts.push(format!("local_url={local_url}"));
            parts.push("open=copy URL or open it in a browser".to_string());
        }
        None if app.kind == "web_app" || app.launch_target.kind == "web_app" => {
            parts.push("local_url=unavailable".to_string());
            parts.push("open=blocked or not launched by hub".to_string());
        }
        None => {
            parts.push("local_url=not_applicable".to_string());
            parts.push("open=use hub-provided terminal app action when available".to_string());
        }
    }
    parts.join(" ")
}

fn navigation_entry_text(entry: &DaemonPackageNavigationEntry) -> String {
    let mut parts = vec![
        format!("package={}", entry.package_name),
        format!("item_id={}", entry.item_id),
        format!("label={}", entry.label),
        format!("route_id={}", entry.route_id),
        format!("path={}", entry.route_path),
        format!("target={}", entry.target.kind),
        format!("source={}", entry.source.kind),
        format!("enabled={}", entry.enabled),
        format!("blocked={}", entry.blocked),
    ];
    if let Some(description) = &entry.description {
        parts.push(format!("description={description}"));
    }
    if let Some(icon) = &entry.icon {
        parts.push(format!("icon={icon}"));
    }
    if let Some(surface_id) = &entry.target.surface_id {
        parts.push(format!("target_surface_id={surface_id}"));
    }
    if let Some(surface_id) = &entry.source.surface_id {
        parts.push(format!("source_surface_id={surface_id}"));
    }
    if let Some(entrypoint_id) = &entry.target.entrypoint_id {
        parts.push(format!("target_entrypoint_id={entrypoint_id}"));
    }
    if let Some(entrypoint_id) = &entry.source.entrypoint_id {
        parts.push(format!("source_entrypoint_id={entrypoint_id}"));
    }
    parts.join(" ")
}

fn navigation_open_payload_for_entry(entry: &DaemonPackageNavigationEntry) -> Option<Value> {
    if entry.target.kind != "plugin_surface" && entry.target.kind != "settings" {
        return None;
    }
    let surface_id = entry
        .target
        .surface_id
        .as_ref()
        .or(entry.source.surface_id.as_ref())?;
    Some(json!({
        "package_name": entry.package_name,
        "surface_id": surface_id,
        "route_id": entry.route_id,
    }))
}

fn navigation_blocked_text(entry: &DaemonPackageNavigationEntry) -> String {
    let mut parts = vec![
        format!("label={}", entry.label),
        format!("route_id={}", entry.route_id),
        format!("enabled={}", entry.enabled),
        format!("blocked={}", entry.blocked),
    ];
    if entry.diagnostics.is_empty() {
        parts.push("diagnostics=none".to_string());
    } else {
        parts.push(format!(
            "diagnostics={}",
            entry
                .diagnostics
                .iter()
                .map(package_diagnostic_text)
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    parts.join(" ")
}

fn navigation_unsupported_text(entry: &DaemonPackageNavigationEntry) -> String {
    let mut parts = vec![
        format!("label={}", entry.label),
        format!("route_id={}", entry.route_id),
        format!("target={}", entry.target.kind),
    ];
    if let Some(surface_id) = &entry.target.surface_id {
        parts.push(format!("target_surface_id={surface_id}"));
    }
    if let Some(entrypoint_id) = &entry.target.entrypoint_id {
        parts.push(format!("target_entrypoint_id={entrypoint_id}"));
    }
    parts.push("open=unsupported in botster-tui".to_string());
    parts.join(" ")
}

fn plugin_surface_text(surface: &DaemonPluginSurface) -> String {
    let body_id = surface
        .body
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let body_kind = surface
        .body
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("missing");
    format!(
        "package={} surface={} kind={} node_id={}",
        surface.package_name, surface.surface_id, body_kind, body_id
    )
}

fn plugin_surface_nodes(surface: &DaemonPluginSurface) -> Vec<UiNode> {
    if let Some(diagnostic) = iframe_unsupported_diagnostic(surface) {
        return vec![node(
            UiNodeKind::Text,
            "dogfood-plugin-surface-iframe-unsupported",
            json!({ "text": diagnostic }),
        )];
    }
    let root = match plugin_surface_body_node(surface) {
        Ok(root) => root,
        Err(error) => {
            return vec![node(
                UiNodeKind::Text,
                "dogfood-plugin-surface-invalid",
                json!({ "text": format!("plugin surface render: {error}") }),
            )];
        }
    };
    let (lines, _) = botster_tui_kit::render_to_lines(&root, 120, 20)
        .expect("validated plugin surface should render in test backend");
    vec![node(
        UiNodeKind::Text,
        "dogfood-plugin-surface-rendered",
        json!({ "text": format!("plugin surface render: {}", lines.join(" | ")) }),
    )]
}

fn plugin_surface_body_node(surface: &DaemonPluginSurface) -> Result<UiNode, String> {
    let node: UiNode = serde_json::from_value(surface.body.clone()).map_err(|error| {
        format!(
            "plugin surface {}:{} failed UiNode deserialize: {error}",
            surface.package_name, surface.surface_id
        )
    })?;
    node.validate().map_err(|error| {
        format!(
            "plugin surface {}:{} failed UiNode validate: {error}",
            surface.package_name, surface.surface_id
        )
    })?;
    renderer::tui_capabilities()
        .validate_node(&node)
        .map_err(|error| {
            format!(
                "plugin surface {}:{} unsupported TUI primitive: {error}",
                surface.package_name, surface.surface_id
            )
        })?;
    Ok(node)
}

fn iframe_unsupported_diagnostic(surface: &DaemonPluginSurface) -> Option<String> {
    let iframe = find_iframe_node(&surface.body)?;
    let title = iframe
        .get("props")
        .and_then(|props| props.get("title"))
        .and_then(Value::as_str)
        .unwrap_or("untitled");
    let src = iframe
        .get("props")
        .and_then(|props| props.get("src"))
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let sandbox = iframe
        .get("props")
        .and_then(|props| props.get("sandbox"))
        .map(compact_json)
        .unwrap_or_else(|| "default".to_string());
    Some(format!(
        "plugin surface iframe unsupported: package={} surface={} title={} src={} sandbox={} open=copy URL or open it in a browser",
        surface.package_name, surface.surface_id, title, src, sandbox
    ))
}

fn find_iframe_node(value: &Value) -> Option<&Value> {
    if value
        .get("type")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind == "iframe")
    {
        return Some(value);
    }
    value
        .get("children")
        .and_then(Value::as_array)
        .and_then(|children| children.iter().find_map(find_iframe_node))
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn plugin_action_result_text(result: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(state) = result.get("state").and_then(Value::as_str) {
        parts.push(format!("state={state}"));
    }
    if let Some(request_id) = result.get("request_id").and_then(Value::as_str) {
        parts.push(format!("request_id={request_id}"));
    }
    if let Some(message) = result
        .get("normalized_values")
        .and_then(|values| values.get("message"))
        .and_then(Value::as_str)
    {
        parts.push(format!("message={message}"));
    }
    if parts.is_empty() {
        "unstructured".to_string()
    } else {
        parts.join(" ")
    }
}

fn action_state_nodes(
    actions: &[botster_hub_client::DaemonPackageActionState],
    label: &str,
    id_prefix: &str,
) -> Vec<UiNode> {
    actions
        .iter()
        .enumerate()
        .map(|(action_index, action)| {
            node(
                UiNodeKind::Text,
                &format!("{id_prefix}-action-{action_index}"),
                json!({ "text": format!("{label}: {}", action_state_text(action)) }),
            )
        })
        .collect()
}

fn action_state_text(action: &botster_hub_client::DaemonPackageActionState) -> String {
    let mut parts = vec![
        format!("action_id={}", action.action_id),
        format!("status={}", action_status_text(action.status)),
    ];
    if let Some(reason) = &action.reason {
        parts.push(format!("reason={reason}"));
    }
    if !action.diagnostics.is_empty() {
        parts.push(format!(
            "diagnostics={}",
            action
                .diagnostics
                .iter()
                .map(package_diagnostic_text)
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if !action.required_references.is_empty() {
        parts.push(format!(
            "required_references={}",
            action
                .required_references
                .iter()
                .map(|reference| format!("{}:{}", reference.kind, reference.key))
                .collect::<Vec<_>>()
                .join(",")
        ));
    }
    if let Some(request) = &action.request {
        parts.push(format!("request={}", action_request_text(request)));
    }
    parts.join(" ")
}

fn action_status_text(status: botster_hub_client::DaemonPackageActionStatus) -> &'static str {
    match status {
        botster_hub_client::DaemonPackageActionStatus::Available => "available",
        botster_hub_client::DaemonPackageActionStatus::Blocked => "blocked",
        botster_hub_client::DaemonPackageActionStatus::Unavailable => "unavailable",
    }
}

fn action_request_text(request: &botster_hub_client::DaemonPackageActionRequest) -> String {
    let mut parts = vec![format!("type={}", request.request_type)];
    if let Some(package_name) = &request.package_name {
        parts.push(format!("package={package_name}"));
    }
    if let Some(entry_id) = &request.entry_id {
        parts.push(format!("entry_id={entry_id}"));
    }
    if let Some(entrypoint_id) = &request.entrypoint_id {
        parts.push(format!("entrypoint_id={entrypoint_id}"));
    }
    if let Some(pin) = &request.pin {
        parts.push(format!("pin={}", pin_text(pin)));
    }
    if request.registry_path.is_some() {
        parts.push("registry_path=provided".to_string());
    }
    parts.join(",")
}

fn install_plan_nodes(plan: &DaemonPackageInstallPlan) -> Vec<UiNode> {
    let mut nodes = vec![node(
        UiNodeKind::Text,
        "dogfood-install-plan-summary",
        json!({
            "text": format!(
                "install plan: package={} mutates_registry={} starts_entrypoints={} {}",
                plan.entry.package_name,
                plan.mutates_registry,
                plan.starts_entrypoints,
                available_package_text(&plan.entry)
            )
        }),
    )];
    for (index, effect) in plan.effects.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("dogfood-install-plan-effect-{index}"),
            json!({ "text": format!("install effect: {}:{}", effect.kind, effect.message) }),
        ));
    }
    for (index, diagnostic) in plan.diagnostics.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("dogfood-install-plan-diagnostic-{index}"),
            json!({ "text": format!("install diagnostic: {}", package_diagnostic_text(diagnostic)) }),
        ));
    }
    nodes
}

fn update_status_nodes(status: &DaemonPackageUpdateStatus) -> Vec<UiNode> {
    let mut text = format!(
        "update status: package={} update_available={} reload_required={} restart_required={}",
        status.package_name,
        status.update_available,
        status.reload_required,
        status.restart_required
    );
    if let Some(pin) = &status.pin {
        text.push_str(&format!(" pin={}", pin_text(pin)));
    }
    let mut nodes = vec![node(
        UiNodeKind::Text,
        "dogfood-update-status-summary",
        json!({ "text": text }),
    )];
    if let Some(pin) = &status.pin {
        nodes.push(button(
            "dogfood-update-status-preview",
            "Preview update",
            "botster.tui.package.update_preview",
            json!({ "package_name": status.package_name, "pin": pin }),
        ));
        nodes.push(button(
            "dogfood-update-status-apply",
            "Apply update",
            "botster.tui.package.update_apply",
            json!({ "package_name": status.package_name, "pin": pin }),
        ));
    }
    for (index, diagnostic) in status.diagnostics.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("dogfood-update-status-diagnostic-{index}"),
            json!({ "text": format!("update diagnostic: {}", package_diagnostic_text(diagnostic)) }),
        ));
    }
    nodes
}

fn availability_state_text(state: DaemonPackageAvailabilityState) -> &'static str {
    match state {
        DaemonPackageAvailabilityState::Available => "available",
        DaemonPackageAvailabilityState::Blocked => "blocked",
    }
}

fn availability_reason_text(reason: &DaemonPackageAvailabilityReason) -> String {
    let mut parts = vec![
        format!("reason={}", reason.reason),
        format!("action={}", reason.action),
    ];
    if let Some(package_name) = &reason.package_name {
        parts.push(format!("package={package_name}"));
    }
    if let Some(capability) = &reason.capability {
        parts.push(format!(
            "capability={}",
            capability_text(std::slice::from_ref(capability))
        ));
    }
    if let Some(requirement) = &reason.requirement {
        parts.push(format!("requirement={requirement}"));
    }
    parts.join(" ")
}

fn pin_text(pin: &DaemonPackagePin) -> String {
    let mut parts = vec![
        format!("revision={}", pin.revision),
        format!("update_policy={}", pin.update_policy),
    ];
    if let Some(branch) = &pin.branch {
        parts.push(format!("branch={branch}"));
    }
    if let Some(tag) = &pin.tag {
        parts.push(format!("tag={tag}"));
    }
    if let Some(rev) = &pin.rev {
        parts.push(format!("rev={rev}"));
    }
    if let Some(checksum) = &pin.checksum {
        parts.push(format!("checksum={checksum}"));
    }
    parts.join(",")
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
    use botster_core::{RequestId, UiActionId, UiActionKind, UiActionRequest, UiSurfaceId};

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
            "--data-dir".to_string(),
            "target/hub-data".to_string(),
            "--headless-dogfood".to_string(),
        ]);

        assert_eq!(args.hub_socket, Some(PathBuf::from("target/hub.sock")));
        assert_eq!(args.hub_data_dir, Some(PathBuf::from("target/hub-data")));
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
        assert!(
            rendered.contains("features sessions,terminal_streaming,resize,package_navigation")
        );
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

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 220);
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

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");

        assert!(rendered.contains(
            "package: local-alpha 0.1.0 classification=local state=enabled capabilities=none provider_profile_admitted=true"
        ));
        assert!(!rendered.contains("entrypoints="));
    }

    #[test]
    fn apps_response_updates_state_and_renders_web_app_launch_url_from_public_dto() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(apps_response(vec![web_app_with_url()]));

        assert_eq!(app.apps.len(), 1);
        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");

        assert!(rendered.contains("apps: 1 installed"));
        assert!(rendered.contains(
            "app: package=workflow.plugin app=dashboard entrypoint=web kind=web_app launch_mode=supervised lifecycle=running"
        ));
        assert!(rendered.contains("launch target: kind=web_app local_url=http://127.0.0.1:49152 open=copy URL or open it in a browser"));
    }

    #[test]
    fn apps_response_keeps_web_app_without_url_visible_without_deriving_one() {
        let mut app = DogfoodApp::new(None);
        let mut app_row = web_app_with_url();
        app_row.launch_target.local_url = None;
        app_row.lifecycle_state = "blocked".to_string();
        app_row.blocked_reasons = vec!["missing_config: port".to_string()];
        app_row.diagnostics = vec![package_diagnostic("blocked", "launch target unavailable")];

        app.apply_response(apps_response(vec![app_row]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains("kind=web_app local_url=unavailable"));
        assert!(rendered.contains("app blocked: missing_config: port"));
        assert!(rendered.contains("app diagnostic: blocked:launch target unavailable"));
        assert!(!rendered.contains("http://localhost"));
        assert!(!rendered.contains("http://127.0.0.1"));
    }

    #[test]
    fn terminal_app_renders_launchability_from_action_descriptors_without_fake_url() {
        let mut app = DogfoodApp::new(None);
        let mut app_row = terminal_app();
        app_row.actions = vec![action_state(
            "open",
            botster_hub_client::DaemonPackageActionStatus::Available,
            None,
            Some(action_request("start_entrypoint")),
        )];

        app.apply_response(apps_response(vec![app_row]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains(
            "app: package=botster-tui app=tui entrypoint=tui kind=terminal_app launch_mode=foreground_stdio lifecycle=launchable"
        ));
        assert!(rendered.contains("launch target: kind=terminal_app local_url=not_applicable open=use hub-provided terminal app action when available"));
        assert!(rendered.contains("app action: action_id=open status=available request=type=start_entrypoint,package=botster-tui,entrypoint_id=tui"));
        assert!(!rendered.contains("http://"));
    }

    #[test]
    fn package_navigation_renders_from_admitted_registry_not_package_routes() {
        let mut app = DogfoodApp::new(None);
        let route = plugin_contract_app_route();
        let mut package = package(
            "botster.plugin-contract-matrix",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        package.routes = vec![route.clone(), plugin_contract_settings_route()];
        let mut app_row = terminal_app();
        app_row.package_name = "botster.plugin-contract-matrix".to_string();
        app_row.app_id = "contract.app".to_string();
        app_row.entrypoint_id = "contract.app".to_string();
        app_row.kind = "plugin_surface".to_string();
        app_row.launch_mode = "host_route".to_string();
        app_row.route = Some(route);

        app.apply_response(packages_response(vec![package]));
        app.apply_response(apps_response(vec![app_row]));
        app.apply_response(package_navigation_response(vec![
            plugin_contract_app_navigation(),
        ]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 500, 180);
        let rendered = lines.join("\n");
        assert!(rendered.contains(
            "navigation entry: package=botster.plugin-contract-matrix item_id=contract.app label=Contract App route_id=surface:contract.app"
        ));
        assert!(
            rendered
                .contains("path=/packages/botster.plugin-contract-matrix/surfaces/contract.app")
        );
        assert!(rendered.contains("target=plugin_surface"));
        assert!(rendered.contains("target_surface_id=contract.app"));
        assert!(rendered.contains("source_surface_id=contract.app"));
        assert!(rendered.contains("Open"));
        assert!(rendered.contains("app route: package=botster.plugin-contract-matrix"));
        assert!(!rendered.contains("package route:"));
        assert!(!rendered.contains("route_id=settings"));
    }

    #[test]
    fn plugin_surface_and_action_results_render_from_public_dtos() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(plugin_surface_response(contract_app_plugin_surface()));
        app.apply_response(plugin_action_response(json!({
            "request_id": "contract-action-success",
            "surface_id": "contract.app",
            "action_id": "contract.action",
            "node_id": "contract-app-action",
            "state": "accepted",
            "normalized_values": {
                "message": "hello"
            }
        })));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 180);
        let rendered = lines.join("\n");
        assert!(rendered.contains(
            "plugin surface: package=botster.plugin-contract-matrix surface=contract.app kind=panel node_id=contract-app-panel"
        ));
        assert!(rendered.contains("plugin surface render:"));
        assert!(rendered.contains("UiNode payload delivered through plugin_surface_render."));
        assert!(rendered.contains(
            "plugin action result: state=accepted request_id=contract-action-success message=hello"
        ));
    }

    #[test]
    fn composite_application_primitives_render_through_tui_kit() {
        let surface = composite_application_primitives_plugin_surface();
        let node = plugin_surface_body_node(&surface).expect("composite surface validates for TUI");

        let (lines, hit_map) = renderer::render_to_lines(&node, 100, 36);
        let rendered = lines.join("\n");

        assert!(rendered.contains("Project Pipeline Overview"));
        assert!(rendered.contains("Active Runs: 3"));
        assert!(rendered.contains("status_badge: Healthy"));
        assert!(rendered.contains("table: Ticket | State"));
        assert!(rendered.contains("> 1783529012 | review"));
        assert!(rendered.contains("No blocked tickets"));
        assert!(rendered.contains("Reviewer"));
        assert!(rendered.contains("Notes"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "contract-composite-refresh")
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "contract-composite-ticket-a")
        );
    }

    #[test]
    fn composite_application_primitives_render_from_production_plugin_surface_path() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(plugin_surface_response(
            composite_application_primitives_plugin_surface(),
        ));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 420, 220);
        let rendered = lines.join("\n");

        assert!(rendered.contains(
            "plugin surface: package=botster.plugin-contract-matrix surface=contract.composite kind=section node_id=contract-composite-section"
        ));
        assert!(rendered.contains("Project Pipeline Overview"));
        assert!(rendered.contains("plugin surface render:"));
    }

    #[test]
    fn composite_table_mouse_selection_dispatches_exact_row_action() {
        let surface = composite_application_primitives_plugin_surface();
        let node = plugin_surface_body_node(&surface).expect("composite surface validates for TUI");
        let (_lines, hit_map) = renderer::render_to_lines(&node, 100, 36);
        let row = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "contract-composite-ticket-a")
            .expect("bordered composite table row should be hit-testable");
        let mut router = InputRouter::new(renderer::action_request_context());

        let dispatch = router.dispatch_event(
            Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: row.rect.x,
                row: row.rect.y,
                modifiers: KeyModifiers::NONE,
            }),
            &hit_map,
        );

        assert!(matches!(
            dispatch,
            InputDispatch::Action(request)
                if request.action_id == botster_core::ui::UiActionId("contract.ticket.open".to_string())
                    && request.node_id == Some(UiNodeId("contract-composite-ticket-a".to_string()))
                    && request.payload == Some(json!({ "ticket_id": "1783529012" }))
        ));
        assert_eq!(
            router
                .selected_row_value("contract-composite-ticket-table")
                .and_then(|value| value.get("id"))
                .and_then(Value::as_str),
            Some("contract-composite-ticket-a")
        );
    }

    #[test]
    fn plugin_surface_invalid_body_diagnostic_renders_from_app_surface() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(plugin_surface_response(invalid_table_plugin_surface()));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 180);
        let rendered = lines.join("\n");
        assert!(rendered.contains(
            "plugin surface: package=botster.plugin-contract-matrix surface=contract.invalid kind=table node_id=contract-invalid-table"
        ));
        assert!(rendered.contains(
            "plugin surface render: plugin surface botster.plugin-contract-matrix:contract.invalid failed UiNode validate"
        ));
        assert!(rendered.contains("contract-invalid-table"));
        assert!(rendered.contains("Table"));
        assert!(rendered.contains("table"));
        assert!(!rendered.contains("plugin surface render: invalid UiNode body"));
    }

    #[test]
    fn navigation_open_requests_public_plugin_surface_render() {
        let mut app = DogfoodApp::new(None);
        app.observed_requests.clear();
        let entry = plugin_contract_app_navigation();

        app.apply_response(package_navigation_response(vec![entry.clone()]));
        app.handle_dispatch(InputDispatch::Action(UiActionRequest {
            request_id: RequestId("req-navigation-open".to_string()),
            surface_id: UiSurfaceId(renderer::DOGFOOD_SURFACE_ID.to_string()),
            action_id: UiActionId("botster.tui.navigation.open".to_string()),
            node_id: Some(UiNodeId("dogfood-package-navigation-0-open".to_string())),
            kind: UiActionKind::Submit,
            values: None,
            payload: navigation_open_payload_for_entry(&entry),
        }));

        assert_eq!(
            app.observed_requests,
            vec![ObservedRequest::PluginSurfaceRender {
                package_name: "botster.plugin-contract-matrix".to_string(),
                surface_id: "contract.app".to_string(),
            }]
        );
        assert_eq!(
            app.action_feedback.as_deref(),
            Some("navigation open requested: botster.plugin-contract-matrix surface:contract.app")
        );
    }

    #[test]
    fn blocked_navigation_entry_stays_visible_without_open_affordance() {
        let mut app = DogfoodApp::new(None);
        let mut entry = plugin_contract_app_navigation();
        entry.enabled = false;
        entry.blocked = true;
        entry.diagnostics = vec![package_diagnostic("blocked", "missing configuration")];

        app.apply_response(package_navigation_response(vec![entry]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 160);
        let rendered = lines.join("\n");
        assert!(rendered.contains("navigation entry: package=botster.plugin-contract-matrix"));
        assert!(rendered.contains("enabled=false"));
        assert!(rendered.contains("blocked=true"));
        assert!(rendered.contains("navigation diagnostic: blocked:missing configuration"));
        assert!(rendered.contains("navigation blocked: label=Contract App"));
        assert!(!rendered.contains("dogfood-package-navigation-0-open"));
    }

    #[test]
    fn unsupported_navigation_target_stays_visible_with_precise_target() {
        let mut app = DogfoodApp::new(None);
        let mut entry = plugin_contract_app_navigation();
        entry.target.kind = "web_app".to_string();
        entry.target.surface_id = None;
        entry.target.entrypoint_id = Some("web".to_string());

        app.apply_response(package_navigation_response(vec![entry]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 160);
        let rendered = lines.join("\n");
        assert!(rendered.contains("navigation unsupported: label=Contract App"));
        assert!(rendered.contains("target=web_app"));
        assert!(rendered.contains("target_entrypoint_id=web"));
        assert!(rendered.contains("open=unsupported in botster-tui"));
    }

    #[test]
    fn iframe_plugin_surface_renders_precise_unsupported_diagnostic() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(plugin_surface_response(iframe_plugin_surface()));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 160);
        let rendered = lines.join("\n");
        assert!(rendered.contains("plugin surface iframe unsupported"));
        assert!(rendered.contains("package=botster.plugin-contract-matrix"));
        assert!(rendered.contains("surface=contract.iframe"));
        assert!(rendered.contains("title=Contract HTML"));
        assert!(rendered.contains("src=/assets/botster.plugin-contract-matrix/contract.html"));
        assert!(rendered.contains(r#"sandbox=["allow_scripts"]"#));
        assert!(rendered.contains("open=copy URL or open it in a browser"));
        assert!(!rendered.contains("failed UiNode deserialize"));
    }

    #[test]
    fn unsupported_uinode_primitive_reports_node_id_and_primitive() {
        let table = node(UiNodeKind::Table, "contract-unsupported-table", json!({}));

        let error = renderer::tui_capabilities()
            .validate_node(&table)
            .expect_err("table without fallback should fail TUI capability validation");
        let message = error.to_string();

        assert!(message.contains("contract-unsupported-table"));
        assert!(message.contains("Table"));
        assert!(message.contains("table"));
    }

    #[test]
    fn blocked_app_reasons_diagnostics_actions_and_request_mapping_are_visible_without_paths() {
        let mut app = DogfoodApp::new(None);
        let mut app_row = terminal_app();
        app_row.lifecycle_state = "blocked".to_string();
        app_row.blocked_reasons = vec![
            "missing_auth: github_token".to_string(),
            "disabled_package: botster-tui".to_string(),
        ];
        app_row.diagnostics = vec![package_diagnostic("warning", "terminal app is blocked")];
        let mut request = action_request("install_package");
        request.registry_path = Some("/redacted/catalog.json".to_string());
        request.entry_id = Some("botster-tui".to_string());
        app_row.actions = vec![action_state(
            "install",
            botster_hub_client::DaemonPackageActionStatus::Blocked,
            Some("missing auth"),
            Some(request),
        )];
        app_row.actions[0].diagnostics = vec![package_diagnostic("auth", "token missing")];
        app_row.actions[0].required_references =
            vec![botster_hub_client::DaemonPackageActionRequiredReference {
                kind: "auth".to_string(),
                key: "github_token".to_string(),
            }];

        app.apply_response(apps_response(vec![app_row]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 160);
        let rendered = lines.join("\n");
        assert!(rendered.contains("app blocked: missing_auth: github_token"));
        assert!(rendered.contains("app blocked: disabled_package: botster-tui"));
        assert!(rendered.contains("app diagnostic: warning:terminal app is blocked"));
        assert!(rendered.contains("app action: action_id=install status=blocked reason=missing auth diagnostics=auth:token missing required_references=auth:github_token request=type=install_package,package=botster-tui,entry_id=botster-tui,entrypoint_id=tui,registry_path=provided"));
        assert!(!rendered.contains("/redacted/catalog"));
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

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 180);
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

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 180);
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

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 180);
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

        let (lines, _) = renderer::render_to_lines(&app.surface(), 240, 120);
        let rendered = lines.join("\n");

        assert!(rendered.contains("id=web,kind=web,state=running"));
        assert!(rendered.contains("id=worker,kind=worker,state=starting"));
    }

    #[test]
    fn package_response_renders_hub_resolved_availability_gates_without_local_inference() {
        let mut app = DogfoodApp::new(None);
        let mut package = package(
            "workflow.plugin",
            "1.0.0",
            "plugin",
            "disabled",
            vec![capability("mcp", Some("tools"))],
            false,
        );
        package.availability = botster_hub_client::DaemonPackageAvailability {
            state: DaemonPackageAvailabilityState::Blocked,
            reasons: vec![
                availability_reason(
                    "missing_config",
                    "configure_package",
                    None,
                    None,
                    Some("endpoint"),
                ),
                availability_reason(
                    "missing_auth",
                    "authenticate",
                    None,
                    None,
                    Some("github_token"),
                ),
            ],
        };
        package.dependency_availability =
            vec![botster_hub_client::DaemonPackageDependencyAvailability {
                id: "dep-db".to_string(),
                package_name: "database.provider".to_string(),
                state: DaemonPackageAvailabilityState::Blocked,
                reasons: vec![
                    availability_reason(
                        "missing_package",
                        "install_package",
                        Some("database.provider"),
                        None,
                        None,
                    ),
                    availability_reason(
                        "disabled_package",
                        "enable_package",
                        Some("database.provider"),
                        None,
                        None,
                    ),
                ],
            }];
        package.feature_availability = vec![botster_hub_client::DaemonPackageFeatureAvailability {
            id: "cloud-sync".to_string(),
            state: DaemonPackageAvailabilityState::Blocked,
            reasons: vec![
                availability_reason(
                    "missing_provider",
                    "install_provider",
                    Some("cloud.provider"),
                    None,
                    None,
                ),
                availability_reason(
                    "missing_capability",
                    "grant_capability",
                    None,
                    Some(capability("http", Some("egress"))),
                    None,
                ),
                availability_reason(
                    "package_disabled",
                    "enable_package",
                    Some("workflow.plugin"),
                    None,
                    None,
                ),
                availability_reason(
                    "invalid_configuration",
                    "fix_configuration",
                    None,
                    None,
                    Some("mode"),
                ),
            ],
        }];

        app.apply_response(packages_response(vec![package]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 260);
        let rendered = lines.join("\n");
        assert!(rendered.contains("availability=blocked"));
        assert!(rendered.contains(
            "package blocked: reason=missing_config action=configure_package requirement=endpoint"
        ));
        assert!(rendered.contains(
            "package blocked: reason=missing_auth action=authenticate requirement=github_token"
        ));
        assert!(rendered.contains("dependency: id=dep-db package=database.provider state=blocked"));
        assert!(rendered.contains("dependency blocked: reason=missing_package action=install_package package=database.provider"));
        assert!(rendered.contains("dependency blocked: reason=disabled_package action=enable_package package=database.provider"));
        assert!(rendered.contains("feature: id=cloud-sync state=blocked"));
        assert!(rendered.contains("feature blocked: reason=missing_provider action=install_provider package=cloud.provider"));
        assert!(rendered.contains("feature blocked: reason=missing_capability action=grant_capability capability=http:egress"));
        assert!(rendered.contains(
            "feature blocked: reason=package_disabled action=enable_package package=workflow.plugin"
        ));
        assert!(rendered.contains("feature blocked: reason=invalid_configuration action=fix_configuration requirement=mode"));
    }

    #[test]
    fn marketplace_lifecycle_responses_render_from_public_dtos_without_paths_or_secrets() {
        let mut app = DogfoodApp::new(None);
        let available = available_package();
        let pin = package_pin();
        let mut install_plan = botster_hub_client::DaemonPackageInstallPlan {
            entry: available.clone(),
            effects: vec![botster_hub_client::DaemonPackageInstallEffect {
                kind: "write_manifest".to_string(),
                message: "registry entry will be installed".to_string(),
            }],
            diagnostics: vec![package_diagnostic("notice", "install preview ok")],
            mutates_registry: true,
            starts_entrypoints: true,
        };
        install_plan.entry.pin = Some(pin.clone());

        app.apply_response(available_packages_response(vec![available]));
        app.apply_response(install_plan_response(install_plan));
        app.apply_response(update_status_response(
            botster_hub_client::DaemonPackageUpdateStatus {
                package_name: "workflow.plugin".to_string(),
                update_available: true,
                reload_required: true,
                restart_required: false,
                pin: Some(pin),
                diagnostics: vec![package_diagnostic("warning", "entrypoint restart optional")],
                actions: Vec::new(),
            },
        ));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains("marketplace: 1 available"));
        assert!(rendered.contains(
            "available package: entry_id=workflow-plugin package=workflow.plugin version=1.2.0"
        ));
        assert!(rendered.contains("source_kind=registry source_label=first-party catalog"));
        assert!(rendered.contains("first_party=true state=available capabilities=mcp:tools"));
        assert!(rendered.contains("compatibility=compatible:>=0.1.0"));
        assert!(rendered.contains("compatibility_diagnostics=requires current hub"));
        assert!(rendered.contains("pin=revision=rev-2026,update_policy=manual,branch=main"));
        assert!(rendered.contains("install plan: package=workflow.plugin"));
        assert!(rendered.contains("entry_id=workflow-plugin"));
        assert!(rendered.contains("mutates_registry=true"));
        assert!(rendered.contains("starts_entrypoints=true"));
        assert!(
            rendered.contains("install effect: write_manifest:registry entry will be installed")
        );
        assert!(rendered.contains("install diagnostic: notice:install preview ok"));
        assert!(rendered.contains("update status: package=workflow.plugin update_available=true reload_required=true restart_required=false"));
        assert!(rendered.contains("update diagnostic: warning:entrypoint restart optional"));
        assert!(!rendered.contains("/Users/"));
        assert!(!rendered.contains("/tmp/"));
        assert!(!rendered.contains("token"));
    }

    #[test]
    fn package_decision_response_keeps_action_result_visible_with_refreshed_packages() {
        let mut app = DogfoodApp::new(None);
        let mut response = package_decision_response(vec![package(
            "workflow.plugin",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        )]);
        response.package_decision = Some(botster_hub_client::DaemonPackageDecision {
            package_name: "workflow.plugin".to_string(),
            action: "enable".to_string(),
            state: "enabled".to_string(),
            classification: "plugin".to_string(),
        });

        app.apply_response(response);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 80);
        let rendered = lines.join("\n");
        assert!(rendered.contains("package: workflow.plugin 1.0.0"));
        assert!(rendered.contains(
            "package decision: package=workflow.plugin action=enable state=enabled classification=plugin"
        ));
    }

    #[test]
    fn lifecycle_action_buttons_emit_public_daemon_requests() {
        let mut app = DogfoodApp::new(None);
        let pin = package_pin();

        app.handle_action(
            "botster.tui.package.enable".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin" })),
        );
        app.handle_action(
            "botster.tui.package.disable".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin" })),
        );
        app.handle_action(
            "botster.tui.package.remove".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin" })),
        );
        app.handle_action(
            "botster.tui.package.update_status".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin" })),
        );
        app.handle_action(
            "botster.tui.package.update_preview".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin", "pin": pin.clone() })),
        );
        app.handle_action(
            "botster.tui.package.update_apply".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin", "pin": pin.clone() })),
        );
        app.handle_action(
            "botster.tui.entrypoint.start".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin", "entrypoint_id": "web" })),
        );
        app.handle_action(
            "botster.tui.entrypoint.stop".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin", "entrypoint_id": "web" })),
        );
        app.handle_action(
            "botster.tui.entrypoint.restart".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin", "entrypoint_id": "web" })),
        );
        app.handle_action(
            "botster.tui.entrypoint.status".to_string(),
            None,
            Some(json!({ "package_name": "workflow.plugin", "entrypoint_id": "web" })),
        );

        assert_eq!(
            app.observed_requests,
            vec![
                ObservedRequest::EnablePackage("workflow.plugin".to_string()),
                ObservedRequest::DisablePackage("workflow.plugin".to_string()),
                ObservedRequest::RemovePackage("workflow.plugin".to_string()),
                ObservedRequest::CheckPackageUpdate("workflow.plugin".to_string()),
                ObservedRequest::PreviewPackageUpdate {
                    package_name: "workflow.plugin".to_string(),
                    pin: pin.clone(),
                },
                ObservedRequest::ApplyPackageUpdate {
                    package_name: "workflow.plugin".to_string(),
                    pin,
                },
                ObservedRequest::StartPackageEntrypoint {
                    package_name: "workflow.plugin".to_string(),
                    entrypoint_id: "web".to_string(),
                },
                ObservedRequest::StopPackageEntrypoint {
                    package_name: "workflow.plugin".to_string(),
                    entrypoint_id: "web".to_string(),
                },
                ObservedRequest::RestartPackageEntrypoint {
                    package_name: "workflow.plugin".to_string(),
                    entrypoint_id: "web".to_string(),
                },
                ObservedRequest::PackageEntrypointStatus {
                    package_name: "workflow.plugin".to_string(),
                    entrypoint_id: "web".to_string(),
                },
            ]
        );
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
    fn package_configuration_response_renders_schema_values_validation_and_redacted_secret() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(packages_response(vec![package_with_configuration()]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");

        assert!(rendered.contains("configuration: schema=yes values=5 missing=1 diagnostics=1"));
        assert!(rendered.contains("Endpoint *: https://example.invalid/hook"));
        assert!(rendered.contains("Debug: [x]"));
        assert!(rendered.contains("Mode: Read"));
        assert!(rendered.contains("Notes: Line one"));
        assert!(
            rendered.contains("API token secret (redacted; Space marks write-only update): [ ]")
        );
        assert!(rendered.contains("configuration missing: endpoint"));
        assert!(rendered.contains("configuration diagnostic: schema:manifest warning"));
        assert!(!rendered.contains("super-secret-token"));
    }

    #[test]
    fn package_configuration_drafts_render_and_submit_hub_shaped_values_without_raw_secrets() {
        let mut app = DogfoodApp::new(None);
        app.apply_response(packages_response(vec![package_with_configuration()]));
        app.set_drafts(BTreeMap::from([
            (
                package_config_field_name("configuration.plugin", "endpoint"),
                Value::String("https://example.invalid/new".to_string()),
            ),
            (
                package_config_field_name("configuration.plugin", "debug"),
                Value::Bool(false),
            ),
            (
                package_config_field_name("configuration.plugin", "mode"),
                Value::String("write".to_string()),
            ),
            (
                package_config_field_name("configuration.plugin", "notes"),
                Value::String("Line one\nLine two".to_string()),
            ),
            (
                package_config_field_name("configuration.plugin", "api_token"),
                Value::Bool(true),
            ),
        ]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Endpoint *: https://example.invalid/new"));
        assert!(rendered.contains("Debug: [ ]"));
        assert!(rendered.contains("Mode: Write"));
        assert!(rendered.contains("Notes: Line one"));

        app.handle_dispatch(InputDispatch::Action(botster_core::ui::UiActionRequest {
            request_id: botster_core::RequestId("req-config-submit".to_string()),
            surface_id: botster_core::ui::UiSurfaceId(renderer::DOGFOOD_SURFACE_ID.to_string()),
            action_id: botster_core::ui::UiActionId(
                "botster.tui.package_config.submit".to_string(),
            ),
            node_id: Some(UiNodeId(
                "dogfood-package-0-configuration-submit".to_string(),
            )),
            kind: botster_core::ui::UiActionKind::Submit,
            values: Some(UiFormValues(
                app.drafts
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            )),
            payload: Some(json!({ "package_name": "configuration.plugin" })),
        }));

        let Some(ObservedRequest::SetPackageConfiguration {
            package_name,
            values,
        }) = app.observed_requests.last()
        else {
            panic!("expected set package configuration request");
        };
        assert_eq!(package_name, "configuration.plugin");
        assert_eq!(
            values["endpoint"],
            json!({"type":"url","value":"https://example.invalid/new"})
        );
        assert_eq!(values["debug"], json!({"type":"boolean","value":false}));
        assert_eq!(values["mode"], json!({"type":"select","value":"write"}));
        assert_eq!(
            values["notes"],
            json!({"type":"multiline_text","value":"Line one\nLine two"})
        );
        assert_eq!(
            values["api_token"],
            json!({"type":"secret","state":"write_only"})
        );
        assert!(
            !serde_json::to_string(values)
                .unwrap()
                .contains("super-secret-token")
        );
    }

    #[test]
    fn package_configuration_success_refreshes_from_package_decision_response() {
        let mut app = DogfoodApp::new(None);
        let mut package = package_with_configuration();
        package.configuration.missing_required.clear();

        app.apply_response(package_decision_response(vec![package]));

        assert_eq!(app.packages.len(), 1);
        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains("configuration: schema=yes values=5 missing=0 diagnostics=1"));
        assert!(!rendered.contains("configuration missing: endpoint"));
    }

    #[test]
    fn package_configuration_operator_error_renders_validation_failure() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(operator_error_response(
            "configuration field endpoint expects url",
        ));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        assert!(
            lines
                .join("\n")
                .contains("error: configuration field endpoint expects url")
        );
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
        app.sessions = vec![SessionRow::running("session-alpha")];
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
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-beta".to_string());

        app.apply_response(attach_state_response("session-beta", "attached"));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert_eq!(app.attached_session.as_deref(), Some("session-beta"));
        assert!(rendered.contains("attached: session-beta"));
        assert!(rendered.contains("session-beta [running] (attached)"));
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
    fn snapshot_and_scrollback_events_append_before_later_terminal_output() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(events_response(vec![
            DaemonEvent::Snapshot {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                data: "snapshot\n".to_string(),
                bytes: 9,
            },
            DaemonEvent::Scrollback {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                data: "scrollback\n".to_string(),
                bytes: 11,
            },
            DaemonEvent::TerminalOutput {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                data: "live\n".to_string(),
            },
        ]));

        assert_eq!(app.terminal_output, "snapshot\nscrollback\nlive\n");

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("snapshot"));
        assert!(rendered.contains("scrollback"));
        assert!(rendered.contains("live"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "dogfood-terminal")
        );
    }

    #[test]
    fn shared_late_attach_history_waits_through_empty_drain_and_renders_once_in_order() {
        let scenario = botster_hub_test_support::late_attach_history_conformance_scenario();
        let mut app = DogfoodApp::new(None);
        app.subscription_id = scenario.subscription_id.clone();
        app.begin_attach_hydration(&scenario.session_id);
        app.observed_requests.clear();

        app.apply_response(events_response(Vec::new()));
        assert!(app.attach_hydration.is_some());
        assert!(app.observed_requests.is_empty());

        app.apply_response(events_response(vec![scenario.history_then_live[0].clone()]));
        assert!(app.attach_hydration.is_some());
        assert!(app.observed_requests.is_empty());

        app.apply_response(events_response(vec![
            scenario.history_then_live[1].clone(),
            scenario.history_then_live[2].clone(),
        ]));

        let restored = match &scenario.history_then_live[1] {
            DaemonEvent::Snapshot { data, .. } => data,
            other => panic!("expected shared snapshot event, got {other:?}"),
        };
        let live = match &scenario.history_then_live[2] {
            DaemonEvent::TerminalOutput { data, .. } => data,
            other => panic!("expected shared live event, got {other:?}"),
        };
        assert_eq!(app.terminal_output, format!("{restored}{live}"));
        assert_eq!(app.terminal_output.matches(restored).count(), 1);
        assert!(app.attach_hydration.is_none());
        assert!(app.observed_requests.is_empty());
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.find(restored.trim()).unwrap() < rendered.find(live.trim()).unwrap());
    }

    #[test]
    fn shared_no_history_live_bytes_suppress_deadline_fallback_before_later_exit() {
        let scenario = botster_hub_test_support::late_attach_history_conformance_scenario();
        let mut app = DogfoodApp::new(None);
        app.subscription_id = scenario.no_history_subscription_id.clone();
        app.begin_attach_hydration(&scenario.no_history_session_id);
        app.attach_hydration.as_mut().unwrap().deadline = Instant::now();
        app.observed_requests.clear();

        app.apply_response(events_response(vec![
            scenario.no_history_then_live[0].clone(),
            scenario.no_history_then_live[1].clone(),
        ]));

        assert!(app.attach_hydration.is_none());
        assert!(!app.terminal_output.is_empty());
        assert!(app.observed_requests.is_empty());

        app.apply_response(events_response(vec![
            scenario.no_history_then_live[2].clone(),
        ]));
        assert!(app.observed_requests.is_empty());
        assert!(app.attached_session.is_none());
    }

    #[test]
    fn expired_empty_hydration_finishes_before_synthetic_screen_response_renders() {
        let mut app = DogfoodApp::new(None);
        app.subscription_id = "sub-captured".to_string();
        app.begin_attach_hydration("session-captured");
        app.attach_hydration.as_mut().unwrap().deadline = Instant::now();
        app.attached_session = None;
        app.observed_requests.clear();

        app.apply_response(events_response(Vec::new()));
        app.apply_response(events_response(Vec::new()));

        assert!(app.attach_hydration.is_none());
        assert!(app.observed_requests.is_empty());

        app.apply_optional_readback_response(
            read_screen_response("session-captured", "fallback screen"),
            "read_screen",
        );
        assert!(app.terminal_output.is_empty());
        assert_eq!(app.read_screen_fallback.as_deref(), Some("fallback screen"));
        assert!(
            renderer::render_to_lines(&app.surface(), 120, 48)
                .0
                .join("\n")
                .contains("fallback screen")
        );
    }

    #[test]
    fn late_screen_response_cannot_replace_restored_history() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());
        app.terminal_output = "ordered history".to_string();

        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "stale screen"),
            "read_screen",
        );

        assert_eq!(app.terminal_output, "ordered history");
        assert!(app.read_screen_fallback.is_none());
    }

    #[test]
    fn authoritative_bytes_after_screen_fallback_replace_it_without_duplication() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());
        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "fallback-only"),
            "read_screen",
        );

        app.apply_response(events_response(vec![DaemonEvent::TerminalOutput {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-alpha".to_string(),
            data: "authoritative-live".to_string(),
        }]));

        assert!(app.read_screen_fallback.is_none());
        assert_eq!(app.terminal_output, "authoritative-live");
        let rendered = renderer::render_to_lines(&app.surface(), 120, 48)
            .0
            .join("\n");
        assert!(!rendered.contains("fallback-only"));
        assert_eq!(rendered.matches("authoritative-live").count(), 1);
    }

    #[test]
    fn snapshot_readback_is_metadata_only_and_renders_status() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());

        app.apply_optional_readback_response(
            capture_snapshot_response("session-alpha", 24, 80, Some("ghostty-page"), 4096),
            "capture_snapshot",
        );

        assert!(app.terminal_output.is_empty());
        assert!(app.read_screen_fallback.is_none());
        let rendered = renderer::render_to_lines(&app.surface(), 120, 48)
            .0
            .join("\n");
        assert!(rendered.contains("rows=24 cols=80"));
        assert!(rendered.contains("format=ghostty-page"));
        assert!(rendered.contains("payload_bytes=4096"));
    }

    #[test]
    fn optional_readback_operator_error_is_non_fatal() {
        let mut app = DogfoodApp::new(None);
        app.attached_session = Some("session-alpha".to_string());
        app.terminal_output = "preserved".to_string();

        app.apply_optional_readback_response(
            operator_error_response("session exited during capture"),
            "capture_snapshot",
        );

        assert_eq!(app.attached_session.as_deref(), Some("session-alpha"));
        assert_eq!(app.terminal_output, "preserved");
        assert!(app.error.is_none());
        assert!(
            app.action_feedback
                .as_deref()
                .unwrap()
                .contains("capture_snapshot unavailable")
        );

        app.apply_optional_readback_response(
            operator_error_response("session exited during screen read"),
            "read_screen",
        );
        assert_eq!(app.attached_session.as_deref(), Some("session-alpha"));
        assert_eq!(app.terminal_output, "preserved");
        assert!(app.error.is_none());
        assert!(
            app.action_feedback
                .as_deref()
                .unwrap()
                .contains("read_screen unavailable")
        );
    }

    #[test]
    fn every_attach_cycle_clears_owned_terminal_and_readback_state() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());
        app.terminal_output = "alpha history".to_string();
        app.read_screen_fallback = Some("alpha fallback".to_string());
        app.snapshot_metadata = Some(DaemonCaptureSnapshot {
            session_id: "session-alpha".to_string(),
            rows: 24,
            cols: 80,
            payload_format: None,
            payload_bytes: 1,
        });

        app.begin_attach_hydration("session-alpha");
        assert!(app.terminal_output.is_empty());
        assert!(app.read_screen_fallback.is_none());
        assert!(app.snapshot_metadata.is_none());
        assert_eq!(
            app.terminal_output_session_id.as_deref(),
            Some("session-alpha")
        );

        app.terminal_output = "replayed alpha history".to_string();
        app.read_screen_fallback = Some("alpha fallback".to_string());

        app.begin_attach_hydration("session-beta");
        assert!(app.terminal_output.is_empty());
        assert!(app.read_screen_fallback.is_none());
        assert!(app.snapshot_metadata.is_none());
        assert_eq!(
            app.terminal_output_session_id.as_deref(),
            Some("session-beta")
        );
    }

    #[test]
    fn process_exit_applies_same_response_bytes_and_suppresses_readbacks() {
        let mut app = DogfoodApp::new(None);
        app.subscription_id = "sub-alpha".to_string();
        app.begin_attach_hydration("session-alpha");
        app.snapshot_metadata = Some(DaemonCaptureSnapshot {
            session_id: "session-alpha".to_string(),
            rows: 24,
            cols: 80,
            payload_format: None,
            payload_bytes: 1,
        });
        app.observed_requests.clear();

        app.apply_response(events_response(vec![
            DaemonEvent::TerminalOutput {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-alpha".to_string(),
                data: "final bytes".to_string(),
            },
            DaemonEvent::ProcessExit {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-alpha".to_string(),
                code: Some(0),
            },
        ]));

        assert_eq!(app.terminal_output, "final bytes");
        assert!(app.attach_hydration.is_none());
        assert!(app.snapshot_metadata.is_none());
        assert!(app.observed_requests.is_empty());
    }

    #[test]
    fn process_exit_preserves_owned_fallback_and_clears_snapshot_metadata() {
        let mut app = DogfoodApp::new(None);
        app.subscription_id = "sub-alpha".to_string();
        app.begin_attach_hydration("session-alpha");
        app.read_screen_fallback = Some("last visible screen".to_string());
        app.snapshot_metadata = Some(DaemonCaptureSnapshot {
            session_id: "session-alpha".to_string(),
            rows: 24,
            cols: 80,
            payload_format: None,
            payload_bytes: 1,
        });

        app.apply_response(events_response(vec![DaemonEvent::ProcessExit {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-alpha".to_string(),
            code: Some(0),
        }]));

        assert_eq!(
            app.read_screen_fallback.as_deref(),
            Some("last visible screen")
        );
        assert!(app.snapshot_metadata.is_none());
        assert!(
            renderer::render_to_lines(&app.surface(), 120, 48)
                .0
                .join("\n")
                .contains("last visible screen")
        );
    }

    #[test]
    fn detach_preserves_owned_fallback_and_clears_snapshot_metadata() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());
        app.read_screen_fallback = Some("last visible screen".to_string());
        app.snapshot_metadata = Some(DaemonCaptureSnapshot {
            session_id: "session-alpha".to_string(),
            rows: 24,
            cols: 80,
            payload_format: None,
            payload_bytes: 1,
        });

        app.apply_response(attach_state_response("session-alpha", "detached"));

        assert_eq!(
            app.read_screen_fallback.as_deref(),
            Some("last visible screen")
        );
        assert!(app.snapshot_metadata.is_none());
    }

    #[test]
    fn empty_history_event_data_is_non_fatal() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output = "existing output\n".to_string();

        app.apply_response(events_response(vec![
            DaemonEvent::Snapshot {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                data: String::new(),
                bytes: 128,
            },
            DaemonEvent::Scrollback {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                data: String::new(),
                bytes: 256,
            },
        ]));

        assert_eq!(app.terminal_output, "existing output\n");
        assert!(app.error.is_none());
    }

    #[test]
    fn history_events_use_same_terminal_output_cap() {
        let mut app = DogfoodApp::new(None);
        app.terminal_output = "a".repeat(7_995);

        app.apply_response(events_response(vec![DaemonEvent::Snapshot {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-test".to_string(),
            data: "bbbbbbbbbb".to_string(),
            bytes: 10,
        }]));

        assert_eq!(app.terminal_output.len(), 8_000);
        assert_eq!(
            app.terminal_output,
            format!("{}{}", "a".repeat(7_990), "b".repeat(10))
        );
    }

    #[test]
    fn focused_session_list_row_updates_attach_selection() {
        let mut app = DogfoodApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-alpha".to_string());
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let mut router = InputRouter::new(renderer::action_request_context());
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
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-beta".to_string());

        app.apply_response(sessions_response(["session-alpha", "session-beta"]));

        assert_eq!(
            app.sessions,
            session_rows([("session-alpha", "running"), ("session-beta", "running"),])
        );
        assert_eq!(app.selected_session.as_deref(), Some("session-beta"));
    }

    #[test]
    fn session_repull_resets_stale_selected_session_to_first_listed_session() {
        let mut app = DogfoodApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-beta".to_string());

        app.apply_response(sessions_response(["session-gamma", "session-delta"]));

        assert_eq!(
            app.sessions,
            session_rows([("session-gamma", "running"), ("session-delta", "running"),])
        );
        assert_eq!(app.selected_session.as_deref(), Some("session-gamma"));
    }

    #[test]
    fn sessions_response_preserves_and_renders_lifecycle_state() {
        let mut app = DogfoodApp::new(None);

        app.apply_response(sessions_response_with_lifecycles([
            ("session-alpha", "running"),
            ("session-beta", "exited"),
        ]));

        assert_eq!(
            app.sessions,
            session_rows([("session-alpha", "running"), ("session-beta", "exited"),])
        );
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("session-alpha [running]"));
        assert!(rendered.contains("session-beta [exited]"));
    }

    #[test]
    fn action_dispatch_rejects_exited_session_before_daemon_attach() {
        let mut app = DogfoodApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "exited")]);
        app.selected_session = Some("session-beta".to_string());
        app.observed_requests.clear();

        app.handle_dispatch(InputDispatch::Action(botster_core::ui::UiActionRequest {
            request_id: botster_core::RequestId("req-attach-exited".to_string()),
            surface_id: botster_core::ui::UiSurfaceId(renderer::DOGFOOD_SURFACE_ID.to_string()),
            action_id: botster_core::ui::UiActionId("botster.tui.attach".to_string()),
            node_id: Some(UiNodeId("dogfood-session-session-beta-attach".to_string())),
            kind: botster_core::ui::UiActionKind::Submit,
            values: None,
            payload: Some(json!({ "session_id": "session-beta" })),
        }));

        assert!(app.observed_requests.is_empty());
        assert_eq!(
            app.error.as_deref(),
            Some("session-beta exited - cannot attach")
        );
    }

    #[test]
    fn repeated_exited_attach_attempts_render_one_deduplicated_error() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec![SessionRow {
            session_id: "session-beta".to_string(),
            lifecycle: "exited".to_string(),
        }];
        app.selected_session = Some("session-beta".to_string());
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let mut router = InputRouter::new(renderer::action_request_context());
        let session_row = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "dogfood-session-session-beta")
            .expect("exited session row should be focusable");

        let mouse_dispatch = router.dispatch_event(
            Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: session_row.rect.x,
                row: session_row.rect.y,
                modifiers: KeyModifiers::NONE,
            }),
            &hit_map,
        );
        app.handle_dispatch(mouse_dispatch);
        let key_dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &hit_map,
        );
        app.handle_dispatch(key_dispatch);

        assert!(app.observed_requests.is_empty());
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert_eq!(
            rendered
                .matches("session-beta exited - cannot attach")
                .count(),
            1
        );
        assert!(!rendered.contains("attached session disappeared"));
    }

    #[test]
    fn terminal_focus_attach_rejects_non_running_session_before_daemon_attach() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec![SessionRow {
            session_id: "session-beta".to_string(),
            lifecycle: "stopped".to_string(),
        }];
        app.selected_session = Some("session-beta".to_string());
        app.observed_requests.clear();

        app.handle_dispatch(InputDispatch::Action(botster_core::ui::UiActionRequest {
            request_id: botster_core::RequestId("req-terminal-focus".to_string()),
            surface_id: botster_core::ui::UiSurfaceId(renderer::DOGFOOD_SURFACE_ID.to_string()),
            action_id: botster_core::ui::UiActionId("botster.terminal.focus".to_string()),
            node_id: Some(UiNodeId("dogfood-terminal".to_string())),
            kind: botster_core::ui::UiActionKind::Submit,
            values: None,
            payload: None,
        }));

        assert!(app.observed_requests.is_empty());
        assert_eq!(
            app.error.as_deref(),
            Some("session-beta stopped - cannot attach")
        );
    }

    #[test]
    fn reconnect_restore_does_not_reattach_known_non_running_session() {
        let mut app = DogfoodApp::new(None);
        app.sessions = vec![SessionRow {
            session_id: "session-beta".to_string(),
            lifecycle: "exited".to_string(),
        }];
        app.selected_session = Some("session-beta".to_string());
        app.observed_requests.clear();

        app.restore_after_connect();

        assert!(app.observed_requests.is_empty());
        assert_eq!(
            app.error.as_deref(),
            Some("session-beta exited - cannot attach")
        );
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
                ObservedRequest::ListApps,
                ObservedRequest::ListPackageNavigation,
                ObservedRequest::ListPackages,
            ]
        );
    }

    #[test]
    fn reconnect_restore_reattaches_selected_session_after_read_model_refresh() {
        let mut app = DogfoodApp::new(None);
        app.observed_requests.clear();
        app.sessions = vec![SessionRow::running("session-alpha")];
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
            .hub_bin(&hub_bin)
            .session_worker_bin(session_worker_bin)
            .root(&root)
            .name("botster-tui-headless-dogfood")
            .start()
            .expect("isolated hub starts");

        run_headless_dogfood(AppArgs {
            smoke: false,
            hub_socket: Some(hub.endpoint().socket_path.clone()),
            hub_data_dir: Some(hub.data_dir().to_path_buf()),
            headless_dogfood: true,
        })
        .expect("headless dogfood surface completes a real hub round trip");

        assert_live_attach_history_readback(&hub);

        let foreground_conformance =
            botster_hub_test_support::run_foreground_terminal_app_open_conformance(&hub)
                .expect("foreground terminal app-open conformance passes");
        assert!(foreground_conformance.hub_socket_env_present);
        assert!(foreground_conformance.hub_data_dir_env_present);
        assert_eq!(foreground_conformance.real_hub_action_operation, "status");

        let Some(contract_matrix_fixture) =
            std::env::var_os("BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE").map(PathBuf::from)
        else {
            skip_or_panic("BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE");
            hub.shutdown().expect("isolated hub shuts down cleanly");
            return;
        };
        let plugin_report = botster_hub_test_support::run_plugin_contract_matrix_conformance(
            &hub,
            contract_matrix_fixture,
        )
        .expect("plugin contract matrix conformance passes");
        assert_plugin_contract_matrix_renders_through_tui(&hub, &plugin_report);

        let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let data_dir = hub.data_dir().to_string_lossy().to_string();
        let package_root = package_root.to_string_lossy().to_string();
        let package_open_output = std::process::Command::new(&hub_bin)
            .args([
                "packages",
                "install",
                "--data-dir",
                data_dir.as_str(),
                "--path",
                package_root.as_str(),
            ])
            .output()
            .expect("run packages install for botster-tui checkout");
        assert!(
            package_open_output.status.success(),
            "packages install failed: stdout={} stderr={}",
            String::from_utf8_lossy(&package_open_output.stdout),
            String::from_utf8_lossy(&package_open_output.stderr)
        );
        let package_enable_output = std::process::Command::new(&hub_bin)
            .args([
                "packages",
                "enable",
                "--data-dir",
                data_dir.as_str(),
                "botster-tui",
            ])
            .output()
            .expect("run packages enable for botster-tui checkout");
        assert!(
            package_enable_output.status.success(),
            "packages enable failed: stdout={} stderr={}",
            String::from_utf8_lossy(&package_enable_output.stdout),
            String::from_utf8_lossy(&package_enable_output.stderr)
        );
        let app_open_output = std::process::Command::new(&hub_bin)
            .args([
                "apps",
                "open",
                "--data-dir",
                data_dir.as_str(),
                "botster-tui",
            ])
            .env("BOTSTER_TUI_HEADLESS_DOGFOOD", "1")
            .output()
            .expect("run apps open for botster-tui package");
        assert!(
            app_open_output.status.success(),
            "apps open failed: stdout={} stderr={}",
            String::from_utf8_lossy(&app_open_output.stdout),
            String::from_utf8_lossy(&app_open_output.stderr)
        );
        let app_open_stdout = String::from_utf8_lossy(&app_open_output.stdout);
        assert!(
            app_open_stdout.contains("terminal-output: echo:botster-tui-headless"),
            "apps open stdout={} stderr={}",
            app_open_stdout,
            String::from_utf8_lossy(&app_open_output.stderr)
        );
        assert!(
            app_open_stdout.contains("hub-data-dir: configured"),
            "apps open stdout={} stderr={}",
            app_open_stdout,
            String::from_utf8_lossy(&app_open_output.stderr)
        );

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

    fn assert_live_attach_history_readback(hub: &botster_hub_test_support::IsolatedHub) {
        let mut daemon =
            HubConnection::connect(hub.endpoint()).expect("connect direct daemon client");
        let prior_session_id = format!("tui-history-{}", short_suffix());
        let prior_marker = format!("history-before-tui-{}", short_suffix());
        let later_marker = format!("live-after-tui-{}", short_suffix());
        daemon
            .request(&DaemonRequest::Spawn {
                session_id: prior_session_id.clone(),
                command: format!("printf '{prior_marker}\\n'; while IFS= read -r line; do :; done"),
            })
            .expect("spawn history-producing session before TUI attach");
        thread::sleep(Duration::from_millis(150));

        let mut app = DogfoodApp::new(Some(hub.endpoint().socket_path.clone()));
        app.selected_session = Some(prior_session_id.clone());
        app.observed_requests.clear();
        app.attach_selected_or_first();
        wait_for_app_output(&mut app, &prior_marker).expect("late TUI attach renders prior output");
        let hydration_deadline = Instant::now() + Duration::from_secs(7);
        while app.attach_hydration.is_some() && Instant::now() < hydration_deadline {
            app.poll_hub();
            thread::sleep(Duration::from_millis(30));
        }
        assert_eq!(app.terminal_output.matches(&prior_marker).count(), 1);
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::CaptureSnapshot(prior_session_id.clone()))
        );
        assert!(
            !app.observed_requests
                .contains(&ObservedRequest::ReadScreen(prior_session_id.clone()))
        );
        assert!(app.observed_requests.iter().any(
            |request| matches!(request, ObservedRequest::Drain(id) if id == &prior_session_id)
        ));
        assert!(app.observed_requests.iter().all(|request| {
            !matches!(request, ObservedRequest::Drain(id) if id != &prior_session_id)
        }));

        daemon
            .request(&DaemonRequest::SendInput {
                session_id: prior_session_id.clone(),
                data: format!("{later_marker}\n"),
            })
            .expect("send later live marker through direct daemon request");
        wait_for_app_output(&mut app, &later_marker).expect("TUI renders later live output");
        assert_eq!(app.terminal_output.matches(&later_marker).count(), 1);
        let rendered = renderer::render_to_lines(&app.surface(), 200, 80)
            .0
            .join("\n");
        assert!(
            rendered.find(&prior_marker).unwrap() < rendered.find(&later_marker).unwrap(),
            "restored history must render before later live output: {rendered}"
        );

        app.force_reconnect();
        let reconnect_deadline = Instant::now() + Duration::from_secs(7);
        while app.attach_hydration.is_some() && Instant::now() < reconnect_deadline {
            app.poll_hub();
            thread::sleep(Duration::from_millis(30));
        }
        assert!(
            app.attach_hydration.is_none(),
            "same-session reconnect hydration must finish"
        );
        wait_for_app_output(&mut app, &prior_marker)
            .expect("same-session reconnect restores prior output");
        wait_for_app_output(&mut app, &later_marker)
            .expect("same-session reconnect restores later output");
        assert_eq!(app.terminal_output.matches(&prior_marker).count(), 1);
        assert_eq!(app.terminal_output.matches(&later_marker).count(), 1);
        let reconnected = renderer::render_to_lines(&app.surface(), 200, 80)
            .0
            .join("\n");
        assert!(
            reconnected.find(&prior_marker).unwrap() < reconnected.find(&later_marker).unwrap(),
            "same-session reconnect must render one ordered replay: {reconnected}"
        );
        daemon
            .request(&DaemonRequest::ShutdownSession {
                session_id: prior_session_id,
            })
            .expect("shut down history-producing session");

        let empty_session_id = format!("tui-empty-{}", short_suffix());
        daemon
            .request(&DaemonRequest::Spawn {
                session_id: empty_session_id.clone(),
                command: "while IFS= read -r line; do :; done".to_string(),
            })
            .expect("spawn empty session");
        thread::sleep(Duration::from_millis(150));

        let mut empty_app = DogfoodApp::new(Some(hub.endpoint().socket_path.clone()));
        empty_app.selected_session = Some(empty_session_id.clone());
        empty_app.observed_requests.clear();
        empty_app.attach_selected_or_first();
        let deadline = Instant::now() + Duration::from_secs(7);
        while empty_app.attach_hydration.is_some() && Instant::now() < deadline {
            empty_app.poll_hub();
            thread::sleep(Duration::from_millis(30));
        }
        assert!(empty_app.attach_hydration.is_none());
        assert_eq!(
            empty_app
                .observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::ReadScreen(id) if id == &empty_session_id))
                .count(),
            1
        );
        assert!(empty_app.observed_requests.iter().any(
            |request| matches!(request, ObservedRequest::Drain(id) if id == &empty_session_id)
        ));
        assert!(empty_app.observed_requests.iter().all(|request| {
            !matches!(request, ObservedRequest::Drain(id) if id != &empty_session_id)
        }));
        assert_eq!(
            empty_app
                .observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::CaptureSnapshot(id) if id == &empty_session_id))
                .count(),
            1
        );
        assert!(empty_app.read_screen_fallback.is_none());
        let rendered = renderer::render_to_lines(&empty_app.surface(), 200, 80)
            .0
            .join("\n");
        assert!(rendered.contains("terminal snapshot: session="));
        daemon
            .request(&DaemonRequest::ShutdownSession {
                session_id: empty_session_id,
            })
            .expect("shut down empty session");
    }

    fn assert_plugin_contract_matrix_renders_through_tui(
        hub: &botster_hub_test_support::IsolatedHub,
        report: &botster_hub_test_support::PluginContractMatrixConformanceReport,
    ) {
        assert_eq!(
            report.failure_classes.client_rendering,
            report.client_render_check.class
        );
        assert_eq!(
            report.app_surface_node_id,
            report.client_render_check.app_surface_node_id
        );
        assert_eq!(
            report.empty_surface_child_id,
            report.client_render_check.empty_surface_child_id
        );
        assert_eq!(
            report.settings_surface_node_id,
            report.client_render_check.settings_surface_node_id
        );
        assert_eq!(
            report.valid_configuration_secret_state,
            report.client_render_check.expected_redacted_secret_state
        );

        let mut client = HubConnection::connect(hub.endpoint()).expect("connect to live hub");
        let list_packages = client
            .request(&DaemonRequest::ListPackages)
            .expect("list packages after contract matrix conformance");
        let list_apps = client
            .request(&DaemonRequest::ListApps)
            .expect("list apps after contract matrix conformance");
        let list_package_navigation = client
            .request(&DaemonRequest::ListPackageNavigation)
            .expect("list package navigation after contract matrix conformance");
        let mut app = DogfoodApp::new(None);
        app.apply_response(list_packages);
        app.apply_response(list_apps);
        app.apply_response(list_package_navigation);
        let (lines, _) = renderer::render_to_lines(&app.surface(), 500, 240);
        let rendered = lines.join("\n");
        assert!(rendered.contains("navigation entry: package=botster.plugin-contract-matrix"));
        assert!(rendered.contains(&report.app_route_path));
        assert!(rendered.contains("route_id=surface:contract.app"));
        assert!(rendered.contains(&format!(
            "target_surface_id={}",
            report.app_route_surface_id
        )));

        let app_surface = request_plugin_surface(&mut client, &report.package_name, "contract.app");
        let app_rendered = assert_rendered_plugin_surface_contains(
            &app_surface,
            &report.client_render_check.app_surface_node_id,
            "plugin_surface_render",
        );
        assert!(app_rendered.contains("Render path: validated"));
        assert!(app_rendered.contains("status_badge: Validated"));

        let empty_surface =
            request_plugin_surface(&mut client, &report.package_name, "contract.empty");
        assert_rendered_plugin_surface_contains(
            &empty_surface,
            &report.client_render_check.empty_surface_child_id,
            "No fixture rows are available.",
        );

        let settings_surface =
            request_plugin_surface(&mut client, &report.package_name, "contract.settings");
        let settings_rendered = assert_rendered_plugin_surface_contains(
            &settings_surface,
            &report.client_render_check.settings_surface_node_id,
            "api_token_state=redacted",
        );
        assert!(settings_rendered.contains("mode=write"));
        assert!(
            settings_rendered
                .contains("endpoint=https://example.invalid/plugin-contract-matrix/acceptance")
        );
        assert!(!settings_rendered.contains("write_only"));
        assert!(!settings_rendered.contains("contract-action-secret"));

        let success = client
            .request(&DaemonRequest::PluginSurfaceAction {
                package_name: report.package_name.clone(),
                surface_id: "contract.app".to_string(),
                action_id: "contract.action".to_string(),
                payload: json!({
                    "request_id": "contract-action-success",
                    "message": "hello",
                }),
            })
            .expect("dispatch contract action success");
        let mut action_app = DogfoodApp::new(None);
        action_app.apply_response(success);
        let (lines, _) = renderer::render_to_lines(&action_app.surface(), 240, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains("plugin action result: state=accepted"));
        assert!(rendered.contains("request_id=contract-action-success"));
        assert!(rendered.contains("message=hello"));

        let failure = client
            .request(&DaemonRequest::PluginSurfaceAction {
                package_name: report.package_name.clone(),
                surface_id: "contract.app".to_string(),
                action_id: "contract.action".to_string(),
                payload: json!({
                    "request_id": "contract-action-error",
                    "fail": true,
                }),
            })
            .expect("dispatch contract action error");
        let mut failure_app = DogfoodApp::new(None);
        failure_app.apply_response(failure);
        let (lines, _) = renderer::render_to_lines(&failure_app.surface(), 240, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains("plugin action result: state=error"));
        assert!(rendered.contains("request_id=contract-action-error"));
        assert!(rendered.contains("action_failure"));
        assert!(rendered.contains("operation=plugin_surface_action"));

        let blocked = client
            .request(&DaemonRequest::PluginSurfaceRender {
                package_name: report.package_name.clone(),
                surface_id: "contract.blocked".to_string(),
                payload: json!({}),
            })
            .expect("render blocked contract surface");
        let mut blocked_app = DogfoodApp::new(None);
        blocked_app.apply_response(blocked);
        let (lines, _) = renderer::render_to_lines(&blocked_app.surface(), 240, 120);
        let rendered = lines.join("\n");
        assert!(rendered.contains("plugin surface render failed"));
        assert!(rendered.contains("plugin_invocation_failed"));
    }

    fn request_plugin_surface(
        client: &mut HubConnection,
        package_name: &str,
        surface_id: &str,
    ) -> DaemonPluginSurface {
        let response = client
            .request(&DaemonRequest::PluginSurfaceRender {
                package_name: package_name.to_string(),
                surface_id: surface_id.to_string(),
                payload: json!({}),
            })
            .expect("render contract plugin surface");
        assert_eq!(response.kind, DaemonResponseKind::PluginSurface);
        response
            .plugin_surface
            .expect("plugin surface response includes body")
    }

    fn assert_rendered_plugin_surface_contains(
        surface: &DaemonPluginSurface,
        expected_node_id: &str,
        expected_text: &str,
    ) -> String {
        assert!(
            surface.body.to_string().contains(expected_node_id),
            "delivered surface body should include node id {expected_node_id}: {}",
            surface.body
        );
        let node = plugin_surface_body_node(surface).expect("delivered surface validates for TUI");
        let (lines, _) = renderer::render_to_lines(&node, 180, 80);
        let rendered = lines.join("\n");
        assert!(
            rendered.contains(expected_text),
            "rendered plugin surface should contain {expected_text:?}: {rendered}"
        );
        rendered
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

    fn session_rows<const N: usize>(sessions: [(&str, &str); N]) -> Vec<SessionRow> {
        sessions
            .into_iter()
            .map(|(session_id, lifecycle)| SessionRow {
                session_id: session_id.to_string(),
                lifecycle: lifecycle.to_string(),
            })
            .collect()
    }

    fn sessions_response<const N: usize>(session_ids: [&str; N]) -> DaemonResponse {
        sessions_response_with_lifecycles(session_ids.map(|session_id| (session_id, "running")))
    }

    fn sessions_response_with_lifecycles<const N: usize>(
        sessions: [(&str, &str); N],
    ) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::Sessions);
        response.sessions = sessions
            .into_iter()
            .map(
                |(session_id, lifecycle)| botster_hub_client::DaemonSession {
                    session_id: session_id.to_string(),
                    lifecycle: lifecycle.to_string(),
                },
            )
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
                    FEATURE_PACKAGE_NAVIGATION.to_string(),
                    FEATURE_TERMINAL_READBACK.to_string(),
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

    fn apps_response(apps: Vec<DaemonApp>) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::Apps);
        response.apps = apps;
        response
    }

    fn package_navigation_response(entries: Vec<DaemonPackageNavigationEntry>) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::PackageNavigation);
        response.package_navigation = entries;
        response
    }

    fn package_decision_response(packages: Vec<DaemonPackage>) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::PackageDecision);
        response.packages = packages;
        response
    }

    fn available_packages_response(packages: Vec<DaemonAvailablePackage>) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::AvailablePackages);
        response.available_packages = packages;
        response
    }

    fn install_plan_response(plan: DaemonPackageInstallPlan) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::PackageInstallPlan);
        response.install_plan = Some(plan);
        response
    }

    fn update_status_response(status: DaemonPackageUpdateStatus) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::PackageUpdateStatus);
        response.update_status = Some(status);
        response
    }

    fn plugin_surface_response(surface: DaemonPluginSurface) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::PluginSurface);
        response.plugin_surface = Some(surface);
        response
    }

    fn plugin_action_response(result: Value) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::PluginActionResult);
        response.plugin_action_result = Some(result);
        response
    }

    fn contract_app_plugin_surface() -> DaemonPluginSurface {
        DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.app".to_string(),
            body: json!({
                "type": "panel",
                "id": "contract-app-panel",
                "props": {
                    "title": "Plugin Contract Matrix"
                },
                "children": [
                    {
                        "type": "text",
                        "id": "contract-app-summary",
                        "props": {
                            "text": "UiNode payload delivered through plugin_surface_render."
                        }
                    },
                    {
                        "type": "button",
                        "id": "contract-app-action",
                        "props": {
                            "label": "Run contract action",
                            "action": {
                                "id": "contract.action"
                            }
                        }
                    }
                ]
            }),
            ui_tree_snapshot: None,
        }
    }

    fn composite_application_primitives_plugin_surface() -> DaemonPluginSurface {
        DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.composite".to_string(),
            body: json!({
                "type": "section",
                "id": "contract-composite-section",
                "props": {
                    "title": "Project Pipeline Overview",
                    "description": "Composite surface for upgraded application primitives"
                },
                "slots": {
                    "toolbar": [
                        {
                            "type": "toolbar",
                            "id": "contract-composite-toolbar",
                            "props": {
                                "label": "Pipeline tools"
                            },
                            "slots": {
                                "actions": [
                                    {
                                        "type": "button",
                                        "id": "contract-composite-refresh",
                                        "props": {
                                            "label": "Refresh",
                                            "action": {
                                                "id": "contract.refresh",
                                                "payload": { "source": "toolbar" }
                                            }
                                        }
                                    }
                                ]
                            }
                        }
                    ],
                    "body": [
                        {
                            "type": "panel",
                            "id": "contract-composite-panel",
                            "props": {
                                "title": "Review queue",
                                "density": "compact",
                                "variant": "subtle"
                            },
                            "slots": {
                                "header": [
                                    {
                                        "type": "status_badge",
                                        "id": "contract-composite-health",
                                        "props": {
                                            "label": "Healthy",
                                            "status": "online",
                                            "tone": "success"
                                        }
                                    }
                                ],
                                "body": [
                                    {
                                        "type": "metric_grid",
                                        "id": "contract-composite-metrics",
                                        "props": {
                                            "density": "compact",
                                            "variant": "plain"
                                        },
                                        "children": [
                                            {
                                                "type": "metric",
                                                "id": "contract-composite-active-runs",
                                                "props": {
                                                    "label": "Active Runs",
                                                    "value": "3",
                                                    "caption": "currently assigned"
                                                }
                                            },
                                            {
                                                "type": "metric",
                                                "id": "contract-composite-findings",
                                                "props": {
                                                    "label": "Open Findings",
                                                    "value": "1",
                                                    "trend": {
                                                        "direction": "down",
                                                        "label": "falling"
                                                    }
                                                }
                                            }
                                        ]
                                    },
                                    {
                                        "type": "table",
                                        "id": "contract-composite-ticket-table",
                                        "props": {
                                            "columns": [
                                                { "id": "ticket", "label": "Ticket" },
                                                { "id": "state", "label": "State" }
                                            ],
                                            "rows": [
                                                {
                                                    "id": "contract-composite-ticket-a",
                                                    "cells": {
                                                        "ticket": "1783529012",
                                                        "state": "review"
                                                    },
                                                    "action": {
                                                        "id": "contract.ticket.open",
                                                        "payload": { "ticket_id": "1783529012" }
                                                    }
                                                },
                                                {
                                                    "id": "contract-composite-ticket-b",
                                                    "cells": {
                                                        "ticket": "1783529013",
                                                        "state": "implement"
                                                    },
                                                    "action": {
                                                        "id": "contract.ticket.open",
                                                        "payload": { "ticket_id": "1783529013" }
                                                    }
                                                }
                                            ],
                                            "selection": {
                                                "mode": "single",
                                                "selected": ["contract-composite-ticket-a"]
                                            },
                                            "empty_state": {
                                                "type": "empty_state",
                                                "id": "contract-composite-empty-table",
                                                "props": {
                                                    "title": "No tickets",
                                                    "description": "Nothing needs attention"
                                                }
                                            }
                                        }
                                    },
                                    {
                                        "type": "list",
                                        "id": "contract-composite-reviewers",
                                        "props": {
                                            "selection": {
                                                "mode": "single",
                                                "selected": ["contract-composite-reviewer-a"]
                                            }
                                        },
                                        "children": [
                                            {
                                                "type": "list_item",
                                                "id": "contract-composite-reviewer-a",
                                                "props": {
                                                    "value": "claude",
                                                    "action": {
                                                        "id": "contract.reviewer.focus"
                                                    }
                                                },
                                                "slots": {
                                                    "title": [
                                                        {
                                                            "type": "text",
                                                            "id": "contract-composite-reviewer-title",
                                                            "props": {
                                                                "text": "Reviewer"
                                                            }
                                                        }
                                                    ]
                                                }
                                            }
                                        ]
                                    },
                                    {
                                        "type": "form",
                                        "id": "contract-composite-form",
                                        "props": {
                                            "action": {
                                                "id": "contract.form.submit"
                                            }
                                        },
                                        "children": [
                                            {
                                                "type": "text_input",
                                                "id": "contract-composite-notes",
                                                "props": {
                                                    "name": "notes",
                                                    "label": "Notes",
                                                    "value": "Ready for review"
                                                }
                                            },
                                            {
                                                "type": "button",
                                                "id": "contract-composite-submit",
                                                "props": {
                                                    "label": "Submit",
                                                    "action": {
                                                        "id": "contract.form.submit"
                                                    }
                                                }
                                            }
                                        ]
                                    },
                                    {
                                        "type": "empty_state",
                                        "id": "contract-composite-empty",
                                        "props": {
                                            "title": "No blocked tickets",
                                            "description": "All current work can continue"
                                        }
                                    }
                                ],
                                "actions": [
                                    {
                                        "type": "button",
                                        "id": "contract-composite-action-feedback",
                                        "props": {
                                            "label": "Acknowledge",
                                            "action": {
                                                "id": "contract.feedback.ack",
                                                "payload": { "state": "accepted" }
                                            }
                                        }
                                    }
                                ]
                            }
                        }
                    ]
                }
            }),
            ui_tree_snapshot: None,
        }
    }

    fn invalid_table_plugin_surface() -> DaemonPluginSurface {
        DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.invalid".to_string(),
            body: json!({
                "type": "table",
                "id": "contract-invalid-table"
            }),
            ui_tree_snapshot: None,
        }
    }

    fn iframe_plugin_surface() -> DaemonPluginSurface {
        DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.iframe".to_string(),
            body: json!({
                "type": "panel",
                "id": "contract-iframe-panel",
                "props": {
                    "title": "Contract HTML Host"
                },
                "children": [
                    {
                        "type": "iframe",
                        "id": "contract-html-frame",
                        "props": {
                            "title": "Contract HTML",
                            "src": "/assets/botster.plugin-contract-matrix/contract.html",
                            "sandbox": ["allow_scripts"]
                        }
                    }
                ]
            }),
            ui_tree_snapshot: None,
        }
    }

    fn plugin_contract_app_navigation() -> DaemonPackageNavigationEntry {
        DaemonPackageNavigationEntry {
            package_name: "botster.plugin-contract-matrix".to_string(),
            item_id: "contract.app".to_string(),
            label: "Contract App".to_string(),
            icon: Some("workflow".to_string()),
            description: Some("Plugin contract app".to_string()),
            route_id: "surface:contract.app".to_string(),
            route_path: "/packages/botster.plugin-contract-matrix/surfaces/contract.app"
                .to_string(),
            target: botster_hub_client::DaemonPackageRouteTarget {
                kind: "plugin_surface".to_string(),
                entrypoint_id: None,
                surface_id: Some("contract.app".to_string()),
            },
            source: botster_hub_client::DaemonPackageNavigationSource {
                kind: "surface".to_string(),
                surface_id: Some("contract.app".to_string()),
                entrypoint_id: None,
            },
            enabled: true,
            blocked: false,
            diagnostics: Vec::new(),
        }
    }

    fn plugin_contract_app_route() -> DaemonPackageRouteDescriptor {
        DaemonPackageRouteDescriptor {
            package_name: "botster.plugin-contract-matrix".to_string(),
            route_id: "surface:contract.app".to_string(),
            route_path: "/packages/botster.plugin-contract-matrix/surfaces/contract.app"
                .to_string(),
            target: botster_hub_client::DaemonPackageRouteTarget {
                kind: "plugin_surface".to_string(),
                entrypoint_id: None,
                surface_id: Some("contract.app".to_string()),
            },
            title: "Contract App".to_string(),
            label: "Contract App".to_string(),
            app_id: Some("contract.app".to_string()),
            surface_id: Some("contract.app".to_string()),
            icon: None,
            category: None,
            layout_mode: "host".to_string(),
            required_capabilities: Vec::new(),
            enabled: true,
            blocked: false,
            diagnostics: Vec::new(),
            supports_settings: false,
        }
    }

    fn plugin_contract_settings_route() -> DaemonPackageRouteDescriptor {
        DaemonPackageRouteDescriptor {
            package_name: "botster.plugin-contract-matrix".to_string(),
            route_id: "settings".to_string(),
            route_path: "/packages/botster.plugin-contract-matrix/settings".to_string(),
            target: botster_hub_client::DaemonPackageRouteTarget {
                kind: "settings".to_string(),
                entrypoint_id: None,
                surface_id: Some("contract.settings".to_string()),
            },
            title: "Contract Settings".to_string(),
            label: "Settings".to_string(),
            app_id: None,
            surface_id: Some("contract.settings".to_string()),
            icon: None,
            category: None,
            layout_mode: "host".to_string(),
            required_capabilities: Vec::new(),
            enabled: true,
            blocked: false,
            diagnostics: Vec::new(),
            supports_settings: true,
        }
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
            source_kind: "local".to_string(),
            state: state.to_string(),
            requested_capabilities,
            surfaces: Vec::new(),
            routes: Vec::new(),
            runnable_entrypoints: Vec::new(),
            configuration: botster_hub_client::DaemonPackageConfiguration::default(),
            availability: botster_hub_client::DaemonPackageAvailability::default(),
            dependency_availability: Vec::new(),
            feature_availability: Vec::new(),
            actions: Vec::new(),
            provider_profile_admitted,
        }
    }

    fn package_with_configuration() -> DaemonPackage {
        let mut package = package(
            "configuration.plugin",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        package.configuration = botster_hub_client::DaemonPackageConfiguration {
            schema: Some(json!({
                "fields": [
                    {
                        "key": "endpoint",
                        "type": "url",
                        "label": "Endpoint",
                        "required": true,
                        "order": 1
                    },
                    {
                        "key": "debug",
                        "type": "boolean",
                        "label": "Debug",
                        "order": 2
                    },
                    {
                        "key": "mode",
                        "type": "select",
                        "label": "Mode",
                        "order": 3,
                        "options": [
                            { "value": "read", "label": "Read" },
                            { "value": "write", "label": "Write" }
                        ]
                    },
                    {
                        "key": "notes",
                        "type": "multiline_text",
                        "label": "Notes",
                        "order": 4
                    },
                    {
                        "key": "api_token",
                        "type": "secret",
                        "label": "API token",
                        "required": true,
                        "order": 5
                    }
                ]
            })),
            effective_values: BTreeMap::from([
                (
                    "endpoint".to_string(),
                    json!({"type":"url","value":"https://example.invalid/hook"}),
                ),
                ("debug".to_string(), json!({"type":"boolean","value":true})),
                ("mode".to_string(), json!({"type":"select","value":"read"})),
                (
                    "notes".to_string(),
                    json!({"type":"multiline_text","value":"Line one"}),
                ),
                (
                    "api_token".to_string(),
                    json!({"type":"secret","state":"redacted"}),
                ),
            ]),
            missing_required: vec!["endpoint".to_string()],
            diagnostics: vec![package_diagnostic("schema", "manifest warning")],
        };
        package
    }

    fn entrypoint(
        id: &str,
        kind: &str,
        process: botster_hub_client::DaemonPackageProcess,
    ) -> botster_hub_client::DaemonPackageRunnableEntrypoint {
        botster_hub_client::DaemonPackageRunnableEntrypoint {
            id: id.to_string(),
            kind: kind.to_string(),
            launch_mode: "dev".to_string(),
            command: "bin/run".to_string(),
            args: Vec::new(),
            working_directory: botster_hub_client::DaemonPackageWorkingDirectory {
                policy: "package_root".to_string(),
                path: None,
            },
            environment: Vec::new(),
            capabilities: Vec::new(),
            may_supervise: true,
            process,
            actions: Vec::new(),
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

    fn availability_reason(
        reason: &str,
        action: &str,
        package_name: Option<&str>,
        capability: Option<botster_hub_client::DaemonCapability>,
        requirement: Option<&str>,
    ) -> DaemonPackageAvailabilityReason {
        DaemonPackageAvailabilityReason {
            reason: reason.to_string(),
            action: action.to_string(),
            package_name: package_name.map(str::to_string),
            capability,
            requirement: requirement.map(str::to_string),
        }
    }

    fn available_package() -> DaemonAvailablePackage {
        DaemonAvailablePackage {
            entry_id: "workflow-plugin".to_string(),
            package_name: "workflow.plugin".to_string(),
            version: "1.2.0".to_string(),
            classification: "plugin".to_string(),
            source_kind: "registry".to_string(),
            source_label: "first-party catalog".to_string(),
            first_party: true,
            state: "available".to_string(),
            requested_capabilities: vec![capability("mcp", Some("tools"))],
            compatibility: botster_hub_client::DaemonPackageCompatibility {
                botster_requirement: ">=0.1.0".to_string(),
                hub_version: "0.1.0".to_string(),
                result: "compatible".to_string(),
                diagnostics: vec!["requires current hub".to_string()],
            },
            pin: Some(package_pin()),
            actions: Vec::new(),
        }
    }

    fn web_app_with_url() -> DaemonApp {
        DaemonApp {
            package_name: "workflow.plugin".to_string(),
            app_id: "dashboard".to_string(),
            entrypoint_id: "web".to_string(),
            kind: "web_app".to_string(),
            launch_mode: "supervised".to_string(),
            lifecycle_state: "running".to_string(),
            diagnostics: Vec::new(),
            actions: Vec::new(),
            blocked_reasons: Vec::new(),
            launch_target: botster_hub_client::DaemonAppLaunchTarget {
                kind: "web_app".to_string(),
                local_url: Some("http://127.0.0.1:49152".to_string()),
            },
            route: None,
        }
    }

    fn terminal_app() -> DaemonApp {
        DaemonApp {
            package_name: "botster-tui".to_string(),
            app_id: "tui".to_string(),
            entrypoint_id: "tui".to_string(),
            kind: "terminal_app".to_string(),
            launch_mode: "foreground_stdio".to_string(),
            lifecycle_state: "launchable".to_string(),
            diagnostics: Vec::new(),
            actions: Vec::new(),
            blocked_reasons: Vec::new(),
            launch_target: botster_hub_client::DaemonAppLaunchTarget {
                kind: "terminal_app".to_string(),
                local_url: None,
            },
            route: None,
        }
    }

    fn action_state(
        action_id: &str,
        status: botster_hub_client::DaemonPackageActionStatus,
        reason: Option<&str>,
        request: Option<botster_hub_client::DaemonPackageActionRequest>,
    ) -> botster_hub_client::DaemonPackageActionState {
        botster_hub_client::DaemonPackageActionState {
            action_id: action_id.to_string(),
            status,
            reason: reason.map(str::to_string),
            diagnostics: Vec::new(),
            required_references: Vec::new(),
            request,
        }
    }

    fn action_request(request_type: &str) -> botster_hub_client::DaemonPackageActionRequest {
        botster_hub_client::DaemonPackageActionRequest {
            request_type: request_type.to_string(),
            pin: None,
            package_name: Some("botster-tui".to_string()),
            entry_id: None,
            entrypoint_id: Some("tui".to_string()),
            registry_path: None,
        }
    }

    fn package_pin() -> DaemonPackagePin {
        DaemonPackagePin {
            revision: "rev-2026".to_string(),
            branch: Some("main".to_string()),
            tag: None,
            rev: Some("3c7a448".to_string()),
            checksum: None,
            update_policy: "manual".to_string(),
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
        events_response(vec![DaemonEvent::AttachState {
            session_id: session_id.to_string(),
            subscription_id: "sub-test".to_string(),
            state: state.to_string(),
        }])
    }

    fn events_response(events: Vec<DaemonEvent>) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::Events);
        response.events = events;
        response
    }

    fn read_screen_response(session_id: &str, text: &str) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::ReadScreen);
        response.read_screen = Some(botster_hub_client::DaemonReadScreen {
            session_id: session_id.to_string(),
            text: text.to_string(),
        });
        response
    }

    fn capture_snapshot_response(
        session_id: &str,
        rows: u16,
        cols: u16,
        payload_format: Option<&str>,
        payload_bytes: usize,
    ) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::CaptureSnapshot);
        response.capture_snapshot = Some(DaemonCaptureSnapshot {
            session_id: session_id.to_string(),
            rows,
            cols,
            payload_format: payload_format.map(str::to_string),
            payload_bytes,
        });
        response
    }

    fn base_response(kind: DaemonResponseKind) -> DaemonResponse {
        DaemonResponse {
            kind,
            status: None,
            sessions: Vec::new(),
            session_templates: Vec::new(),
            resolved_session_template: None,
            session_context: None,
            read_screen: None,
            capture_snapshot: None,
            spawn_targets: Vec::new(),
            spawn_target_validation: None,
            worktrees: Vec::new(),
            apps: Vec::new(),
            resolved_app_launch: None,
            resolved_package_route: None,
            package_navigation: Vec::new(),
            packages: Vec::new(),
            available_packages: Vec::new(),
            install_plan: None,
            update_status: None,
            package_decision: None,
            lifecycle: Vec::new(),
            plugin_tools: Vec::new(),
            plugin_tool_result: Value::Null,
            plugin_surface: None,
            plugin_action_result: None,
            local_webrtc_bootstrap: None,
            local_webrtc_answer: None,
            events: Vec::new(),
            cleanup: None,
            coordination: None,
            error: None,
            diagnostics: Vec::new(),
        }
    }
}
