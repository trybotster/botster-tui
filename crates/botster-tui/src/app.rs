use std::{
    collections::BTreeMap,
    io::{self, Stdout},
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use botster_core::{RunnableEntrypointHubConnection, RunnableEntrypointHubConnectionTransport};
#[cfg(test)]
use botster_hub_client::DaemonOpaqueHistoryPayload;
use botster_hub_client::{
    DaemonApp, DaemonAvailablePackage, DaemonCaptureSnapshot, DaemonCompatibility,
    DaemonCompatibilityRequirement, DaemonDiagnostic, DaemonDiagnosticKind, DaemonEndpoint,
    DaemonEntityFrame, DaemonEvent, DaemonPackage, DaemonPackageAvailabilityReason,
    DaemonPackageAvailabilityState, DaemonPackageInstallPlan, DaemonPackageNavigationEntry,
    DaemonPackagePin, DaemonPackageRouteDescriptor, DaemonPackageUpdateStatus, DaemonPluginSurface,
    DaemonRequest, DaemonResponse, DaemonResponseKind, DaemonSessionEntity, DaemonTransportError,
    DaemonTransportResult, FEATURE_PACKAGE_NAVIGATION, FEATURE_PLUGIN_SURFACE_ACTION,
    FEATURE_PLUGIN_SURFACE_RENDER, FEATURE_RESIZE, FEATURE_SESSION_ENTITY_SUBSCRIPTIONS,
    FEATURE_SESSIONS, FEATURE_TERMINAL_READBACK, FEATURE_TERMINAL_STREAMING, PROTOCOL,
    connect_and_hello_with_requirement, read_frame_from_reader, subscribe_session_entities,
    write_frame,
};
use botster_ui_contract::{
    PackageSurfaceDescriptor, PackageSurfaceKind, PackageSurfaceOperation, UiActionRequest,
    UiActionResult, UiAuthoredNodeId, UiChild, UiCondition, UiConditional, UiFormValues, UiNode,
    UiNodeId, UiNodeKind, UiWidthClass,
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

use crate::acceptance::{Config as AcceptanceConfig, EvidenceWriter, FailureContext, ScenarioCase};
use crate::renderer::{self, HitMap, InputDispatch, InputRouter, RenderState};

const PACKAGE_CONFIG_FIELD_PREFIX: &str = "package-config";
const DEFAULT_COMMAND: &str = "printf 'botster-tui-ready\\n'; while IFS= read -r line; do printf 'echo:%s\\n' \"$line\"; done";
const HEADLESS_INPUT: &str = "botster-tui-headless\n";
const HEADLESS_OUTPUT: &str = "echo:botster-tui-headless";
const SMOKE_MESSAGE: &str = "botster-tui smoke ok";
const ATTACH_HYDRATION_TIMEOUT: Duration = Duration::from_secs(5);
const TERMINAL_MOUSE_MODE_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const SESSION_ENTITY_READ_TIMEOUT: Duration = Duration::from_millis(250);
const SESSION_ENTITY_STOP_TIMEOUT: Duration = Duration::from_millis(750);
const MINIMUM_CONFORMANCE_FIXTURE_REVISION: u16 = 25;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppArgs {
    pub smoke: bool,
    pub hub_connection: Option<RunnableEntrypointHubConnection>,
    pub connection_error: Option<String>,
    pub hub_data_dir: Option<PathBuf>,
    pub headless_live_runtime: bool,
}

impl AppArgs {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Self {
        Self::parse_with_environment(
            args,
            std::env::var_os("BOTSTER_HUB_CONNECTION"),
            std::env::var_os("BOTSTER_HUB_DATA_DIR"),
            std::env::var_os("BOTSTER_TUI_HEADLESS_LIVE_RUNTIME").is_some(),
        )
    }

    fn parse_with_environment(
        args: impl IntoIterator<Item = String>,
        hub_connection: Option<std::ffi::OsString>,
        hub_data_dir: Option<std::ffi::OsString>,
        headless_live_runtime: bool,
    ) -> Self {
        let mut parsed = Self::default();
        for arg in args {
            match arg.as_str() {
                "--smoke" => parsed.smoke = true,
                "--headless-live-runtime" => parsed.headless_live_runtime = true,
                _ => {}
            }
        }
        let (connection, connection_error) = parse_hub_connection(hub_connection);
        parsed.hub_connection = connection;
        parsed.connection_error = connection_error;
        parsed.hub_data_dir = hub_data_dir.map(PathBuf::from);
        if headless_live_runtime {
            parsed.headless_live_runtime = true;
        }
        parsed
    }

    fn daemon_endpoint(&self) -> Option<DaemonEndpoint> {
        self.hub_connection
            .as_ref()
            .map(|connection| match &connection.transport {
                RunnableEntrypointHubConnectionTransport::UnixSocket { path } => {
                    DaemonEndpoint::new(path)
                }
            })
    }
}

fn parse_hub_connection(
    value: Option<std::ffi::OsString>,
) -> (Option<RunnableEntrypointHubConnection>, Option<String>) {
    let Some(value) = value else {
        return (None, Some("BOTSTER_HUB_CONNECTION is required".to_string()));
    };
    let value = match value.into_string() {
        Ok(value) => value,
        Err(_) => {
            return (
                None,
                Some("BOTSTER_HUB_CONNECTION must contain UTF-8 JSON".to_string()),
            );
        }
    };
    let connection = match serde_json::from_str::<RunnableEntrypointHubConnection>(&value) {
        Ok(connection) => connection,
        Err(error) => {
            return (
                None,
                Some(format!("BOTSTER_HUB_CONNECTION is malformed: {error}")),
            );
        }
    };
    if let Err(error) = connection.validate() {
        return (
            None,
            Some(format!("BOTSTER_HUB_CONNECTION is invalid: {error}")),
        );
    }
    (Some(connection), None)
}

pub fn smoke_message() -> &'static str {
    SMOKE_MESSAGE
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionRow {
    session_id: String,
    lifecycle: String,
    failure_reason: Option<String>,
    pending: bool,
}

#[derive(Clone, Debug)]
struct AttachHydration {
    session_id: String,
    subscription_id: String,
    deadline: Instant,
    read_screen_requested: bool,
    buffered_live_output: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DestructiveAction {
    Shutdown(String),
    Remove(String),
}

#[derive(Default)]
struct HydrationEvidence {
    opaque_state_received: bool,
    lifecycle_ended: bool,
}

impl SessionRow {
    #[cfg(test)]
    fn running(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            lifecycle: "running".to_string(),
            failure_reason: None,
            pending: false,
        }
    }

    fn pending(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            lifecycle: "pending".to_string(),
            failure_reason: None,
            pending: true,
        }
    }

    fn from_entity(entity: &DaemonSessionEntity) -> Self {
        Self {
            session_id: entity.session_uuid.clone(),
            lifecycle: entity
                .lifecycle
                .clone()
                .unwrap_or_else(|| entity.registry_state.clone()),
            failure_reason: entity.failure_reason.clone(),
            pending: false,
        }
    }

    fn is_attachable(&self) -> bool {
        !self.pending && self.lifecycle == "running"
    }
}

#[derive(Default)]
struct SessionEntityState {
    subscription_id: Option<String>,
    has_snapshot: bool,
    snapshot_seq: Option<u64>,
    entity_order: Vec<String>,
    entities: BTreeMap<String, DaemonSessionEntity>,
}

impl SessionEntityState {
    fn begin_generation(&mut self, subscription_id: String) {
        self.subscription_id = Some(subscription_id);
        self.has_snapshot = false;
        self.snapshot_seq = None;
        self.entity_order.clear();
        self.entities.clear();
    }

    fn apply(&mut self, frame: DaemonEntityFrame) -> Result<bool, String> {
        match frame {
            DaemonEntityFrame::Snapshot {
                subscription_id,
                entity_type,
                snapshot_seq,
                items,
                ..
            } => {
                if !self.matches(&subscription_id, &entity_type) {
                    return Ok(false);
                }
                self.entity_order = items
                    .iter()
                    .map(|entity| entity.session_uuid.clone())
                    .collect();
                self.entities = items
                    .into_iter()
                    .map(|entity| (entity.session_uuid.clone(), entity))
                    .collect();
                self.has_snapshot = true;
                self.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
            DaemonEntityFrame::Upsert {
                subscription_id,
                entity_type,
                snapshot_seq,
                id,
                entity,
            } => {
                if !self.accepts_delta(&subscription_id, &entity_type, snapshot_seq) {
                    return Ok(false);
                }
                if id != entity.session_uuid {
                    return Err(format!(
                        "session entity id mismatch: frame={id} entity={}",
                        entity.session_uuid
                    ));
                }
                if !self.entities.contains_key(&id) {
                    self.entity_order.push(id.clone());
                }
                self.entities.insert(id, entity);
                self.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
            DaemonEntityFrame::Patch {
                subscription_id,
                entity_type,
                snapshot_seq,
                id,
                patch,
            } => {
                if !self.accepts_delta(&subscription_id, &entity_type, snapshot_seq) {
                    return Ok(false);
                }
                let Some(entity) = self.entities.get(&id) else {
                    return Ok(false);
                };
                let mut value = serde_json::to_value(entity).map_err(|error| error.to_string())?;
                let Some(target) = value.as_object_mut() else {
                    return Err("session entity did not serialize as an object".to_string());
                };
                let Some(fields) = patch.as_object() else {
                    return Err("session entity patch was not an object".to_string());
                };
                for (key, value) in fields {
                    target.insert(key.clone(), value.clone());
                }
                let entity = serde_json::from_value(value).map_err(|error| error.to_string())?;
                self.entities.insert(id, entity);
                self.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
            DaemonEntityFrame::Remove {
                subscription_id,
                entity_type,
                snapshot_seq,
                id,
            } => {
                if !self.accepts_delta(&subscription_id, &entity_type, snapshot_seq) {
                    return Ok(false);
                }
                self.entities.remove(&id);
                self.entity_order.retain(|entity_id| entity_id != &id);
                self.snapshot_seq = Some(snapshot_seq);
                Ok(true)
            }
        }
    }

    fn matches(&self, subscription_id: &str, entity_type: &str) -> bool {
        entity_type == "session" && self.subscription_id.as_deref() == Some(subscription_id)
    }

    fn accepts_delta(&self, subscription_id: &str, entity_type: &str, snapshot_seq: u64) -> bool {
        self.has_snapshot
            && self.matches(subscription_id, entity_type)
            && self
                .snapshot_seq
                .is_none_or(|current| snapshot_seq > current)
    }

    fn binding_rows(&self) -> Result<Vec<Value>, String> {
        let reference = session_binding_reference_row();
        self.entity_order
            .iter()
            .filter_map(|session_uuid| {
                self.entities
                    .get(session_uuid)
                    .map(|entity| (session_uuid, entity))
            })
            .map(|(session_uuid, entity)| {
                let mut value = serde_json::to_value(entity).map_err(|error| {
                    format!("session entity {session_uuid} failed binding serialization: {error}")
                })?;
                let row = value.as_object_mut().ok_or_else(|| {
                    format!("session entity {session_uuid} did not serialize as an object")
                })?;
                for field in reference.keys() {
                    row.entry(field.clone()).or_insert(Value::Null);
                }
                Ok(value)
            })
            .collect()
    }
}

fn session_binding_reference_row() -> serde_json::Map<String, Value> {
    serde_json::to_value(DaemonSessionEntity {
        session_uuid: "reference-session".to_string(),
        registry_state: "running".to_string(),
        lifecycle: Some("running".to_string()),
        lifecycle_class: "current".to_string(),
        rows: 24,
        cols: 80,
        updated_at: 1,
        exit_code: Some(0),
        failure_reason: Some("reference failure".to_string()),
    })
    .expect("exhaustive session binding reference row must serialize")
    .as_object()
    .expect("session binding reference row must serialize as an object")
    .clone()
}

enum SessionSubscriptionMessage {
    Frame(DaemonEntityFrame),
    Disconnected {
        subscription_id: String,
        error: String,
    },
}

struct SessionSubscriptionPump {
    messages: Receiver<SessionSubscriptionMessage>,
    cancel: Option<mpsc::Sender<()>>,
    stopped: Receiver<()>,
    stop_attempted: bool,
    stopped_confirmed: bool,
}

impl SessionSubscriptionPump {
    fn stop(&mut self) -> bool {
        if self.stopped_confirmed {
            return true;
        }
        if self.stop_attempted {
            return false;
        }
        self.stop_attempted = true;
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        self.stopped_confirmed = self
            .stopped
            .recv_timeout(SESSION_ENTITY_STOP_TIMEOUT)
            .is_ok();
        self.stopped_confirmed
    }
}

impl Drop for SessionSubscriptionPump {
    fn drop(&mut self) {
        if !self.stop_attempted {
            let _ = self.stop();
        }
    }
}

pub fn run(args: AppArgs) -> io::Result<()> {
    if let Some(config) = AcceptanceConfig::from_environment()? {
        return run_workspaces_acceptance(args, config);
    }
    if args.headless_live_runtime {
        return run_headless_live_runtime(args)
            .map_err(|error| io::Error::other(format!("headless live runtime failed: {error}")));
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
    let mut app = TuiApp::new_with_runtime_context(
        args.daemon_endpoint(),
        args.connection_error,
        args.hub_data_dir.is_some(),
    );
    let mut router = InputRouter::new(renderer::action_request_context());
    let mut routed_surface_id = None;
    loop {
        app.poll_hub();
        let active_surface_id = app.active_plugin_surface_id().map(ToOwned::to_owned);
        if active_surface_id != routed_surface_id {
            router = InputRouter::new(match active_surface_id.as_deref() {
                Some(surface_id) => renderer::action_request_context_for(surface_id),
                None => renderer::action_request_context(),
            });
            routed_surface_id = active_surface_id;
        }
        app.set_drafts(router.draft_values());

        let mut hit_map = HitMap::default();
        let render_state = router.render_state();
        terminal.draw(|frame| draw(frame, &mut hit_map, &app, &render_state))?;
        app.apply_terminal_mouse_mode(&mut hit_map);
        router.reconcile(&hit_map);

        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            match event {
                Event::Key(key)
                    if key.kind == KeyEventKind::Press && app.handle_tui_owned_key(key) => {}
                Event::Key(key) if key.kind == KeyEventKind::Press && should_quit(key) => break,
                _ => {
                    let dispatch = router.dispatch_event(event, &hit_map);
                    app.sync_focused_session(router.selected_row_value("tui-session-list"));
                    app.handle_dispatch(dispatch);
                }
            }
        }
    }

    Ok(())
}

fn draw(frame: &mut Frame<'_>, hit_map: &mut HitMap, app: &TuiApp, render_state: &RenderState) {
    let node = app.surface();
    renderer::render_node_with_presentation_state(
        frame,
        frame.area(),
        &node,
        hit_map,
        render_state,
        &app.plugin_presentation,
    );
}

fn should_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || matches!(key.code, KeyCode::Char('q' | 'Q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

struct TuiApp {
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
    plugin_presentation: renderer::PresentationState,
    plugin_action_result: Option<UiActionResult>,
    pending_plugin_request: Option<UiActionRequest>,
    session_entities: SessionEntityState,
    pending_sessions: BTreeMap<String, SessionRow>,
    session_subscription: Option<SessionSubscriptionPump>,
    sessions: Vec<SessionRow>,
    selected_session: Option<String>,
    attached_session: Option<String>,
    attached_subscription_id: Option<String>,
    schema_version: Option<u16>,
    subscription_id: String,
    terminal_output: String,
    terminal_output_session_id: Option<String>,
    snapshot_metadata: Option<DaemonCaptureSnapshot>,
    attach_hydration: Option<AttachHydration>,
    terminal_mouse_mode: u8,
    terminal_mouse_mode_attachment: Option<(String, String)>,
    terminal_mouse_mode_refresh_due: bool,
    last_terminal_mouse_mode_probe: Option<Instant>,
    command: String,
    drafts: BTreeMap<String, Value>,
    system_details_visible: bool,
    package_storage_context_configured: bool,
    confirmation: Option<DestructiveAction>,
    #[cfg(test)]
    workspace_test_mode: bool,
    last_reconnect_attempt: Option<Instant>,
    acceptance_audit: Option<AcceptanceRequestAudit>,
    #[cfg(test)]
    observed_requests: Vec<ObservedRequest>,
}

impl TuiApp {
    fn new(endpoint: Option<DaemonEndpoint>) -> Self {
        Self::new_with_connection(endpoint, None)
    }

    fn new_with_connection(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
    ) -> Self {
        Self::new_with_runtime_context(endpoint, connection_error, false)
    }

    fn new_with_runtime_context(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
        package_storage_context_configured: bool,
    ) -> Self {
        let mut app = Self {
            endpoint,
            client: None,
            status: "disconnected".to_string(),
            connection_error,
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
            plugin_presentation: renderer::PresentationState::default(),
            plugin_action_result: None,
            pending_plugin_request: None,
            session_entities: SessionEntityState::default(),
            pending_sessions: BTreeMap::new(),
            session_subscription: None,
            sessions: Vec::new(),
            selected_session: None,
            attached_session: None,
            attached_subscription_id: None,
            schema_version: None,
            subscription_id: format!("btui-sub-{}", short_suffix()),
            terminal_output: String::new(),
            terminal_output_session_id: None,
            snapshot_metadata: None,
            attach_hydration: None,
            terminal_mouse_mode: 0,
            terminal_mouse_mode_attachment: None,
            terminal_mouse_mode_refresh_due: false,
            last_terminal_mouse_mode_probe: None,
            command: DEFAULT_COMMAND.to_string(),
            drafts: BTreeMap::new(),
            system_details_visible: false,
            package_storage_context_configured,
            confirmation: None,
            #[cfg(test)]
            workspace_test_mode: false,
            last_reconnect_attempt: None,
            acceptance_audit: None,
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
        if self.drain_session_subscription() {
            return;
        }
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
        self.refresh_terminal_mouse_mode_if_due();
    }

    fn handle_dispatch(&mut self, dispatch: InputDispatch) {
        match dispatch {
            InputDispatch::Action(request) => {
                if self.plugin_surface.is_some() {
                    self.handle_plugin_action(request);
                } else {
                    self.handle_action(request.action_id.0, request.values, request.payload);
                }
            }
            InputDispatch::TerminalForward { bytes, .. } => {
                let Some(session_id) = self.attached_session.clone() else {
                    self.error = Some(
                        "terminal stream unavailable: attach a session before sending terminal input"
                            .to_string(),
                    );
                    return;
                };
                if self.attached_subscription_id.as_deref() != Some(self.subscription_id.as_str()) {
                    self.error = Some(
                        "terminal stream unavailable: current subscription is not attached"
                            .to_string(),
                    );
                    return;
                }
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

    fn active_plugin_surface_id(&self) -> Option<&str> {
        self.plugin_surface
            .as_ref()
            .map(|surface| surface.surface_id.as_str())
    }

    fn clear_active_plugin_surface(&mut self) -> bool {
        if self.plugin_surface.is_none() {
            return false;
        }
        self.reset_active_plugin_surface();
        self.system_details_visible = true;
        self.action_feedback = Some("returned to System".to_string());
        true
    }

    fn handle_tui_owned_key(&mut self, key: KeyEvent) -> bool {
        if key.code != KeyCode::Esc || key.modifiers != KeyModifiers::NONE {
            return false;
        }
        if self.confirmation.is_some() {
            self.confirmation = None;
            return true;
        }
        self.clear_active_plugin_surface()
    }

    fn reset_active_plugin_surface(&mut self) {
        self.plugin_surface = None;
        self.plugin_presentation = renderer::PresentationState::default();
        self.plugin_action_result = None;
        self.pending_plugin_request = None;
    }

    fn apply_plugin_action_result(&mut self, result: UiActionResult) {
        let Some(request) = self.pending_plugin_request.as_ref() else {
            self.error = Some(format!(
                "ignored plugin action result without an in-flight request: {}",
                result.request_id.0
            ));
            return;
        };
        let Some(surface) = self.plugin_surface.as_mut() else {
            self.error = Some("ignored plugin action result without an active owner".to_string());
            return;
        };
        let identity_matches = result.request_id == request.request_id
            && result.surface_id == request.surface_id
            && result.action_id == request.action_id
            && result.node_id == request.node_id
            && result.surface_id.0 == surface.surface_id;
        if !identity_matches {
            self.error = Some(format!(
                "ignored mismatched plugin action result: request={} result={}",
                request.request_id.0, result.request_id.0
            ));
            return;
        }

        match renderer::apply_action_result(&mut self.plugin_presentation, &result) {
            Ok(transition) => {
                if let Some(replacement) = transition.replacement {
                    surface.body = replacement;
                    // The snapshot validates the Hub-delivered tree at ingestion. An accepted
                    // action replacement is app-owned active state and must not leave a second,
                    // stale structural tree that looks current.
                    surface.ui_tree_snapshot = None;
                }
                self.pending_plugin_request = None;
                self.action_feedback = Some(plugin_action_result_text(&result));
                self.plugin_action_result = Some(result);
            }
            Err(error) => {
                self.error = Some(format!("invalid plugin action result: {error}"));
            }
        }
    }

    fn handle_plugin_action(&mut self, request: UiActionRequest) {
        let Some(surface) = self.plugin_surface.as_ref() else {
            return;
        };
        if request.surface_id.0 != surface.surface_id {
            self.error = Some(format!(
                "plugin action surface mismatch: active={} request={}",
                surface.surface_id, request.surface_id.0
            ));
            return;
        }

        let package_name = surface.package_name.clone();
        self.error = None;
        self.action_feedback = Some(format!(
            "plugin action requested: {package_name}/{}",
            request.action_id.0
        ));
        self.pending_plugin_request = Some(request.clone());
        self.request_and_apply(DaemonRequest::PluginSurfaceAction {
            package_name,
            request,
        });
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
            "botster.tui.select" => {
                if let Some(session_id) = session_id_from_payload(&payload)
                    && self
                        .sessions
                        .iter()
                        .any(|session| session.session_id == session_id)
                {
                    self.selected_session = Some(session_id);
                }
            }
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
            "botster.tui.system.toggle" => {
                self.system_details_visible = !self.system_details_visible;
            }
            "botster.tui.session.shutdown" => {
                if let Some(session_id) =
                    session_id_from_payload(&payload).or_else(|| self.selected_session.clone())
                {
                    self.confirmation = Some(DestructiveAction::Shutdown(session_id));
                }
            }
            "botster.tui.session.remove" => {
                if let Some(session_id) =
                    session_id_from_payload(&payload).or_else(|| self.selected_session.clone())
                {
                    self.confirmation = Some(DestructiveAction::Remove(session_id));
                }
            }
            "botster.tui.confirm.cancel" => {
                self.confirmation = None;
            }
            "botster.tui.confirm.accept" => {
                if let Some(confirmation) = self.confirmation.take() {
                    match confirmation {
                        DestructiveAction::Shutdown(session_id) => {
                            self.action_feedback =
                                Some(format!("shutdown requested: {session_id}"));
                            self.request_and_apply(DaemonRequest::ShutdownSession { session_id });
                        }
                        DestructiveAction::Remove(session_id) => {
                            self.action_feedback = Some(format!("remove requested: {session_id}"));
                            self.request_and_apply(DaemonRequest::RemoveSession { session_id });
                        }
                    }
                }
            }
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
            "botster.tui.package.show" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("show requested: {package_name}"));
                    self.request_and_apply(DaemonRequest::ShowPackage { package_name });
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
        self.reset_active_plugin_surface();
        if !self.invalidate_session_generation() {
            self.error = Some("session subscription cleanup timed out".to_string());
        }
        self.attached_session = None;
        self.attached_subscription_id = None;
        self.attach_hydration = None;
        self.clear_terminal_mouse_mode();
        self.try_connect();
    }

    fn try_connect(&mut self) {
        self.last_reconnect_attempt = Some(Instant::now());
        let Some(endpoint) = &self.endpoint else {
            self.status = "Hub connection not configured".to_string();
            if self.connection_error.is_none() {
                self.connection_error = Some("BOTSTER_HUB_CONNECTION is required".to_string());
            }
            return;
        };
        match HubConnection::connect(endpoint) {
            Ok(client) => {
                self.client = Some(client);
                self.status = "connected".to_string();
                self.connection_error = None;
                self.refresh_read_models();
                if let Err(error) = self.start_session_subscription() {
                    self.record_transport_error(error);
                }
            }
            Err(error) => {
                self.record_transport_error(error);
            }
        }
    }

    fn refresh_read_models(&mut self) {
        self.refresh_status();
        self.refresh_apps();
        self.refresh_package_navigation();
        self.refresh_packages();
    }

    fn refresh_status(&mut self) {
        self.request_and_apply(DaemonRequest::Status);
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

    fn start_session_subscription(&mut self) -> DaemonTransportResult<()> {
        let endpoint = self
            .endpoint
            .as_ref()
            .ok_or(DaemonTransportError::NotRunning)?;
        let subscription_id = format!("btui-sessions-{}", short_suffix());
        let mut subscription = subscribe_session_entities(endpoint, subscription_id.clone())?;
        subscription.set_read_timeout(Some(SESSION_ENTITY_READ_TIMEOUT))?;
        let (sender, receiver) = mpsc::channel();
        let (cancel_sender, cancel_receiver) = mpsc::channel();
        let (stopped_sender, stopped_receiver) = mpsc::channel();
        let reader_subscription_id = subscription_id.clone();

        thread::Builder::new()
            .name("botster-tui-session-entities".to_string())
            .spawn(move || {
                loop {
                    if cancel_receiver.try_recv().is_ok() {
                        let _ = subscription.unsubscribe();
                        break;
                    }
                    match subscription.next_frame() {
                        Ok(frame) => {
                            if sender
                                .send(SessionSubscriptionMessage::Frame(frame))
                                .is_err()
                            {
                                break;
                            }
                        }
                        Err(DaemonTransportError::Io(error))
                            if matches!(
                                error.kind(),
                                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                            ) => {}
                        Err(error) => {
                            let _ = sender.send(SessionSubscriptionMessage::Disconnected {
                                subscription_id: reader_subscription_id,
                                error: error.to_string(),
                            });
                            break;
                        }
                    }
                }
                let _ = stopped_sender.send(());
            })
            .map_err(DaemonTransportError::Io)?;
        self.session_entities
            .begin_generation(subscription_id.clone());
        self.session_subscription = Some(SessionSubscriptionPump {
            messages: receiver,
            cancel: Some(cancel_sender),
            stopped: stopped_receiver,
            stop_attempted: false,
            stopped_confirmed: false,
        });
        self.rebuild_session_rows();
        Ok(())
    }

    fn drain_session_subscription(&mut self) -> bool {
        let messages = self
            .session_subscription
            .as_ref()
            .map(|pump| pump.messages.try_iter().collect::<Vec<_>>())
            .unwrap_or_default();
        for message in messages {
            match message {
                SessionSubscriptionMessage::Frame(frame) => {
                    match self.session_entities.apply(frame) {
                        Ok(true) => self.rebuild_session_rows(),
                        Ok(false) => {}
                        Err(error) => self.error = Some(format!("session sync: {error}")),
                    }
                }
                SessionSubscriptionMessage::Disconnected {
                    subscription_id,
                    error,
                } if self.session_entities.subscription_id.as_deref()
                    == Some(subscription_id.as_str()) =>
                {
                    self.client = None;
                    if !self.invalidate_session_generation() {
                        self.error = Some("session subscription cleanup timed out".to_string());
                    }
                    self.attached_session = None;
                    self.attached_subscription_id = None;
                    self.attach_hydration = None;
                    self.clear_terminal_mouse_mode();
                    self.status = "session subscription disconnected; reconnecting".to_string();
                    self.connection_error = Some(error);
                    return true;
                }
                SessionSubscriptionMessage::Disconnected { .. } => {}
            }
        }
        false
    }

    fn invalidate_session_generation(&mut self) -> bool {
        let stopped = self
            .session_subscription
            .take()
            .is_none_or(|mut pump| pump.stop());
        self.session_entities = SessionEntityState::default();
        self.rebuild_session_rows();
        stopped
    }

    fn rebuild_session_rows(&mut self) {
        self.pending_sessions
            .retain(|session_id, _| !self.session_entities.entities.contains_key(session_id));
        self.sessions = self
            .session_entities
            .entity_order
            .iter()
            .filter_map(|session_id| self.session_entities.entities.get(session_id))
            .map(SessionRow::from_entity)
            .chain(self.pending_sessions.values().cloned())
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
        self.pending_sessions
            .insert(session_id.clone(), SessionRow::pending(session_id.clone()));
        self.selected_session = Some(session_id.clone());
        self.rebuild_session_rows();
        self.action_feedback = Some(format!("spawn pending: {session_id}"));
        match self.request(DaemonRequest::Spawn {
            session_id: session_id.clone(),
            command,
        }) {
            Ok(response) => {
                let failed = response.error.is_some();
                self.apply_response(response);
                if failed {
                    self.pending_sessions.remove(&session_id);
                    self.rebuild_session_rows();
                }
            }
            Err(error) => {
                self.pending_sessions.remove(&session_id);
                self.rebuild_session_rows();
                self.record_transport_error(error);
                return;
            }
        }
        if self.pending_sessions.contains_key(&session_id) {
            self.action_feedback = Some(format!(
                "spawn accepted: {session_id}; waiting for authoritative session"
            ));
        }
    }

    fn attach_selected_or_first(&mut self) {
        let Some(session_id) = self.selected_attachable_session_id() else {
            return;
        };
        self.error = None;
        self.selected_session = Some(session_id.clone());
        self.action_feedback = Some(format!("attach requested: {session_id}"));
        let subscription_id = format!("btui-sub-{}", short_suffix());
        self.begin_attach_hydration(&session_id, &subscription_id);
        self.request_and_apply(DaemonRequest::Attach {
            session_id,
            subscription_id,
        });
    }

    fn begin_attach_hydration(&mut self, session_id: &str, subscription_id: &str) {
        // Every Attach owns a fresh transport-local subscription generation.
        // Visible restoration comes from ReadScreen, never opaque history events.
        self.subscription_id = subscription_id.to_string();
        self.attached_session = None;
        self.attached_subscription_id = None;
        self.clear_terminal_mouse_mode();
        self.terminal_output.clear();
        self.snapshot_metadata = None;
        self.terminal_output_session_id = Some(session_id.to_string());
        self.attach_hydration = Some(AttachHydration {
            session_id: session_id.to_string(),
            subscription_id: subscription_id.to_string(),
            deadline: Instant::now() + ATTACH_HYDRATION_TIMEOUT,
            read_screen_requested: false,
            buffered_live_output: String::new(),
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
        let Some(subscription_id) = self.attached_subscription_id.clone() else {
            self.error = Some("attached terminal stream has no current subscription".to_string());
            return;
        };
        self.error = None;
        self.action_feedback = Some(format!("detach requested: {session_id}"));
        self.attach_hydration = None;
        self.clear_terminal_mouse_mode();
        self.request_and_apply(DaemonRequest::Detach {
            session_id,
            subscription_id,
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
        if let Some(audit) = &mut self.acceptance_audit {
            audit.record(&request);
        }
        match &mut self.client {
            Some(client) => client.request(&request),
            None => Err(DaemonTransportError::NotRunning),
        }
    }

    #[cfg(test)]
    fn record_request(&mut self, request: &DaemonRequest) {
        match request {
            DaemonRequest::Status => self.observed_requests.push(ObservedRequest::Status),
            DaemonRequest::ListApps => self.observed_requests.push(ObservedRequest::ListApps),
            DaemonRequest::ListPackageNavigation => self
                .observed_requests
                .push(ObservedRequest::ListPackageNavigation),
            DaemonRequest::ListPackages => {
                self.observed_requests.push(ObservedRequest::ListPackages)
            }
            DaemonRequest::ShowPackage { package_name } => self
                .observed_requests
                .push(ObservedRequest::ShowPackage(package_name.clone())),
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
            DaemonRequest::PluginSurfaceAction {
                package_name,
                request,
            } => self
                .observed_requests
                .push(ObservedRequest::PluginSurfaceAction {
                    package_name: package_name.clone(),
                    request: request.clone(),
                }),
            DaemonRequest::Attach {
                session_id,
                subscription_id,
            } => self.observed_requests.push(ObservedRequest::Attach {
                session_id: session_id.clone(),
                subscription_id: subscription_id.clone(),
            }),
            DaemonRequest::ShutdownSession { session_id } => self
                .observed_requests
                .push(ObservedRequest::ShutdownSession(session_id.clone())),
            DaemonRequest::RemoveSession { session_id } => self
                .observed_requests
                .push(ObservedRequest::RemoveSession(session_id.clone())),
            DaemonRequest::Drain { session_id } => self
                .observed_requests
                .push(ObservedRequest::Drain(session_id.clone())),
            DaemonRequest::ReadScreen { session_id } => self
                .observed_requests
                .push(ObservedRequest::ReadScreen(session_id.clone())),
            DaemonRequest::ReadModeFlags { session_id } => self
                .observed_requests
                .push(ObservedRequest::ReadModeFlags(session_id.clone())),
            DaemonRequest::CaptureSnapshot { session_id } => self
                .observed_requests
                .push(ObservedRequest::CaptureSnapshot(session_id.clone())),
            DaemonRequest::SendInput { session_id, data } => {
                self.observed_requests.push(ObservedRequest::SendInput {
                    session_id: session_id.clone(),
                    data: data.clone(),
                })
            }
            _ => {}
        }
    }

    fn record_transport_error(&mut self, error: DaemonTransportError) {
        self.client = None;
        self.reset_active_plugin_surface();
        if !self.invalidate_session_generation() {
            self.error = Some("session subscription cleanup timed out".to_string());
        }
        self.attached_session = None;
        self.attached_subscription_id = None;
        self.attach_hydration = None;
        self.clear_terminal_mouse_mode();
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
            if let Some(hydration) = self.attach_hydration.take() {
                self.append_terminal_output(&hydration.buffered_live_output);
            }
            return;
        }
        if evidence.opaque_state_received {
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
        if matches!(response.kind, DaemonResponseKind::PluginSurface)
            && let Some(surface) = response.plugin_surface
        {
            match normalize_plugin_surface(surface) {
                Ok(surface) => {
                    let owner_changed = self.plugin_surface.as_ref().is_none_or(|current| {
                        current.package_name != surface.package_name
                            || current.surface_id != surface.surface_id
                    });
                    if owner_changed {
                        self.plugin_presentation = renderer::PresentationState::default();
                        self.plugin_action_result = None;
                        self.pending_plugin_request = None;
                    }
                    self.plugin_surface = Some(surface);
                }
                Err(error) => {
                    self.error = Some(format!("plugin surface render: {error}"));
                }
            }
        }
        if matches!(response.kind, DaemonResponseKind::PluginActionResult)
            && let Some(result) = response.plugin_action_result
        {
            self.apply_plugin_action_result(result);
        }

        for event in response.events {
            match event {
                DaemonEvent::TerminalOutput {
                    session_id,
                    subscription_id,
                    data,
                } => {
                    if self.hydration_matches(&session_id, &subscription_id) {
                        if let Some(hydration) = self.attach_hydration.as_mut() {
                            hydration.buffered_live_output.push_str(&data);
                        }
                    } else if self.attached_matches(&session_id, &subscription_id) {
                        self.append_terminal_output(&data);
                        self.terminal_mouse_mode_refresh_due = true;
                    }
                }
                DaemonEvent::Snapshot {
                    session_id,
                    subscription_id,
                    ..
                }
                | DaemonEvent::Scrollback {
                    session_id,
                    subscription_id,
                    ..
                } => {
                    if self.hydration_matches(&session_id, &subscription_id) {
                        hydration_evidence.opaque_state_received = true;
                    }
                }
                DaemonEvent::ProcessExit {
                    session_id,
                    subscription_id,
                    code,
                } => {
                    if self.hydration_matches(&session_id, &subscription_id) {
                        hydration_evidence.lifecycle_ended = true;
                    } else if !self.attached_matches(&session_id, &subscription_id) {
                        continue;
                    }
                    self.status = format!("process exited {}", code.unwrap_or_default());
                    self.attached_session = None;
                    self.attached_subscription_id = None;
                    self.clear_terminal_mouse_mode();
                    self.clear_snapshot_metadata_for(&session_id);
                }
                DaemonEvent::AttachState {
                    session_id,
                    subscription_id,
                    state,
                } => {
                    let hydration_matches = self.hydration_matches(&session_id, &subscription_id);
                    let attached_matches = self.attached_matches(&session_id, &subscription_id);
                    if !hydration_matches && !attached_matches {
                        continue;
                    }
                    self.action_feedback = Some(format!("attach {state}: {session_id}"));
                    if state == "attached" && hydration_matches {
                        self.attached_session = Some(session_id.clone());
                        self.attached_subscription_id = Some(subscription_id);
                        self.probe_terminal_mouse_mode(&session_id);
                    } else if state == "detached" {
                        self.attached_session = None;
                        self.attached_subscription_id = None;
                        self.clear_terminal_mouse_mode();
                        self.clear_snapshot_metadata_for(&session_id);
                        if hydration_matches {
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

    fn attached_matches(&self, session_id: &str, subscription_id: &str) -> bool {
        self.attached_session.as_deref() == Some(session_id)
            && self.attached_subscription_id.as_deref() == Some(subscription_id)
    }

    fn clear_snapshot_metadata_for(&mut self, session_id: &str) {
        if self.terminal_output_session_id.as_deref() == Some(session_id) {
            self.snapshot_metadata = None;
        }
    }

    fn complete_attach_hydration(&mut self, _deadline_expired: bool) {
        let Some(hydration) = self.attach_hydration.as_mut() else {
            return;
        };
        if hydration.read_screen_requested {
            return;
        }
        hydration.read_screen_requested = true;
        let session_id = hydration.session_id.clone();
        self.request_optional_readback(DaemonRequest::ReadScreen { session_id }, "read_screen");
    }

    fn finish_attach_hydration(&mut self, session_id: &str, restored_text: &str) {
        let Some(hydration) = self.attach_hydration.take() else {
            return;
        };
        if hydration.session_id != session_id || hydration.subscription_id != self.subscription_id {
            self.attach_hydration = Some(hydration);
            return;
        }

        self.terminal_output.clear();
        self.terminal_output.push_str(restored_text);
        append_non_overlapping(&mut self.terminal_output, &hydration.buffered_live_output);
        self.request_optional_readback(
            DaemonRequest::CaptureSnapshot {
                session_id: session_id.to_string(),
            },
            "capture_snapshot",
        );
        if self.attached_session.as_deref() == Some(session_id) {
            self.probe_terminal_mouse_mode(session_id);
        }
    }

    fn probe_terminal_mouse_mode(&mut self, session_id: &str) {
        self.last_terminal_mouse_mode_probe = Some(Instant::now());
        self.terminal_mouse_mode_refresh_due = false;
        self.request_optional_readback(
            DaemonRequest::ReadModeFlags {
                session_id: session_id.to_string(),
            },
            "read_mode_flags",
        );
    }

    fn refresh_terminal_mouse_mode_if_due(&mut self) {
        if !self.terminal_mouse_mode_refresh_due {
            return;
        }
        let now = Instant::now();
        if self
            .last_terminal_mouse_mode_probe
            .is_some_and(|last| now.duration_since(last) < TERMINAL_MOUSE_MODE_REFRESH_INTERVAL)
        {
            return;
        }
        let Some(session_id) = self.attached_session.clone() else {
            self.clear_terminal_mouse_mode();
            return;
        };
        self.probe_terminal_mouse_mode(&session_id);
    }

    fn clear_terminal_mouse_mode(&mut self) {
        self.terminal_mouse_mode = 0;
        self.terminal_mouse_mode_attachment = None;
        self.terminal_mouse_mode_refresh_due = false;
        self.last_terminal_mouse_mode_probe = None;
    }

    fn current_terminal_mouse_mode(&self) -> u8 {
        match (
            self.terminal_mouse_mode_attachment.as_ref(),
            self.attached_session.as_ref(),
            self.attached_subscription_id.as_ref(),
        ) {
            (Some((mode_session, mode_subscription)), Some(session), Some(subscription))
                if mode_session == session && mode_subscription == subscription =>
            {
                self.terminal_mouse_mode
            }
            _ => 0,
        }
    }

    fn apply_terminal_mouse_mode(&self, hit_map: &mut HitMap) {
        hit_map.set_terminal_mouse_mode("tui-terminal", self.current_terminal_mouse_mode());
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
            if operation == "read_mode_flags" {
                self.clear_terminal_mouse_mode();
            }
            if operation == "read_screen"
                && let Some(session_id) = self
                    .attach_hydration
                    .as_ref()
                    .map(|hydration| hydration.session_id.clone())
            {
                self.finish_attach_hydration(&session_id, "");
            }
            return;
        }
        match response.kind {
            DaemonResponseKind::ReadScreen => {
                if let Some(screen) = response.read_screen {
                    self.finish_attach_hydration(&screen.session_id, &screen.text);
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
            DaemonResponseKind::ReadModeFlags => {
                let Some(mode_flags) = response.mode_flags else {
                    self.clear_terminal_mouse_mode();
                    return;
                };
                let Some(subscription_id) = self.attached_subscription_id.clone() else {
                    self.clear_terminal_mouse_mode();
                    return;
                };
                if self.attached_session.as_deref() != Some(mode_flags.session_id.as_str()) {
                    self.clear_terminal_mouse_mode();
                    return;
                }
                self.terminal_mouse_mode = mode_flags.mouse_mode;
                self.terminal_mouse_mode_attachment =
                    Some((mode_flags.session_id, subscription_id));
            }
            _ => {
                if operation == "read_mode_flags" {
                    self.clear_terminal_mouse_mode();
                }
            }
        }
    }

    fn append_terminal_output(&mut self, data: &str) {
        if data.is_empty() {
            return;
        }
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
        if self.confirmation.is_some() {
            let root = self.confirmation_surface();
            root.validate()
                .expect("confirmation UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("confirmation UiNode should fit TUI renderer capabilities");
            return root;
        }

        if self.plugin_surface.is_some() {
            let root = self.plugin_shell_surface();
            root.validate()
                .expect("plugin shell UiNode should satisfy the UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("plugin shell UiNode should fit TUI renderer capabilities");
            return root;
        }

        #[cfg(test)]
        if !self.workspace_test_mode && self.legacy_test_needs_system_details() {
            let root = self.system_details_panel();
            root.validate()
                .expect("system details UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("system details UiNode should fit TUI renderer capabilities");
            return root;
        }

        let mut root = node(
            UiNodeKind::Stack,
            "workspace-root",
            json!({ "direction": "vertical" }),
        );
        root.children = vec![
            child(self.status_summary()),
            child(self.workspace_toolbar()),
        ];
        if self.system_details_visible {
            root.children.push(child(self.system_details_panel()));
        } else {
            root.children.extend([
                responsive_child(
                    UiWidthClass::Expanded,
                    self.workspace_layout("workspace-expanded", "horizontal"),
                ),
                responsive_child(
                    UiWidthClass::Regular,
                    self.workspace_layout("workspace-regular", "horizontal"),
                ),
                responsive_child(
                    UiWidthClass::Compact,
                    self.workspace_layout("workspace-compact", "vertical"),
                ),
            ]);
        }
        root.validate()
            .expect("workspace UiNode should satisfy the core UI contract");
        renderer::tui_capabilities()
            .validate_node(&root)
            .expect("workspace UiNode should fit TUI renderer capabilities");
        root
    }

    fn plugin_shell_surface(&self) -> UiNode {
        let surface = self
            .plugin_surface
            .as_ref()
            .expect("plugin shell requires an active surface");
        let mut root = node(
            UiNodeKind::Stack,
            "plugin-shell",
            json!({ "direction": "vertical" }),
        );
        root.children = vec![
            child(self.status_summary()),
            child(node(
                UiNodeKind::Text,
                "plugin-shell-owner",
                json!({
                    "text": format!(
                        "Plugin: {} / {} | Esc returns to System",
                        surface.package_name, surface.surface_id
                    )
                }),
            )),
        ];
        if let Some(error) = &self.connection_error {
            root.children.push(child(node(
                UiNodeKind::Text,
                "plugin-shell-connection-error",
                json!({ "text": format!("connection: {error}") }),
            )));
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            root.children.push(child(node(
                UiNodeKind::Text,
                &format!("plugin-shell-diagnostic-{index}"),
                json!({ "text": format!("diagnostic: {}", diagnostic_text(diagnostic)) }),
            )));
        }
        if let Some(feedback) = &self.action_feedback {
            root.children.push(child(node(
                UiNodeKind::Text,
                "plugin-shell-action-feedback",
                json!({ "text": format!("action: {feedback}") }),
            )));
        }
        if let Some(error) = &self.error {
            root.children.push(child(node(
                UiNodeKind::Text,
                "plugin-shell-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        root.children.push(child(plugin_surface_render_root(
            surface,
            self.plugin_action_result.as_ref(),
            &self.session_entities,
        )));
        root
    }

    #[cfg(test)]
    fn legacy_test_needs_system_details(&self) -> bool {
        self.compatibility.is_some()
            || !self.diagnostics.is_empty()
            || !self.apps.is_empty()
            || !self.package_navigation.is_empty()
            || !self.packages.is_empty()
            || !self.available_packages.is_empty()
            || self.install_plan.is_some()
            || self.update_status.is_some()
            || self.package_decision.is_some()
            || self.snapshot_metadata.is_some()
            || !self.drafts.is_empty()
    }

    fn status_summary(&self) -> UiNode {
        let selected = self.selected_session.as_deref().unwrap_or("none");
        let attached = self.attached_session.as_deref().unwrap_or("none");
        node(
            UiNodeKind::Text,
            "workspace-status",
            json!({
                "text": format!(
                    "Botster | hub: {} | protocol: {PROTOCOL} | sessions: {} | selected: {selected} | attached: {attached}",
                    self.status,
                    self.sessions.len()
                )
            }),
        )
    }

    fn workspace_toolbar(&self) -> UiNode {
        let selected = self.selected_session_row();
        let attach_disabled = selected.is_none_or(|session| !session.is_attachable());
        let shutdown_disabled = selected.is_none_or(|session| !session.is_attachable());
        let remove_disabled = selected.is_none_or(|session| {
            session.pending || matches!(session.lifecycle.as_str(), "running" | "pending")
        });
        let payload = json!({ "session_id": self.selected_session });

        let mut toolbar = node(UiNodeKind::Toolbar, "workspace-toolbar", json!({}));
        toolbar.slots.insert(
            "actions".to_string(),
            vec![
                child(workspace_button(
                    "tui-spawn",
                    "Spawn",
                    "botster.tui.spawn",
                    json!({}),
                    "never",
                    false,
                    None,
                )),
                child(workspace_button(
                    "workspace-attach",
                    "Attach",
                    "botster.tui.attach",
                    payload.clone(),
                    "never",
                    attach_disabled,
                    None,
                )),
                child(workspace_button(
                    "tui-detach",
                    "Detach",
                    "botster.tui.detach",
                    json!({}),
                    "auto",
                    self.attached_session.is_none(),
                    None,
                )),
                child(workspace_button(
                    "workspace-system-details",
                    if self.system_details_visible {
                        "Workspace"
                    } else {
                        "System details"
                    },
                    "botster.tui.system.toggle",
                    json!({}),
                    "auto",
                    false,
                    None,
                )),
                child(workspace_button(
                    "workspace-refresh",
                    "Refresh",
                    "botster.tui.refresh",
                    json!({}),
                    "auto",
                    false,
                    None,
                )),
                child(workspace_button(
                    "workspace-shutdown",
                    "Shutdown",
                    "botster.tui.session.shutdown",
                    payload.clone(),
                    "always",
                    shutdown_disabled,
                    Some("danger"),
                )),
                child(workspace_button(
                    "workspace-remove",
                    "Remove",
                    "botster.tui.session.remove",
                    payload,
                    "always",
                    remove_disabled,
                    Some("danger"),
                )),
            ],
        );
        toolbar
    }

    fn workspace_layout(&self, id: &str, direction: &str) -> UiNode {
        let mut layout = node(UiNodeKind::Stack, id, json!({ "direction": direction }));
        layout.children = vec![
            child(self.session_navigator()),
            child(self.focused_session_panel()),
        ];
        layout
    }

    fn session_navigator(&self) -> UiNode {
        let mut panel = node(
            UiNodeKind::Panel,
            "workspace-session-navigator",
            json!({ "title": "Sessions" }),
        );
        let mut scroll = node(UiNodeKind::ScrollArea, "tui-session-list", json!({}));
        if self.sessions.is_empty() {
            scroll.children = vec![
                child(node(
                    UiNodeKind::Text,
                    "workspace-empty-title",
                    json!({ "text": "No sessions yet" }),
                )),
                child(node(
                    UiNodeKind::Text,
                    "workspace-empty-help",
                    json!({ "text": "Spawn starts a session; selection never attaches automatically." }),
                )),
            ];
        } else {
            scroll.children = self
                .sessions
                .iter()
                .map(|session| child(self.session_navigation_row(session)))
                .collect();
        }
        panel.slots.insert("body".to_string(), vec![child(scroll)]);
        panel
    }

    fn session_navigation_row(&self, session: &SessionRow) -> UiNode {
        let selected = self.selected_session.as_deref() == Some(session.session_id.as_str());
        let attached = self.attached_session.as_deref() == Some(session.session_id.as_str());
        let state = if session.pending {
            "pending spawn"
        } else if attached && session.is_attachable() {
            "attached"
        } else {
            session.lifecycle.as_str()
        };
        let mut label = format!("{} · {state}", session.session_id);
        if selected && !attached {
            label.push_str(" · selected");
        }
        if let Some(reason) = &session.failure_reason {
            label.push_str(&format!(" · {reason}"));
        }
        let mut item = node(
            UiNodeKind::ListItem,
            &format!("tui-session-{}", session.session_id),
            json!({
                "selected": selected,
                "value": session.session_id,
                "activation": {
                    "id": "botster.tui.select",
                    "payload": { "session_id": session.session_id }
                }
            }),
        );
        item.slots.insert(
            "title".to_string(),
            vec![child(node(
                UiNodeKind::Text,
                &format!("tui-session-{}-title", session.session_id),
                json!({ "text": label }),
            ))],
        );
        item
    }

    fn focused_session_panel(&self) -> UiNode {
        let mut panel = node(
            UiNodeKind::Panel,
            "workspace-focused-session",
            json!({ "title": "Focused session" }),
        );
        let mut body = node(
            UiNodeKind::Stack,
            "workspace-focused-body",
            json!({ "direction": "vertical" }),
        );
        let detail = match self.selected_session_row() {
            Some(session) if session.pending => format!(
                "{} is pending authoritative lifecycle state. Attachment is disabled.",
                session.session_id
            ),
            Some(session) if session.is_attachable() => format!(
                "{} is running. Selection is local; choose Attach to open its terminal stream.",
                session.session_id
            ),
            Some(session) => format!(
                "{} is {}. Attachment is disabled{}.",
                session.session_id,
                session.lifecycle,
                session
                    .failure_reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            ),
            None if self.connection_error.is_some() => {
                "Hub unavailable. Reconnect from System details; Spawn remains available when connected."
                    .to_string()
            }
            None => "Choose a session, or Spawn to create one.".to_string(),
        };
        if let Some(connection_error) = &self.connection_error {
            body.children.push(child(node(
                UiNodeKind::Text,
                "workspace-connection-error",
                json!({ "text": format!("connection: {connection_error}") }),
            )));
        }
        if let Some(error) = &self.error {
            body.children.push(child(node(
                UiNodeKind::Text,
                "workspace-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        if let Some(feedback) = &self.action_feedback {
            body.children.push(child(node(
                UiNodeKind::Text,
                "workspace-action-feedback",
                json!({ "text": format!("action: {feedback}") }),
            )));
        }
        body.children.extend([
            child(node(
                UiNodeKind::Text,
                "workspace-focused-detail",
                json!({ "text": detail }),
            )),
            child(self.terminal_panel()),
        ]);
        panel.slots.insert("body".to_string(), vec![child(body)]);
        panel
    }

    fn selected_session_row(&self) -> Option<&SessionRow> {
        let selected = self.selected_session.as_deref()?;
        self.sessions
            .iter()
            .find(|session| session.session_id == selected)
    }

    fn confirmation_surface(&self) -> UiNode {
        let confirmation = self
            .confirmation
            .as_ref()
            .expect("confirmation surface requires pending action");
        let (verb, session_id) = match confirmation {
            DestructiveAction::Shutdown(session_id) => ("Shut down", session_id),
            DestructiveAction::Remove(session_id) => ("Remove", session_id),
        };
        let mut actions = node(UiNodeKind::Inline, "workspace-confirm-actions", json!({}));
        actions.children = vec![
            child(workspace_button(
                "workspace-confirm-cancel",
                "Cancel",
                "botster.tui.confirm.cancel",
                json!({}),
                "never",
                false,
                None,
            )),
            child(workspace_button(
                "workspace-confirm-accept",
                verb,
                "botster.tui.confirm.accept",
                json!({}),
                "never",
                false,
                Some("danger"),
            )),
        ];
        let mut body = node(
            UiNodeKind::Stack,
            "workspace-confirm-body",
            json!({ "direction": "vertical" }),
        );
        body.children = vec![
            child(node(
                UiNodeKind::Text,
                "workspace-confirm-message",
                json!({ "text": format!("{verb} session {session_id}? This action cannot be undone from this workspace.") }),
            )),
            child(actions),
        ];
        let mut dialog = node(
            UiNodeKind::Dialog,
            "workspace-confirmation",
            json!({ "title": format!("Confirm {}", verb.to_lowercase()), "presentation": "auto" }),
        );
        dialog.slots.insert("body".to_string(), vec![child(body)]);
        dialog
    }

    fn system_details_panel(&self) -> UiNode {
        let mut panel = node(
            UiNodeKind::Panel,
            "tui-status-panel",
            json!({ "title": "System details" }),
        );
        let mut children = vec![
            child(node(
                UiNodeKind::Text,
                "tui-status",
                json!({ "text": self.status }),
            )),
            child(node(
                UiNodeKind::Text,
                "tui-compatibility",
                json!({ "text": self.compatibility_text() }),
            )),
            child(node(
                UiNodeKind::Text,
                "tui-package-storage-context",
                json!({
                    "text": format!(
                        "package storage context: {}",
                        if self.package_storage_context_configured {
                            "configured"
                        } else {
                            "not supplied"
                        }
                    )
                }),
            )),
            child(button(
                "tui-refresh",
                "Refresh",
                "botster.tui.refresh",
                json!({}),
            )),
            child(button(
                "tui-connect",
                "Reconnect",
                "botster.tui.connect",
                json!({}),
            )),
        ];
        children.push(child(self.command_form()));
        if let Some(error) = &self.connection_error {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-connection-error",
                json!({ "text": format!("connection: {error}") }),
            )));
        }
        children.push(child(node(
            UiNodeKind::Text,
            "tui-package-summary",
            json!({ "text": self.package_summary_text() }),
        )));
        children.extend(self.package_navigation_nodes().into_iter().map(child));
        children.extend(self.app_nodes().into_iter().map(child));
        if self.packages.is_empty() {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-package-empty",
                json!({ "text": "packages: none reported" }),
            )));
        } else {
            for (index, package) in self.packages.iter().enumerate() {
                children.push(child(node(
                    UiNodeKind::Text,
                    &format!("tui-package-{index}"),
                    json!({ "text": format!("package: {}", package_text(package)) }),
                )));
                children.extend(package_surface_nodes(package, index).into_iter().map(child));
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
                        &format!("tui-package-{index}-entrypoint-{entrypoint_index}"),
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
                "tui-marketplace-summary",
                json!({ "text": format!("marketplace: {} available", self.available_packages.len()) }),
            )));
            for (index, available_package) in self.available_packages.iter().enumerate() {
                children.push(child(node(
                    UiNodeKind::Text,
                    &format!("tui-available-package-{index}"),
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
                        node.id = Some(UiNodeId(format!("tui-install-plan-{index}")).into());
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
                        node.id = Some(UiNodeId(format!("tui-update-status-{index}")).into());
                        child(node)
                    }),
            );
        }
        if let Some(decision) = &self.package_decision {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-package-decision",
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
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            children.push(child(node(
                UiNodeKind::Text,
                &format!("tui-diagnostic-{index}"),
                json!({ "text": format!("diagnostic: {}", diagnostic_text(diagnostic)) }),
            )));
        }
        if let Some(feedback) = &self.action_feedback {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-action-feedback",
                json!({ "text": format!("action: {feedback}") }),
            )));
        }
        if let Some(snapshot) = &self.snapshot_metadata {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-terminal-snapshot-metadata",
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
                "tui-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        children.push(child(node(
            UiNodeKind::Text,
            "tui-hints",
            json!({ "text": "hints: Tab focus | up/down select | Enter/Space activate | terminal focus forwards keys" }),
        )));
        let mut scroll = node(
            UiNodeKind::ScrollArea,
            "workspace-system-details-scroll",
            json!({}),
        );
        scroll.children = children;
        panel.slots.insert("body".to_string(), vec![child(scroll)]);
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
            "tui-app-summary",
            json!({ "text": format!("apps: {} installed", self.apps.len()) }),
        )];
        if self.apps.is_empty() {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-app-empty",
                json!({ "text": "apps: none reported" }),
            ));
            return nodes;
        }

        for (app_index, app) in self.apps.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-app-{app_index}"),
                json!({ "text": format!("app: {}", app_text(app)) }),
            ));
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-app-{app_index}-launch-target"),
                json!({ "text": format!("launch target: {}", app_launch_target_text(app)) }),
            ));
            for (reason_index, reason) in app.blocked_reasons.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-app-{app_index}-blocked-{reason_index}"),
                    json!({ "text": format!("app blocked: {reason}") }),
                ));
            }
            for (diagnostic_index, diagnostic) in app.diagnostics.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-app-{app_index}-diagnostic-{diagnostic_index}"),
                    json!({ "text": format!("app diagnostic: {}", package_diagnostic_text(diagnostic)) }),
                ));
            }
            nodes.extend(
                action_state_nodes(&app.actions, "app action", &format!("tui-app-{app_index}"))
                    .into_iter(),
            );
            if let Some(route) = &app.route {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-app-{app_index}-route"),
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
            "tui-package-navigation-summary",
            json!({ "text": format!("navigation: {} admitted entries", self.package_navigation.len()) }),
        )];

        for (index, entry) in self.package_navigation.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-package-navigation-{index}"),
                json!({ "text": format!("navigation entry: {}", navigation_entry_text(entry)) }),
            ));
            for (diagnostic_index, diagnostic) in entry.diagnostics.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-package-navigation-{index}-diagnostic-{diagnostic_index}"),
                    json!({ "text": format!("navigation diagnostic: {}", package_diagnostic_text(diagnostic)) }),
                ));
            }
            match navigation_open_payload_for_entry(entry) {
                Some(payload) if entry.enabled && !entry.blocked => {
                    nodes.push(button(
                        &format!("tui-package-navigation-{index}-open"),
                        "Open",
                        "botster.tui.navigation.open",
                        payload,
                    ));
                }
                Some(_) => nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-package-navigation-{index}-blocked"),
                    json!({ "text": format!("navigation blocked: {}", navigation_blocked_text(entry)) }),
                )),
                None => nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-package-navigation-{index}-unsupported"),
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
            &format!("tui-package-{index}-configuration-summary"),
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
                &format!("tui-package-{index}-configuration-missing-{missing}"),
                json!({ "text": format!("configuration missing: {missing}") }),
            ));
        }

        for (diagnostic_index, diagnostic) in package.configuration.diagnostics.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-package-{index}-configuration-diagnostic-{diagnostic_index}"),
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
                &format!("tui-package-{index}-configuration-submit"),
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
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "select" => {
                props["selected"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                let mut select = node(
                    UiNodeKind::Select,
                    &format!("tui-package-{index}-configuration-{}", field.key),
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
                                    "tui-package-{index}-configuration-{}-option-{option_index}",
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
                    &format!("tui-package-{index}-configuration-{}", field.key),
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
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "string" | "path" | "url" => {
                props["value"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                node(
                    UiNodeKind::TextInput,
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            other => node(
                UiNodeKind::Text,
                &format!("tui-package-{index}-configuration-{}", field.key),
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
        let mut form = node(UiNodeKind::Inline, "tui-command-panel", json!({}));
        form.children = vec![
            child(node(
                UiNodeKind::TextInput,
                "tui-command",
                json!({
                    "name": "command",
                    "label": "command",
                    "value": command
                }),
            )),
            child(button(
                "workspace-command-spawn",
                "Spawn",
                "botster.tui.spawn",
                json!({}),
            )),
        ];
        form
    }

    fn terminal_panel(&self) -> UiNode {
        let mut terminal = node(
            UiNodeKind::TerminalView,
            "tui-terminal",
            json!({
                "title": self.terminal_title(),
                "session_id": self.attached_session.clone().unwrap_or_else(|| "not attached".to_string())
            }),
        );
        terminal.children = vec![child(node(
            UiNodeKind::Text,
            "tui-terminal-output",
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
    ListApps,
    ListPackageNavigation,
    ListPackages,
    ShowPackage(String),
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
    PluginSurfaceAction {
        package_name: String,
        request: UiActionRequest,
    },
    Attach {
        session_id: String,
        subscription_id: String,
    },
    ShutdownSession(String),
    RemoveSession(String),
    Drain(String),
    ReadScreen(String),
    ReadModeFlags(String),
    CaptureSnapshot(String),
    SendInput {
        session_id: String,
        data: String,
    },
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

#[derive(Default)]
struct AcceptanceRequestAudit {
    surface_renders: Vec<(String, String)>,
    surface_actions: Vec<UiActionRequest>,
    list_sessions: usize,
}

impl AcceptanceRequestAudit {
    fn record(&mut self, request: &DaemonRequest) {
        match request {
            DaemonRequest::ListSessions => self.list_sessions += 1,
            DaemonRequest::PluginSurfaceRender {
                package_name,
                surface_id,
                ..
            } => self
                .surface_renders
                .push((package_name.clone(), surface_id.clone())),
            DaemonRequest::PluginSurfaceAction { request, .. } => {
                self.surface_actions.push(request.clone());
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct AcceptanceDiagnostics {
    case_id: Option<String>,
    phase: String,
    expected_condition: String,
    subscription_id: Option<String>,
    snapshot_seq: Option<u64>,
    surface_render_count: usize,
    focusable_ids: Vec<String>,
    last_observation: Value,
}

impl AcceptanceDiagnostics {
    fn stage(&mut self, phase: &str, case_id: Option<&str>, expected_condition: &str) {
        self.phase = phase.to_string();
        self.case_id = case_id.map(ToOwned::to_owned);
        self.expected_condition = expected_condition.to_string();
    }

    fn observe_app(&mut self, app: &TuiApp) {
        self.subscription_id = app.session_entities.subscription_id.clone();
        self.snapshot_seq = app.session_entities.snapshot_seq;
        self.surface_render_count = app
            .acceptance_audit
            .as_ref()
            .map_or(0, |audit| audit.surface_renders.len());
    }

    fn observe_frame(&mut self, app: &TuiApp, hit_map: &HitMap) {
        self.observe_app(app);
        self.focusable_ids = focusable_ids(hit_map);
    }

    fn observe_request(&mut self, request: &UiActionRequest) {
        self.last_observation = json!({
            "kind": "action_request",
            "request_id": request.request_id,
            "surface_id": request.surface_id,
            "action_id": request.action_id,
            "node_id": request.node_id
        });
    }

    fn observe_result(&mut self, result: &UiActionResult) {
        self.last_observation = json!({
            "kind": "action_result",
            "request_id": result.request_id,
            "surface_id": result.surface_id,
            "action_id": result.action_id,
            "node_id": result.node_id,
            "state": result.state,
            "field_errors": result.field_errors,
            "form_errors": result.form_errors,
            "error": result.error
        });
    }

    fn failure_context(&self) -> FailureContext {
        FailureContext {
            case_id: self.case_id.clone(),
            phase: self.phase.clone(),
            expected_condition: self.expected_condition.clone(),
            subscription_id: self.subscription_id.clone(),
            snapshot_seq: self.snapshot_seq,
            surface_render_count: self.surface_render_count,
            focusable_ids: self.focusable_ids.clone(),
            last_observation: self.last_observation.clone(),
        }
    }
}

const ACCEPTANCE_WIDTH: u16 = 500;
const ACCEPTANCE_HEIGHT: u16 = 240;
const ACCEPTANCE_TIMEOUT: Duration = Duration::from_secs(12);
const WORKSPACES_PACKAGE: &str = "botster-workspaces";
const WORKSPACES_SURFACE: &str = "workspaces";

fn run_workspaces_acceptance(args: AppArgs, config: AcceptanceConfig) -> io::Result<()> {
    let mut evidence = EvidenceWriter::create(&config.evidence_path)?;
    let mut diagnostics = AcceptanceDiagnostics {
        last_observation: json!({}),
        ..AcceptanceDiagnostics::default()
    };
    diagnostics.stage(
        "connect",
        None,
        "caller-injected Hub connection and data directory",
    );
    let result = drive_workspaces_acceptance(args, &config, &mut evidence, &mut diagnostics);
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = evidence.failure(&diagnostics.failure_context(), &error.to_string());
            Err(error)
        }
    }
}

fn drive_workspaces_acceptance(
    args: AppArgs,
    config: &AcceptanceConfig,
    evidence: &mut EvidenceWriter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    if let Some(error) = args.connection_error.as_deref() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid Hub connection configuration: {error}"),
        ));
    }
    let endpoint = args.daemon_endpoint().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotConnected,
            "acceptance mode requires BOTSTER_HUB_CONNECTION",
        )
    })?;
    let data_dir = args.hub_data_dir.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "acceptance mode requires BOTSTER_HUB_DATA_DIR",
        )
    })?;
    if !data_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "injected Hub data directory is not a directory",
        ));
    }

    let mut app = TuiApp::new_with_runtime_context(Some(endpoint), None, true);
    if app.client.is_none() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            app.connection_error
                .clone()
                .unwrap_or_else(|| "acceptance driver could not connect to the Hub".to_string()),
        ));
    }
    app.acceptance_audit = Some(AcceptanceRequestAudit::default());
    diagnostics.stage(
        "baseline",
        None,
        "authoritative session snapshot and admitted Workspaces navigation",
    );
    wait_for_acceptance_state(
        &mut app,
        diagnostics,
        "authoritative session baseline",
        |app, _| app.session_entities.has_snapshot,
    )?;
    wait_for_acceptance_state(
        &mut app,
        diagnostics,
        "admitted Workspaces navigation",
        |app, _| {
            app.package_navigation.iter().any(|entry| {
                entry.package_name == WORKSPACES_PACKAGE
                    && entry.target.surface_id.as_deref() == Some(WORKSPACES_SURFACE)
                    && entry.enabled
                    && !entry.blocked
            })
        },
    )?;
    evidence.event(
        "ready",
        None,
        json!({ "workspace_id": config.scenario.workspace_id, "case_count": config.scenario.cases.len() }),
    )?;
    evidence.event(
        "baseline",
        None,
        json!({
            "subscription_id": app.session_entities.subscription_id,
            "snapshot_seq": app.session_entities.snapshot_seq,
            "has_snapshot": app.session_entities.has_snapshot
        }),
    )?;

    let mut router = InputRouter::new(renderer::action_request_context());
    diagnostics.stage(
        "initial_surface_open",
        None,
        "realized Workspaces navigation and exact workspace row",
    );
    if !acceptance_has_action(
        &mut app,
        &mut router,
        "botster.tui.navigation.open",
        |payload| payload_field(payload, "surface_id") == Some(WORKSPACES_SURFACE),
        diagnostics,
    )? {
        activate_acceptance_action(
            &mut app,
            &mut router,
            "botster.tui.system.toggle",
            |_| true,
            evidence,
            None,
            diagnostics,
        )?;
    }
    open_workspaces_surface(
        &mut app,
        &mut router,
        &config.scenario.workspace_id,
        evidence,
        diagnostics,
    )?;

    let old_subscription = app
        .session_entities
        .subscription_id
        .clone()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "initial session subscription has no id",
            )
        })?;
    if !app.handle_tui_owned_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)) {
        return invalid_acceptance(
            "Esc did not return the active plugin surface to System details",
        );
    }
    router = InputRouter::new(renderer::action_request_context());
    diagnostics.stage(
        "reconnect",
        None,
        "keyboard-dispatched reconnect and fresh authoritative subscription",
    );
    activate_acceptance_action(
        &mut app,
        &mut router,
        "botster.tui.connect",
        |_| true,
        evidence,
        None,
        diagnostics,
    )?;
    wait_for_acceptance_state(
        &mut app,
        diagnostics,
        "fresh reconnect snapshot",
        |app, _| {
            app.session_entities.has_snapshot
                && app.session_entities.subscription_id.as_deref()
                    != Some(old_subscription.as_str())
        },
    )?;
    evidence.event(
        "reconnect",
        None,
        json!({
            "previous_subscription_id": old_subscription,
            "subscription_id": app.session_entities.subscription_id,
            "snapshot_seq": app.session_entities.snapshot_seq
        }),
    )?;
    open_workspaces_surface(
        &mut app,
        &mut router,
        &config.scenario.workspace_id,
        evidence,
        diagnostics,
    )?;

    for case in &config.scenario.cases {
        drive_spawn_case(
            &mut app,
            &mut router,
            &config.scenario.workspace_id,
            case,
            evidence,
            diagnostics,
        )?;
    }

    let audit = app
        .acceptance_audit
        .as_ref()
        .expect("acceptance audit enabled");
    if audit.surface_renders.len() != 2 || audit.list_sessions != 0 {
        return invalid_acceptance(format!(
            "request budget violated: surface_renders={} list_sessions={}",
            audit.surface_renders.len(),
            audit.list_sessions
        ));
    }
    evidence.event(
        "request_summary",
        None,
        json!({
            "surface_render_count": audit.surface_renders.len(),
            "surface_action_count": audit.surface_actions.len(),
            "list_sessions_count": audit.list_sessions,
            "surface_renders": audit.surface_renders
        }),
    )?;
    evidence.event(
        "complete",
        None,
        json!({ "case_count": config.scenario.cases.len(), "workspace_id": config.scenario.workspace_id }),
    )
}

fn open_workspaces_surface(
    app: &mut TuiApp,
    router: &mut InputRouter,
    workspace_id: &str,
    evidence: &mut EvidenceWriter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    activate_acceptance_action(
        app,
        router,
        "botster.tui.navigation.open",
        |payload| {
            payload_field(payload, "package_name") == Some(WORKSPACES_PACKAGE)
                && payload_field(payload, "surface_id") == Some(WORKSPACES_SURFACE)
        },
        evidence,
        None,
        diagnostics,
    )?;
    *router = InputRouter::new(renderer::action_request_context_for(WORKSPACES_SURFACE));
    evidence.event(
        "surface_request",
        None,
        json!({ "package_name": WORKSPACES_PACKAGE, "surface_id": WORKSPACES_SURFACE }),
    )?;
    activate_acceptance_action(
        app,
        router,
        "botster_workspaces.open",
        |payload| {
            payload_field(payload, "selected_workspace") == Some(workspace_id)
                && payload
                    .as_ref()
                    .is_none_or(|value| value.get("dialog").is_none())
        },
        evidence,
        None,
        diagnostics,
    )?;
    Ok(())
}

fn drive_spawn_case(
    app: &mut TuiApp,
    router: &mut InputRouter,
    workspace_id: &str,
    case: &ScenarioCase,
    evidence: &mut EvidenceWriter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    diagnostics.stage(
        "spawn_dialog",
        Some(&case.case_id),
        "producer-authored target-first Spawn control",
    );
    activate_acceptance_action(
        app,
        router,
        "botster_workspaces.open",
        |payload| {
            payload_field(payload, "selected_workspace") == Some(workspace_id)
                && payload_field(payload, "dialog") == Some(&format!("spawn-target:{workspace_id}"))
        },
        evidence,
        Some(&case.case_id),
        diagnostics,
    )?;
    diagnostics.stage(
        "target_selection",
        Some(&case.case_id),
        "exact rendered target option and accepted target-selection action",
    );
    select_acceptance_value(
        app,
        router,
        "target_id",
        &case.target_id,
        evidence,
        &case.case_id,
        diagnostics,
    )?;
    activate_acceptance_action(
        app,
        router,
        "botster_workspaces.select_spawn_target",
        |_| true,
        evidence,
        Some(&case.case_id),
        diagnostics,
    )?;
    diagnostics.stage(
        "spawn_form",
        Some(&case.case_id),
        "single eligible template and keyboard-typed requested branch",
    );
    select_only_acceptance_value(
        app,
        router,
        "template_id",
        evidence,
        &case.case_id,
        diagnostics,
    )?;
    type_acceptance_text(
        app,
        router,
        "branch",
        &case.branch,
        evidence,
        &case.case_id,
        diagnostics,
    )?;
    diagnostics.stage(
        "spawn_submit",
        Some(&case.case_id),
        "accepted correlated Spawn result with expected Hub facts",
    );
    let request = activate_acceptance_action(
        app,
        router,
        "botster_workspaces.spawn",
        |_| true,
        evidence,
        Some(&case.case_id),
        diagnostics,
    )?;
    let result = app.plugin_action_result.clone().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "spawn action returned no correlated result",
        )
    })?;
    if result.request_id != request.request_id
        || result.state != botster_ui_contract::UiActionResultState::Accepted
    {
        return invalid_acceptance(format!(
            "case {:?} spawn was not accepted: request_id={:?} state={:?} field_errors={:?} form_errors={:?} error={:?} payload={:?}",
            case.case_id,
            result.request_id,
            result.state,
            result.field_errors,
            result.form_errors,
            result.error,
            result.payload
        ));
    }
    let payload = result.payload.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "accepted spawn result omitted payload",
        )
    })?;
    let session_id = payload
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "spawn payload omitted session_id",
            )
        })?
        .to_string();
    let hub_result = payload.get("hub_result").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "spawn payload omitted hub_result",
        )
    })?;
    for (field, expected) in [
        ("target_id", case.expected.target_id.as_str()),
        ("branch", case.expected.branch.as_str()),
        ("worktree_path", case.expected.worktree_path.as_str()),
    ] {
        if hub_result.get(field).and_then(Value::as_str) != Some(expected) {
            return invalid_acceptance(format!(
                "case {:?} Hub result {field} did not match the scenario",
                case.case_id
            ));
        }
    }
    let surface_count = app
        .acceptance_audit
        .as_ref()
        .expect("acceptance audit enabled")
        .surface_renders
        .len();
    diagnostics.stage(
        "entity_reconciliation",
        Some(&case.case_id),
        "exact current session entity and rendered Workspaces membership metadata",
    );
    wait_for_acceptance_state(
        app,
        diagnostics,
        "spawned session entity and workspace membership",
        |app, diagnostics| {
            let current = app
                .session_entities
                .entities
                .get(&session_id)
                .is_some_and(|entity| entity.lifecycle_class == "current");
            if !current {
                return false;
            }
            acceptance_frame(app, router, diagnostics)
                .map(|(_, hit_map)| {
                    hit_map.regions().iter().any(|region| {
                        region.action.as_ref().is_some_and(|action| {
                            action.id.0 == "botster_workspaces.remove_session"
                                && payload_field(&action.payload, "session_id")
                                    == Some(session_id.as_str())
                                && payload_field(&action.payload, "workspace_id")
                                    == Some(workspace_id)
                        })
                    })
                })
                .unwrap_or(false)
        },
    )?;
    if app
        .acceptance_audit
        .as_ref()
        .expect("acceptance audit enabled")
        .surface_renders
        .len()
        != surface_count
    {
        return invalid_acceptance("entity reconciliation issued a synchronization surface render");
    }
    let entity = app
        .session_entities
        .entities
        .get(&session_id)
        .expect("wait proved entity");
    evidence.event(
        "entity_state",
        Some(&case.case_id),
        json!({
            "session_id": session_id,
            "lifecycle_class": entity.lifecycle_class,
            "subscription_id": app.session_entities.subscription_id,
            "snapshot_seq": app.session_entities.snapshot_seq
        }),
    )?;
    evidence.event(
        "case_complete",
        Some(&case.case_id),
        json!({ "resolution": case.resolution, "request_id": request.request_id.0, "session_id": session_id }),
    )
}

fn wait_for_acceptance_state(
    app: &mut TuiApp,
    diagnostics: &mut AcceptanceDiagnostics,
    expectation: &str,
    mut ready: impl FnMut(&mut TuiApp, &mut AcceptanceDiagnostics) -> bool,
) -> io::Result<()> {
    let deadline = Instant::now() + ACCEPTANCE_TIMEOUT;
    while Instant::now() < deadline {
        app.drain_session_subscription();
        diagnostics.observe_app(app);
        if ready(app, diagnostics) {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(1));
    }
    invalid_acceptance(format!("timed out waiting for {expectation}"))
}

fn acceptance_frame(
    app: &mut TuiApp,
    router: &InputRouter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<(Vec<String>, HitMap)> {
    app.set_drafts(router.draft_values());
    let frame = botster_tui_kit::render_to_lines_with_presentation_state(
        &app.surface(),
        ACCEPTANCE_WIDTH,
        ACCEPTANCE_HEIGHT,
        &router.render_state(),
        &app.plugin_presentation,
    )
    .map_err(io::Error::other)?;
    diagnostics.observe_frame(app, &frame.1);
    Ok(frame)
}

fn acceptance_has_action(
    app: &mut TuiApp,
    router: &mut InputRouter,
    action_id: &str,
    payload_matches: impl Fn(&Option<Value>) -> bool,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<bool> {
    let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
    Ok(hit_map.regions().iter().any(|region| {
        region
            .action
            .as_ref()
            .is_some_and(|action| action.id.0 == action_id && payload_matches(&action.payload))
    }))
}

fn activate_acceptance_action(
    app: &mut TuiApp,
    router: &mut InputRouter,
    action_id: &str,
    payload_matches: impl Fn(&Option<Value>) -> bool,
    evidence: &mut EvidenceWriter,
    case_id: Option<&str>,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<UiActionRequest> {
    let (lines, hit_map) = acceptance_frame(app, router, diagnostics)?;
    let matches = hit_map
        .regions()
        .iter()
        .filter(|region| {
            region
                .action
                .as_ref()
                .is_some_and(|action| action.id.0 == action_id && payload_matches(&action.payload))
        })
        .map(|region| {
            (
                region.node_id.clone(),
                region.action.clone().expect("filtered action"),
            )
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return invalid_acceptance(format!(
            "expected one rendered action {action_id}, found {}; focusable={:?}; rendered={:?}",
            matches.len(),
            focusable_ids(&hit_map),
            lines
                .iter()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .take(30)
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    let (expected_node_id, expected_action) = &matches[0];
    focus_acceptance_node(router, &hit_map, expected_node_id)?;
    evidence.event(
        "focused_control",
        case_id,
        json!({ "node_id": expected_node_id, "action_id": action_id }),
    )?;
    let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
    let dispatch = router.dispatch_event(
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &hit_map,
    );
    let request = match &dispatch {
        InputDispatch::Action(request) => request.clone(),
        other => {
            return invalid_acceptance(format!(
                "focused rendered action {action_id} did not dispatch: {other:?}"
            ));
        }
    };
    let expected_surface_id = if action_id.starts_with("botster.tui.") {
        renderer::WORKSPACE_SURFACE_ID
    } else {
        WORKSPACES_SURFACE
    };
    if request.action_id != expected_action.id
        || request.node_id.as_ref().map(|node_id| node_id.0.as_str())
            != Some(expected_node_id.as_str())
        || request.surface_id.0 != expected_surface_id
        || request.payload != expected_action.payload
        || request.kind != botster_ui_contract::UiActionKind::Submit
    {
        return invalid_acceptance(format!(
            "rendered action identity changed during keyboard dispatch: expected node={expected_node_id:?} action={:?} surface={expected_surface_id:?} payload={:?}; observed node={:?} action={:?} surface={:?} kind={:?} payload={:?}",
            expected_action.id,
            expected_action.payload,
            request.node_id,
            request.action_id,
            request.surface_id,
            request.kind,
            request.payload
        ));
    }
    diagnostics.observe_request(&request);
    evidence.event(
        "dispatched_action",
        case_id,
        serde_json::to_value(&request).map_err(io::Error::other)?,
    )?;
    app.handle_dispatch(dispatch);
    if let Some(error) = app.error.as_deref() {
        return invalid_acceptance(format!("action {action_id} failed: {error}"));
    }
    if let Some(result) = app
        .plugin_action_result
        .clone()
        .filter(|result| result.request_id == request.request_id)
    {
        diagnostics.observe_result(&result);
        evidence.event(
            "action_result",
            case_id,
            serde_json::to_value(&result).map_err(io::Error::other)?,
        )?;
        if result.state != botster_ui_contract::UiActionResultState::Accepted {
            return invalid_acceptance(format!(
                "action {action_id} was not accepted: state={:?} field_errors={:?} form_errors={:?} error={:?}",
                result.state, result.field_errors, result.form_errors, result.error
            ));
        }
    }
    Ok(request)
}

fn focus_acceptance_node(
    router: &mut InputRouter,
    hit_map: &HitMap,
    node_id: &str,
) -> io::Result<()> {
    router.reconcile(hit_map);
    let attempts = hit_map.focusable_regions().count().saturating_add(1);
    for _ in 0..attempts {
        if router.focused_node_id() == Some(node_id) {
            return Ok(());
        }
        router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            hit_map,
        );
    }
    invalid_acceptance(format!(
        "Tab traversal could not focus rendered node {node_id}"
    ))
}

fn select_acceptance_value(
    app: &mut TuiApp,
    router: &mut InputRouter,
    field_name: &str,
    expected: &str,
    evidence: &mut EvidenceWriter,
    case_id: &str,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
    let fields = hit_map
        .regions()
        .iter()
        .filter(|region| {
            region
                .field
                .as_ref()
                .is_some_and(|field| field.name == field_name)
        })
        .collect::<Vec<_>>();
    if fields.len() != 1 {
        return invalid_acceptance(format!(
            "expected one rendered {field_name} field, found {}",
            fields.len()
        ));
    }
    let node_id = fields[0].node_id.clone();
    let field = fields[0].field.clone().expect("filtered field");
    let target = Value::String(expected.to_string());
    let target_index = field
        .options
        .iter()
        .position(|value| value == &target)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("rendered {field_name} omitted option {expected:?}"),
            )
        })?;
    let current = router.draft_value(field_name).unwrap_or(&field.value);
    let current_index = field
        .options
        .iter()
        .position(|value| value == current)
        .unwrap_or(0);
    focus_acceptance_node(router, &hit_map, &node_id)?;
    evidence.event(
        "focused_control",
        Some(case_id),
        json!({ "node_id": node_id, "field": field_name }),
    )?;
    let (_, open_map) = acceptance_frame(app, router, diagnostics)?;
    router.dispatch_event(
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &open_map,
    );
    let steps = (target_index + field.options.len() - current_index) % field.options.len();
    for _ in 0..steps {
        let (_, map) = acceptance_frame(app, router, diagnostics)?;
        router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            &map,
        );
    }
    let (_, commit_map) = acceptance_frame(app, router, diagnostics)?;
    router.dispatch_event(
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &commit_map,
    );
    if router.draft_value(field_name) != Some(&target) {
        return invalid_acceptance(format!("keyboard selection did not choose {expected:?}"));
    }
    Ok(())
}

fn select_only_acceptance_value(
    app: &mut TuiApp,
    router: &mut InputRouter,
    field_name: &str,
    evidence: &mut EvidenceWriter,
    case_id: &str,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
    let field = hit_map
        .regions()
        .iter()
        .find_map(|region| {
            region
                .field
                .as_ref()
                .filter(|field| field.name == field_name)
                .map(|field| (region.node_id.clone(), field.clone()))
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("rendered {field_name} field is missing"),
            )
        })?;
    if field.1.options.len() != 1 {
        return invalid_acceptance(format!(
            "acceptance requires exactly one rendered {field_name} option"
        ));
    }
    let expected = field.1.options[0]
        .as_str()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("rendered {field_name} option is not a string"),
            )
        })?
        .to_string();
    select_acceptance_value(
        app,
        router,
        field_name,
        &expected,
        evidence,
        case_id,
        diagnostics,
    )
}

fn type_acceptance_text(
    app: &mut TuiApp,
    router: &mut InputRouter,
    field_name: &str,
    value: &str,
    evidence: &mut EvidenceWriter,
    case_id: &str,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
    let fields = hit_map
        .regions()
        .iter()
        .filter(|region| {
            region
                .field
                .as_ref()
                .is_some_and(|field| field.name == field_name)
        })
        .collect::<Vec<_>>();
    if fields.len() != 1 {
        return invalid_acceptance(format!(
            "expected one rendered {field_name} field, found {}",
            fields.len()
        ));
    }
    let node_id = fields[0].node_id.clone();
    if fields[0]
        .field
        .as_ref()
        .and_then(|field| field.value.as_str())
        .is_some_and(|initial| !initial.is_empty())
    {
        return invalid_acceptance(format!("rendered {field_name} must start empty"));
    }
    focus_acceptance_node(router, &hit_map, &node_id)?;
    evidence.event(
        "focused_control",
        Some(case_id),
        json!({ "node_id": node_id, "field": field_name }),
    )?;
    let carried_characters = router
        .draft_value(field_name)
        .and_then(Value::as_str)
        .map(|value| value.chars().count())
        .unwrap_or_default();
    for _ in 0..carried_characters {
        let (_, map) = acceptance_frame(app, router, diagnostics)?;
        router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            &map,
        );
    }
    for character in value.chars() {
        let (_, map) = acceptance_frame(app, router, diagnostics)?;
        router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE)),
            &map,
        );
    }
    if router.draft_value(field_name).and_then(Value::as_str) != Some(value) {
        return invalid_acceptance(format!(
            "keyboard typing did not produce requested {field_name}"
        ));
    }
    Ok(())
}

fn payload_field<'a>(payload: &'a Option<Value>, field: &str) -> Option<&'a str> {
    payload.as_ref()?.get(field)?.as_str()
}

fn focusable_ids(hit_map: &HitMap) -> Vec<String> {
    hit_map
        .focusable_regions()
        .take(24)
        .map(|region| region.node_id.clone())
        .collect()
}

fn invalid_acceptance<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, message.into()))
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

fn run_headless_live_runtime(args: AppArgs) -> DaemonTransportResult<()> {
    if let Some(error) = &args.connection_error {
        eprintln!("headless-live-runtime-error: {error}");
        return Err(DaemonTransportError::Protocol(
            "invalid Hub connection configuration",
        ));
    }
    let Some(endpoint) = args.daemon_endpoint() else {
        return Err(DaemonTransportError::NotRunning);
    };
    if let Some(data_dir) = args.hub_data_dir.as_ref() {
        if !data_dir.is_dir() {
            return Err(DaemonTransportError::Protocol(
                "injected hub data dir is not a directory",
            ));
        }
        println!("package-storage-context: configured");
    }
    let mut app = TuiApp::new(Some(endpoint));
    #[cfg(test)]
    {
        app.workspace_test_mode = true;
    }
    app.command = DEFAULT_COMMAND.to_string();
    app.spawn_session();
    if let Some(error) = &app.error {
        eprintln!("headless-live-runtime-error: {error}");
        return Err(DaemonTransportError::Protocol(
            "headless live runtime app error",
        ));
    }
    #[cfg(test)]
    {
        let rendered = renderer::render_to_lines(&app.surface(), 200, 48)
            .0
            .join("\n");
        assert!(rendered.contains("pending spawn"));
        assert_eq!(app.attached_session, None);
    }
    let session_id = app
        .selected_session
        .clone()
        .ok_or(DaemonTransportError::Protocol(
            "headless session was not selected",
        ))?;

    wait_for_authoritative_session(&mut app, &session_id)?;
    app.attach_selected_or_first();
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
        for required_feature in [
            FEATURE_SESSIONS,
            FEATURE_TERMINAL_STREAMING,
            FEATURE_RESIZE,
            FEATURE_PACKAGE_NAVIGATION,
            FEATURE_TERMINAL_READBACK,
            FEATURE_SESSION_ENTITY_SUBSCRIPTIONS,
        ] {
            assert!(
                compatibility
                    .features
                    .iter()
                    .any(|feature| feature == required_feature)
            );
        }
        assert!(rendered.contains("Sessions"));
        assert!(rendered.contains("Focused session"));
        assert!(rendered.contains(HEADLESS_OUTPUT));
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-terminal-output")
        );
    }
    println!("terminal-output: {HEADLESS_OUTPUT}");
    app.request_and_apply(DaemonRequest::ShutdownSession { session_id });
    Ok(())
}

fn wait_for_authoritative_session(app: &mut TuiApp, session_id: &str) -> DaemonTransportResult<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        app.poll_hub();
        if app.sessions.iter().any(|session| {
            session.session_id == session_id && session.is_attachable() && !session.pending
        }) {
            return Ok(());
        }
        thread::yield_now();
    }
    Err(DaemonTransportError::Protocol(
        "timed out waiting for authoritative session entity",
    ))
}

fn wait_for_app_output(app: &mut TuiApp, needle: &str) -> DaemonTransportResult<()> {
    if app.terminal_output.contains(needle) {
        return Ok(());
    }

    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        app.poll_hub();
        if app.terminal_output.contains(needle) {
            return Ok(());
        }
        thread::yield_now();
    }

    let observed_prefix = app.terminal_output.chars().take(256).collect::<String>();
    eprintln!(
        "timed out waiting for terminal output {needle:?}; terminal-output-prefix: {observed_prefix:?}"
    );
    Err(DaemonTransportError::Protocol(
        "timed out waiting for terminal output",
    ))
}

fn node(kind: UiNodeKind, id: &str, props: Value) -> UiNode {
    UiNode {
        kind,
        id: Some(UiNodeId(id.to_string()).into()),
        props: props.as_object().cloned().unwrap_or_default(),
        children: Vec::new(),
        slots: BTreeMap::new(),
    }
}

fn child(node: UiNode) -> UiChild {
    UiChild::Node(Box::new(node))
}

fn responsive_child(width: UiWidthClass, node: UiNode) -> UiChild {
    UiChild::Conditional(UiConditional::When {
        condition: UiCondition {
            width: Some(width),
            ..UiCondition::default()
        },
        node: Box::new(node),
    })
}

fn append_non_overlapping(output: &mut String, buffered: &str) {
    let max_overlap = output.len().min(buffered.len());
    let overlap = (0..=max_overlap)
        .rev()
        .find(|overlap| {
            output.is_char_boundary(output.len() - overlap)
                && buffered.is_char_boundary(*overlap)
                && output.ends_with(&buffered[..*overlap])
        })
        .unwrap_or_default();
    output.push_str(&buffered[overlap..]);
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

#[allow(clippy::too_many_arguments)]
fn workspace_button(
    id: &str,
    label: &str,
    action_id: &str,
    payload: Value,
    toolbar_overflow: &str,
    disabled: bool,
    tone: Option<&str>,
) -> UiNode {
    let mut control = button(id, label, action_id, payload);
    control.props.insert(
        "toolbar_overflow".to_string(),
        Value::String(toolbar_overflow.to_string()),
    );
    if let Some(tone) = tone {
        control
            .props
            .insert("tone".to_string(), Value::String(tone.to_string()));
    }
    if disabled
        && let Some(action) = control
            .props
            .get_mut("action")
            .and_then(Value::as_object_mut)
    {
        action.insert("disabled".to_string(), Value::Bool(true));
    }
    control
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

pub(crate) fn short_suffix() -> u64 {
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
            FEATURE_PLUGIN_SURFACE_RENDER.to_string(),
            FEATURE_PLUGIN_SURFACE_ACTION.to_string(),
            FEATURE_TERMINAL_READBACK.to_string(),
            FEATURE_SESSION_ENTITY_SUBSCRIPTIONS.to_string(),
        ],
        minimum_conformance_fixture_revision: MINIMUM_CONFORMANCE_FIXTURE_REVISION,
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

fn session_id_from_payload(payload: &Option<Value>) -> Option<String> {
    payload
        .as_ref()
        .and_then(|value| value.get("session_id"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
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

fn package_surface_nodes(package: &DaemonPackage, package_index: usize) -> Vec<UiNode> {
    package
        .surfaces
        .iter()
        .enumerate()
        .map(|(surface_index, surface)| {
            node(
                UiNodeKind::Text,
                &format!("tui-package-{package_index}-surface-{surface_index}"),
                json!({
                    "text": format!(
                        "surface: package={} {}",
                        package.package_name,
                        package_surface_text(surface)
                    )
                }),
            )
        })
        .collect()
}

fn package_surface_text(surface: &PackageSurfaceDescriptor) -> String {
    let supports = if surface.supports.is_empty() {
        "none".to_string()
    } else {
        surface
            .supports
            .iter()
            .map(|operation| match operation {
                PackageSurfaceOperation::Render => "render",
                PackageSurfaceOperation::Action => "action",
            })
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "id={} kind={} title={} supports={supports}",
        surface.id,
        match surface.kind {
            PackageSurfaceKind::App => "app",
            PackageSurfaceKind::Settings => "settings",
            PackageSurfaceKind::DashboardWidget => "dashboard_widget",
            PackageSurfaceKind::Diagnostics => "diagnostics",
        },
        surface.title
    )
}

fn package_availability_nodes(package: &DaemonPackage, index: usize) -> Vec<UiNode> {
    let mut nodes = Vec::new();
    for (reason_index, reason) in package.availability.reasons.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-availability-reason-{reason_index}"),
            json!({ "text": format!("package blocked: {}", availability_reason_text(reason)) }),
        ));
    }
    for (dependency_index, dependency) in package.dependency_availability.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-dependency-{dependency_index}"),
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
                    "tui-package-{index}-dependency-{dependency_index}-reason-{reason_index}"
                ),
                json!({ "text": format!("dependency blocked: {}", availability_reason_text(reason)) }),
            ));
        }
    }
    for (feature_index, feature) in package.feature_availability.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-feature-{feature_index}"),
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
                &format!("tui-package-{index}-feature-{feature_index}-reason-{reason_index}"),
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
            &format!("tui-package-{index}-show"),
            "Show",
            "botster.tui.package.show",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-enable"),
            "Enable",
            "botster.tui.package.enable",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-disable"),
            "Disable",
            "botster.tui.package.disable",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-remove"),
            "Remove",
            "botster.tui.package.remove",
            json!({ "package_name": package.package_name }),
        ),
        button(
            &format!("tui-package-{index}-update-status"),
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
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-start"),
            "Start",
            "botster.tui.entrypoint.start",
            payload.clone(),
        ),
        button(
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-stop"),
            "Stop",
            "botster.tui.entrypoint.stop",
            payload.clone(),
        ),
        button(
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-restart"),
            "Restart",
            "botster.tui.entrypoint.restart",
            payload.clone(),
        ),
        button(
            &format!("tui-package-{package_index}-entrypoint-{entrypoint_index}-status"),
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

fn plugin_surface_body_node(surface: &DaemonPluginSurface) -> Result<UiNode, String> {
    // Binding sentinels intentionally do not satisfy the final rendered prop
    // schema. Static delivered trees can be validated immediately; bound trees
    // are validated after materialization in plugin_surface_render_root.
    if !node_requires_binding_materialization(&surface.body) {
        surface.body.validate().map_err(|error| {
            format!(
                "plugin surface {}:{} failed UiNode validate: {error}",
                surface.package_name, surface.surface_id
            )
        })?;
        renderer::tui_capabilities()
            .validate_node(&surface.body)
            .map_err(|error| {
                format!(
                    "plugin surface {}:{} unsupported TUI primitive: {error}",
                    surface.package_name, surface.surface_id
                )
            })?;
    }
    Ok(surface.body.clone())
}

fn normalize_plugin_surface(surface: DaemonPluginSurface) -> Result<DaemonPluginSurface, String> {
    let snapshot = surface.ui_tree_snapshot.as_ref().ok_or_else(|| {
        format!(
            "plugin surface {}:{} omitted ui_tree_snapshot",
            surface.package_name, surface.surface_id
        )
    })?;
    if snapshot.package_name != surface.package_name
        || snapshot.surface_id != surface.surface_id
        || snapshot.body != surface.body
    {
        return Err(format!(
            "plugin surface {}:{} ui_tree_snapshot identity/body mismatch",
            surface.package_name, surface.surface_id
        ));
    }
    Ok(surface)
}

fn materialize_plugin_surface(
    root: &UiNode,
    session_entities: &SessionEntityState,
) -> Result<UiNode, String> {
    let materialized = if node_requires_binding_materialization(root) {
        let rows = session_entities.binding_rows()?;
        materialize_binding_node(root, &rows, None, false)?
    } else {
        root.clone()
    };
    reject_duplicate_realized_node_ids(&materialized)?;
    Ok(materialized)
}

fn node_requires_binding_materialization(node: &UiNode) -> bool {
    matches!(node.id, Some(UiAuthoredNodeId::Bind(_)))
        || node.props.values().any(value_contains_binding)
        || node
            .children
            .iter()
            .chain(node.slots.values().flatten())
            .any(child_requires_binding_materialization)
}

fn child_requires_binding_materialization(child: &UiChild) -> bool {
    match child {
        UiChild::Node(node)
        | UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => {
            node_requires_binding_materialization(node)
        }
        UiChild::BindList(_) | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { .. }) => {
            true
        }
    }
}

fn value_contains_binding(value: &Value) -> bool {
    match value {
        Value::Object(values) => {
            (values.len() == 1 && values.get("$bind").and_then(Value::as_str).is_some())
                || values.values().any(value_contains_binding)
        }
        Value::Array(values) => values.iter().any(value_contains_binding),
        _ => false,
    }
}

fn materialize_binding_node(
    source: &UiNode,
    session_rows: &[Value],
    item: Option<&Value>,
    bound_id_allowed: bool,
) -> Result<UiNode, String> {
    let mut node = source.clone();
    node.id = match source.id.as_ref() {
        None => None,
        Some(UiAuthoredNodeId::Literal(id)) => Some(UiAuthoredNodeId::Literal(id.clone())),
        Some(UiAuthoredNodeId::Bind(binding)) => {
            if !bound_id_allowed {
                return Err(
                    "bound node id is only supported on a direct BindList item template root"
                        .to_string(),
                );
            }
            let value = resolve_item_binding(&binding.path, item)?;
            let id = value
                .as_str()
                .ok_or_else(|| "bound node id did not resolve to a string".to_string())?;
            if id.trim().is_empty() {
                return Err("bound node id resolved to a blank string".to_string());
            }
            Some(UiAuthoredNodeId::Literal(UiNodeId(id.to_string())))
        }
    };
    for value in node.props.values_mut() {
        *value = materialize_binding_value(value, item)?;
    }
    node.children = materialize_binding_children(&source.children, session_rows, item)?;
    node.slots = source
        .slots
        .iter()
        .map(|(name, children)| {
            materialize_binding_children(children, session_rows, item)
                .map(|children| (name.clone(), children))
        })
        .collect::<Result<_, _>>()?;
    Ok(node)
}

fn materialize_binding_children(
    children: &[UiChild],
    session_rows: &[Value],
    item: Option<&Value>,
) -> Result<Vec<UiChild>, String> {
    let mut materialized = Vec::new();
    for child in children {
        match child {
            UiChild::Node(node) => materialized.push(UiChild::Node(Box::new(
                materialize_binding_node(node, session_rows, item, false)?,
            ))),
            UiChild::Conditional(UiConditional::When { condition, node }) => {
                materialized.push(UiChild::Conditional(UiConditional::When {
                    condition: condition.clone(),
                    node: Box::new(materialize_binding_node(node, session_rows, item, false)?),
                }));
            }
            UiChild::Conditional(UiConditional::Hidden { condition, node }) => {
                materialized.push(UiChild::Conditional(UiConditional::Hidden {
                    condition: condition.clone(),
                    node: Box::new(materialize_binding_node(node, session_rows, item, false)?),
                }));
            }
            UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { predicate, node }) => {
                materialized.push(UiChild::BindIf(
                    botster_ui_contract::UiBindIf::PresentationIf {
                        predicate: predicate.clone(),
                        node: Box::new(materialize_binding_node(node, session_rows, item, false)?),
                    },
                ));
            }
            UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { path, node }) => {
                let value = resolve_item_binding(path, item)?;
                if binding_truthy(value) {
                    materialized.push(UiChild::Node(Box::new(materialize_binding_node(
                        node,
                        session_rows,
                        item,
                        false,
                    )?)));
                }
            }
            UiChild::BindList(botster_ui_contract::UiBindList::BindList {
                source,
                r#where,
                item_template,
                empty_template,
            }) => {
                if source != "/session" {
                    return Err(format!("unsupported binding source {source:?}"));
                }
                let reference = session_binding_reference_row();
                for field in r#where.keys() {
                    if !reference.contains_key(field) {
                        return Err(format!(
                            "unsupported /session where field {field:?}; the entity was not treated as unavailable"
                        ));
                    }
                }
                let matching = session_rows
                    .iter()
                    .filter(|row| {
                        r#where
                            .iter()
                            .all(|(field, expected)| row.get(field) == Some(expected))
                    })
                    .collect::<Vec<_>>();
                if matching.is_empty() {
                    if let Some(empty_template) = empty_template {
                        materialized.push(UiChild::Node(Box::new(materialize_binding_node(
                            empty_template,
                            session_rows,
                            None,
                            false,
                        )?)));
                    }
                } else {
                    for row in matching {
                        materialized.push(UiChild::Node(Box::new(materialize_binding_node(
                            item_template,
                            session_rows,
                            Some(row),
                            true,
                        )?)));
                    }
                }
            }
        }
    }
    Ok(materialized)
}

fn reject_duplicate_realized_node_ids(root: &UiNode) -> Result<(), String> {
    collect_realized_node_ids(root).map(|_| ())
}

enum RealizedChildCondition {
    When(UiCondition),
    Hidden(UiCondition),
    Presentation(botster_ui_contract::UiPresentationPredicate),
}

fn collect_realized_node_ids(node: &UiNode) -> Result<std::collections::BTreeSet<String>, String> {
    let mut realized = std::collections::BTreeSet::new();
    if let Some(UiAuthoredNodeId::Literal(id)) = &node.id {
        realized.insert(id.0.clone());
    }

    let mut children = Vec::new();
    for child in node.children.iter().chain(node.slots.values().flatten()) {
        let (ids, condition) = collect_realized_child_ids(child)?;
        reject_realized_node_id_overlap(&realized, &ids)?;
        children.push((ids, condition));
    }
    for (index, (left_ids, left_condition)) in children.iter().enumerate() {
        for (right_ids, right_condition) in children.iter().skip(index + 1) {
            if !realized_children_are_exclusive(left_condition, right_condition) {
                reject_realized_node_id_overlap(left_ids, right_ids)?;
            }
        }
    }
    for (ids, _) in children {
        realized.extend(ids);
    }
    Ok(realized)
}

fn collect_realized_child_ids(
    child: &UiChild,
) -> Result<
    (
        std::collections::BTreeSet<String>,
        Option<RealizedChildCondition>,
    ),
    String,
> {
    match child {
        UiChild::Node(node)
        | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. }) => {
            collect_realized_node_ids(node).map(|ids| (ids, None))
        }
        UiChild::Conditional(UiConditional::When { condition, node }) => {
            collect_realized_node_ids(node)
                .map(|ids| (ids, Some(RealizedChildCondition::When(condition.clone()))))
        }
        UiChild::Conditional(UiConditional::Hidden { condition, node }) => {
            collect_realized_node_ids(node)
                .map(|ids| (ids, Some(RealizedChildCondition::Hidden(condition.clone()))))
        }
        UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { predicate, node }) => {
            collect_realized_node_ids(node).map(|ids| {
                (
                    ids,
                    Some(RealizedChildCondition::Presentation(predicate.clone())),
                )
            })
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            let mut ids = collect_realized_node_ids(item_template)?;
            if let Some(empty_template) = empty_template {
                ids.extend(collect_realized_node_ids(empty_template)?);
            }
            Ok((ids, None))
        }
    }
}

fn realized_children_are_exclusive(
    left: &Option<RealizedChildCondition>,
    right: &Option<RealizedChildCondition>,
) -> bool {
    match (left, right) {
        (Some(RealizedChildCondition::When(left)), Some(RealizedChildCondition::When(right))) => {
            conditions_are_distinct_on_one_axis(left, right)
        }
        (Some(RealizedChildCondition::When(left)), Some(RealizedChildCondition::Hidden(right)))
        | (Some(RealizedChildCondition::Hidden(left)), Some(RealizedChildCondition::When(right))) => {
            left == right
        }
        (
            Some(RealizedChildCondition::Presentation(left)),
            Some(RealizedChildCondition::Presentation(right)),
        ) => presentation_predicates_are_exclusive(left, right),
        _ => false,
    }
}

fn presentation_predicates_are_exclusive(
    left: &botster_ui_contract::UiPresentationPredicate,
    right: &botster_ui_contract::UiPresentationPredicate,
) -> bool {
    match (left, right) {
        (
            botster_ui_contract::UiPresentationPredicate::Equals {
                key: left_key,
                value: left_value,
            },
            botster_ui_contract::UiPresentationPredicate::Equals {
                key: right_key,
                value: right_value,
            },
        ) => left_key == right_key && left_value != right_value,
        _ => false,
    }
}

fn conditions_are_distinct_on_one_axis(left: &UiCondition, right: &UiCondition) -> bool {
    condition_axis_count(left) == 1
        && condition_axis_count(right) == 1
        && ((left.width.is_some() && right.width.is_some() && left.width != right.width)
            || (left.height.is_some() && right.height.is_some() && left.height != right.height)
            || (left.pointer.is_some() && right.pointer.is_some() && left.pointer != right.pointer)
            || (left.orientation.is_some()
                && right.orientation.is_some()
                && left.orientation != right.orientation)
            || (left.keyboard_occluded.is_some()
                && right.keyboard_occluded.is_some()
                && left.keyboard_occluded != right.keyboard_occluded))
}

fn condition_axis_count(condition: &UiCondition) -> usize {
    usize::from(condition.width.is_some())
        + usize::from(condition.height.is_some())
        + usize::from(condition.pointer.is_some())
        + usize::from(condition.orientation.is_some())
        + usize::from(condition.keyboard_occluded.is_some())
}

fn reject_realized_node_id_overlap(
    left: &std::collections::BTreeSet<String>,
    right: &std::collections::BTreeSet<String>,
) -> Result<(), String> {
    if let Some(id) = left.intersection(right).next() {
        return Err(format!("duplicate materialized node id {id:?}"));
    }
    Ok(())
}

fn materialize_binding_value(value: &Value, item: Option<&Value>) -> Result<Value, String> {
    match value {
        Value::Object(values)
            if values.len() == 1 && values.get("$bind").and_then(Value::as_str).is_some() =>
        {
            let path = values
                .get("$bind")
                .and_then(Value::as_str)
                .expect("guarded binding path");
            Ok(resolve_item_binding(path, item)?.clone())
        }
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| {
                materialize_binding_value(value, item).map(|value| (key.clone(), value))
            })
            .collect::<Result<serde_json::Map<_, _>, _>>()
            .map(Value::Object),
        Value::Array(values) => values
            .iter()
            .map(|value| materialize_binding_value(value, item))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        _ => Ok(value.clone()),
    }
}

fn resolve_item_binding<'a>(path: &str, item: Option<&'a Value>) -> Result<&'a Value, String> {
    let relative = path.strip_prefix("@/").ok_or_else(|| {
        if path.starts_with('/') {
            format!("unsupported absolute binding path {path:?}")
        } else {
            format!("unsupported binding path {path:?}")
        }
    })?;
    let item = item.ok_or_else(|| format!("item-relative binding {path:?} has no current row"))?;
    item.pointer(&format!("/{relative}"))
        .ok_or_else(|| format!("binding path {path:?} is missing from the current session row"))
}

fn binding_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|value| value != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

fn iframe_unsupported_diagnostic(surface: &DaemonPluginSurface) -> Option<String> {
    let iframe = find_iframe_node(&surface.body)?;
    let title = iframe
        .props
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("untitled");
    let src = iframe
        .props
        .get("src")
        .and_then(Value::as_str)
        .unwrap_or("missing");
    let sandbox = iframe
        .props
        .get("sandbox")
        .map(compact_json)
        .unwrap_or_else(|| "default".to_string());
    Some(format!(
        "plugin surface iframe unsupported: package={} surface={} title={} src={} sandbox={} open=copy URL or open it in a browser",
        surface.package_name, surface.surface_id, title, src, sandbox
    ))
}

fn find_iframe_node(node: &UiNode) -> Option<&UiNode> {
    if node.kind == UiNodeKind::Iframe {
        return Some(node);
    }
    node.children
        .iter()
        .chain(node.slots.values().flatten())
        .find_map(find_iframe_child)
}

fn find_iframe_child(child: &UiChild) -> Option<&UiNode> {
    match child {
        UiChild::Node(node) => find_iframe_node(node),
        UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. }) => find_iframe_node(node),
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => find_iframe_node(item_template)
            .or_else(|| empty_template.as_deref().and_then(find_iframe_node)),
        UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => {
            find_iframe_node(node)
        }
    }
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn plugin_action_result_text(result: &UiActionResult) -> String {
    let mut parts = vec![
        format!("state={:?}", result.state),
        format!("request_id={}", result.request_id.0),
    ];
    if !result.form_errors.is_empty() {
        parts.push(format!("form_errors={}", result.form_errors.join(" | ")));
    }
    if let Some(error) = &result.error {
        parts.push(format!("error={error}"));
    }
    parts.join(" ")
}

fn plugin_surface_render_root(
    surface: &DaemonPluginSurface,
    result: Option<&UiActionResult>,
    session_entities: &SessionEntityState,
) -> UiNode {
    if let Some(diagnostic) = iframe_unsupported_diagnostic(surface) {
        return node(
            UiNodeKind::Text,
            "tui-plugin-surface-iframe-unsupported",
            json!({ "text": diagnostic }),
        );
    }
    let root = match plugin_surface_body_node(surface) {
        Ok(root) => root,
        Err(error) => {
            return node(
                UiNodeKind::Text,
                "tui-plugin-surface-invalid",
                json!({ "text": format!("plugin surface render: {error}") }),
            );
        }
    };
    let mut root = match materialize_plugin_surface(&root, session_entities) {
        Ok(root) => root,
        Err(error) => {
            return node(
                UiNodeKind::Text,
                "tui-plugin-surface-binding-invalid",
                json!({ "text": format!("plugin surface binding: {error}") }),
            );
        }
    };
    if let Some(result) = result {
        apply_plugin_result_errors(&mut root, result);
    }
    if let Err(error) = root.validate() {
        return node(
            UiNodeKind::Text,
            "tui-plugin-surface-materialized-invalid",
            json!({
                "text": format!(
                    "plugin surface render: plugin surface {}:{} failed UiNode validate: {error}",
                    surface.package_name, surface.surface_id
                )
            }),
        );
    }
    if let Err(error) = renderer::tui_capabilities().validate_node(&root) {
        return node(
            UiNodeKind::Text,
            "tui-plugin-surface-materialized-unsupported",
            json!({
                "text": format!(
                    "plugin surface render: plugin surface {}:{} unsupported TUI primitive: {error}",
                    surface.package_name, surface.surface_id
                )
            }),
        );
    }
    root
}

fn apply_plugin_result_errors(root_node: &mut UiNode, result: &UiActionResult) {
    let field_error = root_node
        .id
        .as_ref()
        .and_then(UiAuthoredNodeId::as_literal)
        .and_then(|id| result.field_errors.get(&id.0))
        .or_else(|| {
            root_node
                .props
                .get("name")
                .and_then(Value::as_str)
                .and_then(|name| result.field_errors.get(name))
        });
    if let Some(messages) = field_error {
        root_node
            .props
            .insert("error".to_string(), Value::String(messages.join(" | ")));
    }
    if root_node.kind == UiNodeKind::Form && !result.form_errors.is_empty() {
        let form_id = root_node
            .id
            .as_ref()
            .and_then(UiAuthoredNodeId::as_literal)
            .map_or("plugin-form", |id| id.0.as_str());
        root_node.children.insert(
            0,
            child(node(
                UiNodeKind::Text,
                &format!("{form_id}-result-error"),
                json!({ "text": format!("error: {}", result.form_errors.join(" | ")) }),
            )),
        );
    }
    for child in root_node
        .children
        .iter_mut()
        .chain(root_node.slots.values_mut().flatten())
    {
        apply_plugin_result_errors_to_child(child, result);
    }
}

#[cfg(test)]
fn static_child_node(child: &UiChild) -> Option<&UiNode> {
    match child {
        UiChild::Node(node) => Some(node),
        UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. }) => Some(node),
        UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => Some(node),
        UiChild::BindList(_) => None,
    }
}

fn apply_plugin_result_errors_to_child(child: &mut UiChild, result: &UiActionResult) {
    match child {
        UiChild::Node(node) => apply_plugin_result_errors(node, result),
        UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. }) => {
            apply_plugin_result_errors(node, result);
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            apply_plugin_result_errors(item_template, result);
            if let Some(empty_template) = empty_template {
                apply_plugin_result_errors(empty_template, result);
            }
        }
        UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => {
            apply_plugin_result_errors(node, result);
        }
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
        "tui-install-plan-summary",
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
            &format!("tui-install-plan-effect-{index}"),
            json!({ "text": format!("install effect: {}:{}", effect.kind, effect.message) }),
        ));
    }
    for (index, diagnostic) in plan.diagnostics.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-install-plan-diagnostic-{index}"),
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
        "tui-update-status-summary",
        json!({ "text": text }),
    )];
    if let Some(pin) = &status.pin {
        nodes.push(button(
            "tui-update-status-preview",
            "Preview update",
            "botster.tui.package.update_preview",
            json!({ "package_name": status.package_name, "pin": pin }),
        ));
        nodes.push(button(
            "tui-update-status-apply",
            "Apply update",
            "botster.tui.package.update_apply",
            json!({ "package_name": status.package_name, "pin": pin }),
        ));
    }
    for (index, diagnostic) in status.diagnostics.iter().enumerate() {
        nodes.push(node(
            UiNodeKind::Text,
            &format!("tui-update-status-diagnostic-{index}"),
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
    use botster_ui_contract::{
        UiActionId, UiActionKind, UiActionRequest, UiActionRequestId, UiSurfaceId,
    };
    use std::path::Path;

    #[derive(Clone, Copy, Debug)]
    enum SessionEntityExpectation<'a> {
        Lifecycle(&'a str),
        Absent,
    }

    fn session_entity_expectation_satisfied(
        state: &SessionEntityState,
        session_id: &str,
        expectation: SessionEntityExpectation<'_>,
    ) -> bool {
        state.subscription_id.is_some()
            && state.has_snapshot
            && match expectation {
                SessionEntityExpectation::Lifecycle(lifecycle_class) => state
                    .entities
                    .get(session_id)
                    .is_some_and(|entity| entity.lifecycle_class == lifecycle_class),
                SessionEntityExpectation::Absent => !state.entities.contains_key(session_id),
            }
    }

    fn session_entity_expectation_diagnostic(
        state: &SessionEntityState,
        session_id: &str,
        expectation: SessionEntityExpectation<'_>,
    ) -> String {
        let observed = state.entities.get(session_id).map_or_else(
            || "absent".to_string(),
            |entity| {
                format!(
                    "lifecycle_class={} lifecycle={:?} registry_state={}",
                    entity.lifecycle_class, entity.lifecycle, entity.registry_state
                )
            },
        );
        format!(
            "subscription_id={:?} has_snapshot={} snapshot_seq={:?} expected_session_id={session_id} expected={expectation:?} observed={observed}",
            state.subscription_id, state.has_snapshot, state.snapshot_seq
        )
    }

    fn wait_for_session_entity_expectation(
        app: &mut TuiApp,
        session_id: &str,
        expectation: SessionEntityExpectation<'_>,
        context: &str,
    ) {
        let deadline = Instant::now() + Duration::from_secs(7);
        while !session_entity_expectation_satisfied(&app.session_entities, session_id, expectation)
            && Instant::now() < deadline
        {
            app.poll_hub();
            thread::yield_now();
        }
        assert!(
            session_entity_expectation_satisfied(&app.session_entities, session_id, expectation,),
            "{context}: {}",
            session_entity_expectation_diagnostic(&app.session_entities, session_id, expectation,)
        );
    }

    fn mouse_event(kind: crossterm::event::MouseEventKind, column: u16, row: u16) -> Event {
        Event::Mouse(crossterm::event::MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn click_dispatch(hit_map: &HitMap, node_id: &str) -> InputDispatch {
        click_dispatch_for_surface(hit_map, node_id, None)
    }

    fn click_dispatch_for_surface(
        hit_map: &HitMap,
        node_id: &str,
        surface_id: Option<&str>,
    ) -> InputDispatch {
        let region = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == node_id)
            .unwrap_or_else(|| panic!("{node_id} should be present in the rendered hit map"));
        let (column, row) = (region.rect.x, region.rect.y);
        let mut router = InputRouter::new(match surface_id {
            Some(surface_id) => renderer::action_request_context_for(surface_id),
            None => renderer::action_request_context(),
        });
        let _ = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            hit_map,
        );
        router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            hit_map,
        )
    }

    fn find_ui_node_by_id<'a>(root: &'a UiNode, node_id: &str) -> Option<&'a UiNode> {
        if root
            .id
            .as_ref()
            .and_then(UiAuthoredNodeId::as_literal)
            .is_some_and(|id| id.0 == node_id)
        {
            return Some(root);
        }
        root.children
            .iter()
            .chain(root.slots.values().flatten())
            .filter_map(static_child_node)
            .find_map(|child| find_ui_node_by_id(child, node_id))
    }

    fn node_action(node: &UiNode) -> botster_ui_contract::UiAction {
        serde_json::from_value(
            node.props
                .get("action")
                .cloned()
                .expect("rendered action-bearing node has action metadata"),
        )
        .expect("rendered action metadata follows the Hub contract")
    }

    fn find_presentation_bound_node<'a>(
        root: &'a UiNode,
        key: &str,
        equals: Option<&Value>,
    ) -> Option<&'a UiNode> {
        for child in root.children.iter().chain(root.slots.values().flatten()) {
            if let UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf {
                predicate,
                node,
            }) = child
            {
                let matches = match predicate {
                    botster_ui_contract::UiPresentationPredicate::Present {
                        key: predicate_key,
                    } => predicate_key.0 == key && equals.is_none(),
                    botster_ui_contract::UiPresentationPredicate::Equals {
                        key: predicate_key,
                        value,
                    } => predicate_key.0 == key && equals == Some(value),
                    botster_ui_contract::UiPresentationPredicate::Truthy { .. } => false,
                };
                if matches {
                    return Some(node);
                }
            }
            if let Some(node) = static_child_node(child)
                && let Some(found) = find_presentation_bound_node(node, key, equals)
            {
                return Some(found);
            }
        }
        None
    }

    const WORKSPACES_PACKAGE_NAME: &str = "botster-workspaces";
    const WORKSPACES_SURFACE_ID: &str = "workspaces";
    const WORKSPACES_DOWNSTREAM_TICKET: &str = "ticket_1785296184_677408";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WorkspacesProfile {
        Plumbing,
        Lifecycle,
    }

    impl WorkspacesProfile {
        fn parse(value: &str) -> Result<Self, String> {
            match value {
                "plumbing" => Ok(Self::Plumbing),
                "lifecycle" => Ok(Self::Lifecycle),
                _ => Err(format!(
                    "BOTSTER_TUI_WORKSPACES_PROFILE must be plumbing or lifecycle, got {value:?}"
                )),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum WorkspacesStage {
        ProfileSelected,
        PackageValidated,
        PackageInstalled,
        PackageEnabledAndReloaded,
        NavigationOpened,
        OwnerIndexRendered,
        OwnerDetailRendered,
        OwnerRowSelected,
        LiteralActionIdentityObserved,
        MouseDispatch,
        KeyboardDispatch,
        AcceptedOwnerAction,
        CanonicalItemRootIdentityObserved,
        CurrentRendered,
        EndedRendered,
        AbsentRendered,
        TransitionWithoutListOrSurfaceRefresh,
        FreshReconnectSubscription,
        FreshReconnectSnapshot,
        SurfaceReopened,
        HistoricalReferencesRehydrated,
        StaleGenerationRejected,
        AbsenceTemplateInert,
        SixteenReferenceScale,
        CleanShutdown,
    }

    impl WorkspacesStage {
        fn plumbing() -> &'static [Self] {
            &[
                Self::ProfileSelected,
                Self::PackageValidated,
                Self::PackageInstalled,
                Self::PackageEnabledAndReloaded,
                Self::NavigationOpened,
                Self::OwnerIndexRendered,
                Self::OwnerDetailRendered,
                Self::OwnerRowSelected,
                Self::LiteralActionIdentityObserved,
                Self::MouseDispatch,
                Self::KeyboardDispatch,
                Self::AcceptedOwnerAction,
                Self::CleanShutdown,
            ]
        }

        fn lifecycle() -> Vec<Self> {
            let mut stages = Self::plumbing().to_vec();
            stages.splice(
                stages.len() - 1..stages.len() - 1,
                [
                    Self::CanonicalItemRootIdentityObserved,
                    Self::CurrentRendered,
                    Self::EndedRendered,
                    Self::AbsentRendered,
                    Self::TransitionWithoutListOrSurfaceRefresh,
                    Self::FreshReconnectSubscription,
                    Self::FreshReconnectSnapshot,
                    Self::SurfaceReopened,
                    Self::HistoricalReferencesRehydrated,
                    Self::StaleGenerationRejected,
                    Self::AbsenceTemplateInert,
                    Self::SixteenReferenceScale,
                ],
            );
            stages
        }
    }

    #[derive(Debug)]
    struct WorkspacesLedger {
        profile: WorkspacesProfile,
        completed: std::collections::BTreeSet<WorkspacesStage>,
    }

    impl WorkspacesLedger {
        fn new(profile: WorkspacesProfile) -> Self {
            let mut ledger = Self {
                profile,
                completed: std::collections::BTreeSet::new(),
            };
            ledger.record(WorkspacesStage::ProfileSelected);
            ledger
        }

        fn record(&mut self, stage: WorkspacesStage) {
            self.completed.insert(stage);
            println!(
                "workspaces-acceptance: profile={:?} stage={stage:?}",
                self.profile
            );
        }

        fn missing(&self) -> Vec<WorkspacesStage> {
            let required = match self.profile {
                WorkspacesProfile::Plumbing => WorkspacesStage::plumbing().to_vec(),
                WorkspacesProfile::Lifecycle => WorkspacesStage::lifecycle(),
            };
            required
                .into_iter()
                .filter(|stage| !self.completed.contains(stage))
                .collect()
        }

        fn assert_complete(&self) -> Result<(), String> {
            let missing = self.missing();
            if missing.is_empty() {
                println!(
                    "workspaces-acceptance: profile={:?} ledger=complete stages={:?}",
                    self.profile, self.completed
                );
                Ok(())
            } else {
                Err(format!(
                    "Workspaces {:?} acceptance ledger incomplete: missing {missing:?}",
                    self.profile
                ))
            }
        }
    }

    fn validate_workspaces_package(path: &Path) -> Result<PathBuf, String> {
        if !path.is_dir() {
            return Err(format!(
                "BOTSTER_WORKSPACES_PACKAGE_PATH is not a directory: {}",
                path.display()
            ));
        }
        let manifest_path = path.join("botster-package.json");
        let plugin_path = path.join("plugin.lua");
        if !manifest_path.is_file() || !plugin_path.is_file() {
            return Err(format!(
                "BOTSTER_WORKSPACES_PACKAGE_PATH must contain botster-package.json and plugin.lua: {}",
                path.display()
            ));
        }
        let manifest: Value = serde_json::from_slice(
            &std::fs::read(&manifest_path)
                .map_err(|error| format!("read {}: {error}", manifest_path.display()))?,
        )
        .map_err(|error| format!("parse {}: {error}", manifest_path.display()))?;
        if manifest.get("name").and_then(Value::as_str) != Some(WORKSPACES_PACKAGE_NAME) {
            return Err(format!(
                "BOTSTER_WORKSPACES_PACKAGE_PATH manifest name must be {WORKSPACES_PACKAGE_NAME}: {}",
                manifest_path.display()
            ));
        }
        std::fs::canonicalize(path)
            .map_err(|error| format!("canonicalize {}: {error}", path.display()))
    }

    fn find_action_node<'a>(
        root: &'a UiNode,
        action_id: &str,
        payload_key: &str,
        payload_value: &str,
    ) -> Option<&'a UiNode> {
        let matches = root
            .props
            .get("action")
            .and_then(|value| {
                serde_json::from_value::<botster_ui_contract::UiAction>(value.clone()).ok()
            })
            .is_some_and(|action| {
                action.id.0 == action_id
                    && action.payload.as_ref().is_some_and(|payload| {
                        payload.get(payload_key).and_then(Value::as_str) == Some(payload_value)
                    })
            });
        if matches {
            return Some(root);
        }
        root.children
            .iter()
            .chain(root.slots.values().flatten())
            .find_map(|child| match child {
                UiChild::BindList(botster_ui_contract::UiBindList::BindList {
                    item_template,
                    empty_template,
                    ..
                }) => find_action_node(item_template, action_id, payload_key, payload_value)
                    .or_else(|| {
                        empty_template.as_deref().and_then(|node| {
                            find_action_node(node, action_id, payload_key, payload_value)
                        })
                    }),
                _ => static_child_node(child)
                    .and_then(|node| find_action_node(node, action_id, payload_key, payload_value)),
            })
    }

    fn unique_hit_action(
        hit_map: &HitMap,
        action_id: &str,
        payload_key: &str,
        payload_value: &str,
    ) -> Result<(UiNodeId, botster_ui_contract::UiAction), String> {
        let matches = hit_map
            .regions()
            .iter()
            .filter(|region| {
                region.action.as_ref().is_some_and(|action| {
                    action.id.0 == action_id
                        && action.payload.as_ref().is_some_and(|payload| {
                            payload.get(payload_key).and_then(Value::as_str) == Some(payload_value)
                        })
                })
            })
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return Ok((
                UiNodeId(matches[0].node_id.clone()),
                matches[0]
                    .action
                    .clone()
                    .expect("matching hit region has action metadata"),
            ));
        }
        let matching_node_ids = matches
            .iter()
            .take(8)
            .map(|region| region.node_id.clone())
            .collect::<Vec<_>>();
        Err(format!(
            "expected exactly one production hit region for {action_id} with {payload_key}={payload_value:?}, found {}; matching_node_ids={matching_node_ids:?}",
            matches.len()
        ))
    }

    #[derive(Debug, Clone, PartialEq)]
    struct SessionBindingDescriptor {
        filters: std::collections::BTreeMap<String, Value>,
        item_template: UiNode,
        empty_template: Option<UiNode>,
    }

    impl SessionBindingDescriptor {
        fn session_uuid(&self) -> Option<&str> {
            self.filters.get("session_uuid").and_then(Value::as_str)
        }

        fn lifecycle_class(&self) -> Option<&str> {
            self.filters.get("lifecycle_class").and_then(Value::as_str)
        }

        fn item_root_id(&self) -> Option<&str> {
            self.item_template
                .id
                .as_ref()
                .and_then(UiAuthoredNodeId::as_literal)
                .map(|id| id.0.as_str())
        }

        fn empty_root_id(&self) -> Option<&str> {
            self.empty_template
                .as_ref()
                .and_then(|node| node.id.as_ref())
                .and_then(UiAuthoredNodeId::as_literal)
                .map(|id| id.0.as_str())
        }
    }

    fn collect_session_bindings(root: &UiNode) -> Vec<SessionBindingDescriptor> {
        fn visit_child(child: &UiChild, descriptors: &mut Vec<SessionBindingDescriptor>) {
            match child {
                UiChild::Node(node)
                | UiChild::Conditional(UiConditional::When { node, .. })
                | UiChild::Conditional(UiConditional::Hidden { node, .. })
                | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. })
                | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. }) => {
                    visit_node(node, descriptors)
                }
                UiChild::BindList(botster_ui_contract::UiBindList::BindList {
                    source,
                    r#where,
                    item_template,
                    empty_template,
                }) => {
                    if source == "/session" {
                        descriptors.push(SessionBindingDescriptor {
                            filters: r#where.clone(),
                            item_template: item_template.as_ref().clone(),
                            empty_template: empty_template.as_deref().cloned(),
                        });
                    }
                    visit_node(item_template, descriptors);
                    if let Some(empty_template) = empty_template {
                        visit_node(empty_template, descriptors);
                    }
                }
            }
        }

        fn visit_node(node: &UiNode, descriptors: &mut Vec<SessionBindingDescriptor>) {
            for child in node.children.iter().chain(node.slots.values().flatten()) {
                visit_child(child, descriptors);
            }
        }

        let mut descriptors = Vec::new();
        visit_node(root, &mut descriptors);
        descriptors
    }

    fn session_binding<'a>(
        bindings: &'a [SessionBindingDescriptor],
        session_uuid: &str,
        lifecycle_class: Option<&str>,
    ) -> &'a SessionBindingDescriptor {
        bindings
            .iter()
            .find(|binding| {
                binding.session_uuid() == Some(session_uuid)
                    && binding.lifecycle_class() == lifecycle_class
            })
            .unwrap_or_else(|| {
                panic!(
                    "owner surface must provide /session binding for session_uuid={session_uuid} lifecycle_class={lifecycle_class:?}; required by {WORKSPACES_DOWNSTREAM_TICKET}"
                )
            })
    }

    fn assert_binding_realization(
        materialized: &UiNode,
        binding: &SessionBindingDescriptor,
        expect_item: bool,
        expect_empty: bool,
    ) {
        let item_id = binding
            .item_root_id()
            .expect("Workspaces keeps per-reference item root identity literal");
        assert_eq!(
            find_ui_node_by_id(materialized, item_id).is_some(),
            expect_item
        );
        match binding.empty_root_id() {
            Some(empty_id) => {
                assert_eq!(
                    find_ui_node_by_id(materialized, empty_id).is_some(),
                    expect_empty
                )
            }
            None => assert!(!expect_empty, "expected empty template is absent"),
        }
    }

    fn materialized_plugin_root(app: &TuiApp) -> UiNode {
        materialize_plugin_surface(
            &app.plugin_surface
                .as_ref()
                .expect("active plugin surface")
                .body,
            &app.session_entities,
        )
        .expect("owner-authored Workspaces bindings materialize through TuiApp")
    }

    fn assert_realized_roots_follow_reference_order(
        materialized: &UiNode,
        roots: impl IntoIterator<Item = String>,
    ) {
        fn collect_realized_node_order(node: &UiNode, order: &mut Vec<String>) {
            if let Some(id) = node.id.as_ref().and_then(UiAuthoredNodeId::as_literal) {
                order.push(id.0.clone());
            }
            for child in node
                .children
                .iter()
                .chain(node.slots.values().flatten())
                .filter_map(static_child_node)
            {
                collect_realized_node_order(child, order);
            }
        }

        let roots = roots.into_iter().collect::<Vec<_>>();
        let mut realized_order = Vec::new();
        collect_realized_node_order(materialized, &mut realized_order);
        let positions = roots
            .iter()
            .map(|root| realized_order.iter().position(|id| id == root))
            .collect::<Option<Vec<_>>>();
        assert!(
            positions.is_some_and(|positions| positions.windows(2).all(|pair| pair[0] < pair[1])),
            "realized roots are missing or not in stable traversal order: roots={roots:?} realized={realized_order:?}"
        );
    }

    #[test]
    fn smoke_message_names_the_workspace() {
        assert_eq!(smoke_message(), "botster-tui smoke ok");
    }

    #[test]
    fn workspaces_profile_is_explicit_and_ledgers_fail_closed() {
        assert_eq!(
            WorkspacesProfile::parse("plumbing"),
            Ok(WorkspacesProfile::Plumbing)
        );
        assert_eq!(
            WorkspacesProfile::parse("lifecycle"),
            Ok(WorkspacesProfile::Lifecycle)
        );
        assert!(WorkspacesProfile::parse("").is_err());
        assert!(WorkspacesProfile::parse("auto").is_err());

        for profile in [WorkspacesProfile::Plumbing, WorkspacesProfile::Lifecycle] {
            let required = match profile {
                WorkspacesProfile::Plumbing => WorkspacesStage::plumbing().to_vec(),
                WorkspacesProfile::Lifecycle => WorkspacesStage::lifecycle(),
            };
            let omitted = required[required.len() / 2];
            let mut ledger = WorkspacesLedger::new(profile);
            for stage in required {
                if stage != omitted {
                    ledger.record(stage);
                }
            }
            let error = ledger
                .assert_complete()
                .expect_err("each profile must reject an incomplete ledger");
            assert!(error.contains(&format!("{omitted:?}")), "{error}");
        }
    }

    #[test]
    fn workspaces_lifecycle_ledger_is_a_strict_plumbing_superset() {
        let plumbing = WorkspacesStage::plumbing()
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let lifecycle = WorkspacesStage::lifecycle()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert!(plumbing.is_subset(&lifecycle));
        assert!(lifecycle.len() > plumbing.len());
    }

    #[test]
    fn workspaces_reference_order_uses_realized_traversal_not_prop_text() {
        let mut group = node(UiNodeKind::Stack, "group", json!({}));
        let mut first_wrapper = node(UiNodeKind::Stack, "first-wrapper", json!({}));
        first_wrapper
            .children
            .push(child(node(UiNodeKind::Text, "first-root", json!({}))));
        let mut second_wrapper = node(UiNodeKind::Stack, "second-wrapper", json!({}));
        second_wrapper
            .children
            .push(child(node(UiNodeKind::Text, "second-root", json!({}))));
        group.children = vec![child(first_wrapper), child(second_wrapper)];
        let mut materialized = node(
            UiNodeKind::Stack,
            "surface",
            json!({ "producer_metadata": "second-root" }),
        );
        materialized.children.push(child(group));

        assert_realized_roots_follow_reference_order(
            &materialized,
            ["first-root".to_string(), "second-root".to_string()],
        );
    }

    #[test]
    fn materialized_action_oracle_dispatches_the_unique_absent_branch() {
        let session_id = "session-historical";
        let action_id = "botster_workspaces.remove_session";
        let body = ui_node(json!({
            "type": "panel",
            "id": "materialized-action-panel",
            "props": { "title": "Materialized action" },
            "children": [
                {
                    "$kind": "bind_list",
                    "source": "/session",
                    "where": {
                        "session_uuid": session_id,
                        "lifecycle_class": "current"
                    },
                    "item_template": {
                        "type": "button",
                        "id": "authored-current-remove",
                        "props": {
                            "label": "Remove current",
                            "action": {
                                "id": action_id,
                                "payload": { "session_id": session_id }
                            }
                        }
                    }
                },
                {
                    "$kind": "bind_list",
                    "source": "/session",
                    "where": { "session_uuid": session_id },
                    "item_template": {
                        "type": "text",
                        "id": "presence-detector",
                        "props": { "text": "" }
                    },
                    "empty_template": {
                        "type": "button",
                        "id": "realized-absent-remove",
                        "props": {
                            "label": "Remove historical reference",
                            "action": {
                                "id": action_id,
                                "payload": { "session_id": session_id }
                            }
                        }
                    }
                }
            ]
        }));
        assert_eq!(
            find_action_node(&body, action_id, "session_id", session_id)
                .and_then(|node| node.id.as_ref())
                .and_then(UiAuthoredNodeId::as_literal)
                .map(|id| id.0.as_str()),
            Some("authored-current-remove"),
            "authored first-match traversal deliberately disagrees with the realized branch"
        );

        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.materialized-action",
            body,
        )));
        let materialized = materialized_plugin_root(&app);
        assert!(find_ui_node_by_id(&materialized, "realized-absent-remove").is_some());
        let mut router = InputRouter::new(renderer::action_request_context_for(
            "contract.materialized-action",
        ));
        let (_lines, hit_map) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            120,
            40,
            &router.render_state(),
            &app.plugin_presentation,
        );
        let (action_node_id, action) =
            unique_hit_action(&hit_map, action_id, "session_id", session_id)
                .expect("absent materialization has one exact production action region");
        assert_eq!(action_node_id.0, "realized-absent-remove");
        router.reconcile(&hit_map);
        let region = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == action_node_id.0)
            .expect("realized absent action is in the production hit map");
        assert!(matches!(
            router.dispatch_event(
                mouse_event(
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                    region.rect.x,
                    region.rect.y,
                ),
                &hit_map,
            ),
            InputDispatch::Focus { .. }
        ));
        let dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &hit_map,
        );
        let InputDispatch::Action(request) = &dispatch else {
            panic!("focused realized action must dispatch, got {dispatch:?}");
        };
        assert_eq!(request.node_id, Some(action_node_id));
        assert_eq!(request.action_id, action.id);
        assert_eq!(request.payload, action.payload);
        let request = request.clone();
        app.handle_dispatch(dispatch);
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::PluginSurfaceAction {
                    package_name: "botster.plugin-contract-matrix".to_string(),
                    request,
                })
        );

        let missing = unique_hit_action(&hit_map, action_id, "session_id", "missing-session")
            .expect_err("zero production matches must fail");
        assert!(missing.contains("found 0"), "{missing}");

        let duplicate = ui_node(json!({
            "type": "stack",
            "id": "duplicate-actions",
            "props": {},
            "children": [
                {
                    "type": "button",
                    "id": "duplicate-remove-one",
                    "props": {
                        "label": "Remove one",
                        "action": {
                            "id": action_id,
                            "payload": { "session_id": session_id }
                        }
                    }
                },
                {
                    "type": "button",
                    "id": "duplicate-remove-two",
                    "props": {
                        "label": "Remove two",
                        "action": {
                            "id": action_id,
                            "payload": { "session_id": session_id }
                        }
                    }
                }
            ]
        }));
        let (_, duplicate_hits) = renderer::render_to_lines(&duplicate, 120, 40);
        let duplicate_error =
            unique_hit_action(&duplicate_hits, action_id, "session_id", session_id)
                .expect_err("duplicate production matches must fail");
        assert!(duplicate_error.contains("found 2"), "{duplicate_error}");
        assert!(
            duplicate_error.contains("duplicate-remove-one"),
            "{duplicate_error}"
        );
        assert!(
            duplicate_error.contains("duplicate-remove-two"),
            "{duplicate_error}"
        );
    }

    fn workspace_fixture() -> TuiApp {
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.status = "connected".to_string();
        app.connection_error = None;
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "exited")]);
        app.selected_session = Some("session-alpha".to_string());
        app
    }

    #[test]
    fn workspace_uses_semantic_widths_for_wide_regular_and_compact_layouts() {
        let app = workspace_fixture();

        for (width, height, horizontal) in [(140, 42, true), (96, 30, true), (72, 24, false)] {
            let (lines, hit_map) = renderer::render_to_lines_with_state(
                &app.surface(),
                width,
                height,
                &RenderState::default(),
            );
            let rendered = lines.join("\n");
            let navigator = hit_map
                .regions()
                .iter()
                .find(|region| region.node_id == "workspace-session-navigator")
                .expect("session navigator should render");
            let focused = hit_map
                .regions()
                .iter()
                .find(|region| region.node_id == "workspace-focused-session")
                .expect("focused session should render");

            assert!(rendered.contains("Botster | hub: connected"));
            assert!(rendered.contains("session-alpha"));
            assert!(rendered.contains("terminal stream unavailable"));
            if horizontal {
                assert_eq!(navigator.rect.y, focused.rect.y);
                assert_ne!(navigator.rect.x, focused.rect.x);
            } else {
                assert_eq!(navigator.rect.x, focused.rect.x);
                assert!(focused.rect.y > navigator.rect.y);
            }
        }
    }

    #[test]
    fn workspace_presents_empty_pending_running_attached_exited_and_unavailable_states() {
        let mut app = workspace_fixture();

        app.sessions.clear();
        app.selected_session = None;
        let empty = renderer::render_to_lines(&app.surface(), 96, 30)
            .0
            .join("\n");
        assert!(empty.contains("No sessions yet"));

        app.sessions = vec![SessionRow::pending("session-pending")];
        app.selected_session = Some("session-pending".to_string());
        let pending = renderer::render_to_lines(&app.surface(), 96, 30)
            .0
            .join("\n");
        assert!(pending.contains("pending spawn"));
        assert!(pending.contains("disabled: Attach"));

        app.sessions = session_rows([("session-active", "running"), ("session-old", "exited")]);
        app.selected_session = Some("session-active".to_string());
        let running = renderer::render_to_lines(&app.surface(), 96, 30)
            .0
            .join("\n");
        assert!(running.contains("session-active · running · selected"));
        assert!(running.contains("session-old · exited"));

        app.attached_session = Some("session-active".to_string());
        app.attached_subscription_id = Some("sub-current".to_string());
        app.subscription_id = "sub-current".to_string();
        let attached = renderer::render_to_lines(&app.surface(), 96, 30)
            .0
            .join("\n");
        assert!(attached.contains("session-active · attached"));
        assert!(attached.contains("terminal attached: session-active"));

        app.sessions.clear();
        app.selected_session = None;
        app.attached_session = None;
        app.status = "hub unavailable; reconnecting".to_string();
        app.connection_error = Some("daemon is not running".to_string());
        let unavailable = renderer::render_to_lines(&app.surface(), 96, 30)
            .0
            .join("\n");
        assert!(unavailable.contains("Hub unavailable"));
        assert!(unavailable.contains("daemon is not running"));
    }

    #[test]
    fn constrained_toolbar_keeps_primary_actions_and_removes_hidden_hits() {
        let app = workspace_fixture();
        let (_wide, wide_hits) = renderer::render_to_lines(&app.surface(), 140, 42);
        assert!(
            wide_hits
                .regions()
                .iter()
                .any(|region| region.node_id == "workspace-attach")
        );

        let (narrow, narrow_hits) = renderer::render_to_lines(&app.surface(), 72, 24);
        assert!(narrow.iter().any(|line| line.contains("…+")));
        assert!(
            narrow_hits
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-spawn")
        );
        assert!(
            narrow_hits
                .regions()
                .iter()
                .any(|region| region.node_id == "workspace-attach")
        );
        assert!(
            !narrow_hits
                .regions()
                .iter()
                .any(|region| region.node_id == "workspace-shutdown")
        );
        assert!(
            !narrow_hits
                .regions()
                .iter()
                .any(|region| region.node_id == "workspace-remove")
        );
    }

    #[test]
    fn disabled_attach_is_visible_but_cannot_dispatch() {
        let mut app = workspace_fixture();
        app.sessions = vec![SessionRow::pending("session-pending")];
        app.selected_session = Some("session-pending".to_string());
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 96, 30);
        let attach = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "workspace-attach")
            .expect("disabled attach should remain visible");
        assert!(lines.iter().any(|line| line.contains("disabled: Attach")));
        assert!(attach.action.is_none());

        let mut router = InputRouter::new(renderer::action_request_context());
        let down = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                attach.rect.x,
                attach.rect.y,
            ),
            &hit_map,
        );
        let up = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                attach.rect.x,
                attach.rect.y,
            ),
            &hit_map,
        );
        assert!(!matches!(down, InputDispatch::Action(_)));
        assert!(!matches!(up, InputDispatch::Action(_)));
    }

    #[test]
    fn session_navigator_scrolls_without_hidden_row_hit_regions() {
        let mut app = workspace_fixture();
        app.sessions = (0..20)
            .map(|index| SessionRow::running(format!("session-{index:02}")))
            .collect();
        app.selected_session = Some("session-00".to_string());

        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 72, 16);
        let bounds = hit_map
            .scroll_bounds("tui-session-list")
            .expect("session navigator should expose scroll bounds");
        let visible_rows = hit_map
            .regions()
            .iter()
            .filter(|region| region.node_id.starts_with("tui-session-session-"))
            .count();

        assert!(bounds.max_offset > 0);
        assert!(visible_rows < app.sessions.len());
    }

    #[test]
    fn destructive_confirmation_isolates_workspace_and_dispatches_only_after_confirm() {
        let mut app = workspace_fixture();
        app.observed_requests.clear();

        app.handle_action(
            "botster.tui.session.shutdown".to_string(),
            None,
            Some(json!({ "session_id": "session-alpha" })),
        );
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 96, 30);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Shut down session session-alpha?"));
        assert!(
            hit_map
                .regions()
                .iter()
                .all(|region| region.node_id != "tui-session-session-alpha")
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "workspace-confirm-accept")
        );

        app.handle_action("botster.tui.confirm.cancel".to_string(), None, None);
        assert!(app.observed_requests.is_empty());

        app.handle_action(
            "botster.tui.session.shutdown".to_string(),
            None,
            Some(json!({ "session_id": "session-alpha" })),
        );
        let (_lines, confirm_hits) = renderer::render_to_lines(&app.surface(), 96, 30);
        let confirm = confirm_hits
            .regions()
            .iter()
            .find(|region| region.node_id == "workspace-confirm-accept")
            .expect("confirm button should be clickable");
        let mut router = InputRouter::new(renderer::action_request_context());
        let down = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                confirm.rect.x,
                confirm.rect.y,
            ),
            &confirm_hits,
        );
        app.handle_dispatch(down);
        let up = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                confirm.rect.x,
                confirm.rect.y,
            ),
            &confirm_hits,
        );
        app.handle_dispatch(up);
        assert_eq!(
            app.observed_requests,
            vec![ObservedRequest::ShutdownSession(
                "session-alpha".to_string()
            )]
        );
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
    fn parses_typed_hub_connection_and_headless_mode() {
        let args = AppArgs::parse_with_environment(
            ["--headless-live-runtime".to_string()],
            Some(
                botster_core_test_support::fixtures::runnable_entrypoint_hub_connection::VALID_UNIX_SOCKET_JSON
                    .into(),
            ),
            Some("target/hub-data".into()),
            false,
        );

        assert_eq!(
            args.daemon_endpoint().map(|endpoint| endpoint.socket_path),
            Some(PathBuf::from("/var/run/botster/hub.sock"))
        );
        assert_eq!(args.connection_error, None);
        assert_eq!(args.hub_data_dir, Some(PathBuf::from("target/hub-data")));
        assert!(args.headless_live_runtime);
    }

    #[test]
    fn interactive_system_details_show_package_storage_context_without_using_it_as_identity() {
        let mut app = TuiApp::new_with_runtime_context(None, None, true);
        app.workspace_test_mode = true;
        app.system_details_visible = true;

        let rendered = renderer::render_to_lines(&app.surface(), 120, 48)
            .0
            .join("\n");

        assert!(rendered.contains("package storage context: configured"));
        assert_eq!(app.endpoint, None);
    }

    #[test]
    fn canonical_invalid_hub_connection_fixtures_are_rejected() {
        for fixture in
            botster_core_test_support::fixtures::runnable_entrypoint_hub_connection::INVALID_FIXTURES
        {
            let (connection, error) = parse_hub_connection(Some(fixture.json.into()));
            assert_eq!(connection, None, "fixture {} was accepted", fixture.name);
            assert!(
                error
                    .as_deref()
                    .is_some_and(|error| error.contains("BOTSTER_HUB_CONNECTION")),
                "fixture {} did not produce an actionable diagnostic: {error:?}",
                fixture.name
            );
        }
    }

    #[test]
    fn retired_raw_socket_inputs_do_not_provide_a_connection() {
        let args = AppArgs::parse_with_environment(
            ["--hub-socket".to_string(), "/tmp/retired.sock".to_string()],
            None,
            None,
            false,
        );

        assert_eq!(args.hub_connection, None);
        assert_eq!(args.daemon_endpoint(), None);
        assert_eq!(
            args.connection_error.as_deref(),
            Some("BOTSTER_HUB_CONNECTION is required")
        );
    }

    #[test]
    fn command_draft_is_rendered_before_submit() {
        let mut app = TuiApp::new(None);
        app.set_drafts(BTreeMap::from([(
            "command".to_string(),
            Value::String("printf draft\\n".to_string()),
        )]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        assert!(lines.join("\n").contains("printf draft"));
    }

    #[test]
    fn blank_command_validation_renders_visible_error_state() {
        let mut app = TuiApp::new(None);
        app.command = " \t\n".to_string();

        app.spawn_session();

        assert_eq!(app.error.as_deref(), Some("command is required"));
        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        assert!(lines.join("\n").contains("error: command is required"));
    }

    #[test]
    fn missing_hub_connection_renders_connection_diagnostic() {
        let app = TuiApp::new_with_connection(
            None,
            Some("BOTSTER_HUB_CONNECTION is required".to_string()),
        );

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("Hub connection not configured"));
        assert!(rendered.contains("BOTSTER_HUB_CONNECTION is required"));
        assert!(rendered.contains(PROTOCOL));
    }

    #[test]
    fn tui_requires_protocol_4_revision_25_and_session_entity_subscriptions() {
        let requirement = tui_compatibility_requirement();

        assert_eq!(
            requirement.minimum_conformance_fixture_revision,
            MINIMUM_CONFORMANCE_FIXTURE_REVISION
        );
        assert_eq!(botster_hub_client::PROTOCOL_VERSION, 4);
        assert_eq!(MINIMUM_CONFORMANCE_FIXTURE_REVISION, 25);
        assert!(
            requirement
                .required_features
                .iter()
                .any(|feature| feature == FEATURE_TERMINAL_READBACK)
        );
        assert!(
            requirement
                .required_features
                .iter()
                .any(|feature| feature == FEATURE_SESSION_ENTITY_SUBSCRIPTIONS)
        );

        for revision in 16..25 {
            let mut older_hub = DaemonCompatibility::current();
            older_hub.conformance_fixture_revision = revision;
            let error = botster_hub_client::ensure_compatible(&requirement, &older_hub)
                .expect_err("pre-presentation fixture revision must be rejected");
            assert!(error.diagnostic.contains(&format!("revision {revision}")));
            assert!(error.diagnostic.contains("requires at least 25"));
        }
        for protocol_version in 2..4 {
            let mut older_hub = DaemonCompatibility::current();
            older_hub.protocol_version = protocol_version;
            let error = botster_hub_client::ensure_compatible(&requirement, &older_hub)
                .expect_err("older protocol must be rejected");
            assert!(
                error
                    .diagnostic
                    .contains(&format!("version {protocol_version}"))
            );
            assert!(error.diagnostic.contains("requires at least 4"));
        }
        botster_hub_client::ensure_compatible(&requirement, &DaemonCompatibility::current())
            .expect("protocol 4 fixture revision 25 hub should connect");
        let mut future_hub = DaemonCompatibility::current();
        future_hub.conformance_fixture_revision = 26;
        botster_hub_client::ensure_compatible(&requirement, &future_hub)
            .expect("runtime compatibility must preserve minimum semantics for revision 26");
    }

    #[test]
    fn session_reducer_consumes_shared_lifecycle_conformance_frames() {
        let scenario =
            botster_hub_test_support::session_lifecycle_subscription_conformance_scenario();
        assert!(scenario.conformance_fixture_revision >= MINIMUM_CONFORMANCE_FIXTURE_REVISION);
        let generation = match &scenario.normalized_frames[0] {
            DaemonEntityFrame::Snapshot {
                subscription_id, ..
            } => subscription_id.clone(),
            other => panic!("first conformance frame must be a snapshot, got {other:?}"),
        };
        let mut state = SessionEntityState::default();
        state.begin_generation(generation);

        for frame in scenario.normalized_frames {
            assert!(state.apply(frame).expect("conformance frame applies"));
        }
        assert!(state.entities.is_empty(), "remove deletes the session");
        assert_eq!(state.snapshot_seq, Some(4));
        assert!(
            state
                .apply(scenario.overflow.resync_snapshot)
                .expect("overflow resync snapshot applies")
        );
        assert!(state.entities.is_empty());

        let fresh = scenario.fresh_subscription.snapshot;
        let fresh_generation = match &fresh {
            DaemonEntityFrame::Snapshot {
                subscription_id, ..
            } => subscription_id.clone(),
            _ => unreachable!(),
        };
        assert!(
            !state
                .apply(fresh.clone())
                .expect("stale generation ignored")
        );
        state.begin_generation(fresh_generation);
        assert!(state.apply(fresh).expect("fresh snapshot applies"));
        assert!(state.has_snapshot);
    }

    #[test]
    fn session_entity_readiness_requires_the_exact_expected_row() {
        let session_id = "expected-session";
        let mut state = SessionEntityState::default();
        state.begin_generation("readiness-generation".to_string());
        assert!(
            state
                .apply(snapshot_frame("readiness-generation", 1, Vec::new()))
                .expect("empty authoritative snapshot applies")
        );

        assert!(state.has_snapshot);
        assert!(!session_entity_expectation_satisfied(
            &state,
            session_id,
            SessionEntityExpectation::Lifecycle("current")
        ));
        assert!(session_entity_expectation_satisfied(
            &state,
            session_id,
            SessionEntityExpectation::Absent
        ));

        assert!(
            state
                .apply(snapshot_frame(
                    "readiness-generation",
                    2,
                    vec![session_entity(session_id, Some("running"))],
                ))
                .expect("authoritative snapshot containing the expected row applies")
        );
        assert!(session_entity_expectation_satisfied(
            &state,
            session_id,
            SessionEntityExpectation::Lifecycle("current")
        ));
        assert!(!session_entity_expectation_satisfied(
            &state,
            session_id,
            SessionEntityExpectation::Absent
        ));
    }

    fn canonical_surface(
        package_name: &str,
        surface_id: &str,
        body: UiNode,
    ) -> DaemonPluginSurface {
        canonical_plugin_surface_fixture(DaemonPluginSurface {
            package_name: package_name.to_string(),
            surface_id: surface_id.to_string(),
            body,
            ui_tree_snapshot: None,
        })
    }

    fn session_binding_values(root: &UiNode, references: &[String]) -> BTreeMap<String, String> {
        references
            .iter()
            .enumerate()
            .map(|(index, session_uuid)| {
                let lifecycle_id = format!("contract-session-{}-lifecycle", index + 1);
                let unavailable_id = format!("contract-session-{}-unavailable", index + 1);
                let value = if let Some(node) = find_ui_node_by_id(root, &lifecycle_id) {
                    node.props
                        .get("text")
                        .and_then(Value::as_str)
                        .expect("materialized lifecycle text")
                        .to_string()
                } else {
                    assert!(find_ui_node_by_id(root, &unavailable_id).is_some());
                    "unavailable".to_string()
                };
                (session_uuid.clone(), value)
            })
            .collect()
    }

    fn assert_session_binding_frame(
        app: &TuiApp,
        expected: &BTreeMap<String, String>,
        references: &[String],
        expected_rows: &[botster_hub_test_support::SessionPluginMaterializedRow],
    ) {
        let root = materialize_plugin_surface(
            &app.plugin_surface.as_ref().expect("active surface").body,
            &app.session_entities,
        )
        .expect("canonical session bindings materialize");
        assert_eq!(session_binding_values(&root, references), *expected);
        let mut actual_rows = Vec::new();
        collect_session_action_rows(&root, &mut actual_rows);
        assert_eq!(actual_rows, expected_rows);

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 180, 60);
        let rendered = lines.join("\n");
        for fallback in ["bind /", "bind @/", "bound list: waiting for entities"] {
            assert!(!rendered.contains(fallback), "{rendered}");
        }
        for (index, session_uuid) in references.iter().enumerate() {
            let value = expected
                .get(session_uuid)
                .expect("published expected value");
            let (suffix, text) = if value == "unavailable" {
                ("unavailable", "Session unavailable")
            } else {
                ("lifecycle", value.as_str())
            };
            assert!(rendered.contains(text), "{rendered}");
            let node_id = format!("contract-session-{}-{suffix}", index + 1);
            assert!(
                hit_map
                    .regions()
                    .iter()
                    .any(|region| region.node_id == node_id),
                "{node_id} should be present in the production frame hit map"
            );
        }
    }

    fn collect_session_action_rows(
        node: &UiNode,
        rows: &mut Vec<botster_hub_test_support::SessionPluginMaterializedRow>,
    ) {
        if let Some(id) = node.id.as_ref().and_then(UiAuthoredNodeId::as_literal)
            && node
                .props
                .get("action")
                .and_then(|action| action.get("id"))
                .and_then(Value::as_str)
                == Some("contract.action")
        {
            rows.push(botster_hub_test_support::SessionPluginMaterializedRow {
                node_id: id.0.clone(),
                action_payload: node_action(node)
                    .payload
                    .expect("canonical row action has a payload"),
            });
        }
        for child in node
            .children
            .iter()
            .chain(node.slots.values().flatten())
            .filter_map(static_child_node)
        {
            collect_session_action_rows(child, rows);
        }
    }

    fn assert_multi_row_keyboard_and_mouse_dispatch(
        app: &mut TuiApp,
        expected_rows: &[botster_hub_test_support::SessionPluginMaterializedRow],
    ) {
        assert!(
            expected_rows.len() >= 2,
            "published oracle must remain multi-row"
        );
        let target = &expected_rows[1];
        let mut router =
            InputRouter::new(renderer::action_request_context_for("contract.sessions"));
        let (_lines, hit_map) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            180,
            60,
            &router.render_state(),
            &app.plugin_presentation,
        );
        router.reconcile(&hit_map);
        let region_rects = expected_rows
            .iter()
            .map(|row| {
                hit_map
                    .regions()
                    .iter()
                    .find(|region| region.node_id == row.node_id)
                    .map(|region| {
                        (
                            region.rect.x,
                            region.rect.y,
                            region.rect.width,
                            region.rect.height,
                        )
                    })
                    .expect("each producer row has a distinct production hit region")
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(region_rects.len(), expected_rows.len());

        let mut focused = std::collections::BTreeSet::new();
        for _ in 0..=(2 * hit_map.regions().len() + 1) {
            if let InputDispatch::Focus { node_id } = router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &hit_map,
            ) {
                focused.insert(node_id);
            }
            if expected_rows
                .iter()
                .all(|row| focused.contains(&row.node_id))
            {
                break;
            }
        }
        for row in expected_rows {
            assert!(
                focused.contains(&row.node_id),
                "Tab traversal did not reach {}",
                row.node_id
            );
        }
        for _ in 0..=hit_map.regions().len() {
            if router.focused_node_id() == Some(target.node_id.as_str()) {
                break;
            }
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &hit_map,
            );
        }
        assert_eq!(router.focused_node_id(), Some(target.node_id.as_str()));

        let mut keyboard_request = None;
        for code in [KeyCode::Enter, KeyCode::Char(' ')] {
            let dispatch = router.dispatch_event(
                Event::Key(KeyEvent::new(code, KeyModifiers::NONE)),
                &hit_map,
            );
            let InputDispatch::Action(request) = &dispatch else {
                panic!("focused row must dispatch for {code:?}, got {dispatch:?}");
            };
            assert_eq!(request.node_id, Some(UiNodeId(target.node_id.clone())));
            assert_eq!(request.payload.as_ref(), Some(&target.action_payload));
            keyboard_request.get_or_insert_with(|| request.clone());
        }

        let mut mouse_router =
            InputRouter::new(renderer::action_request_context_for("contract.sessions"));
        let (_lines, mouse_hits) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            180,
            60,
            &mouse_router.render_state(),
            &app.plugin_presentation,
        );
        let region = mouse_hits
            .regions()
            .iter()
            .find(|region| region.node_id == target.node_id)
            .expect("target row remains in the production hit map");
        let down = mouse_router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                region.rect.x,
                region.rect.y,
            ),
            &mouse_hits,
        );
        assert!(matches!(down, InputDispatch::Focus { .. }));
        let up = mouse_router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                region.rect.x,
                region.rect.y,
            ),
            &mouse_hits,
        );
        let InputDispatch::Action(request) = &up else {
            panic!("target row mouse release must dispatch, got {up:?}");
        };
        assert_eq!(request.node_id, Some(UiNodeId(target.node_id.clone())));
        assert_eq!(request.payload.as_ref(), Some(&target.action_payload));
        app.observed_requests.clear();
        app.handle_dispatch(InputDispatch::Action(
            keyboard_request.expect("Enter produced a typed request"),
        ));
        assert!(app.observed_requests.iter().any(|observed| matches!(
            observed,
            ObservedRequest::PluginSurfaceAction { request, .. }
                if request.node_id == Some(UiNodeId(target.node_id.clone()))
                    && request.payload.as_ref() == Some(&target.action_payload)
        )));
    }

    #[test]
    fn canonical_session_bindings_follow_published_oracle_through_frames_and_reconnect() {
        let scenario = botster_hub_test_support::session_plugin_binding_conformance_scenario();
        assert_eq!(scenario.conformance_fixture_revision, 25);
        let body =
            serde_json::from_value(scenario.surface.clone()).expect("published surface is typed");
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.sessions",
            body,
        )));

        let generation_one = match &scenario.initial_snapshot {
            DaemonEntityFrame::Snapshot {
                subscription_id, ..
            } => subscription_id.clone(),
            _ => panic!("published initial stage must be a snapshot"),
        };
        app.session_entities
            .begin_generation(generation_one.clone());
        assert!(
            app.session_entities
                .apply(scenario.initial_snapshot.clone())
                .expect("initial snapshot applies")
        );
        let initial_frames = vec![scenario.initial_snapshot.clone()];
        let initial_rows = botster_hub_test_support::materialize_session_plugin_rows(
            &scenario.surface,
            &initial_frames,
        )
        .expect("producer initial rows materialize");
        assert_eq!(
            initial_rows
                .iter()
                .map(|row| row.node_id.clone())
                .collect::<Vec<_>>(),
            scenario.row_expected.initial
        );
        assert_session_binding_frame(
            &app,
            &scenario.expected.initial,
            &scenario.references,
            &initial_rows,
        );
        assert_multi_row_keyboard_and_mouse_dispatch(&mut app, &initial_rows);
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.sessions",
            serde_json::from_value(scenario.surface.clone())
                .expect("published surface remains typed"),
        )));
        app.session_entities
            .begin_generation(generation_one.clone());
        assert!(
            app.session_entities
                .apply(scenario.initial_snapshot.clone())
                .expect("initial snapshot reapplies after action seam proof")
        );

        let missing_uuid = scenario
            .references
            .last()
            .expect("missing reference")
            .clone();
        assert!(
            app.session_entities
                .apply(DaemonEntityFrame::Upsert {
                    subscription_id: generation_one.clone(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 2,
                    id: missing_uuid.clone(),
                    entity: DaemonSessionEntity {
                        session_uuid: missing_uuid.clone(),
                        registry_state: "running".to_string(),
                        lifecycle: Some("running".to_string()),
                        lifecycle_class: "current".to_string(),
                        rows: 24,
                        cols: 80,
                        updated_at: 2,
                        exit_code: None,
                        failure_reason: None,
                    },
                })
                .expect("authoritative upsert applies")
        );
        let root = materialize_plugin_surface(
            &app.plugin_surface.as_ref().expect("active surface").body,
            &app.session_entities,
        )
        .expect("upserted bindings materialize");
        assert_eq!(
            session_binding_values(&root, &scenario.references)
                .get(&missing_uuid)
                .map(String::as_str),
            Some("current")
        );

        app.session_entities.begin_generation(generation_one);
        app.session_entities
            .apply(scenario.initial_snapshot.clone())
            .expect("initial snapshot reapplies");
        let removed_row = initial_rows.first().expect("initial row removed by patch");
        let mut removal_router =
            InputRouter::new(renderer::action_request_context_for("contract.sessions"));
        let (_lines, initial_removal_hits) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            180,
            60,
            &removal_router.render_state(),
            &app.plugin_presentation,
        );
        removal_router.reconcile(&initial_removal_hits);
        for _ in 0..=initial_removal_hits.regions().len() {
            if removal_router.focused_node_id() == Some(removed_row.node_id.as_str()) {
                break;
            }
            removal_router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &initial_removal_hits,
            );
        }
        assert_eq!(
            removal_router.focused_node_id(),
            Some(removed_row.node_id.as_str())
        );
        let transition_expectations = [
            (
                &scenario.expected.after_ended_patch,
                &scenario.row_expected.after_ended_patch,
            ),
            (
                &scenario.expected.after_indeterminate_patch,
                &scenario.row_expected.after_indeterminate_patch,
            ),
            (
                &scenario.expected.after_remove,
                &scenario.row_expected.after_remove,
            ),
        ];
        let mut stage_frames = vec![scenario.initial_snapshot.clone()];
        for (stage, (frame, (expected, row_expected))) in scenario
            .transition_frames
            .iter()
            .zip(transition_expectations)
            .enumerate()
        {
            assert!(
                app.session_entities
                    .apply(frame.clone())
                    .expect("transition frame applies")
            );
            stage_frames.push(frame.clone());
            let expected_rows = botster_hub_test_support::materialize_session_plugin_rows(
                &scenario.surface,
                &stage_frames,
            )
            .expect("producer transition rows materialize");
            assert_eq!(
                expected_rows
                    .iter()
                    .map(|row| row.node_id.clone())
                    .collect::<Vec<_>>(),
                *row_expected
            );
            assert_session_binding_frame(&app, expected, &scenario.references, &expected_rows);
            if stage == 0 {
                let surviving_row = expected_rows
                    .first()
                    .expect("ended patch preserves one canonical row");
                let (_lines, removed_hits) = renderer::render_to_lines_with_presentation_state(
                    &app.surface(),
                    180,
                    60,
                    &removal_router.render_state(),
                    &app.plugin_presentation,
                );
                assert!(
                    !removed_hits
                        .regions()
                        .iter()
                        .any(|region| region.node_id == removed_row.node_id)
                );
                removal_router.reconcile(&removed_hits);
                assert_ne!(
                    removal_router.focused_node_id(),
                    Some(removed_row.node_id.as_str())
                );
                for _ in 0..=removed_hits.regions().len() {
                    if removal_router.focused_node_id() == Some(surviving_row.node_id.as_str()) {
                        break;
                    }
                    removal_router.dispatch_event(
                        Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                        &removed_hits,
                    );
                }
                let dispatch = removal_router.dispatch_event(
                    Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                    &removed_hits,
                );
                let InputDispatch::Action(request) = dispatch else {
                    panic!("surviving row must dispatch after reconcile")
                };
                assert_eq!(
                    request.node_id,
                    Some(UiNodeId(surviving_row.node_id.clone()))
                );
                assert_eq!(request.payload, Some(surviving_row.action_payload.clone()));
            }
        }

        let generation_two = match &scenario.reconnect_snapshot {
            DaemonEntityFrame::Snapshot {
                subscription_id, ..
            } => subscription_id.clone(),
            _ => panic!("published reconnect stage must be a snapshot"),
        };
        app.session_entities.begin_generation(generation_two);
        assert!(
            !app.session_entities
                .apply(scenario.transition_frames[0].clone())
                .expect("stale prior-generation delta is ignored")
        );
        assert!(
            app.session_entities
                .apply(scenario.reconnect_snapshot.clone())
                .expect("fresh reconnect snapshot applies")
        );
        let reconnect_frames = vec![scenario.reconnect_snapshot.clone()];
        let reconnect_rows = botster_hub_test_support::materialize_session_plugin_rows(
            &scenario.surface,
            &reconnect_frames,
        )
        .expect("producer reconnect rows materialize");
        assert_eq!(
            reconnect_rows
                .iter()
                .map(|row| row.node_id.clone())
                .collect::<Vec<_>>(),
            scenario.row_expected.after_reconnect
        );
        assert_session_binding_frame(
            &app,
            &scenario.expected.after_reconnect,
            &scenario.references,
            &reconnect_rows,
        );
    }

    #[test]
    fn bound_action_materializes_payload_and_dispatches_from_real_hit_region() {
        let body = ui_node(json!({
            "type": "panel",
            "id": "bound-action-panel",
            "props": { "title": "Bound action" },
            "children": [{
                "$kind": "bind_list",
                "source": "/session",
                "where": { "session_uuid": "session-action" },
                "item_template": {
                    "type": "panel",
                    "id": "bound-session-panel",
                    "props": { "title": "Bound session" },
                    "slots": {
                        "body": [{
                            "$kind": "bind_if",
                            "path": "@/lifecycle_class",
                            "node": {
                                "type": "button",
                                "id": "bound-session-action",
                                "props": {
                                    "label": { "$bind": "@/lifecycle_class" },
                                    "action": {
                                        "id": "bound.open",
                                        "payload": {
                                            "session_uuid": { "$bind": "@/session_uuid" },
                                            "lifecycle_class": { "$bind": "@/lifecycle_class" }
                                        }
                                    }
                                }
                            }
                        }]
                    }
                },
                "empty_template": {
                    "type": "text",
                    "id": "bound-session-unavailable",
                    "props": { "text": "Session unavailable" }
                }
            }]
        }));
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.bound-action",
            body,
        )));
        app.session_entities
            .begin_generation("bound-action-generation".to_string());
        app.session_entities
            .apply(DaemonEntityFrame::Snapshot {
                subscription_id: "bound-action-generation".to_string(),
                entity_type: "session".to_string(),
                snapshot_seq: 1,
                items: vec![DaemonSessionEntity {
                    session_uuid: "session-action".to_string(),
                    registry_state: "running".to_string(),
                    lifecycle: Some("running".to_string()),
                    lifecycle_class: "current".to_string(),
                    rows: 24,
                    cols: 80,
                    updated_at: 1,
                    exit_code: None,
                    failure_reason: None,
                }],
                resync_reason: None,
            })
            .expect("snapshot applies");

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 40);
        let rendered = lines.join("\n");
        assert!(rendered.contains("current"), "{rendered}");
        assert!(!rendered.contains("Session unavailable"), "{rendered}");
        let dispatch = click_dispatch_for_surface(
            &hit_map,
            "bound-session-action",
            Some("contract.bound-action"),
        );
        let InputDispatch::Action(request) = dispatch else {
            panic!("materialized button should dispatch an action");
        };
        assert_eq!(
            request.payload,
            Some(json!({
                "session_uuid": "session-action",
                "lifecycle_class": "current"
            }))
        );
        let surface_fixture = app.plugin_surface.clone().expect("active bound surface");
        app.handle_dispatch(InputDispatch::Action(request.clone()));
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::PluginSurfaceAction {
                    package_name: "botster.plugin-contract-matrix".to_string(),
                    request,
                })
        );
        app.apply_response(plugin_surface_response(surface_fixture));
        app.session_entities
            .begin_generation("bound-action-generation".to_string());
        app.session_entities
            .apply(DaemonEntityFrame::Snapshot {
                subscription_id: "bound-action-generation".to_string(),
                entity_type: "session".to_string(),
                snapshot_seq: 1,
                items: vec![DaemonSessionEntity {
                    session_uuid: "session-action".to_string(),
                    registry_state: "running".to_string(),
                    lifecycle: Some("running".to_string()),
                    lifecycle_class: "current".to_string(),
                    rows: 24,
                    cols: 80,
                    updated_at: 1,
                    exit_code: None,
                    failure_reason: None,
                }],
                resync_reason: None,
            })
            .expect("focus-removal baseline applies");

        let action_region = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "bound-session-action")
            .expect("bound action region");
        let mut router = InputRouter::new(renderer::action_request_context_for(
            "contract.bound-action",
        ));
        let _ = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                action_region.rect.x,
                action_region.rect.y,
            ),
            &hit_map,
        );
        assert_eq!(router.focused_node_id(), Some("bound-session-action"));
        assert!(
            app.session_entities
                .apply(DaemonEntityFrame::Remove {
                    subscription_id: "bound-action-generation".to_string(),
                    entity_type: "session".to_string(),
                    snapshot_seq: 2,
                    id: "session-action".to_string(),
                })
                .expect("remove applies")
        );
        let (removed_lines, removed_hit_map) = renderer::render_to_lines(&app.surface(), 120, 40);
        let removed = removed_lines.join("\n");
        assert!(removed.contains("Session unavailable"), "{removed}");
        assert!(
            !removed_hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "bound-session-action")
        );
        router.reconcile(&removed_hit_map);
        assert_ne!(router.focused_node_id(), Some("bound-session-action"));
    }

    #[test]
    fn optional_session_fields_are_null_and_bind_if_preserves_the_surface() {
        let body = ui_node(json!({
            "type": "panel",
            "id": "optional-field-panel",
            "props": { "title": "Optional field" },
            "children": [
                {
                    "type": "text",
                    "id": "optional-field-surrounding-content",
                    "props": { "text": "Surrounding content remains" }
                },
                {
                    "$kind": "bind_list",
                    "source": "/session",
                    "where": { "session_uuid": "session-indeterminate" },
                    "item_template": {
                        "type": "panel",
                        "id": "optional-field-row",
                        "props": { "title": "Indeterminate session" },
                        "children": [{
                            "$kind": "bind_if",
                            "path": "@/lifecycle",
                            "node": {
                                "type": "text",
                                "id": "optional-field-lifecycle",
                                "props": { "text": { "$bind": "@/lifecycle" } }
                            }
                        }]
                    }
                }
            ]
        }));
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.optional-field",
            body,
        )));
        app.session_entities
            .begin_generation("optional-field-generation".to_string());
        app.session_entities
            .apply(snapshot_frame(
                "optional-field-generation",
                1,
                vec![session_entity("session-indeterminate", None)],
            ))
            .expect("snapshot applies");

        let rows = app
            .session_entities
            .binding_rows()
            .expect("binding rows serialize");
        let row = rows
            .iter()
            .find(|row| row.get("session_uuid") == Some(&json!("session-indeterminate")))
            .expect("session binding row");
        for field in ["lifecycle", "exit_code", "failure_reason"] {
            assert_eq!(row.get(field), Some(&Value::Null), "{field}");
        }

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 40);
        let rendered = lines.join("\n");
        assert!(
            rendered.contains("Surrounding content remains"),
            "{rendered}"
        );
        assert!(rendered.contains("Indeterminate session"), "{rendered}");
        assert!(!rendered.contains("plugin surface binding:"), "{rendered}");
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "optional-field-lifecycle")
        );
    }

    #[test]
    fn duplicate_ids_in_responsive_alternatives_are_render_scoped() {
        let action = |id: &str| {
            node(
                UiNodeKind::Button,
                id,
                json!({
                    "label": "Responsive action",
                    "action": { "id": "contract.responsive" }
                }),
            )
        };
        let mut body = node(
            UiNodeKind::Panel,
            "responsive-panel",
            json!({ "title": "Responsive alternatives" }),
        );
        body.children = vec![
            responsive_child(UiWidthClass::Expanded, action("responsive-action")),
            responsive_child(UiWidthClass::Compact, action("responsive-action")),
        ];
        materialize_plugin_surface(&body, &SessionEntityState::default())
            .expect("mutually exclusive render alternatives may reuse one node id");

        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.responsive",
            body,
        )));
        for width in [40, 180] {
            let (lines, hit_map) = renderer::render_to_lines(&app.surface(), width, 40);
            let rendered = lines.join("\n");
            assert!(!rendered.contains("plugin surface binding:"), "{rendered}");
            assert_eq!(
                hit_map
                    .regions()
                    .iter()
                    .filter(|region| region.node_id == "responsive-action")
                    .count(),
                1
            );
        }

        let mut complementary = node(
            UiNodeKind::Panel,
            "complementary-panel",
            json!({ "title": "Complementary conditions" }),
        );
        complementary.children = vec![
            responsive_child(UiWidthClass::Expanded, action("complementary-action")),
            UiChild::Conditional(UiConditional::Hidden {
                condition: UiCondition {
                    width: Some(UiWidthClass::Expanded),
                    ..UiCondition::default()
                },
                node: Box::new(action("complementary-action")),
            }),
        ];
        materialize_plugin_surface(&complementary, &SessionEntityState::default())
            .expect("When and Hidden with the same condition cannot coexist");

        let mut presentation_alternatives = node(
            UiNodeKind::Panel,
            "presentation-alternatives-panel",
            json!({ "title": "Presentation alternatives" }),
        );
        presentation_alternatives.children = ["first", "second"]
            .into_iter()
            .map(|value| {
                UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf {
                    predicate: botster_ui_contract::UiPresentationPredicate::Equals {
                        key: botster_ui_contract::UiPresentationKey("dialog".to_string()),
                        value: json!(value),
                    },
                    node: Box::new(action("presentation-action")),
                })
            })
            .collect();
        materialize_plugin_surface(&presentation_alternatives, &SessionEntityState::default())
            .expect("different values for one presentation key cannot render together");

        let assert_collision =
            |app: &mut TuiApp, surface_id: &str, node_id: &str, children: Vec<UiChild>| {
                let mut overlapping = node(
                    UiNodeKind::Panel,
                    &format!("{node_id}-panel"),
                    json!({ "title": "Overlapping conditions" }),
                );
                overlapping.children = children;
                let diagnostic = format!("duplicate materialized node id {node_id:?}");
                assert_eq!(
                    materialize_plugin_surface(&overlapping, &SessionEntityState::default())
                        .unwrap_err(),
                    diagnostic
                );
                app.apply_response(plugin_surface_response(canonical_surface(
                    "botster.plugin-contract-matrix",
                    surface_id,
                    overlapping,
                )));
                let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 180, 40);
                let rendered = lines.join("\n");
                assert!(rendered.contains(&diagnostic), "{rendered}");
                assert!(
                    !hit_map
                        .regions()
                        .iter()
                        .any(|region| region.node_id == node_id),
                    "overlapping conditionals must fail before ambiguous regions reach routing"
                );
            };
        assert_collision(
            &mut app,
            "contract.overlapping-hidden",
            "overlapping-hidden-action",
            vec![
                responsive_child(UiWidthClass::Expanded, action("overlapping-hidden-action")),
                UiChild::Conditional(UiConditional::Hidden {
                    condition: UiCondition {
                        width: Some(UiWidthClass::Compact),
                        ..UiCondition::default()
                    },
                    node: Box::new(action("overlapping-hidden-action")),
                }),
            ],
        );
        assert_collision(
            &mut app,
            "contract.identical-when",
            "identical-when-action",
            vec![
                responsive_child(UiWidthClass::Expanded, action("identical-when-action")),
                responsive_child(UiWidthClass::Expanded, action("identical-when-action")),
            ],
        );
        assert_collision(
            &mut app,
            "contract.cross-axis-when",
            "cross-axis-when-action",
            vec![
                responsive_child(UiWidthClass::Expanded, action("cross-axis-when-action")),
                UiChild::Conditional(UiConditional::When {
                    condition: UiCondition {
                        height: Some(botster_ui_contract::UiHeightClass::Tall),
                        ..UiCondition::default()
                    },
                    node: Box::new(action("cross-axis-when-action")),
                }),
            ],
        );
        let presentation_child = |node_id: &str| {
            UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf {
                predicate: botster_ui_contract::UiPresentationPredicate::Equals {
                    key: botster_ui_contract::UiPresentationKey("dialog".to_string()),
                    value: json!("shared"),
                },
                node: Box::new(action(node_id)),
            })
        };
        assert_collision(
            &mut app,
            "contract.identical-presentation",
            "identical-presentation-action",
            vec![
                presentation_child("identical-presentation-action"),
                presentation_child("identical-presentation-action"),
            ],
        );
        assert_collision(
            &mut app,
            "contract.presentation-and-when",
            "presentation-and-when-action",
            vec![
                presentation_child("presentation-and-when-action"),
                responsive_child(
                    UiWidthClass::Expanded,
                    action("presentation-and-when-action"),
                ),
            ],
        );
    }

    #[test]
    fn duplicate_multi_row_identity_and_unknown_where_fields_fail_visibly() {
        let multi_row = ui_node(json!({
            "type": "panel",
            "id": "multi-row-panel",
            "props": { "title": "Multi row" },
            "children": [{
                "$kind": "bind_list",
                "source": "/session",
                "where": { "registry_state": "active" },
                "item_template": {
                    "type": "button",
                    "id": "multi-row-action",
                    "props": {
                        "label": { "$bind": "@/session_uuid" },
                        "action": {
                            "id": "contract.open",
                            "payload": {
                                "session_uuid": { "$bind": "@/session_uuid" }
                            }
                        }
                    }
                }
            }]
        }));
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.multi-row",
            multi_row,
        )));
        app.session_entities
            .begin_generation("multi-row-generation".to_string());
        app.session_entities
            .apply(snapshot_frame(
                "multi-row-generation",
                1,
                vec![
                    session_entity("session-alpha", Some("running")),
                    session_entity("session-beta", Some("running")),
                ],
            ))
            .expect("snapshot applies");

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 40);
        let rendered = lines.join("\n");
        assert!(
            rendered.contains("duplicate materialized node id \"multi-row-action\""),
            "{rendered}"
        );
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "multi-row-action"),
            "ambiguous actions must not reach input routing"
        );

        let static_sibling_collision = ui_node(json!({
            "type": "panel",
            "id": "static-sibling-panel",
            "props": { "title": "Static sibling collision" },
            "children": [
                {
                    "type": "button",
                    "id": "session-alpha",
                    "props": {
                        "label": "Static sibling",
                        "action": { "id": "contract.static" }
                    }
                },
                {
                    "$kind": "bind_list",
                    "source": "/session",
                    "where": { "session_uuid": "session-alpha" },
                    "item_template": {
                        "type": "button",
                        "id": { "$bind": "@/session_uuid" },
                        "props": {
                            "label": "Bound row",
                            "action": { "id": "contract.bound" }
                        }
                    }
                }
            ]
        }));
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.static-sibling-collision",
            static_sibling_collision,
        )));
        let (lines, collision_hits) = renderer::render_to_lines(&app.surface(), 120, 40);
        let rendered = lines.join("\n");
        assert!(
            rendered.contains("duplicate materialized node id \"session-alpha\""),
            "{rendered}"
        );
        assert!(
            !collision_hits
                .regions()
                .iter()
                .any(|region| region.node_id == "session-alpha"),
            "a row-vs-static collision must fail before either action reaches routing"
        );

        let unknown_where = ui_node(json!({
            "type": "panel",
            "id": "unknown-where-panel",
            "props": { "title": "Unknown where" },
            "children": [{
                "$kind": "bind_list",
                "source": "/session",
                "where": { "session_udid": "session-alpha" },
                "item_template": {
                    "type": "text",
                    "id": "unknown-where-row",
                    "props": { "text": "matched" }
                },
                "empty_template": {
                    "type": "text",
                    "id": "unknown-where-empty",
                    "props": { "text": "Session unavailable" }
                }
            }]
        }));
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.unknown-where",
            unknown_where,
        )));
        let rendered = renderer::render_to_lines(&app.surface(), 120, 40)
            .0
            .join("\n");
        assert!(
            rendered.contains("unsupported /session where field"),
            "{rendered}"
        );
        assert!(!rendered.contains("Session unavailable"), "{rendered}");
    }

    #[test]
    fn canonical_snapshot_and_absolute_binding_fail_visibly() {
        let body = ui_node(json!({
            "type": "text",
            "id": "snapshot-body",
            "props": { "text": "body" }
        }));
        let mut missing_snapshot = TuiApp::new(None);
        missing_snapshot.apply_response(plugin_surface_response(DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.missing-snapshot".to_string(),
            body,
            ui_tree_snapshot: None,
        }));
        assert!(
            missing_snapshot
                .error
                .as_deref()
                .is_some_and(|error| error.contains("omitted ui_tree_snapshot"))
        );
        assert!(missing_snapshot.plugin_surface.is_none());

        let invalid_binding = ui_node(json!({
            "type": "panel",
            "id": "invalid-binding-panel",
            "props": { "title": "Invalid binding" },
            "children": [{
                "$kind": "bind_list",
                "source": "/session",
                "where": { "session_uuid": "session-invalid" },
                "item_template": {
                    "type": "text",
                    "id": "invalid-bound-value",
                    "props": {
                        "text": { "$bind": "/session/session-invalid/lifecycle_class" }
                    }
                }
            }]
        }));
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.apply_response(plugin_surface_response(canonical_surface(
            "botster.plugin-contract-matrix",
            "contract.invalid-binding",
            invalid_binding,
        )));
        app.session_entities
            .begin_generation("invalid-binding-generation".to_string());
        app.session_entities
            .apply(DaemonEntityFrame::Snapshot {
                subscription_id: "invalid-binding-generation".to_string(),
                entity_type: "session".to_string(),
                snapshot_seq: 1,
                items: vec![DaemonSessionEntity {
                    session_uuid: "session-invalid".to_string(),
                    registry_state: "running".to_string(),
                    lifecycle: Some("running".to_string()),
                    lifecycle_class: "current".to_string(),
                    rows: 24,
                    cols: 80,
                    updated_at: 1,
                    exit_code: None,
                    failure_reason: None,
                }],
                resync_reason: None,
            })
            .expect("snapshot applies");
        let rendered = renderer::render_to_lines(&app.surface(), 120, 40)
            .0
            .join("\n");
        assert!(
            rendered.contains("unsupported absolute binding path"),
            "{rendered}"
        );
        assert!(!rendered.contains("Session unavailable"), "{rendered}");
    }

    #[test]
    fn invalid_bound_row_ids_fail_before_renderer_state() {
        let mut state = SessionEntityState::default();
        state.begin_generation("invalid-bound-id-generation".to_string());
        state
            .apply(snapshot_frame(
                "invalid-bound-id-generation",
                1,
                vec![session_entity("session-alpha", Some("running"))],
            ))
            .expect("snapshot applies");

        for (path, expected) in [
            ("@/rows", "bound node id did not resolve to a string"),
            (
                "@/does_not_exist",
                "binding path \"@/does_not_exist\" is missing from the current session row",
            ),
            (
                "/session/session-alpha",
                "unsupported absolute binding path",
            ),
        ] {
            let root = ui_node(json!({
                "type": "panel",
                "id": "invalid-bound-id-panel",
                "children": [{
                    "$kind": "bind_list",
                    "source": "/session",
                    "where": { "session_uuid": "session-alpha" },
                    "item_template": {
                        "type": "button",
                        "id": { "$bind": path },
                        "props": {
                            "label": "Invalid bound id",
                            "action": { "id": "contract.invalid" }
                        }
                    }
                }]
            }));
            let error = materialize_plugin_surface(&root, &state)
                .expect_err("invalid bound identity must not materialize");
            assert!(error.contains(expected), "{path}: {error}");
        }

        let mut blank_state = SessionEntityState::default();
        blank_state.begin_generation("blank-bound-id-generation".to_string());
        blank_state
            .apply(snapshot_frame(
                "blank-bound-id-generation",
                1,
                vec![session_entity(" ", Some("running"))],
            ))
            .expect("blank-id snapshot applies");
        let blank = ui_node(json!({
            "type": "panel",
            "id": "blank-bound-id-panel",
            "children": [{
                "$kind": "bind_list",
                "source": "/session",
                "where": { "registry_state": "active" },
                "item_template": {
                    "type": "button",
                    "id": { "$bind": "@/session_uuid" },
                    "props": {
                        "label": "Blank bound id",
                        "action": { "id": "contract.blank" }
                    }
                }
            }]
        }));
        assert_eq!(
            materialize_plugin_surface(&blank, &blank_state).unwrap_err(),
            "bound node id resolved to a blank string"
        );
    }

    #[test]
    fn session_reducer_requires_snapshot_and_strictly_advancing_active_generation() {
        let mut state = SessionEntityState::default();
        state.begin_generation("generation-2".to_string());
        let upsert = DaemonEntityFrame::Upsert {
            subscription_id: "generation-2".to_string(),
            entity_type: "session".to_string(),
            snapshot_seq: 1,
            id: "session-alpha".to_string(),
            entity: session_entity("session-alpha", Some("running")),
        };
        assert!(
            !state
                .apply(upsert.clone())
                .expect("pre-snapshot delta ignored")
        );
        assert!(
            state
                .apply(snapshot_frame("generation-2", 1, Vec::new()))
                .expect("baseline applies")
        );
        assert!(!state.apply(upsert).expect("duplicate sequence ignored"));
        assert!(
            !state
                .apply(snapshot_frame("generation-1", 99, Vec::new()))
                .expect("prior generation ignored")
        );
    }

    #[test]
    fn session_navigator_preserves_authoritative_snapshot_order() {
        let mut app = TuiApp::new(None);
        app.session_entities
            .begin_generation("ordered-generation".to_string());
        app.session_entities
            .apply(snapshot_frame(
                "ordered-generation",
                1,
                vec![
                    session_entity("session-zeta", Some("running")),
                    session_entity("session-alpha", Some("running")),
                ],
            ))
            .expect("out-of-lexicographic-order snapshot applies");
        app.rebuild_session_rows();
        assert_eq!(
            app.sessions
                .iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            ["session-zeta", "session-alpha"]
        );
    }

    #[test]
    fn pending_spawn_is_separate_until_authoritative_upsert_and_never_auto_attaches() {
        let mut app = TuiApp::new(None);
        app.pending_sessions.insert(
            "session-alpha".to_string(),
            SessionRow::pending("session-alpha"),
        );
        app.selected_session = Some("session-alpha".to_string());
        app.session_entities
            .begin_generation("generation-1".to_string());
        app.rebuild_session_rows();
        assert!(app.sessions[0].pending);
        assert!(!app.sessions[0].is_attachable());

        app.session_entities
            .apply(snapshot_frame("generation-1", 0, Vec::new()))
            .expect("empty baseline applies");
        app.rebuild_session_rows();
        assert!(
            app.sessions[0].pending,
            "empty baseline keeps local pending feedback"
        );

        app.session_entities
            .apply(DaemonEntityFrame::Upsert {
                subscription_id: "generation-1".to_string(),
                entity_type: "session".to_string(),
                snapshot_seq: 1,
                id: "session-alpha".to_string(),
                entity: session_entity("session-alpha", Some("running")),
            })
            .expect("authoritative upsert applies");
        app.rebuild_session_rows();
        assert!(app.pending_sessions.is_empty());
        assert!(app.sessions[0].is_attachable());
        assert_eq!(app.attached_session, None);
    }

    #[test]
    fn active_entity_subscription_disconnect_invalidates_attachment_and_generation() {
        let mut app = TuiApp::new(None);
        app.session_entities
            .begin_generation("generation-1".to_string());
        app.attached_session = Some("session-alpha".to_string());
        app.attached_subscription_id = Some("terminal-generation".to_string());
        app.attach_hydration = Some(AttachHydration {
            session_id: "session-alpha".to_string(),
            subscription_id: "terminal-generation".to_string(),
            deadline: Instant::now() + Duration::from_secs(1),
            read_screen_requested: false,
            buffered_live_output: String::new(),
        });
        let (sender, receiver) = mpsc::channel();
        let (cancel_sender, _cancel_receiver) = mpsc::channel();
        let (stopped_sender, stopped_receiver) = mpsc::channel();
        stopped_sender.send(()).expect("reader already stopped");
        app.session_subscription = Some(SessionSubscriptionPump {
            messages: receiver,
            cancel: Some(cancel_sender),
            stopped: stopped_receiver,
            stop_attempted: false,
            stopped_confirmed: false,
        });
        sender
            .send(SessionSubscriptionMessage::Disconnected {
                subscription_id: "generation-1".to_string(),
                error: "closed".to_string(),
            })
            .expect("disconnect message sends");

        assert!(app.drain_session_subscription());
        assert_eq!(app.session_entities.subscription_id, None);
        assert_eq!(app.attached_session, None);
        assert_eq!(app.attached_subscription_id, None);
        assert!(app.attach_hydration.is_none());
    }

    #[test]
    fn session_subscription_pump_cancellation_waits_for_reader_exit() {
        let (_message_sender, messages) = mpsc::channel();
        let (cancel, cancelled) = mpsc::channel();
        let (stopped, reader_stopped) = mpsc::channel();
        let reader = thread::spawn(move || {
            cancelled.recv().expect("reader receives cancellation");
            stopped.send(()).expect("reader reports exit");
        });
        let mut pump = SessionSubscriptionPump {
            messages,
            cancel: Some(cancel),
            stopped: reader_stopped,
            stop_attempted: false,
            stopped_confirmed: false,
        };

        assert!(pump.stop());
        assert!(pump.stopped_confirmed);
        reader.join().expect("reader exits after cancellation");
    }

    #[test]
    fn compatibility_error_branch_renders_distinct_compatibility_diagnostic() {
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);

        app.apply_response(status_response_with_package_counts("running", 7, 3, 1));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("packages: 3 installed; 1 enabled"));
    }

    #[test]
    fn package_response_renders_installed_state_capabilities_and_provider_admission() {
        let mut app = TuiApp::new(None);

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
    fn package_response_renders_hub_owned_surface_descriptors_and_show_uses_real_input() {
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.system_details_visible = true;
        let mut package = package(
            "botster.plugin-contract-matrix",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        package.surfaces = contract_package_surfaces();
        app.apply_response(packages_response(vec![package]));
        app.observed_requests.clear();

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 500, 180);
        let rendered = lines.join("\n");
        assert!(rendered.contains(
            "surface: package=botster.plugin-contract-matrix id=contract.app kind=app title=Contract App supports=render,action"
        ));
        assert!(rendered.contains(
            "surface: package=botster.plugin-contract-matrix id=contract.settings kind=settings title=Contract Settings supports=render"
        ));
        assert!(rendered.contains(
            "surface: package=botster.plugin-contract-matrix id=contract.diagnostics kind=diagnostics title=Contract Diagnostics supports=none"
        ));

        app.handle_dispatch(click_dispatch(&hit_map, "tui-package-0-show"));

        assert!(
            app.observed_requests
                .contains(&ObservedRequest::ShowPackage(
                    "botster.plugin-contract-matrix".to_string()
                ))
        );
    }

    #[test]
    fn show_response_is_scoped_refresh_restores_list_and_errors_preserve_other_surfaces() {
        let mut app = TuiApp::new(None);
        let mut shown = package(
            "botster.plugin-contract-matrix",
            "1.0.0",
            "plugin",
            "enabled",
            Vec::new(),
            true,
        );
        shown.surfaces = contract_package_surfaces();
        let other = package("local-other", "0.1.0", "local", "enabled", Vec::new(), true);
        let full_list = vec![shown.clone(), other];
        app.apply_response(packages_response(full_list.clone()));
        app.apply_response(package_navigation_response(vec![
            plugin_contract_app_navigation(),
        ]));
        app.apply_response(plugin_surface_response(contract_app_plugin_surface()));
        let owner = app.plugin_surface.clone();

        app.apply_response(packages_response(vec![shown.clone()]));

        assert_eq!(app.packages, vec![shown]);
        assert_eq!(
            app.package_navigation,
            vec![plugin_contract_app_navigation()]
        );
        assert_eq!(app.plugin_surface, owner);

        let mut error = base_response(DaemonResponseKind::Packages);
        error.error = Some(botster_hub_client::DaemonOperatorError {
            code: "package_policy_error".to_string(),
            request_id: "show-missing".to_string(),
            operation: "show".to_string(),
            message: "package action failed: PackageNotInstalled".to_string(),
            diagnostics: Vec::new(),
        });
        app.apply_response(error);
        assert_eq!(app.packages.len(), 1);
        assert_eq!(
            app.package_navigation,
            vec![plugin_contract_app_navigation()]
        );
        assert_eq!(app.plugin_surface, owner);
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("package_policy_error"))
        );
        let rendered_error = renderer::render_to_lines(&app.surface(), 320, 120)
            .0
            .join("\n");
        assert!(rendered_error.contains("package_policy_error"));
        assert!(rendered_error.contains("operation=show"));

        app.apply_response(packages_response(full_list.clone()));
        assert_eq!(app.packages, full_list);
        assert_eq!(
            app.package_navigation,
            vec![plugin_contract_app_navigation()]
        );
        assert_eq!(app.plugin_surface, owner);
    }

    #[test]
    fn package_response_preserves_zero_entrypoint_package_row() {
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

        app.apply_response(plugin_surface_response(contract_app_plugin_surface()));
        app.pending_plugin_request = Some(UiActionRequest {
            request_id: UiActionRequestId("contract-action-success".to_string()),
            surface_id: UiSurfaceId("contract.app".to_string()),
            action_id: UiActionId("contract.action".to_string()),
            node_id: Some(UiNodeId("contract-app-action".to_string())),
            kind: UiActionKind::Submit,
            values: None,
            payload: None,
        });
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
        assert!(rendered.contains("Plugin: botster.plugin-contract-matrix / contract.app"));
        assert!(rendered.contains("UiNode payload delivered through plugin_surface_render."));
        assert!(rendered.contains("Run contract action"));
        assert_eq!(
            app.action_feedback.as_deref(),
            Some("state=Accepted request_id=contract-action-success")
        );
    }

    #[test]
    fn active_plugin_routes_arbitrary_and_colliding_actions_with_exact_identity() {
        let mut app = TuiApp::new(None);
        app.observed_requests.clear();
        app.system_details_visible = false;
        app.apply_response(plugin_surface_response(contract_app_plugin_surface()));
        let request = UiActionRequest {
            request_id: UiActionRequestId("request-collision".to_string()),
            surface_id: UiSurfaceId("contract.app".to_string()),
            action_id: UiActionId("botster.tui.toggle_system_details".to_string()),
            node_id: Some(UiNodeId("contract-app-action".to_string())),
            kind: UiActionKind::Submit,
            values: Some(UiFormValues(
                json!({ "message": "hello" })
                    .as_object()
                    .expect("values object")
                    .clone(),
            )),
            payload: Some(json!({ "arbitrary": true })),
        };

        app.handle_dispatch(InputDispatch::Action(request.clone()));

        assert_eq!(
            app.observed_requests,
            vec![ObservedRequest::PluginSurfaceAction {
                package_name: "botster.plugin-contract-matrix".to_string(),
                request,
            }]
        );
        assert!(!app.system_details_visible);
    }

    #[test]
    fn plugin_actions_require_the_active_owning_surface() {
        let mut app = TuiApp::new(None);
        app.observed_requests.clear();
        let request = plugin_request(
            "request-without-owner",
            "contract.app",
            "plugin.arbitrary",
            "contract-app-action",
        );

        app.handle_dispatch(InputDispatch::Action(request));
        assert!(app.observed_requests.is_empty());

        app.apply_response(plugin_surface_response(contract_app_plugin_surface()));
        app.handle_dispatch(InputDispatch::Action(plugin_request(
            "request-wrong-surface",
            "contract.other",
            "plugin.arbitrary",
            "contract-app-action",
        )));
        assert!(app.observed_requests.is_empty());
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("surface mismatch"))
        );
        assert_eq!(app.active_plugin_surface_id(), Some("contract.app"));
    }

    #[test]
    fn tui_owned_escape_clears_plugin_scope_without_dispatch() {
        let mut app = TuiApp::new(None);
        app.observed_requests.clear();
        app.apply_response(plugin_surface_response(presentation_plugin_surface()));
        app.plugin_presentation.set(
            botster_ui_contract::UiPresentationKey("contract-dialog".to_string()),
            Value::Bool(true),
        );
        app.pending_plugin_request = Some(plugin_request(
            "request-pending",
            "contract.presentation",
            "contract.submit",
            "contract-form",
        ));

        assert!(app.handle_tui_owned_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

        assert!(app.plugin_surface.is_none());
        assert_eq!(
            app.plugin_presentation,
            renderer::PresentationState::default()
        );
        assert!(app.pending_plugin_request.is_none());
        assert!(app.plugin_action_result.is_none());
        assert!(app.observed_requests.is_empty());
        assert!(app.system_details_visible);
        assert!(app.surface().id == Some(UiNodeId("workspace-root".to_string()).into()));
    }

    #[test]
    fn matching_results_apply_presentation_rejection_errors_and_replacement() {
        let mut app = TuiApp::new(None);
        app.apply_response(plugin_surface_response(presentation_plugin_surface()));

        app.pending_plugin_request = Some(plugin_request(
            "request-open",
            "contract.presentation",
            "contract.open",
            "contract-open",
        ));
        app.apply_response(plugin_action_response(json!({
            "request_id": "request-open",
            "surface_id": "contract.presentation",
            "action_id": "contract.open",
            "node_id": "contract-open",
            "state": "accepted",
            "presentation": [
                { "kind": "set", "key": "contract-dialog", "value": true },
                { "kind": "set", "key": "selected-workspace", "value": "workspace-alpha" }
            ]
        })));

        let (lines, hit_map) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            160,
            60,
            &RenderState::default(),
            &app.plugin_presentation,
        );
        let rendered = lines.join("\n");
        assert!(rendered.contains("Contract form"));
        assert!(rendered.contains("Selected workspace: workspace-alpha"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "contract-form")
        );

        let retained_root = app.plugin_surface.as_ref().expect("owner").body.clone();
        let retained_presentation = app.plugin_presentation.clone();
        app.pending_plugin_request = Some(plugin_request(
            "request-rejected",
            "contract.presentation",
            "contract.submit",
            "contract-form",
        ));
        app.apply_response(plugin_action_response(json!({
            "request_id": "request-rejected",
            "surface_id": "contract.presentation",
            "action_id": "contract.submit",
            "node_id": "contract-form",
            "state": "rejected",
            "field_errors": {
                "contract-message": ["Message is required"]
            },
            "form_errors": ["Fix the highlighted fields"]
        })));
        assert_eq!(
            app.plugin_surface.as_ref().expect("owner").body,
            retained_root
        );
        assert_eq!(app.plugin_presentation, retained_presentation);
        let (lines, _) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            160,
            60,
            &RenderState::default(),
            &app.plugin_presentation,
        );
        let rendered = lines.join("\n");
        assert!(rendered.contains("Message is required"), "{rendered}");
        assert!(
            rendered.contains("error: Message is required"),
            "{rendered}"
        );
        assert_eq!(
            rendered.matches("Message is required").count(),
            1,
            "{rendered}"
        );
        assert!(
            rendered.contains("Fix the highlighted fields"),
            "{rendered}"
        );
        assert!(rendered.contains("Contract form"));

        app.pending_plugin_request = Some(plugin_request(
            "request-accepted",
            "contract.presentation",
            "contract.submit",
            "contract-form",
        ));
        app.apply_response(plugin_action_response(json!({
            "request_id": "request-accepted",
            "surface_id": "contract.presentation",
            "action_id": "contract.submit",
            "node_id": "contract-form",
            "state": "accepted",
            "presentation": [
                { "kind": "clear", "key": "contract-dialog" }
            ],
            "replacement": {
                "type": "button",
                "id": "contract-action-replacement",
                "props": {
                    "label": "Replacement action",
                    "action": { "id": "contract.replacement" }
                }
            }
        })));
        let surface = app.plugin_surface.as_ref().expect("owner retained");
        assert_eq!(
            surface.body.id,
            Some(UiNodeId("contract-action-replacement".to_string()).into())
        );
        assert!(
            surface.ui_tree_snapshot.is_none(),
            "an app-owned replacement must clear the stale delivered snapshot"
        );
        assert!(
            app.plugin_presentation
                .get(&botster_ui_contract::UiPresentationKey(
                    "contract-dialog".to_string()
                ))
                .is_none()
        );
        let (replacement_lines, hit_map) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            160,
            60,
            &RenderState::default(),
            &app.plugin_presentation,
        );
        let rendered = replacement_lines.join("\n");
        assert!(rendered.contains("Plugin: botster.plugin-contract-matrix"));
        assert!(rendered.contains("Replacement action"));
        assert!(!rendered.contains("Contract form"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "contract-action-replacement")
        );
    }

    #[test]
    fn mismatched_plugin_result_cannot_mutate_active_scope() {
        let mut app = TuiApp::new(None);
        app.apply_response(plugin_surface_response(presentation_plugin_surface()));
        app.pending_plugin_request = Some(plugin_request(
            "request-current",
            "contract.presentation",
            "contract.open",
            "contract-open",
        ));
        let retained_root = app.plugin_surface.as_ref().expect("owner").body.clone();

        app.apply_response(plugin_action_response(json!({
            "request_id": "request-stale",
            "surface_id": "contract.presentation",
            "action_id": "contract.open",
            "node_id": "contract-open",
            "state": "accepted",
            "presentation": [
                { "kind": "set", "key": "contract-dialog", "value": true }
            ],
            "replacement": {
                "type": "text",
                "id": "stale-replacement",
                "props": { "text": "stale" }
            }
        })));

        assert_eq!(
            app.plugin_surface.as_ref().expect("owner").body,
            retained_root
        );
        assert_eq!(
            app.plugin_presentation,
            renderer::PresentationState::default()
        );
        assert_eq!(
            app.pending_plugin_request
                .as_ref()
                .map(|request| request.request_id.0.as_str()),
            Some("request-current")
        );
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("mismatched"))
        );
        let rendered = renderer::render_to_lines(&app.surface(), 160, 60)
            .0
            .join("\n");
        assert!(
            rendered.contains("ignored mismatched plugin action result"),
            "{rendered}"
        );
    }

    #[test]
    fn rejected_dialog_fields_render_one_native_error_row_per_supported_kind() {
        let mut app = TuiApp::new(None);
        app.apply_response(plugin_surface_response(field_error_kinds_plugin_surface()));
        app.pending_plugin_request = Some(plugin_request(
            "request-field-errors",
            "contract.field-errors",
            "contract.submit",
            "contract-field-error-form",
        ));
        app.apply_response(plugin_action_response(json!({
            "request_id": "request-field-errors",
            "surface_id": "contract.field-errors",
            "action_id": "contract.submit",
            "node_id": "contract-field-error-form",
            "state": "rejected",
            "field_errors": {
                "contract-text-input": ["Text input required"],
                "contract-checkbox": ["Checkbox required"],
                "contract-form-field": ["Form field required"],
                "contract-textarea": ["Textarea required"],
                "contract-select": ["Select required"]
            },
            "form_errors": ["Fix the highlighted fields"]
        })));

        let (lines, hit_map) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            180,
            80,
            &RenderState::default(),
            &app.plugin_presentation,
        );
        let rendered = lines.join("\n");

        assert!(rendered.contains("Field error contract"), "{rendered}");
        for message in [
            "Text input required",
            "Checkbox required",
            "Form field required",
            "Textarea required",
            "Select required",
        ] {
            assert!(
                rendered.contains(&format!("error: {message}")),
                "{rendered}"
            );
            assert_eq!(rendered.matches(message).count(), 1, "{rendered}");
        }
        for node_id in [
            "contract-text-input",
            "contract-checkbox",
            "contract-form-field",
            "contract-textarea",
            "contract-select",
        ] {
            assert!(
                hit_map
                    .regions()
                    .iter()
                    .any(|region| region.node_id == node_id),
                "missing hit region for {node_id}: {rendered}"
            );
        }
    }

    #[test]
    fn plugin_shell_renders_action_failure_feedback_and_diagnostics() {
        let mut app = TuiApp::new(None);
        app.apply_response(plugin_surface_response(contract_app_plugin_surface()));
        app.pending_plugin_request = Some(plugin_request(
            "request-error",
            "contract.app",
            "contract.action",
            "contract-app-action",
        ));
        let mut response = plugin_action_response(json!({
            "request_id": "request-error",
            "surface_id": "contract.app",
            "action_id": "contract.action",
            "node_id": "contract-app-action",
            "state": "error",
            "error": "contract action failed"
        }));
        response.diagnostics.push(DaemonDiagnostic {
            kind: DaemonDiagnosticKind::ActionFailure,
            operation: Some("plugin_surface_action".to_string()),
            feature: Some("plugin_surface_actions".to_string()),
            message: Some("contract action failed".to_string()),
        });

        app.apply_response(response);

        let rendered = renderer::render_to_lines(&app.surface(), 200, 80)
            .0
            .join("\n");
        assert!(
            rendered.contains("action: state=Error request_id=request-error"),
            "{rendered}"
        );
        assert!(
            rendered.contains("diagnostic: action_failure"),
            "{rendered}"
        );
        assert!(
            rendered.contains("operation=plugin_surface_action"),
            "{rendered}"
        );
        assert!(rendered.contains("contract action failed"), "{rendered}");
    }

    #[test]
    fn keyboard_dialog_rejection_retains_router_draft_focus_and_submit_identity() {
        let mut app = TuiApp::new(None);
        app.apply_response(plugin_surface_response(presentation_plugin_surface()));
        app.plugin_presentation.set(
            botster_ui_contract::UiPresentationKey("contract-dialog".to_string()),
            Value::Bool(true),
        );
        let mut router = InputRouter::new(renderer::action_request_context_for(
            "contract.presentation",
        ));
        let (_lines, initial_hits) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            160,
            60,
            &router.render_state(),
            &app.plugin_presentation,
        );
        router.reconcile(&initial_hits);
        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &initial_hits,
            ),
            InputDispatch::Focus {
                node_id: "contract-form".to_string()
            }
        );
        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &initial_hits,
            ),
            InputDispatch::Focus {
                node_id: "contract-message".to_string()
            }
        );
        router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Char('H'), KeyModifiers::NONE)),
            &initial_hits,
        );
        assert_eq!(router.draft_value("message"), Some(&json!("H")));

        app.pending_plugin_request = Some(plugin_request(
            "request-keyboard-rejected",
            "contract.presentation",
            "contract.submit",
            "contract-form",
        ));
        app.apply_response(plugin_action_response(json!({
            "request_id": "request-keyboard-rejected",
            "surface_id": "contract.presentation",
            "action_id": "contract.submit",
            "node_id": "contract-form",
            "state": "rejected",
            "field_errors": {
                "contract-message": ["Message is too short"]
            },
            "form_errors": ["Fix the highlighted fields"]
        })));
        let (_lines, rejected_hits) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            160,
            60,
            &router.render_state(),
            &app.plugin_presentation,
        );
        router.reconcile(&rejected_hits);
        assert_eq!(router.focused_node_id(), Some("contract-message"));
        assert_eq!(router.draft_value("message"), Some(&json!("H")));

        assert_eq!(
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                &rejected_hits,
            ),
            InputDispatch::Focus {
                node_id: "contract-form".to_string()
            }
        );
        let dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &rejected_hits,
        );
        assert!(matches!(
            &dispatch,
            InputDispatch::Action(UiActionRequest {
                surface_id,
                action_id,
                node_id,
                kind: UiActionKind::Submit,
                values: Some(values),
                payload: Some(payload),
                ..
            }) if surface_id == &UiSurfaceId("contract.presentation".to_string())
                && action_id == &UiActionId("contract.submit".to_string())
                && node_id == &Some(UiNodeId("contract-form".to_string()))
                && values.0.get("message") == Some(&json!("H"))
                && payload == &json!({ "source": "dialog" })
        ));
        let expected_request = match &dispatch {
            InputDispatch::Action(request) => request.clone(),
            other => panic!("expected action dispatch, got {other:?}"),
        };
        app.observed_requests.clear();
        app.handle_dispatch(dispatch);
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::PluginSurfaceAction {
                    package_name: "botster.plugin-contract-matrix".to_string(),
                    request: expected_request,
                })
        );
    }

    #[test]
    fn rendered_plugin_button_mouse_activation_reaches_hub_request_seam() {
        let mut app = TuiApp::new(None);
        app.apply_response(plugin_surface_response(contract_app_plugin_surface()));
        app.observed_requests.clear();
        let mut router = InputRouter::new(renderer::action_request_context_for("contract.app"));
        let (_lines, hit_map) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            160,
            60,
            &router.render_state(),
            &app.plugin_presentation,
        );
        router.reconcile(&hit_map);
        let action = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "contract-app-action")
            .expect("rendered plugin button should be hit-testable");
        let (column, row) = (action.rect.x, action.rect.y);
        assert_eq!(
            router.dispatch_event(
                mouse_event(
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                    column,
                    row,
                ),
                &hit_map,
            ),
            InputDispatch::Focus {
                node_id: "contract-app-action".to_string()
            }
        );
        let dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        let expected_request = match &dispatch {
            InputDispatch::Action(request) => request.clone(),
            other => panic!("expected action dispatch, got {other:?}"),
        };

        app.handle_dispatch(dispatch);

        assert_eq!(
            expected_request.surface_id,
            UiSurfaceId("contract.app".to_string())
        );
        assert_eq!(
            expected_request.action_id,
            UiActionId("contract.action".to_string())
        );
        assert_eq!(
            expected_request.node_id,
            Some(UiNodeId("contract-app-action".to_string()))
        );
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::PluginSurfaceAction {
                    package_name: "botster.plugin-contract-matrix".to_string(),
                    request: expected_request,
                })
        );
    }

    #[test]
    fn composite_application_primitives_render_through_tui_kit() {
        let surface = composite_application_primitives_plugin_surface();
        let node = plugin_surface_body_node(&surface).expect("composite surface validates for TUI");

        let (lines, hit_map) = renderer::render_to_lines(&node, 100, 36);
        let rendered = lines.join("\n");

        assert!(rendered.contains("Project Pipeline Overview"));
        assert!(rendered.contains("Active Runs: 3"));
        assert!(rendered.contains("Healthy"));
        assert!(rendered.contains("Ticket"));
        assert!(rendered.contains("State"));
        assert!(rendered.contains("1783529012"));
        assert!(rendered.contains("review"));
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
        let mut app = TuiApp::new(None);

        app.apply_response(plugin_surface_response(
            composite_application_primitives_plugin_surface(),
        ));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 420, 220);
        let rendered = lines.join("\n");

        assert!(rendered.contains("Plugin: botster.plugin-contract-matrix / contract.composite"));
        assert!(rendered.contains("Project Pipeline Overview"));
        assert!(rendered.contains("Refresh"));
    }

    #[test]
    fn composite_table_mouse_selection_dispatches_exact_row_action() {
        let surface = composite_application_primitives_plugin_surface();
        let node = plugin_surface_body_node(&surface).expect("composite surface validates for TUI");
        let (_lines, frame_n_hit_map) = renderer::render_to_lines(&node, 100, 36);
        let row = frame_n_hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "contract-composite-ticket-a")
            .expect("bordered composite table row should be hit-testable");
        let (column, row) = (row.rect.x, row.rect.y);
        let mut router = InputRouter::new(renderer::action_request_context());

        let down_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &frame_n_hit_map,
        );
        assert_eq!(
            down_dispatch,
            InputDispatch::Focus {
                node_id: "contract-composite-ticket-a".to_string()
            }
        );
        assert_eq!(router.selected_row("contract-composite-ticket-table"), None);

        let (_lines, frame_n_plus_one_hit_map) = renderer::render_to_lines(&node, 100, 36);
        let up_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &frame_n_plus_one_hit_map,
        );

        assert!(matches!(
            up_dispatch,
            InputDispatch::Action(request)
                if request.action_id == botster_ui_contract::UiActionId("contract.ticket.open".to_string())
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
        let mut app = TuiApp::new(None);

        app.apply_response(plugin_surface_response(invalid_table_plugin_surface()));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 320, 180);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Plugin: botster.plugin-contract-matrix / contract.invalid"));
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
        let mut app = TuiApp::new(None);
        app.observed_requests.clear();
        let entry = plugin_contract_app_navigation();

        app.apply_response(package_navigation_response(vec![entry.clone()]));
        app.handle_dispatch(InputDispatch::Action(UiActionRequest {
            request_id: UiActionRequestId("req-navigation-open".to_string()),
            surface_id: UiSurfaceId(renderer::WORKSPACE_SURFACE_ID.to_string()),
            action_id: UiActionId("botster.tui.navigation.open".to_string()),
            node_id: Some(UiNodeId("tui-package-navigation-0-open".to_string())),
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
        let mut app = TuiApp::new(None);
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
        assert!(!rendered.contains("tui-package-navigation-0-open"));
    }

    #[test]
    fn unsupported_navigation_target_stays_visible_with_precise_target() {
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);
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

        app.handle_dispatch(InputDispatch::Action(
            botster_ui_contract::UiActionRequest {
                request_id: botster_ui_contract::UiActionRequestId("req-config-submit".to_string()),
                surface_id: botster_ui_contract::UiSurfaceId(
                    renderer::WORKSPACE_SURFACE_ID.to_string(),
                ),
                action_id: botster_ui_contract::UiActionId(
                    "botster.tui.package_config.submit".to_string(),
                ),
                node_id: Some(UiNodeId("tui-package-0-configuration-submit".to_string())),
                kind: botster_ui_contract::UiActionKind::Submit,
                values: Some(UiFormValues(
                    app.drafts
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect(),
                )),
                payload: Some(json!({ "package_name": "configuration.plugin" })),
            },
        ));

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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

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
    fn action_failure_survives_unrelated_successful_status_refresh() {
        let mut app = TuiApp::new(None);

        app.apply_response(operator_error_response("spawn failed"));
        app.apply_response(status_response("running", 1));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        assert!(lines.join("\n").contains("error: spawn failed"));
    }

    #[test]
    fn corrected_user_action_clears_stale_validation_error() {
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);

        app.record_transport_error(DaemonTransportError::NotRunning);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("hub unavailable; reconnecting"));
        assert!(!rendered.contains("compatibility mismatch"));
    }

    #[test]
    fn terminal_input_before_attach_renders_stream_unavailable_error() {
        let mut app = TuiApp::new(None);
        app.sessions = vec![SessionRow::running("session-alpha")];
        app.selected_session = Some("session-alpha".to_string());

        app.handle_dispatch(InputDispatch::TerminalForward {
            node_id: "tui-terminal".to_string(),
            bytes: b"echo hello\n".to_vec(),
        });

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("terminal stream unavailable"));
        assert!(rendered.contains("terminal stream unavailable"));
    }

    #[test]
    fn terminal_input_rejects_stale_attached_subscription_generation() {
        let mut app = TuiApp::new(None);
        app.subscription_id = "sub-current".to_string();
        app.attached_session = Some("session-alpha".to_string());
        app.attached_subscription_id = Some("sub-stale".to_string());
        app.observed_requests.clear();

        app.handle_dispatch(InputDispatch::TerminalForward {
            node_id: "tui-terminal".to_string(),
            bytes: b"x".to_vec(),
        });

        assert!(app.observed_requests.is_empty());
        assert_eq!(
            app.error.as_deref(),
            Some("terminal stream unavailable: current subscription is not attached")
        );
    }

    #[test]
    fn attach_state_tracks_attached_session_separately_from_selection() {
        let mut app = TuiApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-beta".to_string());
        app.begin_attach_hydration("session-beta", "sub-test");

        app.apply_response(attach_state_response("session-beta", "attached"));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert_eq!(app.attached_session.as_deref(), Some("session-beta"));
        assert!(rendered.contains("attached: session-beta"));
        assert!(rendered.contains("session-beta · attached"));
        assert!(rendered.contains("terminal attached: session-beta"));
    }

    #[test]
    fn stale_subscription_events_cannot_own_or_mutate_current_terminal_state() {
        let mut app = TuiApp::new(None);
        app.begin_attach_hydration("session-alpha", "sub-current");

        app.apply_response(events_response(vec![DaemonEvent::AttachState {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-stale".to_string(),
            state: "attached".to_string(),
        }]));
        assert!(app.attached_session.is_none());

        app.apply_response(events_response(vec![
            DaemonEvent::AttachState {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-current".to_string(),
                state: "attached".to_string(),
            },
            DaemonEvent::TerminalOutput {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-stale".to_string(),
                data: "stale".to_string(),
            },
            DaemonEvent::TerminalOutput {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-current".to_string(),
                data: "current".to_string(),
            },
        ]));
        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "restored-"),
            "read_screen",
        );

        assert_eq!(app.attached_session.as_deref(), Some("session-alpha"));
        assert_eq!(app.attached_subscription_id.as_deref(), Some("sub-current"));
        assert_eq!(app.terminal_output, "restored-current");

        app.apply_response(events_response(vec![DaemonEvent::AttachState {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-stale".to_string(),
            state: "detached".to_string(),
        }]));
        assert_eq!(app.attached_session.as_deref(), Some("session-alpha"));
        assert_eq!(app.terminal_output, "restored-current");
    }

    #[test]
    fn terminal_view_carries_output_bytes() {
        let mut app = TuiApp::new(None);
        app.terminal_output = "hello terminal".to_string();

        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        assert!(lines.join("\n").contains("hello terminal"));
    }

    #[test]
    fn terminal_output_renders_as_terminal_primitive_content() {
        let mut app = TuiApp::new(None);
        app.terminal_output = "primitive terminal bytes".to_string();

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);

        assert!(lines.join("\n").contains("primitive terminal bytes"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-terminal")
        );
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-terminal-output")
        );
    }

    #[test]
    fn opaque_history_events_never_render_as_terminal_text() {
        let mut app = TuiApp::new(None);
        app.begin_attach_hydration("session-alpha", "sub-test");

        app.apply_response(events_response(vec![
            DaemonEvent::Snapshot {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                history: DaemonOpaqueHistoryPayload::from_bytes(b"snapshot\n"),
            },
            DaemonEvent::Scrollback {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                history: DaemonOpaqueHistoryPayload::from_bytes(b"scrollback\n"),
            },
            DaemonEvent::TerminalOutput {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                data: "live\n".to_string(),
            },
        ]));

        assert!(app.terminal_output.is_empty());
        assert_eq!(
            app.attach_hydration
                .as_ref()
                .map(|hydration| hydration.buffered_live_output.as_str()),
            Some("live\n")
        );
        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "restored\n"),
            "read_screen",
        );
        assert_eq!(app.terminal_output, "restored\nlive\n");

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(!rendered.contains("snapshot"));
        assert!(!rendered.contains("scrollback"));
        assert!(rendered.contains("restored"));
        assert!(rendered.contains("live"));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-terminal")
        );
    }

    #[test]
    fn shared_late_attach_history_waits_through_empty_drain_and_renders_once_in_order() {
        let scenario = botster_hub_test_support::late_attach_history_conformance_scenario();
        assert!(
            scenario.conformance_fixture_revision >= MINIMUM_CONFORMANCE_FIXTURE_REVISION,
            "shared fixture must satisfy the TUI's minimum conformance revision"
        );
        let attaching_index = scenario
            .history_then_live
            .iter()
            .position(|event| {
                matches!(event, DaemonEvent::AttachState { state, .. } if state == "attaching")
            })
            .expect("shared fixture includes attaching state");
        let opaque_state_index = scenario
            .history_then_live
            .iter()
            .position(|event| {
                matches!(
                    event,
                    DaemonEvent::Snapshot { .. } | DaemonEvent::Scrollback { .. }
                )
            })
            .expect("shared fixture includes opaque engine state");
        let attached_index = scenario
            .history_then_live
            .iter()
            .position(|event| {
                matches!(event, DaemonEvent::AttachState { state, .. } if state == "attached")
            })
            .expect("shared fixture includes attached state");
        let live_index = scenario
            .history_then_live
            .iter()
            .position(|event| matches!(event, DaemonEvent::TerminalOutput { .. }))
            .expect("shared fixture includes live output");
        assert!(attaching_index < opaque_state_index);
        assert!(opaque_state_index < attached_index);
        assert!(attached_index < live_index);

        let mut app = TuiApp::new(None);
        app.subscription_id = scenario.subscription_id.clone();
        app.begin_attach_hydration(&scenario.session_id, &scenario.subscription_id);
        app.observed_requests.clear();

        app.apply_response(events_response(Vec::new()));
        assert!(app.attach_hydration.is_some());
        assert!(app.observed_requests.is_empty());

        app.apply_response(events_response(vec![
            scenario.history_then_live[attaching_index].clone(),
        ]));
        assert!(app.attach_hydration.is_some());
        assert!(app.attached_session.is_none());
        assert!(app.observed_requests.is_empty());

        let opaque_event = scenario.history_then_live[opaque_state_index].clone();
        let decoded = match &opaque_event {
            DaemonEvent::Snapshot { history, .. } | DaemonEvent::Scrollback { history, .. } => {
                history
                    .decoded_bytes()
                    .expect("fixture opaque state decodes")
            }
            other => panic!("expected shared opaque history event, got {other:?}"),
        };
        assert_eq!(decoded, vec![0, 255, 71, 84, 89, 1]);
        app.apply_response(events_response(vec![opaque_event]));
        assert!(app.terminal_output.is_empty());
        assert!(app.attach_hydration.is_some());

        app.apply_response(events_response(vec![
            scenario.history_then_live[attached_index].clone(),
        ]));
        assert_eq!(
            app.attached_session.as_deref(),
            Some(scenario.session_id.as_str())
        );

        app.apply_response(events_response(vec![
            scenario.history_then_live[live_index].clone(),
        ]));
        let live = match &scenario.history_then_live[live_index] {
            DaemonEvent::TerminalOutput { data, .. } => data,
            other => panic!("expected shared live event, got {other:?}"),
        };
        assert!(app.terminal_output.is_empty());
        app.apply_optional_readback_response(
            read_screen_response(&scenario.session_id, &scenario.read_screen_text),
            "read_screen",
        );
        assert_eq!(
            app.terminal_output,
            format!("{}{live}", scenario.read_screen_text)
        );
        assert_eq!(
            app.terminal_output
                .matches(&scenario.read_screen_text)
                .count(),
            1
        );
        assert!(app.attach_hydration.is_none());
        assert!(app.observed_requests.is_empty());
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(
            rendered.find(scenario.read_screen_text.trim()).unwrap()
                < rendered.find(live.trim()).unwrap()
        );
    }

    #[test]
    fn shared_no_history_attached_owns_input_while_bounded_hydration_continues() {
        let scenario = botster_hub_test_support::late_attach_history_conformance_scenario();
        let attaching_index = scenario
            .no_history_then_live
            .iter()
            .position(|event| {
                matches!(event, DaemonEvent::AttachState { state, .. } if state == "attaching")
            })
            .expect("idle fixture includes attaching state");
        let attached_index = scenario
            .no_history_then_live
            .iter()
            .position(|event| {
                matches!(event, DaemonEvent::AttachState { state, .. } if state == "attached")
            })
            .expect("idle fixture includes attached state");
        let live_index = scenario
            .no_history_then_live
            .iter()
            .position(|event| matches!(event, DaemonEvent::TerminalOutput { .. }))
            .expect("idle fixture includes live output");
        assert!(attaching_index < attached_index);
        assert!(attached_index < live_index);
        assert!(scenario.no_history_then_live.iter().all(|event| !matches!(
            event,
            DaemonEvent::Snapshot { .. } | DaemonEvent::Scrollback { .. }
        )));

        let mut app = TuiApp::new(None);
        app.subscription_id = scenario.no_history_subscription_id.clone();
        app.begin_attach_hydration(
            &scenario.no_history_session_id,
            &scenario.no_history_subscription_id,
        );
        app.observed_requests.clear();

        app.apply_response(events_response(vec![
            scenario.no_history_then_live[attaching_index].clone(),
            scenario.no_history_then_live[attached_index].clone(),
        ]));

        assert_eq!(
            app.attached_session.as_deref(),
            Some(scenario.no_history_session_id.as_str())
        );
        assert!(app.attach_hydration.is_some());

        app.apply_response(events_response(vec![
            scenario.no_history_then_live[live_index].clone(),
        ]));
        assert!(app.terminal_output.is_empty());
        assert!(app.attach_hydration.is_some());
        assert!(app.observed_requests.is_empty());

        app.attach_hydration.as_mut().unwrap().deadline = Instant::now();
        app.apply_response(events_response(Vec::new()));
        app.apply_optional_readback_response(
            read_screen_response(
                &scenario.no_history_session_id,
                &scenario.no_history_read_screen_text,
            ),
            "read_screen",
        );
        assert!(app.attach_hydration.is_none());
        let live = match &scenario.no_history_then_live[live_index] {
            DaemonEvent::TerminalOutput { data, .. } => data,
            other => panic!("expected shared live event, got {other:?}"),
        };
        assert_eq!(app.terminal_output, *live);
        assert!(app.observed_requests.is_empty());

        let exit = scenario
            .no_history_then_live
            .iter()
            .find(|event| matches!(event, DaemonEvent::ProcessExit { .. }))
            .expect("idle fixture includes process exit")
            .clone();
        app.apply_response(events_response(vec![exit]));
        assert!(app.attached_session.is_none());
    }

    #[test]
    fn opaque_empty_snapshot_does_not_finish_visible_history_hydration() {
        let mut app = TuiApp::new(None);
        app.subscription_id = "sub-opaque".to_string();
        app.begin_attach_hydration("session-opaque", "sub-opaque");

        app.apply_response(events_response(vec![
            DaemonEvent::AttachState {
                session_id: "session-opaque".to_string(),
                subscription_id: "sub-opaque".to_string(),
                state: "attaching".to_string(),
            },
            DaemonEvent::Snapshot {
                session_id: "session-opaque".to_string(),
                subscription_id: "sub-opaque".to_string(),
                history: DaemonOpaqueHistoryPayload::from_bytes(&[0; 128]),
            },
            DaemonEvent::AttachState {
                session_id: "session-opaque".to_string(),
                subscription_id: "sub-opaque".to_string(),
                state: "attached".to_string(),
            },
        ]));

        assert!(app.terminal_output.is_empty());
        assert!(app.attach_hydration.is_some());
        assert_eq!(app.attached_session.as_deref(), Some("session-opaque"));
    }

    #[test]
    fn expired_empty_hydration_finishes_before_synthetic_screen_response_renders() {
        let mut app = TuiApp::new(None);
        app.subscription_id = "sub-captured".to_string();
        app.begin_attach_hydration("session-captured", "sub-captured");
        app.attach_hydration.as_mut().unwrap().deadline = Instant::now();
        app.attached_session = None;
        app.observed_requests.clear();

        app.apply_response(events_response(Vec::new()));
        assert!(app.attach_hydration.is_some());
        assert!(app.observed_requests.is_empty());

        app.apply_optional_readback_response(
            read_screen_response("session-captured", "restored screen"),
            "read_screen",
        );
        assert!(app.attach_hydration.is_none());
        assert_eq!(app.terminal_output, "restored screen");
        assert!(
            renderer::render_to_lines(&app.surface(), 120, 48)
                .0
                .join("\n")
                .contains("restored screen")
        );
    }

    #[test]
    fn late_screen_response_cannot_replace_restored_history() {
        let mut app = TuiApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());
        app.terminal_output = "ordered history".to_string();

        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "stale screen"),
            "read_screen",
        );

        assert_eq!(app.terminal_output, "ordered history");
    }

    #[test]
    fn read_screen_precedes_buffered_live_output_without_duplication() {
        let mut app = TuiApp::new(None);
        app.begin_attach_hydration("session-alpha", "sub-alpha");

        app.apply_response(events_response(vec![DaemonEvent::TerminalOutput {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-alpha".to_string(),
            data: "authoritative-live".to_string(),
        }]));
        assert!(app.terminal_output.is_empty());

        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "restored-first\n"),
            "read_screen",
        );

        assert_eq!(app.terminal_output, "restored-first\nauthoritative-live");
        let rendered = renderer::render_to_lines(&app.surface(), 120, 48)
            .0
            .join("\n");
        assert!(
            rendered.find("restored-first").unwrap() < rendered.find("authoritative-live").unwrap()
        );
        assert_eq!(rendered.matches("authoritative-live").count(), 1);
    }

    #[test]
    fn read_screen_overlap_is_not_duplicated_when_live_output_is_flushed() {
        let mut app = TuiApp::new(None);
        app.begin_attach_hydration("session-alpha", "sub-alpha");
        app.apply_response(events_response(vec![DaemonEvent::TerminalOutput {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-alpha".to_string(),
            data: "marker\r\n".to_string(),
        }]));

        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "prompt marker"),
            "read_screen",
        );

        assert_eq!(app.terminal_output, "prompt marker\r\n");
        assert_eq!(app.terminal_output.matches("marker").count(), 1);
    }

    #[test]
    fn snapshot_readback_is_metadata_only_and_renders_status() {
        let mut app = TuiApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());

        app.apply_optional_readback_response(
            capture_snapshot_response("session-alpha", 24, 80, Some("ghostty-page"), 4096),
            "capture_snapshot",
        );

        assert!(app.terminal_output.is_empty());
        let rendered = renderer::render_to_lines(&app.surface(), 120, 48)
            .0
            .join("\n");
        assert!(rendered.contains("rows=24 cols=80"));
        assert!(rendered.contains("format=ghostty-page"));
        assert!(rendered.contains("payload_bytes=4096"));
    }

    #[test]
    fn optional_readback_operator_error_is_non_fatal() {
        let mut app = TuiApp::new(None);
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
        let mut app = TuiApp::new(None);
        app.terminal_output_session_id = Some("session-alpha".to_string());
        app.terminal_output = "alpha history".to_string();
        app.snapshot_metadata = Some(DaemonCaptureSnapshot {
            session_id: "session-alpha".to_string(),
            rows: 24,
            cols: 80,
            payload_format: None,
            payload_bytes: 1,
        });

        app.begin_attach_hydration("session-alpha", "sub-alpha");
        assert!(app.terminal_output.is_empty());
        assert!(app.snapshot_metadata.is_none());
        assert_eq!(
            app.terminal_output_session_id.as_deref(),
            Some("session-alpha")
        );

        app.terminal_output = "replayed alpha history".to_string();

        app.begin_attach_hydration("session-beta", "sub-alpha");
        assert!(app.terminal_output.is_empty());
        assert!(app.snapshot_metadata.is_none());
        assert_eq!(
            app.terminal_output_session_id.as_deref(),
            Some("session-beta")
        );
    }

    #[test]
    fn process_exit_applies_same_response_bytes_and_suppresses_readbacks() {
        let mut app = TuiApp::new(None);
        app.subscription_id = "sub-alpha".to_string();
        app.begin_attach_hydration("session-alpha", "sub-alpha");
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
    fn process_exit_preserves_restored_screen_and_clears_snapshot_metadata() {
        let mut app = TuiApp::new(None);
        app.subscription_id = "sub-alpha".to_string();
        app.begin_attach_hydration("session-alpha", "sub-alpha");
        app.terminal_output = "last visible screen".to_string();
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

        assert_eq!(app.terminal_output, "last visible screen");
        assert!(app.snapshot_metadata.is_none());
        assert!(
            renderer::render_to_lines(&app.surface(), 120, 48)
                .0
                .join("\n")
                .contains("last visible screen")
        );
    }

    #[test]
    fn detach_preserves_restored_screen_and_clears_snapshot_metadata() {
        let mut app = TuiApp::new(None);
        app.begin_attach_hydration("session-alpha", "sub-test");
        app.apply_response(attach_state_response("session-alpha", "attached"));
        app.terminal_output_session_id = Some("session-alpha".to_string());
        app.terminal_output = "last visible screen".to_string();
        app.snapshot_metadata = Some(DaemonCaptureSnapshot {
            session_id: "session-alpha".to_string(),
            rows: 24,
            cols: 80,
            payload_format: None,
            payload_bytes: 1,
        });

        app.apply_response(attach_state_response("session-alpha", "detached"));

        assert_eq!(app.terminal_output, "last visible screen");
        assert!(app.snapshot_metadata.is_none());
    }

    #[test]
    fn opaque_history_events_do_not_mutate_existing_terminal_output() {
        let mut app = TuiApp::new(None);
        app.terminal_output = "existing output\n".to_string();

        app.apply_response(events_response(vec![
            DaemonEvent::Snapshot {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                history: DaemonOpaqueHistoryPayload::from_bytes(&[0; 128]),
            },
            DaemonEvent::Scrollback {
                session_id: "session-alpha".to_string(),
                subscription_id: "sub-test".to_string(),
                history: DaemonOpaqueHistoryPayload::from_bytes(&[255; 256]),
            },
        ]));

        assert_eq!(app.terminal_output, "existing output\n");
        assert!(app.error.is_none());
    }

    #[test]
    fn stale_opaque_history_cannot_replace_current_terminal_output() {
        let mut app = TuiApp::new(None);
        app.terminal_output = "current output".to_string();
        app.begin_attach_hydration("session-alpha", "sub-current");
        app.apply_optional_readback_response(
            read_screen_response("session-alpha", "current output"),
            "read_screen",
        );

        app.apply_response(events_response(vec![DaemonEvent::Snapshot {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-stale".to_string(),
            history: DaemonOpaqueHistoryPayload::from_bytes(b"stale opaque state"),
        }]));

        assert_eq!(app.terminal_output, "current output");
    }

    #[test]
    fn focused_session_list_row_updates_attach_selection() {
        let mut app = TuiApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-alpha".to_string());
        app.observed_requests.clear();
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let mut router = InputRouter::new(renderer::action_request_context());
        let second_row = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-session-session-beta")
            .expect("second session row should be focusable");

        let (column, row) = (second_row.rect.x, second_row.rect.y);
        let down_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        assert!(matches!(down_dispatch, InputDispatch::Focus { .. }));
        let up_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        assert!(matches!(
            up_dispatch,
            InputDispatch::Action(_) | InputDispatch::Focus { .. }
        ));
        app.handle_dispatch(up_dispatch);

        assert_eq!(app.selected_session.as_deref(), Some("session-beta"));
        assert_eq!(app.attached_session, None);
        assert!(
            !app.observed_requests
                .iter()
                .any(|request| matches!(request, ObservedRequest::Attach { .. }))
        );
    }

    #[test]
    fn session_click_cancels_when_redraw_reorders_another_row_under_release() {
        let mut app = TuiApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-alpha".to_string());
        let (_lines, frame_n_hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let first_row = frame_n_hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-session-session-alpha")
            .expect("alpha row should be hit-testable");
        let (column, row) = (first_row.rect.x, first_row.rect.y);
        let mut router = InputRouter::new(renderer::action_request_context());

        assert!(matches!(
            router.dispatch_event(
                mouse_event(
                    crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left,),
                    column,
                    row,
                ),
                &frame_n_hit_map,
            ),
            InputDispatch::Focus { .. }
        ));

        app.sessions.reverse();
        let (_lines, frame_n_plus_one_hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let moved_under_pointer = frame_n_plus_one_hit_map
            .lookup(column, row)
            .expect("reordered row should remain under the pointer");
        assert_eq!(moved_under_pointer.node_id, "tui-session-session-beta");

        assert_eq!(
            router.dispatch_event(
                mouse_event(
                    crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left,),
                    column,
                    row,
                ),
                &frame_n_plus_one_hit_map,
            ),
            InputDispatch::Ignored
        );
        assert_eq!(router.selected_row("tui-session-list"), None);
        app.sync_focused_session(router.selected_row_value("tui-session-list"));
        assert_eq!(app.selected_session.as_deref(), Some("session-alpha"));
    }

    #[test]
    fn focused_terminal_mouse_pair_attaches_once_and_preserves_key_forwarding() {
        let mut app = TuiApp::new(None);
        app.sessions = vec![SessionRow::running("session-alpha")];
        app.selected_session = Some("session-alpha".to_string());
        app.observed_requests.clear();
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let terminal = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-terminal")
            .expect("terminal should be focusable");
        let (column, row) = (
            terminal.rect.x.saturating_add(1),
            terminal.rect.y.saturating_add(1),
        );
        let mut router = InputRouter::new(renderer::action_request_context());

        let down_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        assert!(matches!(
            &down_dispatch,
            InputDispatch::Action(request)
                if request.action_id
                    == UiActionId("botster.terminal.focus".to_string())
        ));
        assert_eq!(router.focused_node_id(), Some("tui-terminal"));
        app.handle_dispatch(down_dispatch);

        let up_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        assert_eq!(up_dispatch, InputDispatch::Ignored);
        app.handle_dispatch(up_dispatch);
        assert_eq!(
            app.observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::Attach { .. }))
                .count(),
            1
        );

        app.attached_session = Some("session-alpha".to_string());
        app.attached_subscription_id = Some(app.subscription_id.clone());

        let dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            &hit_map,
        );
        assert_eq!(
            dispatch,
            InputDispatch::TerminalForward {
                node_id: "tui-terminal".to_string(),
                bytes: b"x".to_vec(),
            }
        );

        app.handle_dispatch(dispatch);
        assert!(app.observed_requests.contains(&ObservedRequest::SendInput {
            session_id: "session-alpha".to_string(),
            data: "x".to_string(),
        }));
    }

    #[test]
    fn mouse_mode_terminal_release_forwards_sgr_once_without_duplicate_focus_action() {
        let mut app = TuiApp::new(None);
        app.sessions = vec![SessionRow::running("session-alpha")];
        app.selected_session = Some("session-alpha".to_string());
        app.observed_requests.clear();
        let terminal = node(
            UiNodeKind::TerminalView,
            "mouse-mode-terminal",
            json!({ "session_id": "session-alpha" }),
        );
        terminal
            .validate()
            .expect("mouse-mode routing fixture should remain schema-valid");
        let (_lines, mut hit_map) = renderer::render_to_lines(&terminal, 40, 10);
        hit_map.set_terminal_mouse_mode("mouse-mode-terminal", 9);
        let region = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "mouse-mode-terminal")
            .expect("mouse-mode terminal should be hit-testable");
        let (column, row) = (
            region.rect.x.saturating_add(1),
            region.rect.y.saturating_add(1),
        );
        let mut router = InputRouter::new(renderer::action_request_context());

        let down_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        assert!(matches!(
            &down_dispatch,
            InputDispatch::Action(request)
                if request.action_id
                    == UiActionId("botster.terminal.focus".to_string())
        ));
        app.handle_dispatch(down_dispatch);
        assert_eq!(
            app.observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::Attach { .. }))
                .count(),
            1
        );

        app.attached_session = Some("session-alpha".to_string());
        app.attached_subscription_id = Some(app.subscription_id.clone());

        let up_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        let sgr_release = b"\x1b[<0;1;1m";
        assert_eq!(
            up_dispatch,
            InputDispatch::TerminalForward {
                node_id: "mouse-mode-terminal".to_string(),
                bytes: sgr_release.to_vec(),
            }
        );
        app.handle_dispatch(up_dispatch);

        assert_eq!(
            app.observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::Attach { .. }))
                .count(),
            1
        );
        assert_eq!(
            app.observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::SendInput { .. }))
                .count(),
            1
        );
        assert!(app.observed_requests.contains(&ObservedRequest::SendInput {
            session_id: "session-alpha".to_string(),
            data: String::from_utf8(sgr_release.to_vec()).expect("SGR release should be UTF-8"),
        }));
    }

    #[test]
    fn authoritative_mouse_mode_is_attachment_scoped_and_reapplied_after_render() {
        let mut app = TuiApp::new(None);
        app.attached_session = Some("session-alpha".to_string());
        app.attached_subscription_id = Some("sub-alpha".to_string());
        app.subscription_id = "sub-alpha".to_string();

        app.apply_optional_readback_response(
            mode_flags_response("session-alpha", 9),
            "read_mode_flags",
        );
        assert_eq!(app.current_terminal_mouse_mode(), 9);

        for _ in 0..2 {
            let (_lines, mut hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
            let terminal = hit_map
                .regions()
                .iter()
                .find(|region| region.node_id == "tui-terminal")
                .expect("production terminal should be hit-testable");
            assert!(!terminal.terminal_mouse_mode);

            app.apply_terminal_mouse_mode(&mut hit_map);
            let terminal = hit_map
                .regions()
                .iter()
                .find(|region| region.node_id == "tui-terminal")
                .expect("production terminal should still be hit-testable");
            assert!(terminal.terminal_mouse_mode);
        }

        app.apply_optional_readback_response(
            mode_flags_response("session-alpha", 0),
            "read_mode_flags",
        );
        assert_eq!(app.current_terminal_mouse_mode(), 0);

        app.apply_optional_readback_response(
            mode_flags_response("session-alpha", 9),
            "read_mode_flags",
        );

        app.apply_optional_readback_response(
            mode_flags_response("session-stale", 9),
            "read_mode_flags",
        );
        assert_eq!(app.current_terminal_mouse_mode(), 0);
    }

    #[test]
    fn terminal_output_refresh_is_bounded_and_malformed_readback_is_safe_off() {
        let mut app = TuiApp::new(None);
        app.attached_session = Some("session-alpha".to_string());
        app.attached_subscription_id = Some("sub-alpha".to_string());
        app.subscription_id = "sub-alpha".to_string();
        app.apply_optional_readback_response(
            mode_flags_response("session-alpha", 9),
            "read_mode_flags",
        );
        app.last_terminal_mouse_mode_probe = Some(Instant::now());

        app.apply_response(events_response(vec![DaemonEvent::TerminalOutput {
            session_id: "session-alpha".to_string(),
            subscription_id: "sub-alpha".to_string(),
            data: "output".to_string(),
        }]));
        assert!(app.terminal_mouse_mode_refresh_due);
        app.refresh_terminal_mouse_mode_if_due();
        assert!(app.terminal_mouse_mode_refresh_due);

        app.last_terminal_mouse_mode_probe =
            Some(Instant::now() - TERMINAL_MOUSE_MODE_REFRESH_INTERVAL);
        app.refresh_terminal_mouse_mode_if_due();
        assert!(!app.terminal_mouse_mode_refresh_due);

        app.apply_optional_readback_response(
            base_response(DaemonResponseKind::ReadModeFlags),
            "read_mode_flags",
        );
        assert_eq!(app.current_terminal_mouse_mode(), 0);
    }

    #[test]
    fn sgr_encoding_bit_alone_does_not_enable_terminal_mouse_tracking() {
        let mut app = TuiApp::new(None);
        app.attached_session = Some("session-alpha".to_string());
        app.attached_subscription_id = Some("sub-alpha".to_string());
        app.subscription_id = "sub-alpha".to_string();
        app.apply_optional_readback_response(
            mode_flags_response("session-alpha", 8),
            "read_mode_flags",
        );

        let (_lines, mut hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        app.apply_terminal_mouse_mode(&mut hit_map);
        let terminal = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-terminal")
            .expect("production terminal should be hit-testable");
        assert!(!terminal.terminal_mouse_mode);
    }

    #[test]
    fn authoritative_snapshot_preserves_selected_session_when_still_listed() {
        let mut app = TuiApp::new(None);
        app.session_entities
            .begin_generation("generation-1".to_string());
        app.selected_session = Some("session-beta".to_string());

        assert!(
            app.session_entities
                .apply(snapshot_frame(
                    "generation-1",
                    0,
                    vec![
                        session_entity("session-alpha", Some("running")),
                        session_entity("session-beta", Some("running")),
                    ],
                ))
                .expect("snapshot applies")
        );
        app.rebuild_session_rows();

        assert_eq!(
            app.sessions,
            session_rows([("session-alpha", "running"), ("session-beta", "running"),])
        );
        assert_eq!(app.selected_session.as_deref(), Some("session-beta"));
    }

    #[test]
    fn authoritative_snapshot_resets_stale_selection_without_attaching() {
        let mut app = TuiApp::new(None);
        app.session_entities
            .begin_generation("generation-1".to_string());
        app.selected_session = Some("session-beta".to_string());

        app.session_entities
            .apply(snapshot_frame(
                "generation-1",
                0,
                vec![
                    session_entity("session-delta", Some("running")),
                    session_entity("session-gamma", Some("running")),
                ],
            ))
            .expect("snapshot applies");
        app.rebuild_session_rows();

        assert_eq!(
            app.sessions,
            session_rows([("session-delta", "running"), ("session-gamma", "running"),])
        );
        assert_eq!(app.selected_session.as_deref(), Some("session-delta"));
        assert_eq!(app.attached_session, None);
    }

    #[test]
    fn entity_patch_preserves_and_renders_lifecycle_and_failure_state() {
        let mut app = TuiApp::new(None);
        app.session_entities
            .begin_generation("generation-1".to_string());
        app.session_entities
            .apply(snapshot_frame(
                "generation-1",
                0,
                vec![session_entity("session-alpha", Some("running"))],
            ))
            .expect("snapshot applies");
        app.session_entities
            .apply(DaemonEntityFrame::Patch {
                subscription_id: "generation-1".to_string(),
                entity_type: "session".to_string(),
                snapshot_seq: 1,
                id: "session-alpha".to_string(),
                patch: json!({
                    "lifecycle": "failed",
                    "failure_reason": "worker exited",
                    "updated_at": 2
                }),
            })
            .expect("patch applies");
        app.rebuild_session_rows();

        assert_eq!(
            app.sessions,
            vec![SessionRow {
                session_id: "session-alpha".to_string(),
                lifecycle: "failed".to_string(),
                failure_reason: Some("worker exited".to_string()),
                pending: false,
            }]
        );
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("session-alpha · failed"));
        assert!(rendered.contains("worker exited"));
    }

    #[test]
    fn action_dispatch_rejects_exited_session_before_daemon_attach() {
        let mut app = TuiApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "exited")]);
        app.selected_session = Some("session-beta".to_string());
        app.observed_requests.clear();

        app.handle_dispatch(InputDispatch::Action(
            botster_ui_contract::UiActionRequest {
                request_id: botster_ui_contract::UiActionRequestId("req-attach-exited".to_string()),
                surface_id: botster_ui_contract::UiSurfaceId(
                    renderer::WORKSPACE_SURFACE_ID.to_string(),
                ),
                action_id: botster_ui_contract::UiActionId("botster.tui.attach".to_string()),
                node_id: Some(UiNodeId("tui-session-session-beta-attach".to_string())),
                kind: botster_ui_contract::UiActionKind::Submit,
                values: None,
                payload: Some(json!({ "session_id": "session-beta" })),
            },
        ));

        assert!(app.observed_requests.is_empty());
        assert_eq!(
            app.error.as_deref(),
            Some("session-beta exited - cannot attach")
        );
    }

    #[test]
    fn exited_session_row_is_selectable_without_attach_affordance() {
        let mut app = TuiApp::new(None);
        app.sessions = vec![SessionRow {
            session_id: "session-beta".to_string(),
            lifecycle: "exited".to_string(),
            failure_reason: None,
            pending: false,
        }];
        app.selected_session = Some("session-beta".to_string());
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 120, 48);
        let mut router = InputRouter::new(renderer::action_request_context());
        let session_row = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-session-session-beta")
            .expect("exited session row should be focusable");

        let (column, row) = (session_row.rect.x, session_row.rect.y);
        let down_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        assert!(matches!(down_dispatch, InputDispatch::Focus { .. }));
        app.handle_dispatch(down_dispatch);
        let up_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        assert!(matches!(
            up_dispatch,
            InputDispatch::Action(_) | InputDispatch::Focus { .. }
        ));
        app.handle_dispatch(up_dispatch);
        let key_dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &hit_map,
        );
        app.handle_dispatch(key_dispatch);

        assert!(app.observed_requests.is_empty());
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("session-beta · exited"));
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| { region.node_id == "tui-session-session-beta-attach" })
        );
        assert!(!rendered.contains("attached session disappeared"));
    }

    #[test]
    fn terminal_focus_attach_rejects_non_running_session_before_daemon_attach() {
        let mut app = TuiApp::new(None);
        app.sessions = vec![SessionRow {
            session_id: "session-beta".to_string(),
            lifecycle: "stopped".to_string(),
            failure_reason: None,
            pending: false,
        }];
        app.selected_session = Some("session-beta".to_string());
        app.observed_requests.clear();

        app.handle_dispatch(InputDispatch::Action(
            botster_ui_contract::UiActionRequest {
                request_id: botster_ui_contract::UiActionRequestId(
                    "req-terminal-focus".to_string(),
                ),
                surface_id: botster_ui_contract::UiSurfaceId(
                    renderer::WORKSPACE_SURFACE_ID.to_string(),
                ),
                action_id: botster_ui_contract::UiActionId("botster.terminal.focus".to_string()),
                node_id: Some(UiNodeId("tui-terminal".to_string())),
                kind: botster_ui_contract::UiActionKind::Submit,
                values: None,
                payload: None,
            },
        ));

        assert!(app.observed_requests.is_empty());
        assert_eq!(
            app.error.as_deref(),
            Some("session-beta stopped - cannot attach")
        );
    }

    #[test]
    fn reconnect_does_not_auto_attach_known_non_running_session() {
        let mut app = TuiApp::new(None);
        app.sessions = vec![SessionRow {
            session_id: "session-beta".to_string(),
            lifecycle: "exited".to_string(),
            failure_reason: None,
            pending: false,
        }];
        app.selected_session = Some("session-beta".to_string());
        app.observed_requests.clear();

        assert!(app.observed_requests.is_empty());
        assert_eq!(app.attached_session, None);
    }

    #[test]
    fn refresh_read_models_does_not_list_sessions() {
        let mut app = TuiApp::new(None);
        app.observed_requests.clear();

        app.refresh_read_models();

        assert_eq!(
            app.observed_requests,
            vec![
                ObservedRequest::Status,
                ObservedRequest::ListApps,
                ObservedRequest::ListPackageNavigation,
                ObservedRequest::ListPackages,
            ]
        );
    }

    #[test]
    fn acceptance_request_audit_detects_legacy_list_sessions() {
        let mut audit = AcceptanceRequestAudit::default();

        audit.record(&DaemonRequest::ListSessions);

        assert_eq!(audit.list_sessions, 1);
    }

    #[test]
    fn reconnect_does_not_auto_attach_selected_running_session() {
        let mut app = TuiApp::new(None);
        app.observed_requests.clear();
        app.sessions = vec![SessionRow::running("session-alpha")];
        app.selected_session = Some("session-alpha".to_string());
        assert!(app.observed_requests.is_empty());
        assert_eq!(app.attached_session, None);
    }

    #[test]
    fn tui_hub_boundary_uses_public_client_without_private_protocol_plumbing() {
        let source = source_without_line_comments();

        assert!(source.contains("use botster_hub_client"));
        for required in [
            "connect_and_hello_with_requirement",
            "subscribe_session_entities",
            "DaemonEntityFrame",
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
        assert_eq!(
            source.matches(concat!("List", "Sessions")).count(),
            2,
            "the legacy list request may appear only in the acceptance audit and its positive control"
        );
    }

    #[test]
    fn headless_live_runtime_runs_against_isolated_hub_when_binaries_are_available() {
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
            .name("botster-tui-headless-live-runtime")
            .start()
            .expect("isolated hub starts");

        let lifecycle_report =
            botster_hub_test_support::run_session_lifecycle_subscription_conformance(&hub)
                .expect("session lifecycle subscription conformance passes");
        assert!(lifecycle_report.initial_snapshot_authoritative);
        assert!(lifecycle_report.spawn_upsert_observed);
        assert!(lifecycle_report.lifecycle_patch_observed);
        assert!(lifecycle_report.natural_exit_patch_observed);
        assert!(lifecycle_report.remove_observed);
        assert!(lifecycle_report.sequences_strictly_increasing);
        assert!(lifecycle_report.disconnect_cleanup_released_subscription);
        assert!(lifecycle_report.fresh_subscription_snapshot_authoritative);
        println!(
            "session-lifecycle-conformance: revision={} report={lifecycle_report:?}",
            botster_hub_test_support::session_lifecycle_subscription_conformance_scenario()
                .conformance_fixture_revision
        );

        run_headless_live_runtime(AppArgs {
            smoke: false,
            hub_connection: Some(RunnableEntrypointHubConnection {
                transport: RunnableEntrypointHubConnectionTransport::UnixSocket {
                    path: hub.endpoint().socket_path.to_string_lossy().into_owned(),
                },
            }),
            connection_error: None,
            hub_data_dir: Some(hub.data_dir().to_path_buf()),
            headless_live_runtime: true,
        })
        .expect("headless live-runtime surface completes a real Hub round trip");

        assert_live_attach_history_readback(&hub);

        let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let data_dir = hub.data_dir().to_string_lossy().to_string();
        let package_root = package_root.to_string_lossy().to_string();
        let package_install_output = std::process::Command::new(&hub_bin)
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
            package_install_output.status.success(),
            "packages install failed: stdout={} stderr={}",
            String::from_utf8_lossy(&package_install_output.stdout),
            String::from_utf8_lossy(&package_install_output.stderr)
        );
        println!("package-install: ok");

        let contract_matrix_fixture = std::env::var_os("BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE")
            .map(PathBuf::from)
            .expect(
                "BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE is required for live plugin-surface proof",
            );
        let plugin_report = botster_hub_test_support::run_plugin_contract_matrix_conformance(
            &hub,
            contract_matrix_fixture,
        )
        .expect("plugin contract matrix conformance passes");
        assert_plugin_contract_matrix_renders_through_tui(&hub, &plugin_report);

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
        println!("package-enable: ok");
        let app_open_output = std::process::Command::new(&hub_bin)
            .args([
                "apps",
                "open",
                "--data-dir",
                data_dir.as_str(),
                "botster-tui",
            ])
            .env("BOTSTER_TUI_HEADLESS_LIVE_RUNTIME", "1")
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
            app_open_stdout.contains("package-storage-context: configured"),
            "apps open stdout={} stderr={}",
            app_open_stdout,
            String::from_utf8_lossy(&app_open_output.stderr)
        );
        println!("package-open: typed Hub connection accepted");
        let mut requirement = tui_compatibility_requirement();
        requirement
            .required_features
            .push("botster-tui-future-feature".to_string());
        let error = connect_and_hello_with_requirement(hub.endpoint(), &requirement)
            .expect_err("live hub should reject unsatisfied TUI compatibility requirement");
        let mut app = TuiApp::new(None);
        app.record_transport_error(error);
        let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("compatibility mismatch"));
        assert!(rendered.contains("unsupported_feature"));
        assert!(rendered.contains("botster-tui-future-feature"));

        let unavailable_connection = serde_json::to_string(&RunnableEntrypointHubConnection {
            transport: RunnableEntrypointHubConnectionTransport::UnixSocket {
                path: root.join("missing-hub.sock").to_string_lossy().into_owned(),
            },
        })
        .expect("serialize unavailable Hub descriptor");
        let unavailable_args = AppArgs::parse_with_environment(
            Vec::<String>::new(),
            Some(unavailable_connection.into()),
            None,
            false,
        );
        let unavailable_app = TuiApp::new_with_connection(
            unavailable_args.daemon_endpoint(),
            unavailable_args.connection_error,
        );
        let (lines, _) = renderer::render_to_lines(&unavailable_app.surface(), 120, 48);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Hub unavailable"));
        assert!(rendered.contains("connection:"));

        hub.shutdown().expect("isolated hub shuts down cleanly");
    }

    #[test]
    fn installed_workspaces_spawn_driver_runs_through_apps_open() {
        let Some(hub_bin) = std::env::var_os("BOTSTER_HUB_BIN") else {
            skip_or_panic("BOTSTER_HUB_BIN");
            return;
        };
        let Some(session_worker_bin) = std::env::var_os("BOTSTER_SESSION_WORKER_BIN") else {
            skip_or_panic("BOTSTER_SESSION_WORKER_BIN");
            return;
        };
        let workspaces_path = PathBuf::from(
            std::env::var("BOTSTER_WORKSPACES_PACKAGE_PATH")
                .expect("BOTSTER_WORKSPACES_PACKAGE_PATH is required"),
        );
        validate_workspaces_package(&workspaces_path).expect("validate Workspaces package");

        let root = PathBuf::from(format!("/tmp/btid{}", short_suffix() % 1_000_000));
        std::fs::create_dir_all(&root).expect("create installed-driver fixture root");
        let repository = root.join("repository");
        std::fs::create_dir_all(repository.join(".botster"))
            .expect("create repo template directory");
        std::fs::create_dir_all(repository.join("bin")).expect("create repo bin directory");
        std::fs::write(
            repository.join("bin/acceptance-session.sh"),
            "#!/bin/sh\nwhile IFS= read -r line; do :; done\n",
        )
        .expect("write repo session template command");
        std::fs::write(
            repository.join(".botster/session-templates.json"),
            r#"{"session_templates":[{"id":"acceptance","command":"bin/acceptance-session.sh","working_directory":{"policy":"package_root"}}]}"#,
        )
        .expect("write repo session template");
        run_fixture_command(&repository, "chmod", &["+x", "bin/acceptance-session.sh"]);
        run_fixture_command(&repository, "git", &["init", "-b", "main"]);
        run_fixture_command(
            &repository,
            "git",
            &["config", "user.email", "acceptance@botster.dev"],
        );
        run_fixture_command(
            &repository,
            "git",
            &["config", "user.name", "Botster Acceptance"],
        );
        run_fixture_command(&repository, "git", &["add", "."]);
        run_fixture_command(&repository, "git", &["commit", "-m", "acceptance fixture"]);
        run_fixture_command(&repository, "git", &["branch", "feature/existing-worktree"]);
        run_fixture_command(&repository, "git", &["branch", "feature/existing-branch"]);

        let hub = botster_hub_test_support::IsolatedHubBuilder::new()
            .hub_bin(&hub_bin)
            .session_worker_bin(session_worker_bin)
            .root(root.join("hub"))
            .name("botster-tui-installed-workspaces-driver")
            .start()
            .expect("isolated Hub starts for installed driver");
        let mut client =
            HubConnection::connect(hub.endpoint()).expect("connect fixture Hub client");
        let target_id = "tgt_tui_acceptance";
        let target = client
            .request(&DaemonRequest::CreateSpawnTarget {
                target_id: Some(target_id.to_string()),
                label: Some("TUI acceptance".to_string()),
                root: repository.clone(),
                enabled: true,
                kind: Some("git".to_string()),
                base_ref: Some("main".to_string()),
                metadata: BTreeMap::new(),
            })
            .expect("create explicit Git spawn target");
        assert!(target.error.is_none(), "spawn target response: {target:?}");

        for package_path in [
            workspaces_path,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."),
        ] {
            let installed = client
                .request(&DaemonRequest::InstallPackageLocalPath { path: package_path })
                .expect("install package through public Hub request");
            assert!(installed.error.is_none(), "install response: {installed:?}");
        }
        for package_name in [WORKSPACES_PACKAGE, "botster-tui"] {
            let enabled = client
                .request(&DaemonRequest::EnablePackage {
                    package_name: package_name.to_string(),
                })
                .expect("enable installed package");
            assert!(enabled.error.is_none(), "enable response: {enabled:?}");
            let reloaded = client
                .request(&DaemonRequest::ReloadPackage {
                    package_name: package_name.to_string(),
                })
                .expect("reload enabled package");
            assert!(reloaded.error.is_none(), "reload response: {reloaded:?}");
        }
        let created = client
            .request(&DaemonRequest::PluginMcpCallTool {
                name: "botster_workspaces.create".to_string(),
                arguments: json!({ "name": "Installed driver workspace" }),
            })
            .expect("create Workspaces fixture through plugin MCP");
        assert_eq!(created.plugin_tool_result["ok"], true, "{created:?}");
        let workspace_id = created.plugin_tool_result["workspace"]["id"]
            .as_str()
            .expect("workspace id")
            .to_string();

        let managed_root = hub.data_dir().join("managed-worktrees").join(target_id);
        std::fs::create_dir_all(&managed_root).expect("create managed fixture root");
        let managed_root = managed_root
            .canonicalize()
            .expect("canonicalize managed fixture root");
        let existing_worktree = managed_root.join(hex_path_component("feature/existing-worktree"));
        run_fixture_command(
            &repository,
            "git",
            &[
                "worktree",
                "add",
                existing_worktree.to_str().expect("fixture path is UTF-8"),
                "feature/existing-worktree",
            ],
        );
        let branches = [
            (
                "existing-worktree",
                "feature/existing-worktree",
                existing_worktree,
            ),
            (
                "existing-branch",
                "feature/existing-branch",
                managed_root.join(hex_path_component("feature/existing-branch")),
            ),
            (
                "missing-branch",
                "feature/missing-branch",
                managed_root.join(hex_path_component("feature/missing-branch")),
            ),
        ];
        let cases = branches
            .iter()
            .map(|(case_id, branch, path)| {
                json!({
                    "case_id": case_id,
                    "target_id": target_id,
                    "branch": branch,
                    "resolution": case_id.replace('-', "_"),
                    "expected": {
                        "target_id": target_id,
                        "branch": branch,
                        "worktree_path": path.canonicalize().unwrap_or_else(|_| path.clone())
                    }
                })
            })
            .collect::<Vec<_>>();
        let scenario_path = root.join("scenario.json");
        let evidence_path = root.join("evidence.jsonl");
        std::fs::write(
            &scenario_path,
            serde_json::to_vec_pretty(&json!({
                "schema": crate::acceptance::SCHEMA,
                "workspace_id": workspace_id,
                "cases": cases
            }))
            .expect("serialize installed-driver scenario"),
        )
        .expect("write installed-driver scenario");
        let scenario_document: Value = serde_json::from_slice(
            &std::fs::read(&scenario_path).expect("read installed-driver scenario"),
        )
        .expect("decode installed-driver scenario");
        crate::acceptance::validate_contract_document(&scenario_document)
            .expect("installed-driver scenario matches published schema");

        let output = std::process::Command::new(&hub_bin)
            .args([
                "apps",
                "open",
                "--data-dir",
                hub.data_dir().to_str().expect("Hub data path is UTF-8"),
                "botster-tui",
            ])
            .env(crate::acceptance::SCENARIO_ENV, &scenario_path)
            .env(crate::acceptance::EVIDENCE_ENV, &evidence_path)
            .output()
            .expect("launch installed TUI package through apps open");
        assert!(
            output.status.success(),
            "installed driver failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let evidence = std::fs::read_to_string(&evidence_path).expect("read driver evidence");
        let records = evidence
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("evidence line is JSON"))
            .collect::<Vec<_>>();
        for record in &records {
            crate::acceptance::validate_contract_document(record)
                .expect("driver-produced evidence matches published schema");
        }
        let fixture_records = include_str!("../fixtures/workspaces-spawn-driver-v1.evidence.jsonl")
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).expect("fixture evidence line is JSON")
            });
        for fixture in fixture_records {
            let fixture_payload_keys = fixture["payload"]
                .as_object()
                .expect("fixture payload is an object")
                .keys()
                .collect::<std::collections::BTreeSet<_>>();
            assert!(
                records.iter().any(|record| {
                    record["kind"] == fixture["kind"]
                        && record.get("case_id").is_some() == fixture.get("case_id").is_some()
                        && record["payload"].as_object().is_some_and(|payload| {
                            payload.keys().collect::<std::collections::BTreeSet<_>>()
                                == fixture_payload_keys
                        })
                }),
                "canonical fixture record is not shaped like producer output: {fixture}"
            );
        }
        assert_eq!(
            records
                .iter()
                .filter(|record| record["kind"] == "complete")
                .count(),
            1
        );
        assert_eq!(
            records
                .iter()
                .filter(|record| record["kind"] == "case_complete")
                .count(),
            3
        );
        assert!(
            records
                .iter()
                .all(|record| record["schema"] == crate::acceptance::SCHEMA)
        );
        let mut failure_scenario = scenario_document;
        failure_scenario["workspace_id"] = json!("workspace-not-rendered");
        let failure_scenario_path = root.join("failure-scenario.json");
        let failure_evidence_path = root.join("failure-evidence.jsonl");
        std::fs::write(
            &failure_scenario_path,
            serde_json::to_vec_pretty(&failure_scenario)
                .expect("serialize bounded-failure scenario"),
        )
        .expect("write bounded-failure scenario");
        let failure_output = std::process::Command::new(&hub_bin)
            .args([
                "apps",
                "open",
                "--data-dir",
                hub.data_dir().to_str().expect("Hub data path is UTF-8"),
                "botster-tui",
            ])
            .env(crate::acceptance::SCENARIO_ENV, &failure_scenario_path)
            .env(crate::acceptance::EVIDENCE_ENV, &failure_evidence_path)
            .output()
            .expect("launch installed TUI bounded-failure case");
        assert!(!failure_output.status.success());
        let failure_records = std::fs::read_to_string(&failure_evidence_path)
            .expect("read bounded-failure evidence")
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("failure line is JSON"))
            .collect::<Vec<_>>();
        let failure = failure_records.last().expect("terminal failure record");
        crate::acceptance::validate_contract_document(failure)
            .expect("driver-produced failure matches published schema");
        assert_eq!(failure["kind"], "failure");
        assert_eq!(failure["payload"]["phase"], "initial_surface_open");
        assert!(failure["payload"]["subscription_id"].is_string());
        assert!(failure["payload"]["snapshot_seq"].is_number());
        assert!(failure["payload"]["surface_render_count"].is_number());
        assert!(
            failure["payload"]["focusable_ids"]
                .as_array()
                .is_some_and(|ids| !ids.is_empty())
        );
        println!("installed-workspaces-driver: complete cases=3");
        hub.shutdown().expect("installed-driver Hub shuts down");
    }

    fn run_fixture_command(directory: &Path, program: &str, args: &[&str]) {
        let output = std::process::Command::new(program)
            .args(args)
            .current_dir(directory)
            .output()
            .unwrap_or_else(|error| panic!("run {program} {args:?}: {error}"));
        assert!(
            output.status.success(),
            "{program} {args:?} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn hex_path_component(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    #[test]
    fn workspaces_live_acceptance_runs_against_real_package() {
        let Ok(profile_value) = std::env::var("BOTSTER_TUI_WORKSPACES_PROFILE") else {
            if std::env::var_os("BOTSTER_TUI_REQUIRE_HUB_TEST").is_some() {
                panic!("BOTSTER_TUI_WORKSPACES_PROFILE is required");
            }
            eprintln!("skipping Workspaces live acceptance; no explicit profile selected");
            return;
        };
        let profile = WorkspacesProfile::parse(&profile_value)
            .expect("select an explicit Workspaces acceptance profile");
        let mut ledger = WorkspacesLedger::new(profile);
        let package_path = validate_workspaces_package(Path::new(
            &std::env::var("BOTSTER_WORKSPACES_PACKAGE_PATH")
                .expect("BOTSTER_WORKSPACES_PACKAGE_PATH is required"),
        ))
        .expect("validate the explicit real Workspaces package checkout");
        ledger.record(WorkspacesStage::PackageValidated);

        let Some(hub_bin) = std::env::var_os("BOTSTER_HUB_BIN") else {
            skip_or_panic("BOTSTER_HUB_BIN");
            return;
        };
        let Some(session_worker_bin) = std::env::var_os("BOTSTER_SESSION_WORKER_BIN") else {
            skip_or_panic("BOTSTER_SESSION_WORKER_BIN");
            return;
        };
        let root = PathBuf::from(format!("/tmp/btw{}", short_suffix() % 1_000_000));
        let hub = botster_hub_test_support::IsolatedHubBuilder::new()
            .hub_bin(hub_bin)
            .session_worker_bin(session_worker_bin)
            .root(&root)
            .name("botster-tui-workspaces-live-acceptance")
            .start()
            .expect("isolated hub starts for Workspaces acceptance");
        let mut client = HubConnection::connect(hub.endpoint()).expect("connect to isolated hub");

        let installed = client
            .request(&DaemonRequest::InstallPackageLocalPath {
                path: package_path.clone(),
            })
            .expect("install the explicit Workspaces package through the public Hub request");
        assert_eq!(installed.kind, DaemonResponseKind::PackageDecision);
        assert!(installed.error.is_none(), "install response: {installed:?}");
        ledger.record(WorkspacesStage::PackageInstalled);

        let enabled = client
            .request(&DaemonRequest::EnablePackage {
                package_name: WORKSPACES_PACKAGE_NAME.to_string(),
            })
            .expect("enable the installed Workspaces package through the public Hub request");
        assert_eq!(enabled.kind, DaemonResponseKind::PackageDecision);
        assert!(enabled.error.is_none(), "enable response: {enabled:?}");
        let reloaded = client
            .request(&DaemonRequest::ReloadPackage {
                package_name: WORKSPACES_PACKAGE_NAME.to_string(),
            })
            .expect("reload the enabled Workspaces package through the public Hub request");
        assert_eq!(reloaded.kind, DaemonResponseKind::PackageDecision);
        assert!(reloaded.error.is_none(), "reload response: {reloaded:?}");
        ledger.record(WorkspacesStage::PackageEnabledAndReloaded);

        let created = client
            .request(&DaemonRequest::PluginMcpCallTool {
                name: "botster_workspaces.create".to_string(),
                arguments: json!({ "name": "TUI acceptance workspace" }),
            })
            .expect("seed a workspace through the real plugin-worker MCP boundary");
        assert_eq!(created.kind, DaemonResponseKind::PluginMcpToolResult);
        assert_eq!(created.plugin_tool_result["ok"], true, "{created:?}");
        let workspace_id = created.plugin_tool_result["workspace"]["id"]
            .as_str()
            .expect("Workspaces create returns the owner-generated workspace id")
            .to_string();

        let reference_count = match profile {
            WorkspacesProfile::Plumbing => 1,
            WorkspacesProfile::Lifecycle => 16,
        };
        let session_ids = (1..=reference_count)
            .map(|index| format!("00000000-0000-4000-8000-{index:012x}"))
            .collect::<Vec<_>>();
        for session_id in &session_ids {
            let added = client
                .request(&DaemonRequest::PluginMcpCallTool {
                    name: "botster_workspaces.add_session".to_string(),
                    arguments: json!({
                        "workspace_id": workspace_id,
                        "session_id": session_id,
                    }),
                })
                .expect("seed a deliberate session reference through the plugin MCP tool");
            assert_eq!(added.kind, DaemonResponseKind::PluginMcpToolResult);
            assert_eq!(added.plugin_tool_result["ok"], true, "{added:?}");
        }

        let packages = client
            .request(&DaemonRequest::ListPackages)
            .expect("list packages after Workspaces enablement");
        let apps = client
            .request(&DaemonRequest::ListApps)
            .expect("list apps after Workspaces enablement");
        let navigation = client
            .request(&DaemonRequest::ListPackageNavigation)
            .expect("list admitted Workspaces navigation");
        let mut app = TuiApp::new(Some(hub.endpoint().clone()));
        app.workspace_test_mode = true;
        app.system_details_visible = true;
        app.apply_response(packages);
        app.apply_response(apps);
        app.apply_response(navigation);
        let mut expected_historical_keyboard_node_id = None;
        if profile == WorkspacesProfile::Lifecycle {
            for session_id in &session_ids[..2] {
                wait_for_session_entity_expectation(
                    &mut app,
                    session_id,
                    SessionEntityExpectation::Absent,
                    "controlled Workspaces session must be absent from the pre-spawn baseline",
                );
            }
            for session_id in &session_ids[..2] {
                client
                    .request(&DaemonRequest::Spawn {
                        session_id: session_id.clone(),
                        command: "while IFS= read -r line; do :; done".to_string(),
                    })
                    .expect("spawn a controlled authoritative Hub session");
            }
            for session_id in &session_ids[..2] {
                wait_for_session_entity_expectation(
                    &mut app,
                    session_id,
                    SessionEntityExpectation::Lifecycle("current"),
                    "controlled Workspaces session must become authoritative before surface open",
                );
            }
        } else {
            let snapshot_deadline = Instant::now() + Duration::from_secs(7);
            while !app.session_entities.has_snapshot && Instant::now() < snapshot_deadline {
                app.poll_hub();
                thread::yield_now();
            }
            assert!(
                app.session_entities.has_snapshot,
                "Workspaces mode requires an authoritative session snapshot"
            );
        }
        app.observed_requests.clear();

        let navigation_index = app
            .package_navigation
            .iter()
            .position(|entry| {
                entry.package_name == WORKSPACES_PACKAGE_NAME
                    && entry.target.surface_id.as_deref() == Some(WORKSPACES_SURFACE_ID)
            })
            .expect("Hub admits the real Workspaces navigation entry");
        let (_navigation_lines, navigation_hits) =
            renderer::render_to_lines(&app.surface(), 500, 240);
        app.handle_dispatch(click_dispatch(
            &navigation_hits,
            &format!("tui-package-navigation-{navigation_index}-open"),
        ));
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::PluginSurfaceRender {
                    package_name: WORKSPACES_PACKAGE_NAME.to_string(),
                    surface_id: WORKSPACES_SURFACE_ID.to_string(),
                })
        );
        ledger.record(WorkspacesStage::NavigationOpened);

        let owner_surface = app
            .plugin_surface
            .clone()
            .expect("navigation applies the owner-authored Workspaces surface");
        assert_eq!(owner_surface.package_name, WORKSPACES_PACKAGE_NAME);
        assert_eq!(owner_surface.surface_id, WORKSPACES_SURFACE_ID);
        let index_rendered = renderer::render_to_lines(&app.surface(), 500, 240)
            .0
            .join("\n");
        assert!(
            index_rendered.contains("TUI acceptance workspace"),
            "{index_rendered}"
        );
        ledger.record(WorkspacesStage::OwnerIndexRendered);

        let row_node = find_action_node(
            &owner_surface.body,
            "botster_workspaces.open",
            "selected_workspace",
            &workspace_id,
        )
        .expect("discover the owner-authored workspace row from delivered action metadata");
        let row_node_id = row_node
            .id
            .as_ref()
            .and_then(UiAuthoredNodeId::as_literal)
            .expect("current Workspaces row identity is a producer-authored literal")
            .clone();
        let row_action = node_action(row_node);
        let (_lines, row_hits) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            500,
            240,
            &RenderState::default(),
            &app.plugin_presentation,
        );
        let row_dispatch =
            click_dispatch_for_surface(&row_hits, &row_node_id.0, Some(WORKSPACES_SURFACE_ID));
        let row_request = match &row_dispatch {
            InputDispatch::Action(request) => request.clone(),
            other => panic!("real workspace row must dispatch an action, got {other:?}"),
        };
        assert_eq!(row_request.action_id, row_action.id);
        assert_eq!(row_request.node_id, Some(row_node_id));
        assert_eq!(row_request.payload, row_action.payload);
        app.handle_dispatch(row_dispatch);
        assert_eq!(
            app.plugin_action_result.as_ref().map(|result| result.state),
            Some(botster_ui_contract::UiActionResultState::Accepted)
        );
        ledger.record(WorkspacesStage::OwnerRowSelected);
        ledger.record(WorkspacesStage::MouseDispatch);
        ledger.record(WorkspacesStage::LiteralActionIdentityObserved);
        ledger.record(WorkspacesStage::AcceptedOwnerAction);

        let detail_rendered = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            500,
            240,
            &RenderState::default(),
            &app.plugin_presentation,
        )
        .0
        .join("\n");
        assert!(
            detail_rendered.contains(&session_ids[0]),
            "{detail_rendered}"
        );
        ledger.record(WorkspacesStage::OwnerDetailRendered);

        if profile == WorkspacesProfile::Lifecycle {
            let bindings = collect_session_bindings(&owner_surface.body);
            for session_id in &session_ids {
                let reference_bindings = bindings
                    .iter()
                    .filter(|binding| binding.session_uuid() == Some(session_id))
                    .count();
                assert!(
                    reference_bindings <= 4,
                    "retained reference {session_id} exceeds the approved four-bindings-per-reference ceiling with {reference_bindings} bindings"
                );
            }
            for session_id in &session_ids {
                let current = session_binding(&bindings, session_id, Some("current"));
                let ended = session_binding(&bindings, session_id, Some("ended"));
                let indeterminate = session_binding(&bindings, session_id, Some("indeterminate"));
                let absence = session_binding(&bindings, session_id, None);
                assert!(current.empty_template.is_none());
                assert!(ended.empty_template.is_none());
                assert!(indeterminate.empty_template.is_none());
                assert!(absence.empty_template.is_some());
            }
            ledger.record(WorkspacesStage::SixteenReferenceScale);

            let current_root = materialized_plugin_root(&app);
            collect_realized_node_ids(&current_root)
                .expect("current Workspaces render has unique realized identity");
            for session_id in &session_ids[..2] {
                assert_binding_realization(
                    &current_root,
                    session_binding(&bindings, session_id, Some("current")),
                    true,
                    false,
                );
                assert_binding_realization(
                    &current_root,
                    session_binding(&bindings, session_id, Some("ended")),
                    false,
                    false,
                );
                assert_binding_realization(
                    &current_root,
                    session_binding(&bindings, session_id, Some("indeterminate")),
                    false,
                    false,
                );
                assert_binding_realization(
                    &current_root,
                    session_binding(&bindings, session_id, None),
                    true,
                    false,
                );
            }
            for session_id in &session_ids[2..] {
                for lifecycle_class in ["current", "ended", "indeterminate"] {
                    assert_binding_realization(
                        &current_root,
                        session_binding(&bindings, session_id, Some(lifecycle_class)),
                        false,
                        false,
                    );
                }
                assert_binding_realization(
                    &current_root,
                    session_binding(&bindings, session_id, None),
                    false,
                    true,
                );
            }
            assert_realized_roots_follow_reference_order(
                &current_root,
                session_ids[..2].iter().map(|session_id| {
                    session_binding(&bindings, session_id, Some("current"))
                        .item_root_id()
                        .expect("literal current root")
                        .to_string()
                }),
            );
            assert_realized_roots_follow_reference_order(
                &current_root,
                session_ids[2..].iter().map(|session_id| {
                    session_binding(&bindings, session_id, None)
                        .empty_root_id()
                        .expect("literal absent root")
                        .to_string()
                }),
            );
            ledger.record(WorkspacesStage::CurrentRendered);
            ledger.record(WorkspacesStage::AbsentRendered);
            ledger.record(WorkspacesStage::CanonicalItemRootIdentityObserved);

            let absence_binding = session_binding(&bindings, &session_ids[0], None);
            let absence_item_id = absence_binding
                .item_root_id()
                .expect("absence binding item template has literal producer identity");
            let absence_item = find_ui_node_by_id(&current_root, absence_item_id)
                .expect("present reference realizes absence-detection item template");
            let (absence_lines, absence_hits) = renderer::render_to_lines(absence_item, 80, 8);
            assert!(
                absence_lines.join("\n").trim().is_empty(),
                "presence detector item template must be visually inert"
            );
            assert!(
                absence_hits
                    .regions()
                    .iter()
                    .all(|region| region.node_id != absence_item_id),
                "presence detector item template must not publish a hit region"
            );
            ledger.record(WorkspacesStage::AbsenceTemplateInert);

            let descriptor_before_transition = owner_surface.body.clone();
            let render_requests_before_transition = app
                .observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::PluginSurfaceRender { .. }))
                .count();
            client
                .request(&DaemonRequest::ShutdownSession {
                    session_id: session_ids[1].clone(),
                })
                .expect("end the controlled referenced session through Hub authority");
            wait_for_session_entity_expectation(
                &mut app,
                &session_ids[1],
                SessionEntityExpectation::Lifecycle("ended"),
                "controlled Workspaces session must become authoritative ended state",
            );
            assert_eq!(
                app.session_entities.entities[&session_ids[1]].lifecycle_class,
                "ended"
            );
            assert_eq!(
                app.plugin_surface
                    .as_ref()
                    .expect("surface remains active")
                    .body,
                descriptor_before_transition,
                "entity lifecycle transition must not replace the owner-authored surface tree"
            );
            assert_eq!(
                app.observed_requests
                    .iter()
                    .filter(|request| matches!(
                        request,
                        ObservedRequest::PluginSurfaceRender { .. }
                    ))
                    .count(),
                render_requests_before_transition,
                "entity lifecycle transition must not request a fresh surface"
            );
            let ended_root = materialized_plugin_root(&app);
            collect_realized_node_ids(&ended_root)
                .expect("ended Workspaces render has unique realized identity");
            assert_binding_realization(
                &ended_root,
                session_binding(&bindings, &session_ids[1], Some("current")),
                false,
                false,
            );
            assert_binding_realization(
                &ended_root,
                session_binding(&bindings, &session_ids[1], Some("ended")),
                true,
                false,
            );
            assert_binding_realization(
                &ended_root,
                session_binding(&bindings, &session_ids[1], None),
                true,
                false,
            );
            ledger.record(WorkspacesStage::EndedRendered);
            ledger.record(WorkspacesStage::TransitionWithoutListOrSurfaceRefresh);

            client
                .request(&DaemonRequest::ShutdownSession {
                    session_id: session_ids[0].clone(),
                })
                .expect("end the controlled session before removing Hub history");
            wait_for_session_entity_expectation(
                &mut app,
                &session_ids[0],
                SessionEntityExpectation::Lifecycle("ended"),
                "controlled Workspaces session must end before history removal",
            );
            client
                .request(&DaemonRequest::RemoveSession {
                    session_id: session_ids[0].clone(),
                })
                .expect("remove one controlled Hub session while retaining workspace history");
            wait_for_session_entity_expectation(
                &mut app,
                &session_ids[0],
                SessionEntityExpectation::Absent,
                "controlled Workspaces session must be authoritatively removed",
            );
            assert!(!app.session_entities.entities.contains_key(&session_ids[0]));
            let absent_root = materialized_plugin_root(&app);
            collect_realized_node_ids(&absent_root)
                .expect("absent Workspaces render has unique realized identity");
            for lifecycle_class in ["current", "ended", "indeterminate"] {
                assert_binding_realization(
                    &absent_root,
                    session_binding(&bindings, &session_ids[0], Some(lifecycle_class)),
                    false,
                    false,
                );
            }
            assert_binding_realization(
                &absent_root,
                session_binding(&bindings, &session_ids[0], None),
                false,
                true,
            );

            let old_generation = app
                .session_entities
                .subscription_id
                .clone()
                .expect("pre-reconnect session generation exists");
            app.force_reconnect();
            let reconnect_deadline = Instant::now() + Duration::from_secs(7);
            while (!app.session_entities.has_snapshot
                || app.session_entities.subscription_id.as_deref() == Some(old_generation.as_str()))
                && Instant::now() < reconnect_deadline
            {
                app.poll_hub();
                thread::yield_now();
            }
            assert_ne!(
                app.session_entities.subscription_id.as_deref(),
                Some(old_generation.as_str())
            );
            assert!(app.session_entities.has_snapshot);
            ledger.record(WorkspacesStage::FreshReconnectSubscription);
            ledger.record(WorkspacesStage::FreshReconnectSnapshot);
            wait_for_session_entity_expectation(
                &mut app,
                &session_ids[1],
                SessionEntityExpectation::Lifecycle("ended"),
                "reconnect must rehydrate the exact controlled session in its authoritative ended state",
            );

            let stale_seq = app.session_entities.snapshot_seq.unwrap_or_default() + 1;
            assert!(
                !app.session_entities
                    .apply(DaemonEntityFrame::Patch {
                        subscription_id: old_generation,
                        entity_type: "session".to_string(),
                        snapshot_seq: stale_seq,
                        id: session_ids[1].clone(),
                        patch: json!({ "lifecycle_class": "current" }),
                    })
                    .expect("stale prior-generation patch is rejected")
            );
            ledger.record(WorkspacesStage::StaleGenerationRejected);

            let navigation_deadline = Instant::now() + Duration::from_secs(7);
            while !app.package_navigation.iter().any(|entry| {
                entry.package_name == WORKSPACES_PACKAGE_NAME
                    && entry.target.surface_id.as_deref() == Some(WORKSPACES_SURFACE_ID)
            }) && Instant::now() < navigation_deadline
            {
                app.poll_hub();
                thread::yield_now();
            }
            app.system_details_visible = true;
            let navigation_index = app
                .package_navigation
                .iter()
                .position(|entry| {
                    entry.package_name == WORKSPACES_PACKAGE_NAME
                        && entry.target.surface_id.as_deref() == Some(WORKSPACES_SURFACE_ID)
                })
                .expect("reconnect refreshes admitted Workspaces navigation");
            let (_lines, reconnect_navigation_hits) =
                renderer::render_to_lines(&app.surface(), 500, 240);
            app.handle_dispatch(click_dispatch(
                &reconnect_navigation_hits,
                &format!("tui-package-navigation-{navigation_index}-open"),
            ));
            let reopened_surface = app
                .plugin_surface
                .clone()
                .expect("reconnect explicitly pulls the Workspaces surface");
            let reopened_row = find_action_node(
                &reopened_surface.body,
                "botster_workspaces.open",
                "selected_workspace",
                &workspace_id,
            )
            .expect("reopened surface retains the workspace row");
            let reopened_row_id = reopened_row
                .id
                .as_ref()
                .and_then(UiAuthoredNodeId::as_literal)
                .expect("reopened workspace row has literal identity")
                .0
                .clone();
            let (_lines, reopened_hits) = renderer::render_to_lines_with_presentation_state(
                &app.surface(),
                500,
                240,
                &RenderState::default(),
                &app.plugin_presentation,
            );
            app.handle_dispatch(click_dispatch_for_surface(
                &reopened_hits,
                &reopened_row_id,
                Some(WORKSPACES_SURFACE_ID),
            ));
            ledger.record(WorkspacesStage::SurfaceReopened);

            let rehydrated_root = materialized_plugin_root(&app);
            collect_realized_node_ids(&rehydrated_root)
                .expect("rehydrated Workspaces render has unique realized identity");
            assert_binding_realization(
                &rehydrated_root,
                session_binding(&bindings, &session_ids[1], Some("ended")),
                true,
                false,
            );
            assert_binding_realization(
                &rehydrated_root,
                session_binding(&bindings, &session_ids[0], None),
                false,
                true,
            );
            let historical_keyboard_binding = session_binding(&bindings, &session_ids[2], None);
            assert_binding_realization(&rehydrated_root, historical_keyboard_binding, false, true);
            expected_historical_keyboard_node_id = Some(UiNodeId(
                historical_keyboard_binding
                    .empty_root_id()
                    .expect("historical keyboard binding has a literal empty root")
                    .to_string(),
            ));
            ledger.record(WorkspacesStage::HistoricalReferencesRehydrated);
        }

        let active_surface = app
            .plugin_surface
            .clone()
            .expect("Workspaces retains its owner surface after row selection");
        let mut router =
            InputRouter::new(renderer::action_request_context_for(WORKSPACES_SURFACE_ID));
        let (_lines, keyboard_hits) = renderer::render_to_lines_with_presentation_state(
            &app.surface(),
            500,
            240,
            &router.render_state(),
            &app.plugin_presentation,
        );
        let (keyboard_node_id, keyboard_action, keyboard_expectation) = match profile {
            WorkspacesProfile::Plumbing => {
                let keyboard_node = find_action_node(
                    &active_surface.body,
                    "botster_workspaces.open",
                    "dialog",
                    &format!("rename:{workspace_id}"),
                )
                .expect("discover an owner-authored detail action from the delivered tree");
                (
                    keyboard_node
                        .id
                        .as_ref()
                        .and_then(UiAuthoredNodeId::as_literal)
                        .expect("owner-authored keyboard action has literal identity")
                        .clone(),
                    node_action(keyboard_node),
                    None,
                )
            }
            WorkspacesProfile::Lifecycle => {
                let (node_id, action) = unique_hit_action(
                    &keyboard_hits,
                    "botster_workspaces.remove_session",
                    "session_id",
                    &session_ids[2],
                )
                .expect("discover one exact membership action in the production hit map");
                assert_eq!(
                    Some(&node_id),
                    expected_historical_keyboard_node_id.as_ref(),
                    "lifecycle keyboard action belongs to the realized absent reference root"
                );
                (node_id, action, Some(&session_ids[2]))
            }
        };
        router.reconcile(&keyboard_hits);
        let keyboard_region = keyboard_hits
            .regions()
            .iter()
            .find(|region| region.node_id == keyboard_node_id.0)
            .unwrap_or_else(|| {
                panic!(
                    "owner-authored keyboard action {} is in the production hit map; regions={:?}",
                    keyboard_node_id.0,
                    keyboard_hits
                        .regions()
                        .iter()
                        .map(|region| region.node_id.as_str())
                        .collect::<Vec<_>>()
                )
            });
        let focus_dispatch = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                keyboard_region.rect.x,
                keyboard_region.rect.y,
            ),
            &keyboard_hits,
        );
        assert!(matches!(focus_dispatch, InputDispatch::Focus { .. }));
        assert_eq!(router.focused_node_id(), Some(keyboard_node_id.0.as_str()));
        let keyboard_dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &keyboard_hits,
        );
        let keyboard_request = match &keyboard_dispatch {
            InputDispatch::Action(request) => request.clone(),
            other => panic!("focused owner action must dispatch, got {other:?}"),
        };
        assert_eq!(keyboard_request.action_id, keyboard_action.id);
        assert_eq!(keyboard_request.node_id, Some(keyboard_node_id));
        assert_eq!(keyboard_request.payload, keyboard_action.payload);
        app.handle_dispatch(keyboard_dispatch);
        assert_eq!(
            app.plugin_action_result.as_ref().map(|result| result.state),
            Some(botster_ui_contract::UiActionResultState::Accepted)
        );
        if let Some(removed_membership_id) = keyboard_expectation {
            let (_lines, after_remove_hits) = renderer::render_to_lines_with_presentation_state(
                &app.surface(),
                500,
                240,
                &RenderState::default(),
                &app.plugin_presentation,
            );
            unique_hit_action(
                &after_remove_hits,
                "botster_workspaces.remove_session",
                "session_id",
                &session_ids[0],
            )
            .expect(
                "retained historical references still publish their exact removal action after an unrelated removal",
            );
            let removed = unique_hit_action(
                &after_remove_hits,
                "botster_workspaces.remove_session",
                "session_id",
                removed_membership_id,
            )
            .expect_err("accepted removal eliminates the exact membership action");
            assert!(removed.contains("found 0"), "{removed}");
        }
        ledger.record(WorkspacesStage::KeyboardDispatch);

        hub.shutdown()
            .expect("Workspaces acceptance isolated hub shuts down cleanly");
        ledger.record(WorkspacesStage::CleanShutdown);
        ledger
            .assert_complete()
            .expect("selected Workspaces ledger completes");
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
                command: format!(
                    "printf '{prior_marker}\\n'; printf '\\033[?1000h\\033[?1006h'; while IFS= read -r line; do case \"$line\" in enable-mouse) printf '\\033[?1000h\\033[?1006h' ;; disable-mouse) printf '\\033[?1000l\\033[?1006l' ;; esac; done"
                ),
            })
            .expect("spawn history-producing session before TUI attach");
        thread::yield_now();

        let mut app = TuiApp::new(Some(hub.endpoint().clone()));
        app.workspace_test_mode = true;
        wait_for_authoritative_session(&mut app, &prior_session_id)
            .expect("external session appears through entity subscription");
        app.selected_session = Some(prior_session_id.clone());
        app.observed_requests.clear();
        app.attach_selected_or_first();
        let first_subscription_id = app.subscription_id.clone();
        wait_for_app_output(&mut app, &prior_marker).expect("late TUI attach renders prior output");
        let hydration_deadline = Instant::now() + Duration::from_secs(7);
        while app.attach_hydration.is_some() && Instant::now() < hydration_deadline {
            app.poll_hub();
            thread::yield_now();
        }
        assert_eq!(
            app.terminal_output.matches(&prior_marker).count(),
            1,
            "initial restoration duplicated prior output: {:?}",
            app.terminal_output
        );
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::CaptureSnapshot(prior_session_id.clone()))
        );
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::ReadScreen(prior_session_id.clone()))
        );
        assert!(app.observed_requests.iter().any(
            |request| matches!(request, ObservedRequest::Drain(id) if id == &prior_session_id)
        ));
        assert!(app.observed_requests.iter().all(|request| {
            !matches!(request, ObservedRequest::Drain(id) if id != &prior_session_id)
        }));

        let attached_deadline = Instant::now() + Duration::from_secs(7);
        while app.attached_session.as_deref() != Some(prior_session_id.as_str())
            && Instant::now() < attached_deadline
        {
            app.poll_hub();
            thread::yield_now();
        }
        assert_eq!(
            app.attached_session.as_deref(),
            Some(prior_session_id.as_str()),
            "TUI must observe Attached before forwarding terminal input"
        );
        let mode_on_deadline = Instant::now() + Duration::from_secs(3);
        while app.current_terminal_mouse_mode() != 9 && Instant::now() < mode_on_deadline {
            app.poll_hub();
            thread::yield_now();
        }
        assert_eq!(
            app.current_terminal_mouse_mode(),
            9,
            "real Ghostty mode flags must reach the attachment shadow"
        );
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::ReadModeFlags(prior_session_id.clone())),
            "the production attachment path must issue targeted mode readback"
        );

        let (_lines, mut mouse_hit_map) = renderer::render_to_lines(&app.surface(), 200, 80);
        app.apply_terminal_mouse_mode(&mut mouse_hit_map);
        let terminal = mouse_hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-terminal")
            .expect("production terminal should be hit-testable");
        assert!(terminal.terminal_mouse_mode);
        let (column, row) = (
            terminal.rect.x.saturating_add(1),
            terminal.rect.y.saturating_add(1),
        );
        let mut router = InputRouter::new(renderer::action_request_context());
        let focus = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &mouse_hit_map,
        );
        assert!(matches!(focus, InputDispatch::Action(_)));
        let sgr_release = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &mouse_hit_map,
        );
        assert!(matches!(
            &sgr_release,
            InputDispatch::TerminalForward { bytes, .. } if bytes == b"\x1b[<0;1;1m"
        ));
        app.handle_dispatch(sgr_release);
        assert!(app.observed_requests.iter().any(|request| {
            matches!(request, ObservedRequest::SendInput { session_id, data }
                if session_id == &prior_session_id && data == "\x1b[<0;1;1m")
        }));

        app.handle_dispatch(InputDispatch::TerminalForward {
            node_id: "tui-terminal".to_string(),
            bytes: b"\ndisable-mouse\n".to_vec(),
        });
        thread::yield_now();
        let mode_off_deadline = Instant::now() + Duration::from_secs(3);
        while app.current_terminal_mouse_mode() != 0 && Instant::now() < mode_off_deadline {
            app.poll_hub();
            thread::yield_now();
        }
        assert_eq!(
            app.current_terminal_mouse_mode(),
            0,
            "real DECRST output must restore outer mouse routing"
        );

        app.handle_dispatch(InputDispatch::TerminalForward {
            node_id: "tui-terminal".to_string(),
            bytes: b"enable-mouse\n".to_vec(),
        });
        thread::yield_now();
        let mode_reenabled_deadline = Instant::now() + Duration::from_secs(3);
        while app.current_terminal_mouse_mode() != 9 && Instant::now() < mode_reenabled_deadline {
            app.poll_hub();
            thread::yield_now();
        }
        assert_eq!(app.current_terminal_mouse_mode(), 9);

        app.handle_dispatch(InputDispatch::TerminalForward {
            node_id: "tui-terminal".to_string(),
            bytes: format!("{later_marker}\n").into_bytes(),
        });
        assert!(app.observed_requests.contains(&ObservedRequest::SendInput {
            session_id: prior_session_id.clone(),
            data: format!("{later_marker}\n"),
        }));
        wait_for_app_output(&mut app, &later_marker).expect("TUI renders later live output");
        assert_eq!(app.terminal_output.matches(&later_marker).count(), 1);
        let rendered = renderer::render_to_lines(&app.surface(), 200, 80)
            .0
            .join("\n");
        assert!(
            rendered.find(&prior_marker).unwrap() < rendered.find(&later_marker).unwrap(),
            "restored history must render before later live output: {rendered}"
        );

        let prior_entity_generation = app
            .session_entities
            .subscription_id
            .clone()
            .expect("initial entity generation exists");
        let attach_count_before_reconnect = app
            .observed_requests
            .iter()
            .filter(|request| matches!(request, ObservedRequest::Attach { .. }))
            .count();
        app.force_reconnect();
        assert_ne!(
            app.error.as_deref(),
            Some("session subscription cleanup timed out")
        );
        subscribe_session_entities(hub.endpoint(), prior_entity_generation.clone())
            .expect("client reconnect releases the old hub subscription id")
            .unsubscribe()
            .expect("cleanup probe unsubscribes");
        let reconnect_entity_generation = app.session_entities.subscription_id.clone();
        assert_ne!(
            reconnect_entity_generation.as_deref(),
            Some(prior_entity_generation.as_str()),
            "reconnect must establish a fresh entity subscription generation"
        );
        assert_eq!(app.attached_session, None, "reconnect must not auto-attach");
        assert_eq!(
            app.observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::Attach { .. }))
                .count(),
            attach_count_before_reconnect,
            "reconnect must not issue a terminal attach request"
        );
        wait_for_authoritative_session(&mut app, &prior_session_id)
            .expect("fresh generation snapshot restores the session row");
        app.selected_session = Some(prior_session_id.clone());
        app.attach_selected_or_first();
        let reconnect_subscription_id = app.subscription_id.clone();
        assert_ne!(reconnect_subscription_id, first_subscription_id);
        let reconnect_deadline = Instant::now() + Duration::from_secs(7);
        while app.attach_hydration.is_some() && Instant::now() < reconnect_deadline {
            app.poll_hub();
            thread::yield_now();
        }
        assert!(
            app.attach_hydration.is_none(),
            "same-session reconnect hydration must finish"
        );
        let restored_mode_deadline = Instant::now() + Duration::from_secs(3);
        while app.current_terminal_mouse_mode() != 9 && Instant::now() < restored_mode_deadline {
            app.poll_hub();
            thread::yield_now();
        }
        assert_eq!(
            app.current_terminal_mouse_mode(),
            9,
            "reattach must restore current authoritative mouse mode"
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
                session_id: prior_session_id.clone(),
            })
            .expect("shut down history-producing session");
        let exit_deadline = Instant::now() + Duration::from_secs(7);
        while app
            .sessions
            .iter()
            .any(|session| session.session_id == prior_session_id && session.lifecycle != "exited")
            && Instant::now() < exit_deadline
        {
            app.poll_hub();
            thread::yield_now();
        }
        let exited = renderer::render_to_lines(&app.surface(), 200, 80)
            .0
            .join("\n");
        assert!(
            exited.contains(&format!("{prior_session_id} · exited")),
            "natural exit patch must render through the app surface: {exited}"
        );
        daemon
            .request(&DaemonRequest::RemoveSession {
                session_id: prior_session_id.clone(),
            })
            .expect("remove history-producing session");
        let remove_deadline = Instant::now() + Duration::from_secs(7);
        while app
            .sessions
            .iter()
            .any(|session| session.session_id == prior_session_id)
            && Instant::now() < remove_deadline
        {
            app.poll_hub();
            thread::yield_now();
        }
        let removed = renderer::render_to_lines(&app.surface(), 200, 80)
            .0
            .join("\n");
        assert!(
            !removed.contains(&format!("{prior_session_id} ·")),
            "remove delta must delete the rendered session row: {removed}"
        );

        let empty_session_id = format!("tui-empty-{}", short_suffix());
        daemon
            .request(&DaemonRequest::Spawn {
                session_id: empty_session_id.clone(),
                command: "while IFS= read -r line; do :; done".to_string(),
            })
            .expect("spawn empty session");
        thread::yield_now();

        let mut empty_app = TuiApp::new(Some(hub.endpoint().clone()));
        wait_for_authoritative_session(&mut empty_app, &empty_session_id)
            .expect("empty external session appears through entity subscription");
        empty_app.selected_session = Some(empty_session_id.clone());
        empty_app.observed_requests.clear();
        empty_app.attach_selected_or_first();
        let deadline = Instant::now() + Duration::from_secs(7);
        while empty_app.attach_hydration.is_some() && Instant::now() < deadline {
            empty_app.poll_hub();
            thread::yield_now();
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
        let live_session_uuid = format!("tui-binding-{}", short_suffix());
        let missing_session_uuid = format!("tui-binding-missing-{}", short_suffix());
        let mut binding_app = TuiApp::new(Some(hub.endpoint().clone()));
        binding_app.workspace_test_mode = true;
        wait_for_session_entity_expectation(
            &mut binding_app,
            &live_session_uuid,
            SessionEntityExpectation::Absent,
            "generated contract-matrix session must be absent from the pre-spawn baseline",
        );
        client
            .request(&DaemonRequest::Spawn {
                session_id: live_session_uuid.clone(),
                command: "while IFS= read -r line; do :; done".to_string(),
            })
            .expect("spawn session after the TUI subscription baseline");
        wait_for_session_entity_expectation(
            &mut binding_app,
            &live_session_uuid,
            SessionEntityExpectation::Lifecycle("current"),
            "live spawn must reach the TUI-owned entity store",
        );
        binding_app.observed_requests.clear();
        binding_app.request_and_apply(DaemonRequest::PluginSurfaceRender {
            package_name: report.package_name.clone(),
            surface_id: report.session_surface_id.clone(),
            payload: json!({
                "session_uuids": [&live_session_uuid, &missing_session_uuid]
            }),
        });
        assert!(
            binding_app
                .observed_requests
                .contains(&ObservedRequest::PluginSurfaceRender {
                    package_name: report.package_name.clone(),
                    surface_id: report.session_surface_id.clone(),
                })
        );
        let lifecycle_class = binding_app
            .session_entities
            .entities
            .get(&live_session_uuid)
            .expect("live row is held by the app-owned store")
            .lifecycle_class
            .clone();
        let rendered = renderer::render_to_lines(&binding_app.surface(), 180, 60)
            .0
            .join("\n");
        assert!(rendered.contains(&lifecycle_class), "{rendered}");
        assert!(rendered.contains("Session unavailable"), "{rendered}");
        for fallback in ["bind /", "bind @/", "bound list: waiting for entities"] {
            assert!(!rendered.contains(fallback), "{rendered}");
        }

        let prior_generation = binding_app
            .session_entities
            .subscription_id
            .clone()
            .expect("initial binding generation");
        let render_requests_before_reconnect = binding_app
            .observed_requests
            .iter()
            .filter(|request| matches!(request, ObservedRequest::PluginSurfaceRender { .. }))
            .count();
        binding_app.force_reconnect();
        assert_ne!(
            binding_app.session_entities.subscription_id.as_deref(),
            Some(prior_generation.as_str())
        );
        assert_eq!(
            binding_app
                .observed_requests
                .iter()
                .filter(|request| matches!(request, ObservedRequest::PluginSurfaceRender { .. }))
                .count(),
            render_requests_before_reconnect,
            "reconnect must not refresh the plugin surface"
        );
        wait_for_session_entity_expectation(
            &mut binding_app,
            &live_session_uuid,
            SessionEntityExpectation::Lifecycle("current"),
            "fresh app-owned generation must restore the exact bound row",
        );
        binding_app.request_and_apply(DaemonRequest::PluginSurfaceRender {
            package_name: report.package_name.clone(),
            surface_id: report.session_surface_id.clone(),
            payload: json!({
                "session_uuids": [&live_session_uuid, &missing_session_uuid]
            }),
        });
        let rebound = renderer::render_to_lines(&binding_app.surface(), 180, 60)
            .0
            .join("\n");
        assert!(rebound.contains(&lifecycle_class), "{rebound}");

        client
            .request(&DaemonRequest::ShutdownSession {
                session_id: live_session_uuid.clone(),
            })
            .expect("shutdown live bound session");
        wait_for_session_entity_expectation(
            &mut binding_app,
            &live_session_uuid,
            SessionEntityExpectation::Lifecycle("ended"),
            "live bound session must become authoritative ended state",
        );
        let ended = renderer::render_to_lines(&binding_app.surface(), 180, 60)
            .0
            .join("\n");
        assert!(ended.contains("ended"), "{ended}");
        client
            .request(&DaemonRequest::RemoveSession {
                session_id: live_session_uuid.clone(),
            })
            .expect("remove live bound session");
        wait_for_session_entity_expectation(
            &mut binding_app,
            &live_session_uuid,
            SessionEntityExpectation::Absent,
            "live bound session must be authoritatively removed",
        );
        let removed = renderer::render_to_lines(&binding_app.surface(), 180, 60)
            .0
            .join("\n");
        assert!(removed.contains("Session unavailable"), "{removed}");

        let list_packages = client
            .request(&DaemonRequest::ListPackages)
            .expect("list packages after contract matrix conformance");
        let list_apps = client
            .request(&DaemonRequest::ListApps)
            .expect("list apps after contract matrix conformance");
        let list_package_navigation = client
            .request(&DaemonRequest::ListPackageNavigation)
            .expect("list package navigation after contract matrix conformance");
        let listed_packages = list_packages.packages.clone();
        assert!(
            listed_packages.len() > 1,
            "live Show/Refresh proof requires multiple packages, got {}",
            listed_packages.len()
        );
        let listed_fixture = listed_packages
            .iter()
            .find(|package| package.package_name == report.package_name)
            .expect("listed packages include the installed contract matrix fixture")
            .clone();
        assert_eq!(
            listed_fixture
                .surfaces
                .iter()
                .map(|surface| surface.id.clone())
                .collect::<Vec<_>>(),
            report.surface_ids
        );
        assert!(report.list_surfaces_match_enabled);
        assert!(report.show_routes_match_list);
        let mut app = TuiApp::new(None);
        app.workspace_test_mode = true;
        app.system_details_visible = true;
        app.apply_response(list_packages);
        app.apply_response(list_apps);
        app.apply_response(list_package_navigation);
        app.client =
            Some(HubConnection::connect(hub.endpoint()).expect("connect package app to live hub"));
        app.observed_requests.clear();
        let fixture_index = app
            .packages
            .iter()
            .position(|package| package.package_name == report.package_name)
            .expect("fixture package remains visible in TUI state");
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 500, 240);
        let rendered = lines.join("\n");
        assert!(rendered.contains(&format!(
            "id=contract.settings kind={} title=Contract Settings supports={}",
            report.settings_surface_kind,
            report.settings_surface_supports.join(",")
        )));
        assert!(rendered.contains("navigation entry: package=botster.plugin-contract-matrix"));
        assert!(rendered.contains(&report.app_route_path));
        assert!(rendered.contains("route_id=surface:contract.app"));
        assert!(rendered.contains(&format!(
            "target_surface_id={}",
            report.app_route_surface_id
        )));

        app.handle_dispatch(click_dispatch(
            &hit_map,
            &format!("tui-package-{fixture_index}-show"),
        ));
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::ShowPackage(report.package_name.clone()))
        );
        assert_eq!(app.packages.len(), 1);
        assert_eq!(app.packages[0].surfaces, listed_fixture.surfaces);
        let (show_lines, show_hit_map) = renderer::render_to_lines(&app.surface(), 500, 240);
        let show_rendered = show_lines.join("\n");
        assert!(show_rendered.contains(&format!(
            "id=contract.settings kind={} title=Contract Settings supports={}",
            report.settings_surface_kind,
            report.settings_surface_supports.join(",")
        )));

        app.handle_dispatch(click_dispatch(&show_hit_map, "tui-refresh"));
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::ListPackages)
        );
        assert_eq!(app.packages, listed_packages);

        let navigation_index = app
            .package_navigation
            .iter()
            .position(|entry| {
                entry.package_name == report.package_name
                    && entry.target.surface_id.as_deref() == Some("contract.app")
            })
            .expect("Hub-projected contract app navigation remains visible after refresh");
        let (_lines, navigation_hit_map) = renderer::render_to_lines(&app.surface(), 500, 240);
        app.handle_dispatch(click_dispatch(
            &navigation_hit_map,
            &format!("tui-package-navigation-{navigation_index}-open"),
        ));
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::PluginSurfaceRender {
                    package_name: report.package_name.clone(),
                    surface_id: "contract.app".to_string(),
                })
        );
        let app_surface = app
            .plugin_surface
            .clone()
            .expect("real navigation Open applies the delivered plugin surface");
        let packages_before_missing_show = app.packages.clone();
        let navigation_before_missing_show = app.package_navigation.clone();
        let owner_before_missing_show = app.plugin_surface.clone();
        app.request_and_apply(DaemonRequest::ShowPackage {
            package_name: "missing-package".to_string(),
        });
        assert!(
            app.observed_requests
                .contains(&ObservedRequest::ShowPackage("missing-package".to_string()))
        );
        assert_eq!(app.packages, packages_before_missing_show);
        assert_eq!(app.package_navigation, navigation_before_missing_show);
        assert_eq!(app.plugin_surface, owner_before_missing_show);
        assert!(app.error.as_deref().is_some_and(
            |error| error.contains("package_policy_error") && error.contains("operation=show")
        ));
        let missing_show_rendered = renderer::render_to_lines(&app.surface(), 500, 240)
            .0
            .join("\n");
        assert!(missing_show_rendered.contains("package_policy_error"));
        assert!(missing_show_rendered.contains("operation=show"));
        let app_rendered = assert_rendered_plugin_surface_contains(
            &app_surface,
            &report.client_render_check.app_surface_node_id,
            "plugin_surface_render",
        );
        assert!(app_rendered.contains("Render path: validated"));
        assert!(app_rendered.contains("Validated"));

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

        let submit_node = find_ui_node_by_id(&app_surface.body, "contract-app-form")
            .expect("delivered app surface includes its action-bearing form");
        let submit_action = node_action(submit_node);
        assert_eq!(submit_action.id.0, report.submit_action_id);
        let submit_node_id = submit_node
            .id
            .as_ref()
            .and_then(UiAuthoredNodeId::as_literal)
            .expect("action-bearing form has an id")
            .clone();
        let success_request = UiActionRequest {
            request_id: UiActionRequestId("contract-action-success".to_string()),
            surface_id: UiSurfaceId(app_surface.surface_id.clone()),
            action_id: submit_action.id.clone(),
            node_id: Some(submit_node_id.clone()),
            kind: UiActionKind::Submit,
            values: Some(UiFormValues(
                json!({ "message": "hello" })
                    .as_object()
                    .expect("values object")
                    .clone(),
            )),
            payload: submit_action.payload.clone(),
        };
        let mut action_app = TuiApp::new(None);
        action_app.apply_response(plugin_surface_response(app_surface.clone()));
        action_app.client =
            Some(HubConnection::connect(hub.endpoint()).expect("connect action app to live hub"));
        action_app.observed_requests.clear();
        action_app.handle_dispatch(InputDispatch::Action(success_request.clone()));
        assert!(
            action_app
                .observed_requests
                .contains(&ObservedRequest::PluginSurfaceAction {
                    package_name: report.package_name.clone(),
                    request: success_request.clone(),
                })
        );
        let (_lines, hit_map) = renderer::render_to_lines_with_presentation_state(
            &action_app.surface(),
            240,
            120,
            &RenderState::default(),
            &action_app.plugin_presentation,
        );
        assert_eq!(
            action_app
                .plugin_surface
                .as_ref()
                .expect("owner retained")
                .body
                .id
                .as_ref()
                .and_then(UiAuthoredNodeId::as_literal)
                .map(|id| id.0.as_str()),
            Some(report.action_success_replacement_node_id.as_str())
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == report.action_success_replacement_node_id)
        );
        assert_eq!(
            action_app
                .plugin_action_result
                .as_ref()
                .map(|result| result.request_id.0.as_str()),
            Some("contract-action-success")
        );

        let open_node = find_ui_node_by_id(&app_surface.body, &report.open_action_node_id)
            .expect("delivered app surface includes the reported open control");
        let open_action = node_action(open_node);
        assert_eq!(open_action.id.0, report.open_action_id);
        assert_eq!(
            open_action.payload,
            Some(report.open_action_payload.clone())
        );
        let mut open_app = TuiApp::new(None);
        open_app.apply_response(plugin_surface_response(app_surface.clone()));
        open_app.client =
            Some(HubConnection::connect(hub.endpoint()).expect("connect open app to live hub"));
        open_app.observed_requests.clear();
        let (_lines, open_hit_map) = renderer::render_to_lines_with_presentation_state(
            &open_app.surface(),
            240,
            120,
            &RenderState::default(),
            &open_app.plugin_presentation,
        );
        let open_dispatch = click_dispatch_for_surface(
            &open_hit_map,
            &report.open_action_node_id,
            Some(&app_surface.surface_id),
        );
        let open_request = match &open_dispatch {
            InputDispatch::Action(request) => request.clone(),
            other => panic!("rendered plugin Open must dispatch an action, got {other:?}"),
        };
        assert_eq!(open_request.surface_id.0, app_surface.surface_id);
        assert_eq!(open_request.action_id.0, report.open_action_id);
        assert_eq!(
            open_request.node_id,
            open_node
                .id
                .as_ref()
                .and_then(UiAuthoredNodeId::as_literal)
                .cloned()
        );
        assert_eq!(open_request.kind, UiActionKind::Submit);
        assert!(
            open_request
                .values
                .as_ref()
                .is_some_and(|values| values.0.is_empty())
        );
        assert_eq!(
            open_request.payload,
            Some(report.open_action_payload.clone())
        );
        open_app.handle_dispatch(open_dispatch);
        assert!(
            open_app
                .observed_requests
                .contains(&ObservedRequest::PluginSurfaceAction {
                    package_name: report.package_name.clone(),
                    request: open_request.clone(),
                })
        );
        assert_eq!(
            open_app
                .plugin_action_result
                .as_ref()
                .map(|result| &result.request_id),
            Some(&open_request.request_id)
        );
        for (key, value) in &report.open_set_values {
            assert_eq!(
                open_app
                    .plugin_presentation
                    .get(&botster_ui_contract::UiPresentationKey(key.clone())),
                Some(value)
            );
        }
        assert!(report.dialog_visible_after_open);
        assert!(report.selected_workspace_visible_after_open);
        let dialog_node =
            find_presentation_bound_node(&app_surface.body, &report.dialog_presence_key, None)
                .expect("delivered tree binds its dialog to the reported presence key");
        let selected_node = find_presentation_bound_node(
            &app_surface.body,
            &report.selected_workspace_equality_key,
            Some(&Value::String(
                report.selected_workspace_equality_value.clone(),
            )),
        )
        .expect("delivered tree binds selected detail to the reported equality");
        let dialog_title = dialog_node
            .props
            .get("title")
            .and_then(Value::as_str)
            .expect("bound dialog has visible title");
        let selected_text = selected_node
            .props
            .get("text")
            .and_then(Value::as_str)
            .expect("bound selected detail has visible text");
        let rendered = renderer::render_to_lines_with_presentation_state(
            &open_app.surface(),
            240,
            120,
            &RenderState::default(),
            &open_app.plugin_presentation,
        )
        .0
        .join("\n");
        assert!(rendered.contains(dialog_title), "{rendered}");
        assert!(rendered.contains(selected_text), "{rendered}");

        let mut failure_payload = submit_action
            .payload
            .clone()
            .expect("delivered submit action includes payload");
        failure_payload
            .as_object_mut()
            .expect("delivered submit payload is an object")
            .insert("fail".to_string(), Value::Bool(true));
        let failure_request = UiActionRequest {
            request_id: UiActionRequestId("contract-action-error".to_string()),
            surface_id: UiSurfaceId(app_surface.surface_id.clone()),
            action_id: submit_action.id,
            node_id: Some(submit_node_id),
            kind: UiActionKind::Submit,
            values: None,
            payload: Some(failure_payload),
        };
        let mut failure_app = TuiApp::new(None);
        failure_app.apply_response(plugin_surface_response(app_surface));
        failure_app.client =
            Some(HubConnection::connect(hub.endpoint()).expect("connect failure app to live hub"));
        failure_app.observed_requests.clear();
        let original_root = failure_app
            .plugin_surface
            .as_ref()
            .expect("active fixture")
            .body
            .clone();
        failure_app.handle_dispatch(InputDispatch::Action(failure_request.clone()));
        assert!(
            failure_app
                .observed_requests
                .contains(&ObservedRequest::PluginSurfaceAction {
                    package_name: report.package_name.clone(),
                    request: failure_request,
                })
        );
        assert_eq!(
            failure_app
                .plugin_action_result
                .as_ref()
                .map(|result| result.request_id.0.as_str()),
            Some("contract-action-error")
        );
        assert_eq!(
            failure_app
                .plugin_surface
                .as_ref()
                .expect("owner retained")
                .body,
            original_root
        );
        assert!(failure_app.diagnostics.iter().any(|diagnostic| {
            diagnostic.kind == DaemonDiagnosticKind::ActionFailure
                && diagnostic.operation.as_deref() == Some("plugin_surface_action")
        }));
        let rendered = renderer::render_to_lines(&failure_app.surface(), 240, 120)
            .0
            .join("\n");
        assert!(rendered.contains("state=Error"), "{rendered}");
        assert!(
            rendered.contains("diagnostic: action_failure"),
            "{rendered}"
        );

        let blocked = client
            .request(&DaemonRequest::PluginSurfaceRender {
                package_name: report.package_name.clone(),
                surface_id: "contract.blocked".to_string(),
                payload: json!({}),
            })
            .expect("render blocked contract surface");
        let mut blocked_app = TuiApp::new(None);
        blocked_app.workspace_test_mode = true;
        blocked_app.system_details_visible = true;
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
        let body =
            serde_json::to_string(&surface.body).expect("delivered surface body should serialize");
        assert!(
            body.contains(expected_node_id),
            "delivered surface body should include node id {expected_node_id}: {body}",
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
        eprintln!("skipping isolated Hub live-runtime test; {variable} is not set");
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
                failure_reason: None,
                pending: false,
            })
            .collect()
    }

    fn session_entity(session_id: &str, lifecycle: Option<&str>) -> DaemonSessionEntity {
        DaemonSessionEntity {
            session_uuid: session_id.to_string(),
            registry_state: "active".to_string(),
            lifecycle: lifecycle.map(str::to_string),
            lifecycle_class: "current".to_string(),
            rows: 24,
            cols: 80,
            updated_at: 1,
            exit_code: None,
            failure_reason: None,
        }
    }

    fn snapshot_frame(
        subscription_id: &str,
        snapshot_seq: u64,
        items: Vec<DaemonSessionEntity>,
    ) -> DaemonEntityFrame {
        DaemonEntityFrame::Snapshot {
            subscription_id: subscription_id.to_string(),
            entity_type: "session".to_string(),
            snapshot_seq,
            items,
            resync_reason: None,
        }
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
            lifecycle_counters: botster_hub_client::DaemonLifecycleCounters::default(),
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
        response.plugin_action_result =
            Some(serde_json::from_value(result).expect("fixture action result should be valid"));
        response
    }

    fn ui_node(value: Value) -> UiNode {
        serde_json::from_value(value).expect("fixture UiNode should be valid")
    }

    fn plugin_request(
        request_id: &str,
        surface_id: &str,
        action_id: &str,
        node_id: &str,
    ) -> UiActionRequest {
        UiActionRequest {
            request_id: UiActionRequestId(request_id.to_string()),
            surface_id: UiSurfaceId(surface_id.to_string()),
            action_id: UiActionId(action_id.to_string()),
            node_id: Some(UiNodeId(node_id.to_string())),
            kind: UiActionKind::Submit,
            values: None,
            payload: None,
        }
    }

    fn canonical_plugin_surface_fixture(mut surface: DaemonPluginSurface) -> DaemonPluginSurface {
        surface.ui_tree_snapshot = Some(botster_hub_client::DaemonUiTreeSnapshot {
            package_name: surface.package_name.clone(),
            surface_id: surface.surface_id.clone(),
            body: surface.body.clone(),
        });
        surface
    }

    fn presentation_plugin_surface() -> DaemonPluginSurface {
        canonical_plugin_surface_fixture(DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.presentation".to_string(),
            body: ui_node(json!({
                "type": "stack",
                "id": "contract-presentation-root",
                "props": { "direction": "vertical" },
                "children": [
                    {
                        "type": "button",
                        "id": "contract-open",
                        "props": {
                            "label": "Open contract form",
                            "action": { "id": "contract.open" }
                        }
                    },
                    {
                        "$kind": "presentation_if",
                        "predicate": {
                            "kind": "equals",
                            "key": "selected-workspace",
                            "value": "workspace-alpha"
                        },
                        "node": {
                            "type": "text",
                            "id": "contract-selected-workspace",
                            "props": {
                                "text": "Selected workspace: workspace-alpha"
                            }
                        }
                    },
                    {
                        "$kind": "presentation_if",
                        "predicate": {
                            "kind": "present",
                            "key": "contract-dialog"
                        },
                        "node": {
                            "type": "dialog",
                            "id": "contract-dialog",
                            "props": {
                                "title": "Contract form",
                                "presentation": "auto"
                            },
                            "slots": {
                                "body": [
                                    {
                                        "type": "form",
                                        "id": "contract-form",
                                        "props": {
                                            "submit_label": "Submit",
                                            "action": {
                                                "id": "contract.submit",
                                                "payload": { "source": "dialog" }
                                            }
                                        },
                                        "children": [
                                            {
                                                "type": "text_input",
                                                "id": "contract-message",
                                                "props": {
                                                    "name": "message",
                                                    "label": "Message",
                                                    "value": ""
                                                }
                                            }
                                        ]
                                    }
                                ]
                            }
                        }
                    }
                ]
            })),
            ui_tree_snapshot: None,
        })
    }

    fn field_error_kinds_plugin_surface() -> DaemonPluginSurface {
        canonical_plugin_surface_fixture(DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.field-errors".to_string(),
            body: ui_node(json!({
                "type": "dialog",
                "id": "contract-field-error-dialog",
                "props": {
                    "title": "Field error contract",
                    "presentation": "auto"
                },
                "slots": {
                    "body": [
                        {
                            "type": "form",
                            "id": "contract-field-error-form",
                            "props": {
                                "submit_label": "Submit",
                                "action": { "id": "contract.submit" }
                            },
                            "children": [
                                {
                                    "type": "text_input",
                                    "id": "contract-text-input",
                                    "props": {
                                        "name": "text_input",
                                        "label": "Text input"
                                    }
                                },
                                {
                                    "type": "checkbox",
                                    "id": "contract-checkbox",
                                    "props": {
                                        "name": "checkbox",
                                        "label": "Checkbox"
                                    }
                                },
                                {
                                    "type": "form_field",
                                    "id": "contract-form-field",
                                    "props": {
                                        "schema": {
                                            "kind": "text",
                                            "name": "form_field",
                                            "label": "Form field"
                                        }
                                    }
                                },
                                {
                                    "type": "textarea",
                                    "id": "contract-textarea",
                                    "props": {
                                        "name": "textarea",
                                        "label": "Textarea",
                                        "value": "line one\nline two\nline three"
                                    }
                                },
                                {
                                    "type": "select",
                                    "id": "contract-select",
                                    "props": {
                                        "name": "select",
                                        "label": "Select",
                                        "selected": "alpha"
                                    },
                                    "slots": {
                                        "options": [
                                            {
                                                "type": "select_option",
                                                "id": "contract-select-alpha",
                                                "props": {
                                                    "label": "Alpha",
                                                    "value": "alpha"
                                                }
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    ]
                }
            })),
            ui_tree_snapshot: None,
        })
    }

    fn contract_app_plugin_surface() -> DaemonPluginSurface {
        canonical_plugin_surface_fixture(DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.app".to_string(),
            body: ui_node(json!({
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
            })),
            ui_tree_snapshot: None,
        })
    }

    fn composite_application_primitives_plugin_surface() -> DaemonPluginSurface {
        canonical_plugin_surface_fixture(DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.composite".to_string(),
            body: ui_node(json!({
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
                                            "submit_label": "Submit",
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
            })),
            ui_tree_snapshot: None,
        })
    }

    fn invalid_table_plugin_surface() -> DaemonPluginSurface {
        canonical_plugin_surface_fixture(DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.invalid".to_string(),
            body: ui_node(json!({
                "type": "table",
                "id": "contract-invalid-table"
            })),
            ui_tree_snapshot: None,
        })
    }

    fn iframe_plugin_surface() -> DaemonPluginSurface {
        canonical_plugin_surface_fixture(DaemonPluginSurface {
            package_name: "botster.plugin-contract-matrix".to_string(),
            surface_id: "contract.iframe".to_string(),
            body: ui_node(json!({
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
            })),
            ui_tree_snapshot: None,
        })
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

    fn contract_package_surfaces() -> Vec<PackageSurfaceDescriptor> {
        vec![
            PackageSurfaceDescriptor {
                id: "contract.app".to_string(),
                kind: PackageSurfaceKind::App,
                title: "Contract App".to_string(),
                description: Some("Contract application surface".to_string()),
                icon: None,
                order: Some(1),
                category: Some("contracts".to_string()),
                supports: vec![
                    PackageSurfaceOperation::Render,
                    PackageSurfaceOperation::Action,
                ],
            },
            PackageSurfaceDescriptor {
                id: "contract.settings".to_string(),
                kind: PackageSurfaceKind::Settings,
                title: "Contract Settings".to_string(),
                description: None,
                icon: None,
                order: None,
                category: None,
                supports: vec![PackageSurfaceOperation::Render],
            },
            PackageSurfaceDescriptor {
                id: "contract.diagnostics".to_string(),
                kind: PackageSurfaceKind::Diagnostics,
                title: "Contract Diagnostics".to_string(),
                description: None,
                icon: None,
                order: None,
                category: None,
                supports: Vec::new(),
            },
        ]
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

    fn mode_flags_response(session_id: &str, mouse_mode: u8) -> DaemonResponse {
        let mut response = base_response(DaemonResponseKind::ReadModeFlags);
        response.mode_flags = Some(botster_hub_client::DaemonModeFlags {
            session_id: session_id.to_string(),
            mouse_mode,
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
            mode_flags: None,
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
            plugin_worker_counters: None,
            error: None,
            diagnostics: Vec::new(),
        }
    }
}
