use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    io::{self, Stdout},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use crate::entity_options::{
    EntityOptionsStore, demanded_entity_option_families, is_process_wide_entity_family,
    materialize_entity_options_selects,
};
use crate::hub_io::{AppWake, HubIo};
use crate::terminal_input::{self, InputWindow};
use botster_core::{
    RunnableEntrypointHubConnection, RunnableEntrypointHubConnectionTransport,
    contract::terminal_screen::TerminalScreenSize,
};
use botster_hub_client::{
    DaemonApp, DaemonAvailablePackage, DaemonCompatibility, DaemonCompatibilityError,
    DaemonCompatibilityRequirement, DaemonDiagnostic, DaemonDiagnosticKind, DaemonEndpoint,
    DaemonEntityFrame, DaemonEvent, DaemonHelloAck, DaemonPackage, DaemonPackageAvailabilityReason,
    DaemonPackageAvailabilityState, DaemonPackageInstallPlan, DaemonPackageNavigationEntry,
    DaemonPackagePin, DaemonPackageRouteDescriptor, DaemonPackageUpdateStatus, DaemonPluginSurface,
    DaemonRequest, DaemonRequestError, DaemonResponse, DaemonResponseKind, DaemonSessionEntity,
    DaemonSessionType, DaemonSessionTypeDefinition, DaemonSessionTypeEditableDefinition,
    DaemonSessionTypeExecution, DaemonSessionTypeMutationSource, DaemonSessionTypeRequest,
    DaemonSessionTypeWorkingDirectory, DaemonSoftwareIdentity, DaemonSpawnTarget,
    DaemonTransportError, DaemonTransportResult, FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS,
    FEATURE_PACKAGE_NAVIGATION, FEATURE_PLUGIN_SURFACE_ACTION, FEATURE_PLUGIN_SURFACE_RENDER,
    FEATURE_SESSION_ENTITY_SUBSCRIPTIONS, FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS,
    FEATURE_SESSIONS, FEATURE_TERMINAL_READBACK, FEATURE_TERMINAL_SUBSCRIPTION_CLOSED,
    FEATURE_UNIX_TERMINAL_ADAPTER, PROTOCOL, TerminalCompatibilityRequirement,
    ensure_terminal_compatible,
};
use botster_terminal_ghostty::{
    GhosttyAdapterConfig, GhosttyClientProjection, GhosttySnapshotDecodeProgress, ScrollOp,
    ViewportProjection,
};
#[cfg(test)]
use botster_terminal_protocol_client::decode_terminal_input;
use botster_terminal_protocol_client::{
    AttachStateCode, HistoryUnavailableReason, InputOutcome, InputResultBody, ModesBody, RouteId,
    RoutedTerminalFrame, TerminalEvent, TerminalInputCommand, decode_terminal_event, encode_paste,
    encode_terminal_input,
};
use botster_ui_contract::{
    EntityFamilyStore, PackageNoticeReactionDescriptor, PackageSurfaceDescriptor,
    PackageSurfaceKind, PackageSurfaceOperation, UiActionRequest, UiActionResult, UiAuthoredNodeId,
    UiChild, UiCondition, UiConditional, UiFormValues, UiNode, UiNodeId, UiNodeKind, UiWidthClass,
    realize_bind_list_descendant_id, resolve_notice_text,
};
use crossterm::{
    cursor::Show,
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
        KeyModifiers, MouseEvent,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
#[cfg(test)]
use ratatui::backend::TestBackend;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
};
use serde_json::{Value, json};

use crate::acceptance::{
    AcceptanceMode, CLAIM_SCHEMA, ClaimConfig, Config as AcceptanceConfig, EvidenceWriter,
    FailureContext, SCHEMA, ScenarioCase, verify_claim_pins,
};
use crate::projection_paint::tui_terminal_region;
use crate::renderer::{self, HitMap, InputDispatch, InputRouter, RenderState};

const PACKAGE_CONFIG_FIELD_PREFIX: &str = "package-config";
const DEFAULT_COMMAND: &str = "printf 'botster-tui-ready\\n'; while IFS= read -r line; do printf 'echo:%s\\n' \"$line\"; done";
const HEADLESS_INPUT: &str = "botster-tui-headless\n";
const HEADLESS_OUTPUT: &str = "echo:botster-tui-headless";
const SMOKE_MESSAGE: &str = "botster-tui smoke ok";
const MINIMUM_CONFORMANCE_FIXTURE_REVISION: u16 = 49;
/// Absolute deadline for an ordinary host-control request.
const REQUEST_DEADLINE: Duration = Duration::from_secs(10);
/// Absolute deadline for Detach and for connection teardown.
const DETACH_ON_DISCONNECT_BOUND: Duration = Duration::from_secs(2);
/// Bound for stopping the I/O owner at exit.
const SHUTDOWN_BOUND: Duration = Duration::from_secs(2);
/// Connection deadline for the headless live runtime smoke.
const HEADLESS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_millis(750);
const RECONNECT_BACKOFF_CAP: Duration = Duration::from_secs(8);
/// Live OUTPUT retained while SNAPSHOT_HISTORY is still arriving for a route.
const MAX_HYDRATION_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
/// Encoded input retained while a route is still attaching.
const MAX_PENDING_HYDRATION_INPUT_BYTES: usize = 2 * 1024 * 1024;
/// Package events parked per candidate subscription until EventSubscribed lands.
const MAX_PARKED_NOTICE_EVENTS: usize = 8;
/// Wakes applied per loop turn before one paint.
const WAKE_BATCH: usize = 64;
const MAX_NOTICE_SUBSCRIPTIONS: usize = 64;
const ENTITY_OPTIONS_BACKOFF_INITIAL: Duration = Duration::from_millis(750);
const ENTITY_OPTIONS_BACKOFF_CAP: Duration = Duration::from_secs(30);
const GHOSTTY_SCROLLBACK_BYTES: usize = 8 * 1024 * 1024;
const WORKSPACE_TOOLBAR_OVERFLOW_ID: &str = "workspace-toolbar__overflow";
const DEFAULT_TERMINAL_ROWS: u16 = 24;
const DEFAULT_TERMINAL_COLS: u16 = 80;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum EventSubscriptionState {
    #[default]
    Idle,
    Candidate(String),
    Active(String),
}

impl EventSubscriptionState {
    fn active_id(&self) -> Option<&str> {
        match self {
            Self::Active(id) => Some(id.as_str()),
            Self::Idle | Self::Candidate(_) => None,
        }
    }

    fn candidate_id(&self) -> Option<&str> {
        match self {
            Self::Candidate(id) => Some(id.as_str()),
            Self::Idle | Self::Active(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TransientNotice {
    text: String,
    deadline: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NoticeSubscriptionEntry {
    descriptor: PackageNoticeReactionDescriptor,
    subject: String,
    state: EventSubscriptionState,
}

type NoticeSubscriptionKey = (String, String);

#[derive(Clone, Debug)]
struct EntityOptionsRetryState {
    consecutive_failures: u32,
    next_attempt_at: Instant,
}

fn entity_options_backoff_delay(consecutive_failures: u32) -> Duration {
    let shift = consecutive_failures.saturating_sub(1).min(6);
    let millis = ENTITY_OPTIONS_BACKOFF_INITIAL
        .as_millis()
        .saturating_mul(1u128 << shift);
    let capped = millis.min(ENTITY_OPTIONS_BACKOFF_CAP.as_millis());
    Duration::from_millis(capped as u64)
}

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
        // Parent claim-stack prose uses BOTSTER_LIVE_DATA_DIR; prefer the established
        // BOTSTER_HUB_DATA_DIR injector when both are present.
        let hub_data_dir = std::env::var_os("BOTSTER_HUB_DATA_DIR")
            .or_else(|| std::env::var_os(crate::acceptance::LIVE_DATA_DIR_ENV));
        Self::parse_with_environment(
            args,
            std::env::var_os("BOTSTER_HUB_CONNECTION"),
            hub_data_dir,
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

#[cfg(test)]
fn parse_shared_session_id(value: Option<std::ffi::OsString>) -> Result<String, String> {
    let Some(value) = value else {
        return Err("BOTSTER_SHARED_SESSION_ID is required".to_string());
    };
    let value = value
        .into_string()
        .map_err(|_| "BOTSTER_SHARED_SESSION_ID must contain UTF-8".to_string())?;
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("BOTSTER_SHARED_SESSION_ID is required".to_string());
    }
    Ok(trimmed.to_string())
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
    session_type_id: Option<String>,
    session_type_source: Option<String>,
    role: Option<String>,
    traits: Vec<String>,
    interaction: Option<String>,
    session_type_lifecycle: Option<String>,
}

/// One attach campaign between the Attach request and the open live path.
///
/// Scheme 2 ordering per route: ATTACH_STATE attached, MODES, SNAPSHOT_READY,
/// live OUTPUT interleaved with SNAPSHOT_HISTORY, SNAPSHOT_FINISH, then OUTPUT.
/// Live OUTPUT that arrives before SNAPSHOT_FINISH is retained here (bounded)
/// and applied after the last history page.
#[derive(Clone, Debug)]
struct AttachHydration {
    session_id: String,
    route: String,
    /// Frames that arrived before the trusted Attach response fixed the
    /// attachment generation. Charged against the connection's aggregate
    /// pending budget through `HubIo::try_retain`; replayed once the response
    /// lands, released when the campaign ends.
    pending_frames: VecDeque<RoutedTerminalFrame>,
    pending_frame_bytes: usize,
    buffered_live_output: Vec<u8>,
    pending_input: Vec<PendingTerminalInput>,
    pending_input_bytes: usize,
    pending_resize: Option<TerminalScreenSize>,
    /// True after the incremental decoder validates READY.
    snapshot_ready: bool,
    /// True after SNAPSHOT_FINISH or HISTORY_UNAVAILABLE after READY.
    snapshot_finished: bool,
    /// True after the matching attached state arrives.
    attached_seen: bool,
}

impl AttachHydration {
    fn new(session_id: &str, route: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            route: route.to_string(),
            pending_frames: VecDeque::new(),
            pending_frame_bytes: 0,
            buffered_live_output: Vec::new(),
            pending_input: Vec::new(),
            pending_input_bytes: 0,
            pending_resize: None,
            snapshot_ready: false,
            snapshot_finished: false,
            attached_seen: false,
        }
    }
}

/// Input captured while a route is still attaching. Operation ids are
/// assigned when the live path opens so they stay strictly increasing.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingTerminalInput {
    Key(KeyEvent),
    Focus(bool),
    Paste(Vec<u8>),
}

impl PendingTerminalInput {
    /// Bytes retained by the client for this pending input.
    fn retained_bytes(&self) -> usize {
        match self {
            // KEY body prefix plus at most one UTF-8 scalar of text.
            Self::Key(_) => 16,
            Self::Focus(_) => 1,
            Self::Paste(data) => data.len(),
        }
    }
}

/// Whether a kit node id names the production terminal view.
fn is_terminal_node(node_id: Option<&str>) -> bool {
    matches!(node_id, Some("tui-terminal" | "tui-terminal-output"))
}

/// Entity family of one subscription frame.
fn entity_frame_type(frame: &DaemonEntityFrame) -> &str {
    match frame {
        DaemonEntityFrame::Snapshot { entity_type, .. }
        | DaemonEntityFrame::Upsert { entity_type, .. }
        | DaemonEntityFrame::Patch { entity_type, .. }
        | DaemonEntityFrame::Remove { entity_type, .. }
        | DaemonEntityFrame::Error { entity_type, .. } => entity_type,
    }
}

/// The attached route and its adopted stream generation.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AttachedRoute {
    session_id: String,
    route: String,
}

/// Last MODES frame for the current route.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalModeState {
    route: String,
    modes: ModesBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum DestructiveAction {
    Shutdown(String),
    Remove(String),
}

/// What the application does with one host-control completion.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingReply {
    /// Apply the response to read models and diagnostics.
    Apply,
    /// Session types for the target-first spawn picker (flow-local only).
    ListForTarget {
        target_id: String,
        target_label: String,
    },
    /// SpawnSessionType or freeform Spawn for a locally pending row.
    Spawn { session_id: String },
    /// ShowSessionTypeDefinition for the edit form.
    ShowSessionTypeDefinition { session_type_id: String },
    /// CreateSessionType or UpdateSessionType from the form.
    SessionTypeForm,
    /// Attach for the campaign on `route`.
    Attach { session_id: String, route: String },
    /// Detach for a retired route; the response only updates diagnostics.
    Detach,
    /// SubscribeEvents candidate for one notice descriptor.
    SubscribeEvents {
        key: NoticeSubscriptionKey,
        subscription_id: String,
    },
    /// SubscribeEntities for one entity family on this connection.
    SubscribeEntities {
        family: String,
        subscription_id: String,
    },
    /// UnsubscribeEvents or UnsubscribeEntities; the response is informational.
    Unsubscribe,
}

/// Parked package events for a candidate notice subscription.
#[derive(Clone, Debug, Default)]
struct ParkedNoticeEvents {
    events: VecDeque<(String, String, Value)>,
    gap: bool,
}

impl SessionRow {
    #[cfg(test)]
    fn running(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            lifecycle: "running".to_string(),
            failure_reason: None,
            pending: false,
            session_type_id: None,
            session_type_source: None,
            role: None,
            traits: Vec::new(),
            interaction: None,
            session_type_lifecycle: None,
        }
    }

    fn pending(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            lifecycle: "pending".to_string(),
            failure_reason: None,
            pending: true,
            session_type_id: None,
            session_type_source: None,
            role: None,
            traits: Vec::new(),
            interaction: None,
            session_type_lifecycle: None,
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
            session_type_id: entity.session_type_id.clone(),
            session_type_source: entity.session_type_source.clone(),
            role: entity.role.clone(),
            traits: entity.traits.clone(),
            interaction: entity.interaction.clone(),
            session_type_lifecycle: entity.session_type_lifecycle.clone(),
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
                let items = items
                    .into_iter()
                    .map(decode_session_entity)
                    .collect::<Result<Vec<_>, String>>()?;
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
                let entity = decode_session_entity(entity)?;
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
            DaemonEntityFrame::Error {
                subscription_id,
                entity_type,
                code,
                message,
            } => {
                if !self.matches(&subscription_id, &entity_type) {
                    return Ok(false);
                }
                Err(format!(
                    "session entity subscription error: code={code} message={message}"
                ))
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

#[derive(Default)]
struct SessionTypeEntityState {
    subscription_id: Option<String>,
    has_snapshot: bool,
    snapshot_seq: Option<u64>,
    entity_order: Vec<String>,
    entities: BTreeMap<String, DaemonSessionType>,
}

impl SessionTypeEntityState {
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
                let items = items
                    .into_iter()
                    .map(decode_session_type_entity)
                    .collect::<Result<Vec<_>, String>>()?;
                self.entity_order = items
                    .iter()
                    .map(|entity| entity.session_type_id.clone())
                    .collect();
                self.entities = items
                    .into_iter()
                    .map(|entity| (entity.session_type_id.clone(), entity))
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
                let entity = decode_session_type_entity(entity)?;
                if id != entity.session_type_id {
                    return Err(format!(
                        "session type entity id mismatch: frame={id} entity={}",
                        entity.session_type_id
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
                ..
            } => {
                if !self.matches(&subscription_id, &entity_type) {
                    return Ok(false);
                }
                Err(
                    "session type entity patch is unsupported; expected snapshot/upsert/remove only"
                        .to_string(),
                )
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
            DaemonEntityFrame::Error {
                subscription_id,
                entity_type,
                code,
                message,
            } => {
                if !self.matches(&subscription_id, &entity_type) {
                    return Ok(false);
                }
                Err(format!(
                    "session type entity subscription error: code={code} message={message}"
                ))
            }
        }
    }

    fn matches(&self, subscription_id: &str, entity_type: &str) -> bool {
        entity_type == "session_type" && self.subscription_id.as_deref() == Some(subscription_id)
    }

    fn accepts_delta(&self, subscription_id: &str, entity_type: &str, snapshot_seq: u64) -> bool {
        self.has_snapshot
            && self.matches(subscription_id, entity_type)
            && self
                .snapshot_seq
                .is_none_or(|current| snapshot_seq > current)
    }

    fn ordered(&self) -> Vec<&DaemonSessionType> {
        self.entity_order
            .iter()
            .filter_map(|id| self.entities.get(id))
            .collect()
    }
}

fn decode_session_type_entity(entity: Value) -> Result<DaemonSessionType, String> {
    serde_json::from_value(entity)
        .map_err(|error| format!("session type entity failed to decode: {error}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum SessionTypeFormMode {
    Create,
    Edit,
}

/// Draft for create/edit. Edit is seeded only from ShowSessionTypeDefinition.
#[derive(Clone, Debug, PartialEq, Eq)]
struct SessionTypeFormDraft {
    mode: SessionTypeFormMode,
    source: String,
    source_target_id: String,
    /// Effective Hub session_type_id while editing; unused for create.
    session_type_id: Option<String>,
    /// Lossless seed retained for edit wholesale replacement.
    seed_definition: Option<DaemonSessionTypeDefinition>,
    seed_source: Option<DaemonSessionTypeMutationSource>,
    id: String,
    label: String,
    description: String,
    icon: String,
    role: String,
    interaction: String,
    traits: String,
    lifecycle: String,
    execution: String,
    command: String,
    args: String,
    working_directory_policy: String,
    working_directory_path: String,
    environment: String,
    allowed_environment_overrides: String,
    context_keys: String,
    /// Preserved authored collections when text controls are left untouched.
    seeded_traits: Option<Vec<String>>,
    seeded_args: Option<Vec<String>>,
    seeded_context: Option<Vec<String>>,
    seeded_allowed_environment_overrides: Option<Vec<String>>,
    seeded_environment: Option<BTreeMap<String, String>>,
    definition_target_id: String,
    error: Option<String>,
}

impl SessionTypeFormDraft {
    fn create_default() -> Self {
        Self {
            mode: SessionTypeFormMode::Create,
            source: "device".to_string(),
            source_target_id: String::new(),
            session_type_id: None,
            seed_definition: None,
            seed_source: None,
            id: String::new(),
            label: String::new(),
            description: String::new(),
            icon: String::new(),
            role: "botster.agent".to_string(),
            interaction: "interactive".to_string(),
            traits: String::new(),
            lifecycle: "task".to_string(),
            execution: "relative_executable".to_string(),
            command: String::new(),
            args: String::new(),
            working_directory_policy: "package_root".to_string(),
            working_directory_path: String::new(),
            environment: String::new(),
            allowed_environment_overrides: String::new(),
            context_keys: String::new(),
            seeded_traits: None,
            seeded_args: None,
            seeded_context: None,
            seeded_allowed_environment_overrides: None,
            seeded_environment: None,
            definition_target_id: String::new(),
            error: None,
        }
    }

    fn from_authoring(editable: DaemonSessionTypeEditableDefinition) -> Self {
        let working_directory_policy;
        let working_directory_path;
        match &editable.definition.working_directory {
            DaemonSessionTypeWorkingDirectory::PackageRoot => {
                working_directory_policy = "package_root".to_string();
                working_directory_path = String::new();
            }
            DaemonSessionTypeWorkingDirectory::Relative { path } => {
                working_directory_policy = "relative".to_string();
                working_directory_path = path.clone();
            }
        }
        let (source, source_target_id) = match &editable.source {
            DaemonSessionTypeMutationSource::Device => ("device".to_string(), String::new()),
            DaemonSessionTypeMutationSource::Repo { target_id } => {
                ("repo".to_string(), target_id.clone())
            }
            DaemonSessionTypeMutationSource::Package { package_name } => {
                ("package".to_string(), package_name.clone())
            }
        };
        let seeded_traits = editable.definition.traits.clone();
        let seeded_args = editable.definition.args.clone();
        let seeded_context = editable.definition.context.clone();
        let seeded_allowed = editable.definition.allowed_environment_overrides.clone();
        let seeded_environment = editable.definition.environment.clone();
        Self {
            mode: SessionTypeFormMode::Edit,
            source,
            source_target_id,
            session_type_id: Some(editable.session_type_id),
            seed_definition: Some(editable.definition.clone()),
            seed_source: Some(editable.source),
            id: editable.definition.id.clone(),
            label: editable.definition.label.clone(),
            description: editable.definition.description.clone().unwrap_or_default(),
            icon: editable.definition.icon.clone().unwrap_or_default(),
            role: editable.definition.role.clone(),
            interaction: editable.definition.interaction.clone(),
            traits: join_tokens(&seeded_traits),
            lifecycle: editable.definition.lifecycle.clone(),
            execution: match &editable.definition.execution {
                DaemonSessionTypeExecution::RelativeExecutable => "relative_executable".to_string(),
                DaemonSessionTypeExecution::ShellCommand => "shell_command".to_string(),
            },
            command: editable.definition.command.clone(),
            args: join_tokens(&seeded_args),
            working_directory_policy,
            working_directory_path,
            environment: format_environment(&seeded_environment),
            allowed_environment_overrides: join_tokens(&seeded_allowed),
            context_keys: join_tokens(&seeded_context),
            seeded_traits: Some(seeded_traits),
            seeded_args: Some(seeded_args),
            seeded_context: Some(seeded_context),
            seeded_allowed_environment_overrides: Some(seeded_allowed),
            seeded_environment: Some(seeded_environment),
            definition_target_id: editable.definition.target_id.clone().unwrap_or_default(),
            error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TargetFirstSpawnStep {
    PickTarget,
    PickSessionType {
        target_id: String,
        target_label: String,
        /// Available winners from Hub `ListSessionTypesForTarget` for `target_id`.
        /// Flow-local only — never written into the session-type entity store.
        session_types: Vec<DaemonSessionType>,
    },
    Prompt {
        target_id: String,
        target_label: String,
        session_type_id: String,
        prompt: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetFirstSpawnFlow {
    step: TargetFirstSpawnStep,
}

/// One pickable launch target: an enabled admitted Hub spawn target only.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LaunchTargetOption {
    target_id: String,
    label: String,
}

/// Test-only stub for list-for-target responses so hermetic handlers can prove
/// success and operator/transport failure without a live Hub client.
#[cfg(test)]
#[derive(Clone, Debug)]
enum ListForTargetStub {
    Ok(Vec<DaemonSessionType>),
    OperatorError {
        code: String,
        operation: String,
        message: String,
    },
    TransportError,
}

fn join_tokens(values: &[String]) -> String {
    values.join(", ")
}

fn parse_token_list(input: &str, seeded: Option<&Vec<String>>) -> Vec<String> {
    let trimmed = input.trim();
    // Empty means the user cleared the field. Untouched fields keep the seeded
    // rendering (join_tokens(seed) == trimmed) so wholesale update stays lossless.
    if trimmed.is_empty() {
        return Vec::new();
    }
    if let Some(seeded) = seeded
        && join_tokens(seeded) == trimmed
    {
        return seeded.clone();
    }
    trimmed
        .split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect()
}

fn format_environment(environment: &BTreeMap<String, String>) -> String {
    environment
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_environment(
    input: &str,
    seeded: Option<&BTreeMap<String, String>>,
) -> BTreeMap<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return BTreeMap::new();
    }
    if let Some(seeded) = seeded
        && format_environment(seeded) == trimmed
    {
        return seeded.clone();
    }
    let mut map = BTreeMap::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.to_string());
        }
    }
    map
}

fn definition_from_session_type_form(
    form: &SessionTypeFormDraft,
) -> Result<DaemonSessionTypeDefinition, String> {
    let working_directory = if form.working_directory_policy == "relative" {
        DaemonSessionTypeWorkingDirectory::Relative {
            path: form.working_directory_path.trim().to_string(),
        }
    } else {
        DaemonSessionTypeWorkingDirectory::PackageRoot
    };
    let description = form.description.trim();
    let icon = form.icon.trim();
    let definition_target_id = form.definition_target_id.trim();
    let execution = match form.execution.as_str() {
        "relative_executable" => DaemonSessionTypeExecution::RelativeExecutable,
        "shell_command" => DaemonSessionTypeExecution::ShellCommand,
        other => return Err(format!("unsupported session type execution mode: {other}")),
    };
    Ok(DaemonSessionTypeDefinition {
        id: form.id.trim().to_string(),
        label: form.label.trim().to_string(),
        description: if description.is_empty() {
            None
        } else {
            Some(description.to_string())
        },
        icon: if icon.is_empty() {
            None
        } else {
            Some(icon.to_string())
        },
        role: form.role.trim().to_string(),
        interaction: form.interaction.trim().to_string(),
        traits: parse_token_list(&form.traits, form.seeded_traits.as_ref()),
        lifecycle: form.lifecycle.trim().to_string(),
        execution,
        command: form.command.trim().to_string(),
        args: parse_token_list(&form.args, form.seeded_args.as_ref()),
        working_directory,
        environment: parse_environment(&form.environment, form.seeded_environment.as_ref()),
        allowed_environment_overrides: parse_token_list(
            &form.allowed_environment_overrides,
            form.seeded_allowed_environment_overrides.as_ref(),
        ),
        context: parse_token_list(&form.context_keys, form.seeded_context.as_ref()),
        target_id: if definition_target_id.is_empty() {
            None
        } else {
            Some(definition_target_id.to_string())
        },
    })
}

fn mutation_source_from_form(
    form: &SessionTypeFormDraft,
) -> Result<DaemonSessionTypeMutationSource, String> {
    match form.source.as_str() {
        "device" => Ok(DaemonSessionTypeMutationSource::Device),
        "repo" => {
            let target_id = form.source_target_id.trim();
            if target_id.is_empty() {
                return Err("repo session types require a spawn target".to_string());
            }
            Ok(DaemonSessionTypeMutationSource::Repo {
                target_id: target_id.to_string(),
            })
        }
        other => Err(format!(
            "unsupported session type source for mutation: {other}"
        )),
    }
}

/// Decodes one authoritative entity record into the typed session projection.
///
/// Hub entity frames carry validated records as [`Value`]; `botster-hub-client`
/// prescribes deserializing them as [`DaemonSessionEntity`]. A malformed record
/// surfaces as an error through the reducer's existing diagnostic channel rather
/// than being silently dropped.
fn decode_session_entity(entity: Value) -> Result<DaemonSessionEntity, String> {
    serde_json::from_value(entity)
        .map_err(|error| format!("session entity failed to decode: {error}"))
}

/// Builds an intentionally exhaustive session-entity row so bind-list templates
/// observe every key, including those the Hub omits when absent.
///
/// The values are deliberately reference-shaped placeholders: only
/// [`session_binding_reference_row`]'s keys are consumed, and the TUI must not
/// imply ownership of the Hub's role/interaction/lifecycle vocabulary.
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
        session_type_id: Some("reference-session-type".to_string()),
        session_type_source: Some("reference-source".to_string()),
        role: Some("reference-role".to_string()),
        traits: vec!["reference-trait".to_string()],
        interaction: Some("reference-interaction".to_string()),
        session_type_lifecycle: Some("reference-lifecycle".to_string()),
    })
    .expect("exhaustive session binding reference row must serialize")
    .as_object()
    .expect("session binding reference row must serialize as an object")
    .clone()
}

pub fn run(args: AppArgs) -> io::Result<()> {
    match AcceptanceMode::from_environment()? {
        Some(AcceptanceMode::Spawn(config)) => return run_workspaces_acceptance(args, config),
        Some(AcceptanceMode::Claim(config)) => {
            return run_workspaces_claim_acceptance(args, config);
        }
        None => {}
    }
    if args.headless_live_runtime {
        return run_headless_live_runtime(args)
            .map_err(|error| io::Error::other(format!("headless live runtime failed: {error}")));
    }

    let hub_io = HubIo::with_terminal_input()?;
    let mut terminal = setup_terminal()?;
    let run_result = run_loop(&mut terminal, args, hub_io);
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
    if let Err(error) = execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        EnableFocusChange
    ) {
        let _ = disable_raw_mode();
        return Err(error);
    }

    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(error) => {
            let mut stdout = io::stdout();
            let _ = execute!(
                stdout,
                DisableFocusChange,
                DisableBracketedPaste,
                DisableMouseCapture,
                LeaveAlternateScreen,
                Show
            );
            let _ = disable_raw_mode();
            Err(error)
        }
    }
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let leave_result = execute!(
        terminal.backend_mut(),
        DisableFocusChange,
        DisableBracketedPaste,
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

/// The interactive event loop.
///
/// One wait per turn: `HubIo::next_wake` blocks until an input event, a Hub
/// frame, a request completion, or the earliest absolute deadline. Every wake
/// that is already available is applied before one paint. There is no
/// periodic poll.
fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    args: AppArgs,
    hub_io: HubIo,
) -> io::Result<()> {
    let mut app = TuiApp::new_with_runtime_context(
        args.daemon_endpoint(),
        args.connection_error,
        args.hub_data_dir.is_some(),
        hub_io,
    );
    app.connect();
    let mut router = InputRouter::new(renderer::action_request_context());
    let mut routed_surface_id = None;
    let mut running = true;
    while running {
        let active_surface_id = app.active_plugin_surface_id().map(ToOwned::to_owned);
        if active_surface_id != routed_surface_id {
            router = InputRouter::new(match active_surface_id.as_deref() {
                Some(surface_id) => renderer::action_request_context_for(surface_id),
                None => renderer::action_request_context(),
            });
            routed_surface_id = active_surface_id;
        }
        app.set_drafts(router.draft_values());

        let render_state = router.render_state();
        let mut hit_map = HitMap::default();
        app.prepare_paint();
        terminal.draw(|frame| draw(frame, &mut hit_map, &app, &render_state))?;
        app.apply_terminal_mouse_mode(&mut hit_map);
        router.reconcile(&hit_map);

        let wake = app.next_wake();
        running = apply_wake(&mut app, &mut router, &hit_map, wake);
        let mut applied = 1;
        while running && applied < WAKE_BATCH {
            let Some(wake) = app.try_next_wake() else {
                break;
            };
            running = apply_wake(&mut app, &mut router, &hit_map, wake);
            applied += 1;
        }
    }
    app.shutdown();
    Ok(())
}

/// Apply one wake. Returns false when the application should exit.
fn apply_wake(app: &mut TuiApp, router: &mut InputRouter, hit_map: &HitMap, wake: AppWake) -> bool {
    match wake {
        AppWake::Input(event) => route_input_event(app, router, hit_map, event),
        AppWake::Shutdown => false,
        other => {
            app.apply_wake(other);
            true
        }
    }
}

fn route_input_event(
    app: &mut TuiApp,
    router: &mut InputRouter,
    hit_map: &HitMap,
    event: Event,
) -> bool {
    match event {
        Event::Key(key)
            if key.kind == KeyEventKind::Press
                && app.handle_tui_owned_key(key, router.focused_node_id()) => {}
        Event::Key(key) if app.handle_focused_terminal_key(key, router.focused_node_id()) => {}
        Event::Paste(ref text)
            if app.handle_focused_terminal_paste(text, router.focused_node_id()) => {}
        Event::Mouse(mouse)
            if app.handle_focused_terminal_mouse(mouse, router.focused_node_id(), hit_map) => {}
        Event::FocusGained => app.handle_host_focus(true),
        Event::FocusLost => app.handle_host_focus(false),
        Event::Key(key) if key.kind == KeyEventKind::Press && should_quit(key) => return false,
        event => {
            let dispatch = router.dispatch_event(event, hit_map);
            app.sync_focused_session(router.selected_row_value("tui-session-list"));
            app.handle_dispatch(dispatch);
        }
    }
    true
}

fn draw(frame: &mut Frame<'_>, hit_map: &mut HitMap, app: &TuiApp, render_state: &RenderState) {
    if app.uses_workspace_shell() {
        draw_workspace_shell(frame, hit_map, app, render_state);
        return;
    }
    let node = app.surface();
    renderer::render_node_with_presentation_state(
        frame,
        frame.area(),
        &node,
        hit_map,
        render_state,
        &app.plugin_presentation,
    );
    app.paint_ghostty_projection(frame, hit_map);
}

fn draw_workspace_shell(
    frame: &mut Frame<'_>,
    hit_map: &mut HitMap,
    app: &TuiApp,
    render_state: &RenderState,
) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    // This is deliberately a multi-root render into one HitMap. Confirmation
    // dialogs and plugin surfaces must remain excluded by uses_workspace_shell:
    // a modal root clears regions registered by earlier roots.
    let width_class = renderer::viewport_for_area(area).width_class;
    let status = app.status_summary_node(width_class);
    let alert = app.connection_alert();
    let notice = app.transient_notice_band();
    let toolbar = app.workspace_toolbar();
    let navigator = app.session_navigator();
    let focused_session = app.focused_session_panel();
    for node in [
        Some(&status),
        alert.as_ref(),
        notice.as_ref(),
        Some(&toolbar),
        Some(&navigator),
        Some(&focused_session),
    ]
    .into_iter()
    .flatten()
    {
        node.validate()
            .expect("workspace shell node should satisfy the core UI contract");
        renderer::tui_capabilities()
            .validate_node(node)
            .expect("workspace shell node should fit TUI renderer capabilities");
    }

    let status_area = Rect::new(area.x, area.y, area.width, 1);
    renderer::render_node_with_presentation_state(
        frame,
        status_area,
        &status,
        hit_map,
        render_state,
        &app.plugin_presentation,
    );

    let mut next_y = area.y.saturating_add(1);
    for band in [alert.as_ref(), notice.as_ref()].into_iter().flatten() {
        let band_area = Rect::new(area.x, next_y, area.width, 1);
        renderer::render_node_with_presentation_state(
            frame,
            band_area,
            band,
            hit_map,
            render_state,
            &app.plugin_presentation,
        );
        next_y = next_y.saturating_add(1);
        if next_y >= area.y.saturating_add(area.height) {
            return;
        }
    }

    if next_y >= area.y.saturating_add(area.height) {
        return;
    }
    let toolbar_y = next_y;
    let toolbar_area = Rect::new(
        area.x,
        toolbar_y,
        area.width,
        area.y.saturating_add(area.height).saturating_sub(toolbar_y),
    );
    let overflow_open = render_state.is_expanded(WORKSPACE_TOOLBAR_OVERFLOW_ID);
    if !overflow_open {
        renderer::render_node_with_presentation_state(
            frame,
            toolbar_area,
            &toolbar,
            hit_map,
            render_state,
            &app.plugin_presentation,
        );
    }

    next_y = next_y.saturating_add(1);
    let body = Rect::new(
        area.x,
        next_y,
        area.width,
        area.y.saturating_add(area.height).saturating_sub(next_y),
    );
    if body.width > 0 && body.height > 0 {
        let panes = workspace_panes(body, app.sessions.len());
        if let Some(navigator_area) = panes.first().copied() {
            renderer::render_node_with_presentation_state(
                frame,
                navigator_area,
                &navigator,
                hit_map,
                render_state,
                &app.plugin_presentation,
            );
        }
        if let Some(terminal_area) = panes.get(1).copied() {
            renderer::render_node_with_presentation_state(
                frame,
                terminal_area,
                &focused_session,
                hit_map,
                render_state,
                &app.plugin_presentation,
            );
            // TUI-owned styled paint after kit TerminalView chrome (HitMap region).
            app.paint_ghostty_projection(frame, hit_map);
        }
    }

    if overflow_open {
        // Render an open overflow last so its occluder and regions win hit
        // testing. The menu captures focus traversal while it is expanded.
        renderer::render_node_with_presentation_state(
            frame,
            toolbar_area,
            &toolbar,
            hit_map,
            render_state,
            &app.plugin_presentation,
        );
    }
}

fn workspace_panes(area: Rect, session_count: usize) -> Vec<Rect> {
    match renderer::viewport_for_area(area).width_class {
        UiWidthClass::Expanded => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(40), Constraint::Min(1)])
            .split(area)
            .to_vec(),
        UiWidthClass::Regular => Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Min(1)])
            .split(area)
            .to_vec(),
        UiWidthClass::Compact => compact_workspace_panes(area, session_count),
    }
}

fn compact_workspace_panes(area: Rect, session_count: usize) -> Vec<Rect> {
    if area.height < 2 {
        return vec![area];
    }
    let maximum_navigator_height = (area.height / 2)
        .clamp(3, 10)
        .min(area.height.saturating_sub(1));
    let navigator_height = u16::try_from(session_count.max(2))
        .unwrap_or(maximum_navigator_height)
        .saturating_add(2)
        .min(maximum_navigator_height)
        .max(1);
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(navigator_height), Constraint::Min(1)])
        .split(area)
        .to_vec()
}

#[cfg(test)]
fn render_app_to_lines(
    app: &TuiApp,
    width: u16,
    height: u16,
    state: &RenderState,
) -> (Vec<String>, HitMap) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    let mut hit_map = HitMap::default();
    terminal
        .draw(|frame| draw(frame, &mut hit_map, app, state))
        .expect("application shell should render");
    let buffer = terminal.backend().buffer();
    let lines = (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().chars().next().unwrap_or(' '))
                .collect::<String>()
        })
        .collect();
    (lines, hit_map)
}

fn should_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || matches!(key.code, KeyCode::Char('q' | 'Q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

/// User-facing text for an INPUT_RESULT that is not `Written`.
fn input_outcome_message(result: &InputResultBody) -> String {
    let written = result
        .written_pty_bytes
        .map(|bytes| format!(" after {bytes} bytes"))
        .unwrap_or_default();
    let detail = if result.detail.is_empty() {
        String::new()
    } else {
        format!(": {}", result.detail)
    };
    let operation = result.operation_id;
    match result.outcome {
        InputOutcome::Written => format!("terminal input {operation} written"),
        InputOutcome::PartialWrite => {
            format!("terminal input {operation} partially written{written}{detail}")
        }
        InputOutcome::WriteFailed => format!("terminal input {operation} write failed{detail}"),
        InputOutcome::Cancelled => format!("terminal input {operation} cancelled{written}"),
        InputOutcome::RejectedNotWritable => {
            format!("terminal input {operation} rejected: session is not writable{detail}")
        }
        InputOutcome::RejectedTooLarge => {
            format!("terminal input {operation} rejected: payload too large{detail}")
        }
        InputOutcome::RejectedUnsafePaste => {
            format!("terminal paste {operation} rejected: unsafe paste{detail}")
        }
        InputOutcome::RejectedLaneFull => {
            format!("terminal input {operation} rejected: input lane full{detail}")
        }
        InputOutcome::RejectedProtocol => {
            format!("terminal input {operation} rejected: protocol error{detail}")
        }
        InputOutcome::SessionEnded => format!("terminal input {operation} rejected: session ended"),
        InputOutcome::OutcomeUnknown => {
            format!("terminal input {operation} outcome unknown: worker link failed{detail}")
        }
    }
}

/// Reconnect delay after `failures` consecutive connection failures.
fn reconnect_backoff_delay(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(8);
    RECONNECT_BACKOFF_INITIAL
        .saturating_mul(1_u32 << exponent)
        .min(RECONNECT_BACKOFF_CAP)
}

struct TuiApp {
    endpoint: Option<DaemonEndpoint>,
    host_requirement: DaemonCompatibilityRequirement,
    /// The single I/O owner: input thread, socket threads, request deadlines.
    hub_io: HubIo,
    /// Generation of the connection that completed Hello, when connected.
    connected_generation: Option<u64>,
    /// Absolute time of the next reconnect attempt, when scheduled.
    reconnect_at: Option<Instant>,
    reconnect_failures: u32,
    /// Continuation for every outstanding host-control request.
    pending_requests: BTreeMap<u64, PendingReply>,
    status: String,
    connection_error: Option<String>,
    error: Option<String>,
    action_feedback: Option<String>,
    compatibility: Option<DaemonCompatibility>,
    /// Authoritative Hub identity, sourced only from `DaemonStatus.software`.
    /// Hub identity is never derived from an installed package row.
    software: Option<DaemonSoftwareIdentity>,
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
    session_type_entities: SessionTypeEntityState,
    session_type_subscription_error: Option<String>,
    /// Multi-family entity-options store (non-process-wide families + generation).
    entity_options: EntityOptionsStore,
    /// Entity-options families with an open or requested SubscribeEntities.
    entity_options_subscriptions: BTreeSet<String>,
    /// Per-family backoff for optional entity-options subscribe admission.
    entity_options_retry: BTreeMap<String, EntityOptionsRetryState>,
    /// Field names whose entity-backed selection was invalidated (visible error).
    entity_options_invalid_fields: BTreeSet<String>,
    notice_subscriptions: BTreeMap<NoticeSubscriptionKey, NoticeSubscriptionEntry>,
    notice_subscription_by_id: BTreeMap<String, NoticeSubscriptionKey>,
    /// Package events that arrived before EventSubscribed for a candidate.
    notice_parked: BTreeMap<String, ParkedNoticeEvents>,
    notice_overflow_dropped: usize,
    transient_notice: Option<TransientNotice>,
    session_types_supported: bool,
    spawn_targets: Vec<DaemonSpawnTarget>,
    selected_session_type_id: Option<String>,
    session_type_form: Option<SessionTypeFormDraft>,
    target_first_spawn: Option<TargetFirstSpawnFlow>,
    sessions: Vec<SessionRow>,
    selected_session: Option<String>,
    /// Live attached route after SNAPSHOT_FINISH and attached state.
    attached: Option<AttachedRoute>,
    schema_version: Option<u16>,
    /// Route id of the current attach campaign or attachment.
    subscription_id: String,
    next_terminal_subscription_sequence: u64,
    /// Fixed attachment generation for the current route, adopted from the
    /// Attach response or the first ATTACH_STATE attached frame; frames with
    /// any other generation are dropped.
    route_generation: Option<u64>,
    /// Accepted stream epoch for snapshot and live continuity within the
    /// attachment: 0 after ATTACH_STATE attached, `to_epoch` after an accepted
    /// ROUTE_RESYNC. Data frames with another epoch are dropped.
    route_epoch: Option<u32>,
    /// Core-owned Ghostty projection for incremental GHOSTSNP and live output.
    ghostty_projection: Option<GhosttyClientProjection>,
    ghostty_projection_session_id: Option<String>,
    /// Last projected viewport for immutable frame paint after kit TerminalView.
    ghostty_viewport_cache: Option<ViewportProjection>,
    /// True when the projection changed since the last `project_viewport`.
    projection_dirty: bool,
    attach_hydration: Option<AttachHydration>,
    /// One automatic recovery already used for the current attach campaign.
    attach_recovery_used: bool,
    /// Retired route ids whose late frames and close events must be ignored.
    retired_subscription_ids: BTreeSet<String>,
    /// Close-event evidence from `TerminalSubscriptionClosed` (generation, reason).
    terminal_close_evidence: Option<(u64, String)>,
    /// Last MODES frame for the current route.
    terminal_modes: Option<TerminalModeState>,
    /// Client-side input window for the current route generation.
    input_window: InputWindow,
    terminal_viewport_size: TerminalScreenSize,
    drafts: BTreeMap<String, Value>,
    system_details_visible: bool,
    package_storage_context_configured: bool,
    confirmation: Option<DestructiveAction>,
    #[cfg(test)]
    workspace_test_mode: bool,
    acceptance_audit: Option<AcceptanceRequestAudit>,
    #[cfg(test)]
    observed_requests: Vec<ObservedRequest>,
    #[cfg(test)]
    observed_terminal_inputs: Vec<TerminalInputCommand>,
    #[cfg(test)]
    list_for_target_stub: Option<ListForTargetStub>,
    /// Exact live payloads passed to the Ghostty apply path (test observer only).
    #[cfg(test)]
    applied_live_payloads: Vec<Vec<u8>>,
    #[cfg(test)]
    entity_options_forced_subscribe_error: Option<&'static str>,
    #[cfg(test)]
    entity_options_subscribe_attempts: BTreeMap<String, usize>,
}

impl TuiApp {
    /// Application state without a terminal input thread. The caller starts
    /// the connection with `connect`.
    fn new(endpoint: Option<DaemonEndpoint>) -> Self {
        Self::new_with_connection(endpoint, None)
    }

    fn new_with_connection(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
    ) -> Self {
        Self::new_with_runtime_context(endpoint, connection_error, false, HubIo::new())
    }

    fn new_with_runtime_context(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
        package_storage_context_configured: bool,
        hub_io: HubIo,
    ) -> Self {
        Self::new_with_runtime_context_and_requirement(
            endpoint,
            connection_error,
            package_storage_context_configured,
            tui_compatibility_requirement(),
            hub_io,
        )
    }

    fn new_with_runtime_context_and_requirement(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
        package_storage_context_configured: bool,
        host_requirement: DaemonCompatibilityRequirement,
        hub_io: HubIo,
    ) -> Self {
        Self {
            endpoint,
            host_requirement,
            hub_io,
            connected_generation: None,
            reconnect_at: None,
            reconnect_failures: 0,
            pending_requests: BTreeMap::new(),
            status: "disconnected".to_string(),
            connection_error,
            error: None,
            action_feedback: None,
            compatibility: None,
            software: None,
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
            session_type_entities: SessionTypeEntityState::default(),
            session_type_subscription_error: None,
            entity_options: EntityOptionsStore::default(),
            entity_options_subscriptions: BTreeSet::new(),
            entity_options_retry: BTreeMap::new(),
            entity_options_invalid_fields: BTreeSet::new(),
            notice_subscriptions: BTreeMap::new(),
            notice_subscription_by_id: BTreeMap::new(),
            notice_parked: BTreeMap::new(),
            notice_overflow_dropped: 0,
            transient_notice: None,
            session_types_supported: true,
            spawn_targets: Vec::new(),
            selected_session_type_id: None,
            session_type_form: None,
            target_first_spawn: None,
            sessions: Vec::new(),
            selected_session: None,
            attached: None,
            schema_version: None,
            subscription_id: format!("btui-sub-{}", short_suffix()),
            next_terminal_subscription_sequence: 1,
            route_generation: None,
            route_epoch: None,
            ghostty_projection: None,
            ghostty_projection_session_id: None,
            ghostty_viewport_cache: None,
            projection_dirty: false,
            attach_hydration: None,
            attach_recovery_used: false,
            retired_subscription_ids: BTreeSet::new(),
            terminal_close_evidence: None,
            terminal_modes: None,
            input_window: InputWindow::new(),
            terminal_viewport_size: TerminalScreenSize::new(
                DEFAULT_TERMINAL_ROWS,
                DEFAULT_TERMINAL_COLS,
            ),
            drafts: BTreeMap::new(),
            system_details_visible: false,
            package_storage_context_configured,
            confirmation: None,
            #[cfg(test)]
            workspace_test_mode: false,
            acceptance_audit: None,
            #[cfg(test)]
            observed_requests: Vec::new(),
            #[cfg(test)]
            observed_terminal_inputs: Vec::new(),
            #[cfg(test)]
            list_for_target_stub: None,
            #[cfg(test)]
            applied_live_payloads: Vec::new(),
            #[cfg(test)]
            entity_options_forced_subscribe_error: None,
            #[cfg(test)]
            entity_options_subscribe_attempts: BTreeMap::new(),
        }
    }

    /// Whether a Hello-complete connection is installed.
    fn is_connected(&self) -> bool {
        self.connected_generation.is_some() && self.hub_io.is_connected()
    }

    /// Session id of the live attached route.
    fn attached_session_id(&self) -> Option<&str> {
        self.attached
            .as_ref()
            .map(|attached| attached.session_id.as_str())
    }

    /// Harness helper: apply wakes until `ready` holds or `deadline` passes.
    ///
    /// Input events are ignored here; harness drivers dispatch their own
    /// synthetic events. Returns whether `ready` held.
    fn pump_until(&mut self, deadline: Instant, mut ready: impl FnMut(&mut Self) -> bool) -> bool {
        loop {
            if ready(self) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            let until = self
                .next_deadline()
                .map_or(deadline, |candidate| candidate.min(deadline));
            match self.hub_io.next_wake(Some(until)) {
                AppWake::Input(_) => {}
                AppWake::Shutdown => return ready(self),
                other => self.apply_wake(other),
            }
        }
    }

    /// Harness helper: wait at most until `until` for one wake and apply it.
    fn pump_once(&mut self, until: Instant) {
        let until = self
            .next_deadline()
            .map_or(until, |candidate| candidate.min(until));
        match self.hub_io.next_wake(Some(until)) {
            AppWake::Input(_) | AppWake::Shutdown => {}
            other => self.apply_wake(other),
        }
        while let Some(wake) = self.hub_io.try_next_wake() {
            match wake {
                AppWake::Input(_) | AppWake::Shutdown => {}
                other => self.apply_wake(other),
            }
        }
    }

    /// Harness helper: apply wakes until no host-control request is outstanding.
    fn settle(&mut self, deadline: Instant) -> bool {
        self.pump_until(deadline, |app| app.pending_requests.is_empty())
    }

    /// Text of the projected viewport, rows joined by newlines.
    fn viewport_text(&mut self) -> String {
        self.prepare_paint();
        let Some(viewport) = self.ghostty_viewport_cache.as_ref() else {
            return String::new();
        };
        let cols = viewport.cols as usize;
        if cols == 0 {
            return String::new();
        }
        let mut text = String::new();
        for (index, cell) in viewport.cells.iter().enumerate() {
            if index > 0 && index % cols == 0 {
                text.push('\n');
            }
            if cell.grapheme.is_empty() {
                text.push(' ');
            } else {
                text.push_str(&cell.grapheme);
            }
        }
        text
    }

    /// Earliest absolute deadline the loop must wake for.
    fn next_deadline(&self) -> Option<Instant> {
        let mut deadline = self.hub_io.earliest_deadline();
        let mut consider = |candidate: Option<Instant>| {
            if let Some(candidate) = candidate {
                deadline = Some(deadline.map_or(candidate, |current| current.min(candidate)));
            }
        };
        consider(self.reconnect_at);
        consider(self.transient_notice.as_ref().map(|notice| notice.deadline));
        consider(
            self.entity_options_retry
                .values()
                .map(|state| state.next_attempt_at)
                .min(),
        );
        deadline
    }

    /// Block for the next wake or the earliest deadline.
    fn next_wake(&mut self) -> AppWake {
        let until = self.next_deadline();
        self.hub_io.next_wake(until)
    }

    /// Take one wake without blocking.
    fn try_next_wake(&mut self) -> Option<AppWake> {
        self.hub_io.try_next_wake()
    }

    /// Apply one non-input wake.
    fn apply_wake(&mut self, wake: AppWake) {
        match wake {
            AppWake::Input(_) | AppWake::Shutdown => {}
            AppWake::Terminal(routed) => self.apply_routed_terminal_frame(routed),
            AppWake::Completed { request_id, result } => self.complete_request(request_id, result),
            AppWake::Event(event) => self.apply_mux_event(event),
            AppWake::Entity(frame) => self.apply_entity_frame(frame),
            AppWake::Connected { generation, ack } => self.apply_connected(generation, *ack),
            AppWake::Disconnected { generation, error } => {
                self.apply_disconnected(generation, error);
            }
            AppWake::RouteFault {
                route,
                generation,
                reason,
            } => self.apply_route_fault(route, generation, &reason),
            AppWake::Deadline => self.apply_deadlines(),
        }
    }

    /// Run every absolute-deadline action that is due.
    fn apply_deadlines(&mut self) {
        let now = Instant::now();
        self.expire_transient_notice();
        if self.reconnect_at.is_some_and(|at| at <= now) {
            self.reconnect_at = None;
            self.connect();
        }
        if self.is_connected() {
            self.heal_entity_options_subscriptions();
        }
    }

    /// Project the viewport once when the projection changed since last paint.
    fn prepare_paint(&mut self) {
        self.expire_transient_notice();
        if self.projection_dirty {
            self.refresh_ghostty_viewport_cache();
        }
    }

    /// Stop the I/O owner within the shutdown bound.
    fn shutdown(self) {
        let mut app = self;
        app.detach_owner_if_writable();
        app.hub_io.shutdown(SHUTDOWN_BOUND);
    }

    fn set_drafts(&mut self, drafts: BTreeMap<String, Value>) {
        self.drafts = drafts;
        // A fresh router draft for a previously invalidated field clears the banner.
        self.entity_options_invalid_fields
            .retain(|field| !self.drafts.contains_key(field));
        self.reconcile_entity_option_drafts();
    }

    fn set_selected_session(&mut self, session_id: Option<String>) {
        if self.selected_session == session_id {
            return;
        }
        self.selected_session = session_id;
        self.sync_notice_subscriptions();
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
            self.set_selected_session(Some(session_id.to_string()));
        }
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
            InputDispatch::TerminalResize { rows, cols, .. } => {
                let size = TerminalScreenSize::new(rows, cols);
                if let Some(hydration) = self.attach_hydration.as_mut() {
                    hydration.pending_resize = Some(size);
                    return;
                }
                self.apply_local_resize(size);
                self.send_resize(size);
            }
            InputDispatch::Scroll { node_id, lines } => {
                // Map kit scroll deltas on the terminal node to Ghostty ScrollOp.
                // Non-terminal scroll areas are kit-owned presentation scroll.
                if is_terminal_node(Some(node_id.as_str())) && lines != 0 {
                    self.scroll_projection(ScrollOp::Delta(i32::from(lines)));
                }
            }
            // Kit classic key bytes never reach the PTY: the TUI intercepts
            // terminal-focused keys before the router and sends typed KEY frames.
            InputDispatch::TerminalForward { .. }
            | InputDispatch::Hover { .. }
            | InputDispatch::Focus { .. }
            | InputDispatch::Ignored => {}
        }
    }

    /// Terminal-focused keys become typed KEY frames. While a route is still
    /// attaching the key is queued in order and released on the live path.
    fn handle_focused_terminal_key(
        &mut self,
        key: KeyEvent,
        focused_node_id: Option<&str>,
    ) -> bool {
        if !is_terminal_node(focused_node_id) {
            return false;
        }
        if self.attach_hydration.is_some() {
            self.queue_pending_input(PendingTerminalInput::Key(key));
            return true;
        }
        if self.attached.is_none() {
            return false;
        }
        self.send_key(key);
        true
    }

    fn handle_focused_terminal_paste(&mut self, text: &str, focused_node_id: Option<&str>) -> bool {
        if !is_terminal_node(focused_node_id) {
            return false;
        }
        if text.is_empty() {
            return true;
        }
        let data = text.as_bytes().to_vec();
        if self.attach_hydration.is_some() {
            if self.input_window.has_paste()
                || self.attach_hydration.as_ref().is_some_and(|hydration| {
                    hydration
                        .pending_input
                        .iter()
                        .any(|input| matches!(input, PendingTerminalInput::Paste(_)))
                })
            {
                self.error =
                    Some("terminal paste unavailable: another paste is in flight".to_string());
                return true;
            }
            self.queue_pending_input(PendingTerminalInput::Paste(data));
            return true;
        }
        if self.attached.is_none() {
            self.error = Some(
                "terminal stream unavailable: attach a session before sending terminal input"
                    .to_string(),
            );
            return true;
        }
        self.send_paste(data);
        true
    }

    /// Mouse events over the live terminal become MOUSE frames when the nested
    /// application enabled mouse tracking. Other pointer events stay with the kit.
    fn handle_focused_terminal_mouse(
        &mut self,
        mouse: MouseEvent,
        focused_node_id: Option<&str>,
        hit_map: &HitMap,
    ) -> bool {
        if !is_terminal_node(focused_node_id) || self.attached.is_none() {
            return false;
        }
        if !terminal_input::mouse_tracking_enabled(self.current_mode_bits()) {
            return false;
        }
        let Some(outer) = tui_terminal_region(hit_map) else {
            return false;
        };
        // Occluded points (open menus, modals) belong to the kit router.
        if !hit_map
            .lookup(mouse.column, mouse.row)
            .is_some_and(|region| is_terminal_node(Some(region.node_id.as_str())))
        {
            return false;
        }
        let inner = botster_tui_kit::terminal_inner_rect(outer);
        self.send_mouse(mouse, inner)
    }

    /// Host focus changes are forwarded as FOCUS frames on the live route.
    fn handle_host_focus(&mut self, focused: bool) {
        if self.attach_hydration.is_some() {
            self.queue_pending_input(PendingTerminalInput::Focus(focused));
            return;
        }
        if self.attached.is_some() {
            self.send_focus(focused);
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
        self.submit_apply(DaemonRequest::PluginSurfaceAction {
            package_name,
            request,
        });
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

    fn handle_tui_owned_key(&mut self, key: KeyEvent, focused_node_id: Option<&str>) -> bool {
        // Terminal scroll shortcuts only when the production terminal owns focus.
        let terminal_focused = focused_node_id == Some("tui-terminal")
            || focused_node_id == Some("tui-terminal-output");
        if terminal_focused && self.ghostty_projection.is_some() && self.attached.is_some() {
            if key.modifiers == KeyModifiers::NONE {
                match key.code {
                    KeyCode::PageUp => {
                        self.scroll_projection(ScrollOp::Delta(
                            -(i32::from(self.terminal_viewport_size.rows)),
                        ));
                        return true;
                    }
                    KeyCode::PageDown => {
                        self.scroll_projection(ScrollOp::Delta(i32::from(
                            self.terminal_viewport_size.rows,
                        )));
                        return true;
                    }
                    _ => {}
                }
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Home => {
                        self.scroll_projection(ScrollOp::Top);
                        return true;
                    }
                    KeyCode::End => {
                        self.scroll_projection(ScrollOp::Bottom);
                        return true;
                    }
                    _ => {}
                }
            }
        }
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
        self.drop_entity_options_subscriptions();
        self.entity_options_invalid_fields.clear();
        if self.is_connected() {
            self.sync_entity_options_subscriptions();
        }
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
                let body_replaced = transition.replacement.is_some();
                if let Some(replacement) = transition.replacement {
                    // Owner-authored replacement is authoritative, including success /
                    // confirmation trees that drop entity-options producers.
                    surface.body = replacement;
                    // The snapshot validates the Hub-delivered tree at ingestion. An accepted
                    // action replacement is app-owned active state and must not leave a second,
                    // stale structural tree that looks current.
                    surface.ui_tree_snapshot = None;
                }
                self.pending_plugin_request = None;
                self.action_feedback = Some(plugin_action_result_text(&result));
                self.plugin_action_result = Some(result);
                // Replacement can add/remove options_source families — resync demand.
                if body_replaced {
                    self.sync_entity_options_subscriptions();
                }
            }
            Err(error) => {
                self.error = Some(format!("invalid plugin action result: {error}"));
            }
        }
    }

    fn handle_action(
        &mut self,
        action_id: String,
        values: Option<UiFormValues>,
        payload: Option<Value>,
    ) {
        if let Some(values) = values.as_ref() {
            self.apply_session_type_form_values(values);
            self.apply_spawn_flow_values(values);
        }

        match action_id.as_str() {
            "botster.tui.connect" => self.force_reconnect(),
            "botster.tui.spawn" => self.begin_target_first_spawn(),
            "botster.tui.attach" => {
                if let Some(session_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_id"))
                    .and_then(Value::as_str)
                {
                    self.set_selected_session(Some(session_id.to_string()));
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
                            self.submit_apply(DaemonRequest::ShutdownSession { session_id });
                        }
                        DestructiveAction::Remove(session_id) => {
                            self.action_feedback = Some(format!("remove requested: {session_id}"));
                            self.submit_apply(DaemonRequest::RemoveSession { session_id });
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
                    self.submit_apply(DaemonRequest::ShowPackage { package_name });
                }
            }
            "botster.tui.package.enable" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("enable requested: {package_name}"));
                    self.submit_apply(DaemonRequest::EnablePackage { package_name });
                }
            }
            "botster.tui.package.disable" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("disable requested: {package_name}"));
                    self.submit_apply(DaemonRequest::DisablePackage { package_name });
                }
            }
            "botster.tui.package.remove" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("remove requested: {package_name}"));
                    self.submit_apply(DaemonRequest::RemovePackage { package_name });
                }
            }
            "botster.tui.package.update_status" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("update status requested: {package_name}"));
                    self.submit_apply(DaemonRequest::CheckPackageUpdate { package_name });
                }
            }
            "botster.tui.package.update_preview" => {
                if let Some((package_name, pin)) = package_name_and_pin_from_payload(&payload) {
                    self.action_feedback =
                        Some(format!("update preview requested: {package_name}"));
                    self.submit_apply(DaemonRequest::PreviewPackageUpdate { package_name, pin });
                }
            }
            "botster.tui.package.update_apply" => {
                if let Some((package_name, pin)) = package_name_and_pin_from_payload(&payload) {
                    self.action_feedback = Some(format!("update apply requested: {package_name}"));
                    self.submit_apply(DaemonRequest::ApplyPackageUpdate { package_name, pin });
                }
            }
            "botster.tui.entrypoint.start" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint start requested: {package_name}/{entrypoint_id}"
                    ));
                    self.submit_apply(DaemonRequest::StartPackageEntrypoint {
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
                    self.submit_apply(DaemonRequest::StopPackageEntrypoint {
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
                    self.submit_apply(DaemonRequest::RestartPackageEntrypoint {
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
                    self.submit_apply(DaemonRequest::PackageEntrypointStatus {
                        package_name,
                        entrypoint_id,
                    });
                }
            }
            // The input router already focuses the terminal. Attachment is an
            "botster.tui.session_type.select" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.selected_session_type_id = Some(session_type_id.to_string());
                }
            }
            "botster.tui.session_type.create" => {
                self.session_type_form = Some(SessionTypeFormDraft::create_default());
            }
            "botster.tui.session_type.edit" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.open_session_type_edit(session_type_id);
                }
            }
            "botster.tui.session_type.delete" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.delete_session_type(session_type_id);
                }
            }
            "botster.tui.session_type.form.cancel" => {
                self.session_type_form = None;
            }
            "botster.tui.session_type.form.submit" => {
                self.submit_session_type_form();
            }
            "botster.tui.spawn.cancel" => {
                self.target_first_spawn = None;
            }
            "botster.tui.spawn.pick_target" => {
                if let Some(target_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("target_id"))
                    .and_then(Value::as_str)
                {
                    self.spawn_pick_target(target_id);
                }
            }
            "botster.tui.spawn.pick_session_type" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.spawn_pick_session_type(session_type_id);
                }
            }
            "botster.tui.spawn.submit" => {
                self.submit_target_first_spawn();
            }
            // explicit session activation and must not be a terminal side effect.
            "botster.terminal.focus" => {}
            _ => {}
        }
    }

    /// Start a connection attempt on the I/O owner. Hello completes as
    /// `AppWake::Connected` or `AppWake::Disconnected`.
    fn connect(&mut self) {
        self.reconnect_at = None;
        let Some(endpoint) = self.endpoint.clone() else {
            self.status = "Hub connection not configured".to_string();
            if self.connection_error.is_none() {
                self.connection_error = Some("BOTSTER_HUB_CONNECTION is required".to_string());
            }
            return;
        };
        self.connected_generation = None;
        self.status = "connecting".to_string();
        self.hub_io.connect(
            endpoint,
            self.host_requirement.clone(),
            tui_terminal_compatibility_requirement(),
        );
    }

    /// Schedule the next reconnect with capped exponential backoff.
    fn schedule_reconnect(&mut self) {
        if self.endpoint.is_none() {
            return;
        }
        self.reconnect_failures = self.reconnect_failures.saturating_add(1);
        self.reconnect_at = Some(Instant::now() + reconnect_backoff_delay(self.reconnect_failures));
    }

    /// Operator-requested reconnect: detach, drop connection state, connect now.
    fn force_reconnect(&mut self) {
        self.detach_owner_if_writable();
        self.drop_connection_state();
        self.reconnect_failures = 0;
        self.connect();
    }

    /// Forget every connection-scoped state.
    fn drop_connection_state(&mut self) {
        self.hub_io.disconnect(DETACH_ON_DISCONNECT_BOUND);
        self.connected_generation = None;
        self.pending_requests.clear();
        self.reset_active_plugin_surface();
        self.invalidate_session_generation();
        self.invalidate_session_type_generation();
        self.drop_entity_options_subscriptions();
        self.clear_event_subscription_state();
        self.clear_route_state();
        self.attach_recovery_used = false;
        self.terminal_close_evidence = None;
    }

    /// Forget the current route: attachment, hydration, projection, modes, input window.
    fn clear_route_state(&mut self) {
        self.attached = None;
        self.drop_attach_hydration();
        self.route_generation = None;
        self.route_epoch = None;
        self.terminal_modes = None;
        self.resolve_unknown_input_operations("route closed");
        self.clear_ghostty_projection();
    }

    /// Resolve every in-flight input operation as unknown when its route
    /// closes. Their INPUT_RESULT frames can no longer arrive; the user sees
    /// one explicit line instead of a silently dropped result.
    fn resolve_unknown_input_operations(&mut self, reason: &str) {
        let unresolved = self.input_window.in_flight_len();
        self.input_window = InputWindow::new();
        if unresolved > 0 {
            self.action_feedback = Some(format!(
                "{unresolved} terminal input operation(s) unresolved: {reason}"
            ));
        }
    }

    fn apply_connected(&mut self, generation: u64, ack: DaemonHelloAck) {
        if generation != self.hub_io.generation() {
            return;
        }
        if let Err(error) = admit_terminal_hello(&ack) {
            self.hub_io.disconnect(Duration::ZERO);
            self.apply_link_failure(error);
            return;
        }
        self.connected_generation = Some(generation);
        self.reconnect_failures = 0;
        self.reconnect_at = None;
        self.status = "connected".to_string();
        self.connection_error = None;
        self.record_diagnostics(ack.diagnostics);
        self.refresh_read_models();
        self.start_session_subscription();
        self.start_session_type_subscription_if_supported();
        self.sync_notice_subscriptions();
        self.sync_entity_options_subscriptions();
    }

    fn apply_disconnected(&mut self, generation: u64, error: DaemonTransportError) {
        if generation != self.hub_io.generation() {
            return;
        }
        self.apply_link_failure(error);
    }

    /// The connection ended. Reset connection-scoped state and schedule a reconnect.
    fn apply_link_failure(&mut self, error: DaemonTransportError) {
        self.drop_connection_state();
        match error {
            DaemonTransportError::Protocol(message) => {
                self.status = "compatibility mismatch".to_string();
                self.connection_error = Some(format!(
                    "expected daemon protocol {PROTOCOL}; daemon protocol error: {message}"
                ));
                self.record_diagnostic(DaemonDiagnostic::compatibility_mismatch(message));
            }
            DaemonTransportError::ProtocolViolation(code) => {
                self.status = "protocol violation; reconnecting".to_string();
                let message = format!("hub protocol violation: {}", code.as_str());
                self.connection_error = Some(message.clone());
                self.record_diagnostic(DaemonDiagnostic::disconnected(message));
            }
            DaemonTransportError::Compatibility(error) => {
                self.status = "compatibility mismatch".to_string();
                self.connection_error = Some(error.diagnostic.clone());
                self.record_diagnostics(error.diagnostics);
            }
            DaemonTransportError::NotRunning => {
                self.status = "hub unavailable; reconnecting".to_string();
                self.connection_error = Some(DaemonTransportError::NotRunning.to_string());
            }
            DaemonTransportError::ClientDisconnected => {
                self.status = "disconnected; reconnecting".to_string();
                let message = DaemonTransportError::ClientDisconnected.to_string();
                self.connection_error = Some(message.clone());
                self.record_diagnostic(DaemonDiagnostic::disconnected(message));
            }
            DaemonTransportError::ClosedByHub(reason) => {
                self.status = "closed by hub; reconnecting".to_string();
                let message = format!("hub closed the connection: {reason:?}");
                self.connection_error = Some(message.clone());
                self.record_diagnostic(DaemonDiagnostic::disconnected(message));
            }
            other => {
                self.status = "reconnecting".to_string();
                self.connection_error = Some(other.to_string());
            }
        }
        self.schedule_reconnect();
    }

    fn refresh_read_models(&mut self) {
        self.refresh_status();
        self.refresh_apps();
        self.refresh_package_navigation();
        self.refresh_packages();
        self.refresh_spawn_targets();
    }

    /// Submit one request whose response only updates read models.
    fn submit_apply(&mut self, request: DaemonRequest) {
        self.submit(request, PendingReply::Apply, REQUEST_DEADLINE);
    }

    /// Submit one request with a continuation. Returns the request id.
    ///
    /// Completion, expiry, or loss arrives as `AppWake::Completed` and is
    /// routed through `complete_request`. Nothing waits here.
    fn submit(&mut self, request: DaemonRequest, reply: PendingReply, deadline: Duration) -> u64 {
        if let Some(audit) = &mut self.acceptance_audit {
            audit.record(&request);
        }
        #[cfg(test)]
        self.record_request(&request);
        let request_id = self.hub_io.submit(&request, Instant::now() + deadline);
        self.pending_requests.insert(request_id, reply);
        request_id
    }

    fn complete_request(
        &mut self,
        request_id: u64,
        result: Result<DaemonResponse, DaemonRequestError>,
    ) {
        let Some(reply) = self.pending_requests.remove(&request_id) else {
            return;
        };
        match result {
            Ok(response) => self.apply_completion(reply, response),
            Err(error) => self.apply_request_failure(reply, error),
        }
    }

    fn apply_completion(&mut self, reply: PendingReply, response: DaemonResponse) {
        match reply {
            PendingReply::Apply | PendingReply::Detach | PendingReply::Unsubscribe => {
                self.apply_response(response);
            }
            PendingReply::ListForTarget {
                target_id,
                target_label,
            } => self.apply_list_for_target(&target_id, &target_label, response),
            PendingReply::Spawn { session_id } => {
                let failed = response.error.is_some();
                self.apply_response(response);
                if failed {
                    self.pending_sessions.remove(&session_id);
                    self.rebuild_session_rows();
                } else if self.pending_sessions.contains_key(&session_id) {
                    self.action_feedback = Some(format!(
                        "spawn accepted: {session_id}; waiting for authoritative session"
                    ));
                }
            }
            PendingReply::ShowSessionTypeDefinition { session_type_id } => {
                self.apply_show_session_type_definition(&session_type_id, response);
            }
            PendingReply::SessionTypeForm => {
                let failed = response.error.clone();
                self.apply_response(response);
                match failed {
                    Some(error) => {
                        if let Some(form) = self.session_type_form.as_mut() {
                            form.error = Some(format!("{}: {}", error.code, error.message));
                        }
                    }
                    None => self.session_type_form = None,
                }
            }
            PendingReply::Attach { session_id, route } => {
                let failed = response.error.clone();
                let attached = response.terminal_attach.clone();
                self.apply_response(response);
                if !self.hydration_matches_route(&route) {
                    return;
                }
                if let Some(error) = failed {
                    self.fail_attach_campaign(
                        &session_id,
                        &route,
                        &format!("attach rejected: {}", error.message),
                        false,
                    );
                    return;
                }
                // The Attach response is the only source of the attachment
                // generation. Frames parked before it replay now.
                match attached {
                    Some(attach) if attach.subscription_id == route => {
                        self.route_generation = Some(attach.generation);
                        self.replay_pre_attach_frames();
                    }
                    _ => self.fail_attach_campaign(
                        &session_id,
                        &route,
                        "attach response omitted the terminal attachment",
                        true,
                    ),
                }
            }
            PendingReply::SubscribeEvents {
                key,
                subscription_id,
            } => self.complete_notice_subscription(&key, &subscription_id, response),
            PendingReply::SubscribeEntities {
                family,
                subscription_id,
            } => self.complete_entity_subscription(&family, &subscription_id, response),
        }
    }

    fn apply_request_failure(&mut self, reply: PendingReply, error: DaemonRequestError) {
        let message = error.to_string();
        match reply {
            PendingReply::Apply => self.error = Some(format!("request failed: {message}")),
            PendingReply::Detach | PendingReply::Unsubscribe => {}
            PendingReply::ListForTarget { target_label, .. } => {
                self.error = Some(format!("session types failed to load: {message}"));
                self.action_feedback = Some(format!(
                    "session types for {target_label} failed to load; pick another target or cancel"
                ));
            }
            PendingReply::Spawn { session_id } => {
                self.pending_sessions.remove(&session_id);
                self.rebuild_session_rows();
                self.error = Some(format!("spawn failed: {message}"));
            }
            PendingReply::ShowSessionTypeDefinition { session_type_id } => {
                self.error = Some(format!(
                    "show_session_type_definition failed for {session_type_id}: {message}"
                ));
            }
            PendingReply::SessionTypeForm => {
                if let Some(form) = self.session_type_form.as_mut() {
                    form.error = Some(message);
                }
            }
            PendingReply::Attach { session_id, route } => {
                if self.hydration_matches_route(&route) {
                    // The Hub may have attached after the deadline; retire the route
                    // with a bounded Detach so a late attachment cannot leak.
                    self.fail_attach_campaign(
                        &session_id,
                        &route,
                        &format!("attach request failed: {message}"),
                        true,
                    );
                }
            }
            PendingReply::SubscribeEvents {
                subscription_id, ..
            } => self.reject_event_subscription_candidate(
                &subscription_id,
                format!("event subscription failed: {message}"),
            ),
            PendingReply::SubscribeEntities {
                family,
                subscription_id,
            } => self.fail_entity_subscription(&family, &subscription_id, message),
        }
    }

    fn refresh_spawn_targets(&mut self) {
        self.submit_apply(DaemonRequest::ListSpawnTargets);
    }

    fn refresh_status(&mut self) {
        self.submit_apply(DaemonRequest::Status);
    }

    fn refresh_apps(&mut self) {
        self.submit_apply(DaemonRequest::ListApps);
    }

    fn refresh_package_navigation(&mut self) {
        self.submit_apply(DaemonRequest::ListPackageNavigation);
    }

    fn refresh_packages(&mut self) {
        self.submit_apply(DaemonRequest::ListPackages);
    }

    /// Subscribe to the built-in session family on the current connection.
    fn start_session_subscription(&mut self) {
        let subscription_id = format!("btui-sessions-{}", short_suffix());
        self.session_entities
            .begin_generation(subscription_id.clone());
        self.rebuild_session_rows();
        self.submit(
            DaemonRequest::SubscribeEntities {
                entity_type: "session".to_string(),
                subscription_id: subscription_id.clone(),
            },
            PendingReply::SubscribeEntities {
                family: "session".to_string(),
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    /// Drop the session generation and unsubscribe when connected.
    fn invalidate_session_generation(&mut self) {
        if let Some(subscription_id) = self.session_entities.subscription_id.take()
            && self.is_connected()
        {
            self.submit(
                DaemonRequest::UnsubscribeEntities { subscription_id },
                PendingReply::Unsubscribe,
                REQUEST_DEADLINE,
            );
        }
        self.session_entities = SessionEntityState::default();
        self.rebuild_session_rows();
    }

    fn start_session_type_subscription_if_supported(&mut self) {
        self.session_types_supported =
            Self::session_types_supported_from_compatibility(self.compatibility.as_ref());
        if !self.session_types_supported {
            self.invalidate_session_type_generation();
            self.session_type_subscription_error = None;
            return;
        }
        self.start_session_type_subscription();
    }

    fn start_session_type_subscription(&mut self) {
        let subscription_id = format!("btui-session-types-{}", short_suffix());
        self.session_type_entities
            .begin_generation(subscription_id.clone());
        self.session_type_subscription_error = None;
        self.submit(
            DaemonRequest::SubscribeEntities {
                entity_type: "session_type".to_string(),
                subscription_id: subscription_id.clone(),
            },
            PendingReply::SubscribeEntities {
                family: "session_type".to_string(),
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    fn invalidate_session_type_generation(&mut self) {
        if let Some(subscription_id) = self.session_type_entities.subscription_id.take()
            && self.is_connected()
        {
            self.submit(
                DaemonRequest::UnsubscribeEntities { subscription_id },
                PendingReply::Unsubscribe,
                REQUEST_DEADLINE,
            );
        }
        self.session_type_entities = SessionTypeEntityState::default();
        self.session_type_subscription_error = None;
    }

    /// Route one entity frame from the connection to its family state.
    fn apply_entity_frame(&mut self, frame: DaemonEntityFrame) {
        let entity_type = entity_frame_type(&frame).to_string();
        match entity_type.as_str() {
            "session" => match self.session_entities.apply(frame) {
                Ok(true) => {
                    self.rebuild_session_rows();
                    // Session family feeds entity-options when demanded.
                    self.reconcile_entity_option_drafts();
                }
                Ok(false) => {}
                Err(error) => {
                    self.error = Some(format!("session sync: {error}"));
                    self.invalidate_session_generation();
                    if self.is_connected() {
                        self.start_session_subscription();
                    }
                }
            },
            "session_type" => match self.session_type_entities.apply(frame) {
                Ok(true) => {
                    if self
                        .selected_session_type_id
                        .as_ref()
                        .is_some_and(|id| !self.session_type_entities.entities.contains_key(id))
                    {
                        self.selected_session_type_id = None;
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    self.session_type_subscription_error = Some(error.clone());
                    self.error = Some(format!("session type sync: {error}"));
                    self.invalidate_session_type_generation();
                    self.session_type_subscription_error = Some(error);
                    if self.is_connected() && self.session_types_supported {
                        self.start_session_type_subscription();
                    }
                }
            },
            family => self.apply_entity_options_frame(family, frame),
        }
    }

    fn complete_entity_subscription(
        &mut self,
        family: &str,
        subscription_id: &str,
        response: DaemonResponse,
    ) {
        self.record_diagnostics(response.diagnostics);
        if response.kind == DaemonResponseKind::EntitySubscribed && response.error.is_none() {
            if !is_process_wide_entity_family(family) {
                self.reset_entity_options_backoff(family);
            }
            return;
        }
        let detail = response
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| format!("{:?}", response.kind));
        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
        }
        self.fail_entity_subscription(
            family,
            subscription_id,
            format!("entity subscription was not accepted: {detail}"),
        );
    }

    fn fail_entity_subscription(&mut self, family: &str, subscription_id: &str, message: String) {
        match family {
            "session" => {
                if self.session_entities.subscription_id.as_deref() == Some(subscription_id) {
                    self.session_entities = SessionEntityState::default();
                    self.rebuild_session_rows();
                    self.error = Some(format!("session subscription failed: {message}"));
                }
            }
            "session_type" => {
                if self.session_type_entities.subscription_id.as_deref() == Some(subscription_id) {
                    self.session_type_entities = SessionTypeEntityState::default();
                    self.session_type_subscription_error = Some(message.clone());
                    self.error = Some(format!("session type subscription failed: {message}"));
                }
            }
            other => {
                let matches = self
                    .entity_options
                    .family(other)
                    .and_then(|state| state.subscription_id.as_deref())
                    == Some(subscription_id);
                if matches {
                    self.entity_options_subscriptions.remove(other);
                    self.entity_options.drop_family(other);
                    self.note_entity_options_admission_failure(other, message);
                }
            }
        }
    }

    fn drop_entity_options_subscriptions(&mut self) {
        self.drop_entity_options_families(None);
    }

    fn drop_entity_options_families(&mut self, keep: Option<BTreeSet<String>>) {
        let keep = keep.unwrap_or_default();
        let stale: Vec<String> = self
            .entity_options_subscriptions
            .iter()
            .filter(|family| !keep.contains(*family))
            .cloned()
            .collect();
        for family in stale {
            self.stop_entity_options_subscription(&family);
        }
        if keep.is_empty() {
            self.entity_options = EntityOptionsStore::default();
            self.entity_options_retry.clear();
        } else {
            self.entity_options.retain_families(&keep);
        }
    }

    /// Collect options_source families from the active plugin surface and ensure
    /// SubscribeEntities for non-process-wide families. Process-wide families
    /// (session, session_type) are served from the existing stores.
    fn sync_entity_options_subscriptions(&mut self) {
        let owned = self.demanded_entity_option_families_now();

        let stale: Vec<String> = self
            .entity_options_subscriptions
            .iter()
            .filter(|family| !owned.contains(*family))
            .cloned()
            .collect();
        for family in stale {
            self.stop_entity_options_subscription(&family);
        }
        self.entity_options.retain_families(&owned);

        if !self.is_connected() {
            self.reconcile_entity_option_drafts();
            return;
        }

        for family in owned {
            if self.entity_options_subscriptions.contains(&family) {
                continue;
            }
            if !self.entity_options_retry_ready(&family) {
                continue;
            }
            self.start_entity_options_subscription(&family);
        }
        self.reconcile_entity_option_drafts();
    }

    /// Unsubscribe one entity-options family and forget its generation.
    fn stop_entity_options_subscription(&mut self, family: &str) {
        self.entity_options_subscriptions.remove(family);
        let subscription_id = self
            .entity_options
            .family(family)
            .and_then(|state| state.subscription_id.clone());
        if let Some(subscription_id) = subscription_id
            && self.is_connected()
        {
            self.submit(
                DaemonRequest::UnsubscribeEntities { subscription_id },
                PendingReply::Unsubscribe,
                REQUEST_DEADLINE,
            );
        }
        self.entity_options.drop_family(family);
        self.entity_options_retry.remove(family);
    }

    fn start_entity_options_subscription(&mut self, entity_type: &str) {
        #[cfg(test)]
        {
            *self
                .entity_options_subscribe_attempts
                .entry(entity_type.to_string())
                .or_insert(0) += 1;
            if let Some(message) = self.entity_options_forced_subscribe_error {
                self.note_entity_options_admission_failure(entity_type, message.to_string());
                return;
            }
        }
        let subscription_id = format!("btui-entity-options-{entity_type}-{}", short_suffix());
        self.entity_options
            .begin_generation(entity_type, subscription_id.clone());
        self.entity_options_subscriptions
            .insert(entity_type.to_string());
        self.submit(
            DaemonRequest::SubscribeEntities {
                entity_type: entity_type.to_string(),
                subscription_id: subscription_id.clone(),
            },
            PendingReply::SubscribeEntities {
                family: entity_type.to_string(),
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    /// Re-open demanded entity-options families whose backoff expired.
    fn heal_entity_options_subscriptions(&mut self) {
        let demanded: Vec<String> = self
            .demanded_entity_option_families_now()
            .into_iter()
            .filter(|family| !self.entity_options_subscriptions.contains(family))
            .collect();
        for family in demanded {
            if !self.entity_options_retry_ready(&family) {
                continue;
            }
            self.start_entity_options_subscription(&family);
        }
    }

    fn note_entity_options_admission_failure(&mut self, family: &str, error: String) {
        let previous = self.entity_options_retry.get(family).cloned();
        let consecutive_failures = previous
            .as_ref()
            .map(|state| state.consecutive_failures.saturating_add(1))
            .unwrap_or(1);
        let delay = entity_options_backoff_delay(consecutive_failures);
        self.entity_options_retry.insert(
            family.to_string(),
            EntityOptionsRetryState {
                consecutive_failures,
                next_attempt_at: Instant::now() + delay,
            },
        );
        if previous.is_none() {
            self.error = Some(format!(
                "entity options subscription failed for {family}: {error}"
            ));
        }
    }

    /// Apply one entity-options frame. A sync error drops the generation and
    /// opens a fresh SubscribeEntities when the family is still demanded.
    fn apply_entity_options_frame(&mut self, family: &str, frame: DaemonEntityFrame) {
        match self.entity_options.apply_daemon_frame(frame) {
            Ok(true) => self.reconcile_entity_option_drafts(),
            Ok(false) => {}
            Err(error) => {
                self.error = Some(format!("entity options sync: {error}"));
                self.stop_entity_options_subscription(family);
                if self.is_connected()
                    && self.family_still_demanded(family)
                    && self.entity_options_retry_ready(family)
                {
                    self.start_entity_options_subscription(family);
                }
            }
        }
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
            self.set_selected_session(
                self.sessions
                    .first()
                    .map(|session| session.session_id.clone()),
            );
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
        self.submit_apply(DaemonRequest::PluginSurfaceRender {
            package_name,
            surface_id,
            payload: json!({}),
        });
    }

    fn launch_target_options(&self) -> Vec<LaunchTargetOption> {
        let mut options: BTreeMap<String, LaunchTargetOption> = BTreeMap::new();
        for target in &self.spawn_targets {
            if !target.enabled {
                continue;
            }
            options.insert(
                target.target_id.clone(),
                LaunchTargetOption {
                    target_id: target.target_id.clone(),
                    label: target.label.clone(),
                },
            );
        }
        options.into_values().collect()
    }

    fn begin_target_first_spawn(&mut self) {
        self.error = None;
        self.session_type_form = None;
        if !self.session_types_supported {
            self.error = Some(
                "session types unavailable: hub does not provide session_type_entity_subscriptions"
                    .to_string(),
            );
            return;
        }
        if self.launch_target_options().is_empty() {
            self.error =
                Some("no launch targets available (no enabled admitted spawn targets)".to_string());
            return;
        }
        self.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickTarget,
        });
        self.action_feedback = Some("select a launch target".to_string());
    }

    fn spawn_pick_target(&mut self, target_id: &str) {
        let Some(target) = self
            .launch_target_options()
            .into_iter()
            .find(|target| target.target_id == target_id)
        else {
            self.error = Some(format!("launch target not found: {target_id}"));
            return;
        };
        // Clear any prior picker rows before the list request so a failed
        // load cannot leave selectable stale rows from a previous target.
        self.error = None;
        self.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickTarget,
        });
        #[cfg(test)]
        if let Some(stub) = self.list_for_target_stub.take() {
            match stub {
                ListForTargetStub::Ok(session_types) => {
                    self.target_first_spawn = Some(TargetFirstSpawnFlow {
                        step: TargetFirstSpawnStep::PickSessionType {
                            target_id: target.target_id.clone(),
                            target_label: target.label.clone(),
                            session_types,
                        },
                    });
                    self.action_feedback =
                        Some(format!("select a session type for {}", target.label));
                }
                ListForTargetStub::OperatorError {
                    code,
                    operation,
                    message,
                } => {
                    self.error = Some(format!("{message} (code={code} operation={operation})"));
                    self.action_feedback = Some(format!(
                        "session types for {} unavailable; pick another target or cancel",
                        target.label
                    ));
                }
                ListForTargetStub::TransportError => self.apply_request_failure(
                    PendingReply::ListForTarget {
                        target_id: target.target_id.clone(),
                        target_label: target.label.clone(),
                    },
                    DaemonRequestError::ConnectionClosed,
                ),
            }
            return;
        }
        self.submit(
            DaemonRequest::ListSessionTypesForTarget {
                target_id: target.target_id.clone(),
            },
            PendingReply::ListForTarget {
                target_id: target.target_id,
                target_label: target.label,
            },
            REQUEST_DEADLINE,
        );
    }

    /// ListSessionTypesForTarget completed. Flow-local only: the entity store
    /// is not touched.
    fn apply_list_for_target(
        &mut self,
        target_id: &str,
        target_label: &str,
        response: DaemonResponse,
    ) {
        let picking = matches!(
            self.target_first_spawn.as_ref().map(|flow| &flow.step),
            Some(TargetFirstSpawnStep::PickTarget)
        );
        if !picking {
            return;
        }
        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
            self.error = Some(format!(
                "{} (code={} operation={})",
                error.message, error.code, error.operation
            ));
            self.action_feedback = Some(format!(
                "session types for {target_label} unavailable; pick another target or cancel"
            ));
            return;
        }
        self.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickSessionType {
                target_id: target_id.to_string(),
                target_label: target_label.to_string(),
                session_types: response.session_types,
            },
        });
        self.action_feedback = Some(format!("select a session type for {target_label}"));
    }

    fn execute_spawn_session_type(
        &mut self,
        session_type_id: &str,
        request: DaemonSessionTypeRequest,
    ) {
        self.error = None;
        self.target_first_spawn = None;
        let session_id = format!("btui-{}", short_suffix());
        self.pending_sessions
            .insert(session_id.clone(), SessionRow::pending(session_id.clone()));
        self.set_selected_session(Some(session_id.clone()));
        self.rebuild_session_rows();
        self.action_feedback = Some(format!("spawn pending: {session_id} via {session_type_id}"));
        self.submit(
            DaemonRequest::SpawnSessionType {
                session_type_id: session_type_id.to_string(),
                session_id: session_id.clone(),
                request,
            },
            PendingReply::Spawn { session_id },
            REQUEST_DEADLINE,
        );
    }

    fn open_session_type_edit(&mut self, session_type_id: &str) {
        self.error = None;
        self.action_feedback = Some(format!("loading authoring definition: {session_type_id}"));
        self.submit(
            DaemonRequest::ShowSessionTypeDefinition {
                session_type_id: session_type_id.to_string(),
            },
            PendingReply::ShowSessionTypeDefinition {
                session_type_id: session_type_id.to_string(),
            },
            REQUEST_DEADLINE,
        );
    }

    fn apply_show_session_type_definition(
        &mut self,
        session_type_id: &str,
        response: DaemonResponse,
    ) {
        if let Some(error) = response.error.clone() {
            self.apply_response(response);
            self.error = Some(format!("{}: {}", error.code, error.message));
            return;
        }
        let definition = response.session_type_definition.clone();
        self.apply_response(response);
        match definition {
            Some(editable) => {
                self.session_type_form = Some(SessionTypeFormDraft::from_authoring(editable));
                self.action_feedback = Some(format!("edit ready: {session_type_id}"));
            }
            None => {
                self.error =
                    Some("show_session_type_definition returned no definition".to_string());
            }
        }
    }

    fn delete_session_type(&mut self, session_type_id: &str) {
        let Some(entity) = self
            .session_type_entities
            .entities
            .get(session_type_id)
            .cloned()
        else {
            self.error = Some(format!("session type not found: {session_type_id}"));
            return;
        };
        if !entity.editable {
            self.error = Some(format!("session type is not editable: {session_type_id}"));
            return;
        }
        // Prefer source_name (owning source) over target_id (eligibility stamp),
        // matching Hub show_session_type_definition mutation source construction.
        let source = match entity.source.as_str() {
            "device" => DaemonSessionTypeMutationSource::Device,
            "repo" => DaemonSessionTypeMutationSource::Repo {
                target_id: if !entity.source_name.is_empty() {
                    entity.source_name.clone()
                } else {
                    entity.target_id.clone()
                },
            },
            other => {
                self.error = Some(format!("cannot delete session type source: {other}"));
                return;
            }
        };
        self.action_feedback = Some(format!("delete requested: {session_type_id}"));
        self.submit_apply(DaemonRequest::DeleteSessionType {
            source,
            session_type_id: entity.id.clone(),
        });
    }

    fn submit_session_type_form(&mut self) {
        let Some(form) = self.session_type_form.clone() else {
            return;
        };
        if form.id.trim().is_empty()
            || form.label.trim().is_empty()
            || form.role.trim().is_empty()
            || form.interaction.trim().is_empty()
            || form.lifecycle.trim().is_empty()
            || form.command.trim().is_empty()
        {
            if let Some(form) = self.session_type_form.as_mut() {
                form.error = Some(
                    "id, label, role, interaction, lifecycle, and command are required".to_string(),
                );
            }
            return;
        }
        let source = match mutation_source_from_form(&form) {
            Ok(source) => source,
            Err(error) => {
                if let Some(form) = self.session_type_form.as_mut() {
                    form.error = Some(error);
                }
                return;
            }
        };
        let definition = match definition_from_session_type_form(&form) {
            Ok(definition) => definition,
            Err(error) => {
                if let Some(form) = self.session_type_form.as_mut() {
                    form.error = Some(error);
                }
                return;
            }
        };
        let request = match form.mode {
            SessionTypeFormMode::Create => DaemonRequest::CreateSessionType { source, definition },
            SessionTypeFormMode::Edit => DaemonRequest::UpdateSessionType { source, definition },
        };
        self.action_feedback = Some(match form.mode {
            SessionTypeFormMode::Create => "create session type requested".to_string(),
            SessionTypeFormMode::Edit => "update session type requested".to_string(),
        });
        self.submit(request, PendingReply::SessionTypeForm, REQUEST_DEADLINE);
    }

    fn spawn_pick_session_type(&mut self, session_type_id: &str) {
        let Some(flow) = self.target_first_spawn.as_ref() else {
            return;
        };
        let TargetFirstSpawnStep::PickSessionType {
            target_id,
            target_label,
            session_types,
        } = &flow.step
        else {
            return;
        };
        let Some(session_type) = session_types
            .iter()
            .find(|session_type| session_type.session_type_id == session_type_id)
            .cloned()
        else {
            self.error = Some(format!("session type not found: {session_type_id}"));
            return;
        };
        if !session_type.available {
            self.error = Some(format!(
                "session type unavailable: {}{}",
                session_type.session_type_id,
                if session_type.diagnostics.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", session_type.diagnostics.join("; "))
                }
            ));
            return;
        }
        let needs_prompt = session_type.context_keys.iter().any(|key| key == "prompt");
        if needs_prompt {
            self.target_first_spawn = Some(TargetFirstSpawnFlow {
                step: TargetFirstSpawnStep::Prompt {
                    target_id: target_id.clone(),
                    target_label: target_label.clone(),
                    session_type_id: session_type_id.to_string(),
                    prompt: String::new(),
                },
            });
            self.action_feedback = Some("enter prompt context".to_string());
            return;
        }
        self.execute_spawn_session_type(
            session_type_id,
            DaemonSessionTypeRequest {
                target_id: Some(target_id.clone()),
                ..DaemonSessionTypeRequest::default()
            },
        );
    }

    fn submit_target_first_spawn(&mut self) {
        let Some(flow) = self.target_first_spawn.clone() else {
            return;
        };
        match flow.step {
            TargetFirstSpawnStep::Prompt {
                target_id,
                session_type_id,
                prompt,
                ..
            } => {
                let mut request = DaemonSessionTypeRequest {
                    target_id: Some(target_id.clone()),
                    ..DaemonSessionTypeRequest::default()
                };
                if !prompt.trim().is_empty() {
                    request.context.prompt = Some(prompt.trim().to_string());
                }
                self.execute_spawn_session_type(&session_type_id, request);
            }
            _ => {
                self.error = Some("spawn form is incomplete".to_string());
            }
        }
    }

    fn session_types_supported_from_compatibility(
        compatibility: Option<&DaemonCompatibility>,
    ) -> bool {
        // Permissive only before Hub status arrives (web parity).
        let Some(compatibility) = compatibility else {
            return true;
        };
        compatibility.supports_feature(FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS)
    }

    /// Build the multi-family store for shared projection, injecting process-wide
    /// session / session_type maps when those families are demanded.
    fn entity_options_projection_store(&self) -> EntityFamilyStore {
        let mut process_wide = EntityFamilyStore::new();
        if !self.session_entities.entities.is_empty() || self.session_entities.has_snapshot {
            let mut session_records = BTreeMap::new();
            for (id, entity) in &self.session_entities.entities {
                if let Ok(Value::Object(fields)) = serde_json::to_value(entity) {
                    session_records.insert(id.clone(), fields);
                }
            }
            process_wide.insert("session".to_string(), session_records);
        }
        if !self.session_type_entities.entities.is_empty()
            || self.session_type_entities.has_snapshot
        {
            let mut type_records = BTreeMap::new();
            for (id, entity) in &self.session_type_entities.entities {
                if let Ok(Value::Object(fields)) = serde_json::to_value(entity) {
                    type_records.insert(id.clone(), fields);
                }
            }
            process_wide.insert("session_type".to_string(), type_records);
        }
        self.entity_options
            .projection_store_with_process_wide(&process_wide)
    }

    fn demanded_entity_option_families_now(&self) -> BTreeSet<String> {
        let mut owned = BTreeSet::new();
        if let Some(surface) = self.plugin_surface.as_ref() {
            owned.extend(
                demanded_entity_option_families(&surface.body)
                    .into_iter()
                    .filter(|family| !is_process_wide_entity_family(family)),
            );
        }
        owned
    }

    fn entity_options_retry_ready(&self, family: &str) -> bool {
        self.entity_options_retry
            .get(family)
            .is_none_or(|state| Instant::now() >= state.next_attempt_at)
    }

    fn reset_entity_options_backoff(&mut self, family: &str) {
        self.entity_options_retry.remove(family);
    }

    fn family_still_demanded(&self, family: &str) -> bool {
        self.demanded_entity_option_families_now().contains(family)
    }

    /// Clear drafts whose selected values disappeared or became excluded.
    fn reconcile_entity_option_drafts(&mut self) {
        let Some(surface) = self.plugin_surface.as_ref() else {
            return;
        };
        let store = self.entity_options_projection_store();
        let mut invalid = BTreeSet::new();
        collect_invalid_entity_option_fields(&surface.body, &store, &self.drafts, &mut invalid);
        for field in &invalid {
            self.drafts.remove(field);
            self.entity_options_invalid_fields.insert(field.clone());
        }
    }

    fn apply_session_type_form_values(&mut self, values: &UiFormValues) {
        let Some(form) = self.session_type_form.as_mut() else {
            return;
        };
        let set = |key: &str, target: &mut String| {
            if let Some(value) = values.0.get(key).and_then(Value::as_str) {
                *target = value.to_string();
            }
        };
        set("session_type_source", &mut form.source);
        set("session_type_source_target_id", &mut form.source_target_id);
        set("session_type_id", &mut form.id);
        set("session_type_label", &mut form.label);
        set("session_type_description", &mut form.description);
        set("session_type_icon", &mut form.icon);
        set("session_type_role", &mut form.role);
        set("session_type_interaction", &mut form.interaction);
        set("session_type_traits", &mut form.traits);
        set("session_type_lifecycle", &mut form.lifecycle);
        set("session_type_execution", &mut form.execution);
        set("session_type_command", &mut form.command);
        set("session_type_args", &mut form.args);
        set(
            "session_type_working_directory_policy",
            &mut form.working_directory_policy,
        );
        set(
            "session_type_working_directory_path",
            &mut form.working_directory_path,
        );
        set("session_type_environment", &mut form.environment);
        set(
            "session_type_allowed_environment_overrides",
            &mut form.allowed_environment_overrides,
        );
        set("session_type_context_keys", &mut form.context_keys);
    }

    fn apply_spawn_flow_values(&mut self, values: &UiFormValues) {
        let Some(flow) = self.target_first_spawn.as_mut() else {
            return;
        };
        if let TargetFirstSpawnStep::Prompt { prompt, .. } = &mut flow.step
            && let Some(value) = values.0.get("spawn_prompt").and_then(Value::as_str)
        {
            *prompt = value.to_string();
        }
    }

    fn attach_selected_or_first(&mut self) {
        let Some(session_id) = self.selected_attachable_session_id() else {
            return;
        };
        self.error = None;
        self.set_selected_session(Some(session_id.clone()));
        self.action_feedback = Some(format!("attach requested: {session_id}"));
        self.detach_owner_if_writable();
        self.reset_attach_campaign();
        let route = self.mint_subscription_id();
        self.begin_attach_hydration(&session_id, &route);
        self.submit(
            DaemonRequest::Attach {
                session_id: session_id.clone(),
                subscription_id: route.clone(),
            },
            PendingReply::Attach { session_id, route },
            REQUEST_DEADLINE,
        );
    }

    fn begin_attach_hydration(&mut self, session_id: &str, route: &str) {
        // Every Attach owns a unique route and one incremental decoder.
        self.subscription_id = route.to_string();
        self.attached = None;
        self.route_generation = None;
        self.route_epoch = None;
        self.resolve_unknown_input_operations("route replaced");
        self.terminal_modes = None;
        self.clear_ghostty_projection();
        self.drop_attach_hydration();
        self.attach_hydration = Some(AttachHydration::new(session_id, route));
    }

    /// The Attach request itself failed. Close the campaign without a retry.
    fn fail_attach_campaign(&mut self, session_id: &str, route: &str, reason: &str, detach: bool) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_route_state();
        self.retire_subscription(route);
        if detach && self.is_connected() {
            self.send_bounded_detach(session_id.to_string(), route.to_string());
        }
        self.error = Some(format!("attach failed (closed): {reason}: {session_id}"));
    }

    fn detach_attached(&mut self) {
        let cancelling_hydration = self.attach_hydration.is_some();
        let Some((session_id, route)) = self.current_owner_pair() else {
            self.error = Some("no attached terminal stream to detach".to_string());
            return;
        };
        self.error = None;
        self.action_feedback = Some(format!("detach requested: {session_id}"));
        if cancelling_hydration && let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        // A detached projection stays readable for scrollback; hydration state does not.
        if cancelling_hydration {
            self.clear_ghostty_projection();
        }
        self.retire_subscription(&route);
        self.drop_attach_hydration();
        self.attached = None;
        self.terminal_modes = None;
        self.send_bounded_detach(session_id, route);
    }

    fn send_bounded_detach(&mut self, session_id: String, route: String) {
        self.submit(
            DaemonRequest::Detach {
                session_id,
                subscription_id: route,
            },
            PendingReply::Detach,
            DETACH_ON_DISCONNECT_BOUND,
        );
    }

    /// Retire the current route and send a bounded Detach when connected.
    fn detach_owner_if_writable(&mut self) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            return;
        };
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.retire_subscription(&route);
        if !self.is_connected() {
            return;
        }
        self.send_bounded_detach(session_id, route);
    }

    /// Retired routes ignore late frames and close events. The input window
    /// and adopted generation belong to the route and are dropped with it.
    fn retire_subscription(&mut self, route: &str) {
        self.retired_subscription_ids.insert(route.to_string());
        self.hub_io.forget_route(route);
        self.resolve_unknown_input_operations("route retired");
        self.route_generation = None;
        self.route_epoch = None;
    }

    fn current_owner_pair(&self) -> Option<(String, String)> {
        self.attach_hydration
            .as_ref()
            .map(|hydration| (hydration.session_id.clone(), hydration.route.clone()))
            .or_else(|| {
                self.attached
                    .as_ref()
                    .map(|attached| (attached.session_id.clone(), attached.route.clone()))
            })
    }

    fn reset_attach_campaign(&mut self) {
        self.attach_recovery_used = false;
        self.retired_subscription_ids.clear();
        self.terminal_close_evidence = None;
    }

    fn mint_subscription_id(&mut self) -> String {
        let sequence = self.next_terminal_subscription_sequence;
        self.next_terminal_subscription_sequence = sequence.saturating_add(1);
        format!("btui-sub-{}-{sequence}", short_suffix())
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
        self.set_selected_session(Some(session_id.clone()));

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
        self.submit_apply(DaemonRequest::SetPackageConfiguration {
            package_name: package_name.to_string(),
            values: updates,
        });
    }

    fn apply_mux_event(&mut self, event: DaemonEvent) {
        match event {
            DaemonEvent::TerminalSubscriptionClosed {
                session_id,
                subscription_id,
                generation,
                reason,
            } => self.handle_terminal_subscription_closed(
                session_id,
                subscription_id,
                generation,
                reason,
            ),
            DaemonEvent::PackageEvent {
                subscription_id,
                owner,
                name,
                payload,
            } => self.handle_package_event(subscription_id, owner, name, payload),
            DaemonEvent::EventGap {
                subscription_id,
                owner,
                name,
            } => self.handle_event_gap(subscription_id, owner, name),
            other => {
                let _ = other;
            }
        }
    }

    fn desired_notice_subscriptions(&self) -> Vec<NoticeSubscriptionEntry> {
        let Some(subject) = self.selected_session.clone() else {
            return Vec::new();
        };
        let mut desired = BTreeMap::<NoticeSubscriptionKey, NoticeSubscriptionEntry>::new();
        for package in &self.packages {
            for descriptor in &package.notice_reactions {
                let key = (descriptor.owner.clone(), descriptor.name.clone());
                desired
                    .entry(key)
                    .or_insert_with(|| NoticeSubscriptionEntry {
                        descriptor: descriptor.clone(),
                        subject: subject.clone(),
                        state: EventSubscriptionState::Idle,
                    });
            }
        }
        desired.into_values().collect()
    }

    fn sync_notice_subscriptions(&mut self) {
        let mut desired = self.desired_notice_subscriptions();
        desired.sort_by(|left, right| {
            (
                left.descriptor.owner.as_str(),
                left.descriptor.name.as_str(),
            )
                .cmp(&(
                    right.descriptor.owner.as_str(),
                    right.descriptor.name.as_str(),
                ))
        });
        let dropped = desired.len().saturating_sub(MAX_NOTICE_SUBSCRIPTIONS);
        if dropped > 0 {
            desired.truncate(MAX_NOTICE_SUBSCRIPTIONS);
            self.notice_overflow_dropped = dropped;
            self.error = Some(format!(
                "notice subscriptions dropped {dropped} descriptors over the {MAX_NOTICE_SUBSCRIPTIONS} connection limit"
            ));
        } else {
            self.notice_overflow_dropped = 0;
        }
        let desired_keys: BTreeSet<NoticeSubscriptionKey> = desired
            .iter()
            .map(|entry| {
                (
                    entry.descriptor.owner.clone(),
                    entry.descriptor.name.clone(),
                )
            })
            .collect();

        let stale: Vec<NoticeSubscriptionKey> = self
            .notice_subscriptions
            .keys()
            .filter(|key| !desired_keys.contains(*key))
            .cloned()
            .collect();
        for key in stale {
            self.unsubscribe_notice_entry(&key);
        }

        for entry in desired {
            let key = (
                entry.descriptor.owner.clone(),
                entry.descriptor.name.clone(),
            );
            match self.notice_subscriptions.get(&key) {
                Some(current)
                    if current.subject != entry.subject
                        || matches!(current.state, EventSubscriptionState::Idle) =>
                {
                    if matches!(current.state, EventSubscriptionState::Idle) {
                        self.notice_subscriptions.remove(&key);
                    } else {
                        self.unsubscribe_notice_entry(&key);
                    }
                    self.subscribe_notice_entry(entry);
                }
                Some(_) => {
                    if let Some(current) = self.notice_subscriptions.get_mut(&key) {
                        current.descriptor = entry.descriptor;
                    }
                }
                None => self.subscribe_notice_entry(entry),
            }
        }
    }

    fn subscribe_notice_entry(&mut self, mut entry: NoticeSubscriptionEntry) {
        let subscription_id = format!(
            "btui-events-{}-{}",
            short_suffix(),
            entry.descriptor.name.replace('.', "-")
        );
        let key = (
            entry.descriptor.owner.clone(),
            entry.descriptor.name.clone(),
        );
        entry.state = EventSubscriptionState::Candidate(subscription_id.clone());
        self.notice_subscription_by_id
            .insert(subscription_id.clone(), key.clone());
        self.notice_subscriptions.insert(key.clone(), entry.clone());
        self.submit(
            DaemonRequest::SubscribeEvents {
                subscription_id: subscription_id.clone(),
                owner: entry.descriptor.owner.clone(),
                name: entry.descriptor.name.clone(),
                subjects: vec![entry.subject.clone()],
            },
            PendingReply::SubscribeEvents {
                key,
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    /// EventSubscribed promotes the candidate and replays events parked while
    /// the response was outstanding.
    fn complete_notice_subscription(
        &mut self,
        key: &NoticeSubscriptionKey,
        subscription_id: &str,
        response: DaemonResponse,
    ) {
        self.record_diagnostics(response.diagnostics);
        if response.kind == DaemonResponseKind::EventSubscribed && response.error.is_none() {
            let promoted = match self.notice_subscriptions.get_mut(key) {
                Some(current) if current.state.candidate_id() == Some(subscription_id) => {
                    current.state = EventSubscriptionState::Active(subscription_id.to_string());
                    true
                }
                _ => false,
            };
            if promoted {
                self.promote_parked_notice_events(subscription_id);
            } else {
                self.notice_parked.remove(subscription_id);
            }
            return;
        }
        let detail = response
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| format!("{:?}", response.kind));
        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
        }
        self.reject_event_subscription_candidate(
            subscription_id,
            format!("event subscription was not accepted: {detail}"),
        );
    }

    fn unsubscribe_notice_entry(&mut self, key: &NoticeSubscriptionKey) {
        let Some(entry) = self.notice_subscriptions.remove(key) else {
            return;
        };
        let subscription_id = match &entry.state {
            EventSubscriptionState::Idle => None,
            EventSubscriptionState::Candidate(id) | EventSubscriptionState::Active(id) => {
                Some(id.clone())
            }
        };
        if let Some(subscription_id) = subscription_id {
            self.notice_subscription_by_id.remove(&subscription_id);
            self.notice_parked.remove(&subscription_id);
            if !matches!(entry.state, EventSubscriptionState::Idle) && self.is_connected() {
                self.submit(
                    DaemonRequest::UnsubscribeEvents { subscription_id },
                    PendingReply::Unsubscribe,
                    REQUEST_DEADLINE,
                );
            }
        }
    }

    fn reject_event_subscription_candidate(&mut self, subscription_id: &str, message: String) {
        if let Some(key) = self.notice_subscription_by_id.get(subscription_id).cloned()
            && let Some(entry) = self.notice_subscriptions.get_mut(&key)
            && entry.state.candidate_id() == Some(subscription_id)
        {
            entry.state = EventSubscriptionState::Idle;
            self.notice_subscription_by_id.remove(subscription_id);
        }
        self.notice_parked.remove(subscription_id);
        self.error = Some(message);
    }

    fn clear_event_subscription_state(&mut self) {
        self.notice_subscriptions.clear();
        self.notice_subscription_by_id.clear();
        self.notice_parked.clear();
        self.notice_overflow_dropped = 0;
        self.transient_notice = None;
    }

    fn candidate_notice_entry(&self, subscription_id: &str) -> Option<&NoticeSubscriptionEntry> {
        let key = self.notice_subscription_by_id.get(subscription_id)?;
        let entry = self.notice_subscriptions.get(key)?;
        (entry.state.candidate_id() == Some(subscription_id)).then_some(entry)
    }

    fn handle_package_event(
        &mut self,
        subscription_id: String,
        owner: String,
        name: String,
        payload: Value,
    ) {
        if self.active_notice_entry(&subscription_id).is_some() {
            self.apply_active_package_event(&subscription_id, &owner, &name, &payload);
            return;
        }
        // Hub may complete SubscribeEvents after the first event on the new
        // subscription is already delivered. Park a bounded tail until
        // EventSubscribed promotes the candidate.
        if self.candidate_notice_entry(&subscription_id).is_some() {
            let parked = self.notice_parked.entry(subscription_id).or_default();
            if parked.events.len() >= MAX_PARKED_NOTICE_EVENTS {
                parked.events.pop_front();
                parked.gap = true;
            }
            parked.events.push_back((owner, name, payload));
        }
    }

    fn apply_active_package_event(
        &mut self,
        subscription_id: &str,
        owner: &str,
        name: &str,
        payload: &Value,
    ) {
        let Some(entry) = self.active_notice_entry(subscription_id) else {
            return;
        };
        if entry.descriptor.owner != owner || entry.descriptor.name != name {
            return;
        }
        let text_pointer = entry.descriptor.text_pointer.clone();
        let ttl_ms = entry.descriptor.ttl_ms;
        match resolve_notice_text(payload, &text_pointer) {
            Ok(text) => {
                self.transient_notice = Some(TransientNotice {
                    text: text.to_string(),
                    deadline: Instant::now() + Duration::from_millis(u64::from(ttl_ms)),
                });
            }
            Err(error) => {
                self.transient_notice = None;
                self.error = Some(error.to_string());
            }
        }
    }

    fn handle_event_gap(&mut self, subscription_id: String, owner: String, name: String) {
        if self.active_notice_entry(&subscription_id).is_some() {
            self.apply_active_event_gap(&subscription_id, &owner, &name);
            return;
        }
        if self.candidate_notice_entry(&subscription_id).is_some() {
            self.notice_parked.entry(subscription_id).or_default().gap = true;
        }
    }

    fn apply_active_event_gap(&mut self, subscription_id: &str, owner: &str, name: &str) {
        let Some(entry) = self.active_notice_entry(subscription_id) else {
            return;
        };
        if entry.descriptor.owner != owner || entry.descriptor.name != name {
            return;
        }
        self.transient_notice = None;
        self.error = Some("package event gap; durable package state is unchanged".to_string());
    }

    fn promote_parked_notice_events(&mut self, subscription_id: &str) {
        let Some(parked) = self.notice_parked.remove(subscription_id) else {
            return;
        };
        let (owner, name) = match self.active_notice_entry(subscription_id) {
            Some(entry) => (
                entry.descriptor.owner.clone(),
                entry.descriptor.name.clone(),
            ),
            None => return,
        };
        if parked.gap {
            self.apply_active_event_gap(subscription_id, &owner, &name);
        }
        for (event_owner, event_name, payload) in parked.events {
            self.apply_active_package_event(subscription_id, &event_owner, &event_name, &payload);
        }
    }

    fn expire_transient_notice(&mut self) {
        if self
            .transient_notice
            .as_ref()
            .is_some_and(|notice| Instant::now() >= notice.deadline)
        {
            self.transient_notice = None;
        }
    }

    fn active_notice_entry(&self, subscription_id: &str) -> Option<&NoticeSubscriptionEntry> {
        let key = self.notice_subscription_by_id.get(subscription_id)?;
        let entry = self.notice_subscriptions.get(key)?;
        if entry.state.active_id() == Some(subscription_id) {
            Some(entry)
        } else {
            None
        }
    }

    fn transient_notice_band(&self) -> Option<UiNode> {
        let notice = self.transient_notice.as_ref()?;
        if Instant::now() >= notice.deadline {
            return None;
        }
        Some(node(
            UiNodeKind::Text,
            "workspace-transient-notice",
            json!({ "text": notice.text }),
        ))
    }

    /// One routed scheme 2 frame from the connection.
    ///
    /// Identity rules (root ruling on attachment identity versus resync state):
    ///
    /// - Frames for retired or foreign routes are dropped.
    /// - `generation` is the fixed attachment generation from the trusted
    ///   Attach response and never changes; frames that arrive before the
    ///   response wait (bounded) and frames with another generation are dropped.
    /// - `stream_epoch` fences snapshot and live continuity. It is 0 after
    ///   ATTACH_STATE attached. A ROUTE_RESYNC is accepted only when its
    ///   `from_epoch` equals the accepted epoch and its envelope epoch equals
    ///   `to_epoch`; other RESYNC frames are stale and dropped. Data frames
    ///   with another epoch are dropped. No numeric comparison is used.
    /// - A ROUTE_RESYNC must also change the epoch (`to_epoch != from_epoch`).
    /// - INPUT_RESULT is correlated by operation id within the attachment on
    ///   any epoch, so an accepted operation is never left unresolved by a
    ///   resync; unknown or already completed ids are reported, not tracked.
    fn apply_routed_terminal_frame(&mut self, routed: RoutedTerminalFrame) {
        let route = routed.route.as_str().to_string();
        if self.retired_subscription_ids.contains(&route) {
            return;
        }
        if !self.hydration_matches_route(&route) && !self.attached_matches_route(&route) {
            return;
        }
        let event = match decode_terminal_event(&routed.frame) {
            Ok(event) => event,
            Err(error) => {
                self.recover_from_decode_or_phase_gap(&format!(
                    "terminal event decode failed: {error}"
                ));
                return;
            }
        };
        // The attachment generation comes only from the trusted Attach
        // response. Frames that arrive first wait, bounded, and replay once the
        // response lands; a frame is never allowed to set the reservation.
        let Some(generation) = self.route_generation else {
            self.park_pre_attach_frame(routed);
            return;
        };
        if routed.generation != generation {
            return;
        }
        let epoch_ok = match &event {
            TerminalEvent::AttachState(AttachStateCode::Attached) => routed.stream_epoch == 0,
            TerminalEvent::AttachState(_) => self
                .route_epoch
                .is_none_or(|accepted| accepted == routed.stream_epoch),
            TerminalEvent::RouteResync(transition) => {
                transition.to_epoch != transition.from_epoch
                    && self.route_epoch == Some(transition.from_epoch)
                    && routed.stream_epoch == transition.to_epoch
            }
            TerminalEvent::InputResult(_) => true,
            _ => self.route_epoch == Some(routed.stream_epoch),
        };
        if !epoch_ok {
            return;
        }
        match &event {
            TerminalEvent::AttachState(AttachStateCode::Attached) => self.route_epoch = Some(0),
            TerminalEvent::RouteResync(transition) => {
                self.route_epoch = Some(transition.to_epoch);
            }
            _ => {}
        }
        self.apply_terminal_event(&route, event);
    }

    /// Retain one frame until the Attach response fixes the generation.
    ///
    /// The frame is charged against the connection's aggregate pending budget,
    /// not a separate per-route buffer. At the bound only this route fails.
    fn park_pre_attach_frame(&mut self, routed: RoutedTerminalFrame) {
        let Some(hydration) = self.attach_hydration.as_ref() else {
            return;
        };
        let bytes = routed.frame.len();
        if !self.hub_io.try_retain(bytes) {
            let session_id = hydration.session_id.clone();
            let route = hydration.route.clone();
            self.recover_current_subscription(
                &session_id,
                &route,
                "frames before the attach response exceeded the pending budget",
            );
            return;
        }
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.pending_frame_bytes += bytes;
            hydration.pending_frames.push_back(routed);
        }
    }

    /// Take the parked frames out of the campaign and release their budget.
    fn take_parked_frames(&mut self) -> VecDeque<RoutedTerminalFrame> {
        let Some(hydration) = self.attach_hydration.as_mut() else {
            return VecDeque::new();
        };
        let parked = std::mem::take(&mut hydration.pending_frames);
        hydration.pending_frame_bytes = 0;
        for routed in &parked {
            self.hub_io.release_retained(routed.frame.len());
        }
        parked
    }

    /// Replay frames parked before the Attach response, in arrival order.
    /// Output only: queued user input is never replayed here.
    fn replay_pre_attach_frames(&mut self) {
        for routed in self.take_parked_frames() {
            if self.attach_hydration.is_none() && self.attached.is_none() {
                return;
            }
            self.apply_routed_terminal_frame(routed);
        }
    }

    /// Drop the attach campaign and release every frame it retained.
    fn drop_attach_hydration(&mut self) {
        let _ = self.take_parked_frames();
        self.attach_hydration = None;
    }

    /// The route restarts from a fresh SNAPSHOT_READY. The decoder state is
    /// reset, never continued. Input captured so far stays queued; in-flight
    /// operations keep their window slots because INPUT_RESULT frames are
    /// accepted on every epoch of the attachment.
    fn begin_route_resync(&mut self) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            return;
        };
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_ghostty_projection();
        let was_attached = self.attached.take().is_some();
        let _ = self.take_parked_frames();
        let mut hydration = AttachHydration::new(&session_id, &route);
        if let Some(previous) = self.attach_hydration.take() {
            hydration.attached_seen = previous.attached_seen;
            hydration.pending_input = previous.pending_input;
            hydration.pending_input_bytes = previous.pending_input_bytes;
            hydration.pending_resize = previous.pending_resize;
        }
        hydration.attached_seen |= was_attached;
        self.attach_hydration = Some(hydration);
        self.action_feedback = Some(format!("terminal route resync: {session_id}"));
    }

    fn apply_terminal_event(&mut self, route: &str, event: TerminalEvent) {
        let Some((session_id, _)) = self.current_owner_pair() else {
            return;
        };
        match event {
            TerminalEvent::Output(frame) => {
                let bytes = frame.body();
                if let Some(hydration) = self.attach_hydration.as_mut() {
                    if hydration
                        .buffered_live_output
                        .len()
                        .saturating_add(bytes.len())
                        > MAX_HYDRATION_OUTPUT_BYTES
                    {
                        self.recover_current_subscription(
                            &session_id,
                            route,
                            "live output exceeded the attach buffer bound",
                        );
                        return;
                    }
                    hydration.buffered_live_output.extend_from_slice(bytes);
                } else {
                    self.apply_live_terminal_output(bytes);
                }
            }
            TerminalEvent::SnapshotReady(frame) => {
                self.apply_snapshot_ready(&session_id, frame.body());
            }
            TerminalEvent::SnapshotHistory(frame) => {
                self.apply_snapshot_history(&session_id, frame.body());
            }
            TerminalEvent::SnapshotFinish => self.apply_snapshot_finish(&session_id),
            TerminalEvent::ProcessExit(exit) => {
                self.apply_process_exit(session_id, route.to_string(), exit.code);
            }
            TerminalEvent::Modes(modes) => {
                self.terminal_modes = Some(TerminalModeState {
                    route: route.to_string(),
                    modes,
                });
            }
            TerminalEvent::AttachState(state) => {
                self.apply_attach_state_kind(session_id, route.to_string(), state);
            }
            TerminalEvent::InputResult(result) => self.apply_terminal_input_result(route, result),
            TerminalEvent::HistoryUnavailable(reason) => {
                self.apply_history_unavailable(&session_id, reason);
            }
            TerminalEvent::RouteResync(_) => self.begin_route_resync(),
        }
    }

    fn handle_terminal_subscription_closed(
        &mut self,
        session_id: String,
        subscription_id: String,
        generation: u64,
        reason: String,
    ) {
        if self.retired_subscription_ids.contains(&subscription_id) {
            return;
        }
        if !self.hydration_matches_route(&subscription_id)
            && !self.attached_matches_route(&subscription_id)
        {
            return;
        }
        self.terminal_close_evidence = Some((generation, reason.clone()));
        self.action_feedback = Some(format!(
            "terminal subscription closed generation={generation} reason={reason}: {session_id}"
        ));
        self.recover_current_subscription(
            &session_id,
            &subscription_id,
            &format!("terminal subscription closed ({reason})"),
        );
    }

    /// The client queue shed frames for a route or a frame failed to decode.
    /// Byte continuity is gone: detach and re-attach that route only.
    fn apply_route_fault(&mut self, route: RouteId, generation: u64, reason: &str) {
        let route = route.as_str().to_string();
        if self.retired_subscription_ids.contains(&route) {
            return;
        }
        if !self.hydration_matches_route(&route) && !self.attached_matches_route(&route) {
            return;
        }
        if self
            .route_generation
            .is_some_and(|current| generation < current)
        {
            return;
        }
        let Some((session_id, _)) = self.current_owner_pair() else {
            return;
        };
        self.recover_current_subscription(&session_id, &route, reason);
    }

    fn recover_from_decode_or_phase_gap(&mut self, reason: &str) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            self.error = Some(reason.to_string());
            return;
        };
        self.recover_current_subscription(&session_id, &route, reason);
    }

    /// Retire the current route with a bounded Detach and, once per attach
    /// campaign, re-attach with a fresh route.
    fn recover_current_subscription(&mut self, session_id: &str, route: &str, reason: &str) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_route_state();
        self.retire_subscription(route);
        if self.is_connected() {
            self.send_bounded_detach(session_id.to_string(), route.to_string());
        }
        if self.attach_recovery_used {
            self.error = Some(format!(
                "terminal attach failed closed after recovery: {reason}"
            ));
            return;
        }
        self.attach_recovery_used = true;
        self.error = Some(format!("terminal attach recovering: {reason}"));
        let replacement = self.mint_subscription_id();
        self.begin_attach_hydration(session_id, &replacement);
        if self.is_connected() {
            self.submit(
                DaemonRequest::Attach {
                    session_id: session_id.to_string(),
                    subscription_id: replacement.clone(),
                },
                PendingReply::Attach {
                    session_id: session_id.to_string(),
                    route: replacement,
                },
                REQUEST_DEADLINE,
            );
        }
    }

    fn apply_process_exit(&mut self, session_id: String, route: String, code: Option<i32>) {
        let hydration_matches = self.hydration_matches_route(&route);
        if !hydration_matches && !self.attached_matches_route(&route) {
            return;
        }
        self.status = format!("process exited {}", code.unwrap_or_default());
        self.retire_subscription(&route);
        self.attached = None;
        self.terminal_modes = None;
        self.clear_ghostty_projection();
        if hydration_matches {
            self.drop_attach_hydration();
        }
        let _ = session_id;
    }

    fn apply_attach_state_kind(
        &mut self,
        session_id: String,
        route: String,
        state: AttachStateCode,
    ) {
        let hydration_matches = self.hydration_matches_route(&route);
        let attached_matches = self.attached_matches_route(&route);
        if !hydration_matches && !attached_matches {
            return;
        }
        self.action_feedback = Some(format!("attach {state:?}: {session_id}"));
        match state {
            AttachStateCode::Attached if hydration_matches => {
                if let Some(hydration) = self.attach_hydration.as_mut() {
                    hydration.attached_seen = true;
                }
                self.maybe_open_attach_live_path(&session_id);
            }
            AttachStateCode::Detached => {
                if let Some(projection) = self.ghostty_projection.as_mut() {
                    projection.abort_ghostsnp_history();
                }
                self.retire_subscription(&route);
                self.attached = None;
                self.drop_attach_hydration();
                self.terminal_modes = None;
                self.clear_ghostty_projection();
            }
            AttachStateCode::Failed if hydration_matches => {
                // Capture failed before READY; Hub tears the route down. One
                // fresh attach with a new route is the allowed recovery, with no
                // input replay.
                self.recover_current_subscription(
                    &session_id,
                    &route,
                    "attach failed before READY",
                );
            }
            AttachStateCode::Attaching | AttachStateCode::Attached | AttachStateCode::Failed => {}
        }
    }

    /// HISTORY_UNAVAILABLE on the stream: the remaining history pages are
    /// replaced and SNAPSHOT_FINISH still follows.
    ///
    /// Core guarantees SNAPSHOT_READY precedes HISTORY_UNAVAILABLE on a live
    /// attach, so the live screen is already authoritative and only retained
    /// history is missing. Before READY the frame is a phase gap; a capture
    /// failure before READY arrives as ATTACH_STATE failed instead.
    fn apply_history_unavailable(&mut self, session_id: &str, reason: HistoryUnavailableReason) {
        let ready = self
            .attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.snapshot_ready);
        if self.attach_hydration.is_some() && !ready {
            self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP phase gap (closed): HISTORY_UNAVAILABLE ({reason:?}) before SNAPSHOT_READY"
            ));
            return;
        }
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.action_feedback = Some(format!(
            "terminal history unavailable ({reason:?}): {session_id}"
        ));
    }

    fn hydration_matches_route(&self, route: &str) -> bool {
        self.attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.route == route)
    }

    fn attached_matches_route(&self, route: &str) -> bool {
        self.attached
            .as_ref()
            .is_some_and(|attached| attached.route == route)
    }

    fn apply_snapshot_ready(&mut self, session_id: &str, bytes: &[u8]) {
        if self
            .attach_hydration
            .as_ref()
            .is_none_or(|hydration| hydration.snapshot_ready)
        {
            self.recover_from_decode_or_phase_gap(
                "GHOSTSNP phase gap (closed): unexpected SNAPSHOT_READY",
            );
            return;
        }
        self.ensure_ghostty_projection(session_id);
        let Some(projection) = self.ghostty_projection.as_mut() else {
            return;
        };
        match projection.install_ghostsnp_ready(bytes) {
            Ok(GhosttySnapshotDecodeProgress::Ready) => {
                if let Some(hydration) = self.attach_hydration.as_mut() {
                    hydration.snapshot_ready = true;
                }
                self.ghostty_projection_session_id = Some(session_id.to_string());
                self.terminal_viewport_size = projection.dimensions();
                self.projection_dirty = true;
            }
            Ok(progress) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental sequence failed (closed): unexpected {progress:?}"
            )),
            Err(error) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental apply failed (closed): {error}"
            )),
        }
    }

    fn apply_snapshot_history(&mut self, session_id: &str, bytes: &[u8]) {
        let ready = self
            .attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.snapshot_ready && !hydration.snapshot_finished);
        if !ready {
            self.recover_from_decode_or_phase_gap(
                "GHOSTSNP phase gap (closed): unexpected SNAPSHOT_HISTORY",
            );
            return;
        }
        let _ = session_id;
        let Some(projection) = self.ghostty_projection.as_mut() else {
            return;
        };
        match projection.apply_ghostsnp_history(bytes) {
            // One SNAPSHOT_HISTORY frame carries one page; the GHOSTSNP finish
            // record is the last page. Paint the new retained history at the
            // next paint. The live path opens on the SNAPSHOT_FINISH frame.
            Ok(GhosttySnapshotDecodeProgress::History | GhosttySnapshotDecodeProgress::Finish) => {
                self.projection_dirty = true;
            }
            Ok(progress) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental sequence failed (closed): unexpected {progress:?}"
            )),
            Err(error) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental apply failed (closed): {error}"
            )),
        }
    }

    fn apply_snapshot_finish(&mut self, session_id: &str) {
        let ready = self
            .attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.snapshot_ready && !hydration.snapshot_finished);
        if !ready {
            self.recover_from_decode_or_phase_gap(
                "GHOSTSNP phase gap (closed): unexpected SNAPSHOT_FINISH",
            );
            return;
        }
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.snapshot_finished = true;
        }
        self.projection_dirty = true;
        self.maybe_open_attach_live_path(session_id);
    }

    fn apply_live_terminal_output(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        #[cfg(test)]
        self.applied_live_payloads.push(data.to_vec());
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.apply_terminal_output(data);
            self.projection_dirty = true;
        }
    }

    fn maybe_open_attach_live_path(&mut self, session_id: &str) {
        let Some(hydration) = self.attach_hydration.as_ref() else {
            return;
        };
        if hydration.session_id != session_id
            || !hydration.snapshot_finished
            || !hydration.attached_seen
        {
            return;
        }
        self.open_attach_live_path(session_id);
    }

    /// Open the post-barrier live path and release queued client operations.
    fn open_attach_live_path(&mut self, session_id: &str) {
        let Some(hydration) = self.attach_hydration.take() else {
            return;
        };
        if hydration.session_id != session_id
            || hydration.route != self.subscription_id
            || !hydration.snapshot_finished
            || !hydration.attached_seen
        {
            self.attach_hydration = Some(hydration);
            return;
        }
        self.attached = Some(AttachedRoute {
            session_id: session_id.to_string(),
            route: hydration.route.clone(),
        });
        if !hydration.buffered_live_output.is_empty() {
            self.apply_live_terminal_output(&hydration.buffered_live_output);
        }
        let size = hydration
            .pending_resize
            .unwrap_or(self.terminal_viewport_size);
        self.apply_local_resize(size);
        self.send_resize(size);
        for input in hydration.pending_input {
            match input {
                PendingTerminalInput::Key(key) => self.send_key(key),
                PendingTerminalInput::Focus(focused) => self.send_focus(focused),
                PendingTerminalInput::Paste(data) => self.send_paste(data),
            }
        }
    }

    fn apply_local_resize(&mut self, size: TerminalScreenSize) {
        self.terminal_viewport_size = size;
        if let Some(projection) = self.ghostty_projection.as_mut()
            && let Err(error) = projection.resize(size)
        {
            self.error = Some(format!("terminal resize failed: {error}"));
        }
        self.projection_dirty = true;
    }

    fn scroll_projection(&mut self, op: ScrollOp) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.scroll(op);
            self.projection_dirty = true;
        }
    }

    /// Project the viewport once. Called from `prepare_paint` when dirty and
    /// from tests that inspect the cache directly.
    fn refresh_ghostty_viewport_cache(&mut self) {
        self.projection_dirty = false;
        let Some(projection) = self.ghostty_projection.as_mut() else {
            self.ghostty_viewport_cache = None;
            return;
        };
        self.ghostty_viewport_cache = projection.project_viewport().ok();
    }

    /// MODES bits for the live route, or zero.
    fn current_mode_bits(&self) -> u32 {
        match (self.terminal_modes.as_ref(), self.attached.as_ref()) {
            (Some(state), Some(attached)) if state.route == attached.route => state.modes.mode_bits,
            _ => 0,
        }
    }

    fn apply_terminal_mouse_mode(&self, hit_map: &mut HitMap) {
        hit_map.set_terminal_mouse_mode(
            "tui-terminal",
            terminal_input::kit_mouse_bits(self.current_mode_bits()),
        );
    }

    /// Reserve the next operation id for the live route.
    fn next_input_operation_id(&mut self) -> Option<u64> {
        match self.input_window.next_operation_id() {
            Ok(id) => Some(id),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    /// Encode one typed command, admit it into the window, and write it.
    fn send_command(&mut self, operation_id: u64, command: TerminalInputCommand) {
        let frame = match encode_terminal_input(&command) {
            Ok(frame) => frame,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        #[cfg(test)]
        self.observed_terminal_inputs.push(command);
        self.send_operation_frames(operation_id, false, vec![frame.into_bytes()]);
    }

    /// Admit one operation of encoded frames into the window and write what
    /// the window releases, in order.
    fn send_operation_frames(&mut self, operation_id: u64, paste: bool, frames: Vec<Vec<u8>>) {
        match self.input_window.admit(operation_id, paste, frames) {
            Ok(ready) => self.send_encoded_frames(ready),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn send_encoded_frames(&mut self, frames: Vec<Vec<u8>>) {
        if frames.is_empty() {
            return;
        }
        let Some(route) = self
            .attached
            .as_ref()
            .map(|attached| attached.route.clone())
        else {
            self.error = Some("terminal stream unavailable: no attached route".to_string());
            return;
        };
        let Some(generation) = self.route_generation else {
            self.error = Some("terminal stream unavailable: route generation unknown".to_string());
            return;
        };
        for frame in frames {
            if !self.hub_io.send_terminal(&route, generation, &frame) {
                self.error = Some("terminal stream unavailable: not connected".to_string());
                return;
            }
        }
        self.error = None;
    }

    fn send_key(&mut self, key: KeyEvent) {
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let Some(command) = terminal_input::key_command(key, operation_id) else {
            return;
        };
        self.send_command(operation_id, command);
    }

    fn send_focus(&mut self, focused: bool) {
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let command = terminal_input::focus_command(focused, operation_id);
        self.send_command(operation_id, command);
    }

    fn send_resize(&mut self, size: TerminalScreenSize) {
        if self.attached.is_none() {
            return;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let command = terminal_input::resize_command(size.rows, size.cols, operation_id);
        self.send_command(operation_id, command);
    }

    fn send_paste(&mut self, data: Vec<u8>) {
        if self.input_window.has_paste() {
            self.error = Some("terminal paste unavailable: another paste is in flight".to_string());
            return;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let frames = match encode_paste(operation_id, false, &data) {
            Ok(frames) => frames,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        #[cfg(test)]
        self.observed_terminal_inputs.extend(
            frames
                .iter()
                .filter_map(|frame| decode_terminal_input(frame).ok()),
        );
        let frames = frames
            .into_iter()
            .map(botster_terminal_protocol_client::TerminalInputFrame::into_bytes)
            .collect();
        self.send_operation_frames(operation_id, true, frames);
    }

    fn send_mouse(&mut self, mouse: MouseEvent, inner: Rect) -> bool {
        let Some(operation_id) = self.next_input_operation_id() else {
            return true;
        };
        let Some(command) = terminal_input::mouse_command(mouse, inner, operation_id) else {
            return false;
        };
        self.send_command(operation_id, command);
        true
    }

    /// Queue input for a route that is still attaching, bounded by bytes.
    fn queue_pending_input(&mut self, input: PendingTerminalInput) {
        let Some(hydration) = self.attach_hydration.as_mut() else {
            return;
        };
        let bytes = input.retained_bytes();
        if hydration.pending_input_bytes.saturating_add(bytes) > MAX_PENDING_HYDRATION_INPUT_BYTES {
            self.error = Some(format!(
                "terminal input unavailable: {} bytes queued at the {MAX_PENDING_HYDRATION_INPUT_BYTES} byte attach bound",
                hydration.pending_input_bytes
            ));
            return;
        }
        hydration.pending_input_bytes += bytes;
        hydration.pending_input.push(input);
        self.error = None;
    }

    fn apply_terminal_input_result(&mut self, route: &str, result: InputResultBody) {
        if !self.attached_matches_route(route) {
            return;
        }
        // `result.mode_bits` is the mode set at the time of that operation;
        // current stream state comes only from epoch-valid MODES frames.
        let (completed, released) = self.input_window.complete(result.operation_id);
        if completed.is_none() {
            self.error = Some(format!(
                "terminal input result {} has no pending operation",
                result.operation_id
            ));
        }
        self.send_encoded_frames(released);
        match result.outcome {
            InputOutcome::Written => {
                if completed.is_some() {
                    self.error = None;
                }
            }
            _ => self.error = Some(input_outcome_message(&result)),
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
            DaemonRequest::Detach {
                session_id,
                subscription_id,
            } => self.observed_requests.push(ObservedRequest::Detach {
                session_id: session_id.clone(),
                subscription_id: subscription_id.clone(),
            }),
            DaemonRequest::ShutdownSession { session_id } => self
                .observed_requests
                .push(ObservedRequest::ShutdownSession(session_id.clone())),
            DaemonRequest::RemoveSession { session_id } => self
                .observed_requests
                .push(ObservedRequest::RemoveSession(session_id.clone())),
            DaemonRequest::ReadScreen { session_id } => self
                .observed_requests
                .push(ObservedRequest::ReadScreen(session_id.clone())),
            DaemonRequest::ReadModeFlags { session_id } => self
                .observed_requests
                .push(ObservedRequest::ReadModeFlags(session_id.clone())),
            DaemonRequest::CaptureSnapshot { session_id } => self
                .observed_requests
                .push(ObservedRequest::CaptureSnapshot(session_id.clone())),
            DaemonRequest::ListSpawnTargets => self
                .observed_requests
                .push(ObservedRequest::ListSpawnTargets),
            DaemonRequest::ListSessionTypesForTarget { target_id } => {
                self.observed_requests
                    .push(ObservedRequest::ListSessionTypesForTarget {
                        target_id: target_id.clone(),
                    })
            }
            DaemonRequest::ShowSessionTypeDefinition { session_type_id } => self
                .observed_requests
                .push(ObservedRequest::ShowSessionTypeDefinition(
                    session_type_id.clone(),
                )),
            DaemonRequest::CreateSessionType { .. } => self
                .observed_requests
                .push(ObservedRequest::CreateSessionType),
            DaemonRequest::UpdateSessionType { .. } => self
                .observed_requests
                .push(ObservedRequest::UpdateSessionType),
            DaemonRequest::DeleteSessionType {
                source,
                session_type_id,
            } => self
                .observed_requests
                .push(ObservedRequest::DeleteSessionType {
                    source: source.clone(),
                    session_type_id: session_type_id.clone(),
                }),
            DaemonRequest::SpawnSessionType {
                session_type_id,
                session_id,
                request,
            } => self
                .observed_requests
                .push(ObservedRequest::SpawnSessionType {
                    session_type_id: session_type_id.clone(),
                    session_id: session_id.clone(),
                    target_id: request.target_id.clone(),
                }),
            DaemonRequest::Spawn {
                session_id,
                command,
            } => self.observed_requests.push(ObservedRequest::Spawn {
                session_id: session_id.clone(),
                command: command.clone(),
            }),
            DaemonRequest::SubscribeEvents {
                subscription_id,
                owner,
                name,
                subjects,
            } => self
                .observed_requests
                .push(ObservedRequest::SubscribeEvents {
                    subscription_id: subscription_id.clone(),
                    owner: owner.clone(),
                    name: name.clone(),
                    subjects: subjects.clone(),
                }),
            DaemonRequest::UnsubscribeEvents { subscription_id } => {
                self.observed_requests
                    .push(ObservedRequest::UnsubscribeEvents {
                        subscription_id: subscription_id.clone(),
                    })
            }
            _ => {}
        }
    }

    /// Apply one host-control response to read models and diagnostics.
    ///
    /// Terminal-stream events never travel in responses on v9; the terminal
    /// plane is the only source of OUTPUT, snapshots, attach state, and results.
    fn apply_response(&mut self, response: DaemonResponse) {
        self.record_diagnostics(response.diagnostics);

        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
            self.error = Some(format!(
                "{} (code={} operation={})",
                error.message, error.code, error.operation
            ));
            return;
        }

        if let Some(status) = response.status {
            self.connection_error = None;
            self.clear_connection_diagnostics();
            self.schema_version = Some(status.schema_version);
            self.compatibility = Some(status.compatibility);
            self.software = Some(status.software);
            self.record_diagnostics(status.diagnostics);
            self.status = format!("connected ({})", status.lifecycle_state);
            self.package_count = status.package_count;
            self.enabled_package_count = status.enabled_package_count;
            let supported =
                Self::session_types_supported_from_compatibility(self.compatibility.as_ref());
            if supported != self.session_types_supported {
                self.session_types_supported = supported;
                if supported {
                    if self.session_type_entities.subscription_id.is_none() {
                        self.start_session_type_subscription_if_supported();
                    }
                } else {
                    self.invalidate_session_type_generation();
                }
            }
        }

        if matches!(
            response.kind,
            DaemonResponseKind::Packages | DaemonResponseKind::PackageDecision
        ) {
            self.packages = response.packages;
            self.sync_notice_subscriptions();
        }
        if matches!(response.kind, DaemonResponseKind::Apps) {
            self.apps = response.apps;
        }
        if matches!(response.kind, DaemonResponseKind::PackageNavigation) {
            self.package_navigation = response.package_navigation;
        }
        if matches!(response.kind, DaemonResponseKind::SpawnTargets) {
            self.spawn_targets = response.spawn_targets;
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
                        // Surface replacement drops prior surface-demanded generations.
                        self.drop_entity_options_subscriptions();
                        self.entity_options_invalid_fields.clear();
                    }
                    self.plugin_surface = Some(surface);
                    self.sync_entity_options_subscriptions();
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
    }

    fn clear_ghostty_projection(&mut self) {
        self.ghostty_projection = None;
        self.ghostty_projection_session_id = None;
        self.ghostty_viewport_cache = None;
    }

    fn ensure_ghostty_projection(&mut self, session_id: &str) {
        if self.ghostty_projection.is_some()
            && self.ghostty_projection_session_id.as_deref() == Some(session_id)
        {
            return;
        }
        match GhosttyClientProjection::with_config(
            self.terminal_viewport_size,
            GhosttyAdapterConfig::with_max_scrollback_bytes(GHOSTTY_SCROLLBACK_BYTES),
        ) {
            Ok(projection) => {
                self.ghostty_projection = Some(projection);
                self.ghostty_projection_session_id = Some(session_id.to_string());
                self.refresh_ghostty_viewport_cache();
            }
            Err(error) => {
                self.error = Some(format!("ghostty projection unavailable: {error}"));
                self.clear_ghostty_projection();
            }
        }
    }

    fn paint_ghostty_projection(&self, frame: &mut Frame<'_>, hit_map: &HitMap) {
        let Some(viewport) = self.ghostty_viewport_cache.as_ref() else {
            return;
        };
        crate::projection_paint::paint_projection_on_hit_map(frame, hit_map, viewport);
    }

    fn clear_connection_diagnostics(&mut self) {
        self.diagnostics.retain(|diagnostic| {
            !matches!(
                diagnostic.kind,
                DaemonDiagnosticKind::CompatibilityMismatch
                    | DaemonDiagnosticKind::UnsupportedFeature
                    | DaemonDiagnosticKind::Disconnected
                    | DaemonDiagnosticKind::DaemonStartupFailure
                    | DaemonDiagnosticKind::WorkerCompatibility
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

        if self.target_first_spawn.is_some() {
            let root = self.target_first_spawn_dialog();
            root.validate()
                .expect("target-first spawn UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("target-first spawn UiNode should fit TUI renderer capabilities");
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
        root.children = self.status_summary_children();
        if let Some(alert) = self.connection_alert() {
            root.children.push(child(alert));
        }
        if let Some(notice) = self.transient_notice_band() {
            root.children.push(child(notice));
        }
        root.children.push(child(self.workspace_toolbar()));
        if self.system_details_visible {
            root.children.push(child(self.system_details_panel()));
        } else {
            root.children.push(child(self.session_navigator()));
            root.children.push(child(self.focused_session_panel()));
        }
        root.validate()
            .expect("workspace UiNode should satisfy the core UI contract");
        renderer::tui_capabilities()
            .validate_node(&root)
            .expect("workspace UiNode should fit TUI renderer capabilities");
        root
    }

    fn uses_workspace_shell(&self) -> bool {
        if self.confirmation.is_some()
            || self.target_first_spawn.is_some()
            || self.plugin_surface.is_some()
            || self.system_details_visible
        {
            return false;
        }
        #[cfg(test)]
        if !self.workspace_test_mode && self.legacy_test_needs_system_details() {
            return false;
        }
        true
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
        root.children = self.status_summary_children();
        root.children.push(child(node(
            UiNodeKind::Text,
            "plugin-shell-owner",
            json!({
                "text": format!(
                    "Plugin: {} / {} | Esc returns to System",
                    surface.package_name, surface.surface_id
                )
            }),
        )));
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
            &self.entity_options_projection_store(),
            &self.drafts,
            &self.entity_options_invalid_fields,
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
            || !self.drafts.is_empty()
    }

    fn status_summary_children(&self) -> Vec<UiChild> {
        [
            UiWidthClass::Expanded,
            UiWidthClass::Regular,
            UiWidthClass::Compact,
        ]
        .into_iter()
        .map(|width| responsive_child(width, self.status_summary_node(width)))
        .collect()
    }

    fn status_summary_node(&self, width: UiWidthClass) -> UiNode {
        let selected = self.selected_session.as_deref().unwrap_or("none");
        let attached = self.attached_session_id().unwrap_or("none");
        let session_count = match self.sessions.len() {
            1 => "1 session".to_string(),
            count => format!("{count} sessions"),
        };
        let compact = match self.attached_session_id() {
            Some(attached) => format!("Botster · {} · attached: {attached}", self.status),
            None => format!("Botster · {} · {session_count}", self.status),
        };
        match width {
            UiWidthClass::Expanded => node(
                UiNodeKind::Text,
                "workspace-status-expanded",
                json!({
                    "text": format!(
                        "Botster · Hub: {} · {session_count} · Selected: {selected} · Attached: {attached}",
                        self.status,
                    )
                }),
            ),
            UiWidthClass::Regular => node(
                UiNodeKind::Text,
                "workspace-status-regular",
                json!({
                    "text": format!(
                        "Botster · {} · {session_count} · Selected: {selected} · Attached: {attached}",
                        self.status,
                    )
                }),
            ),
            UiWidthClass::Compact => node(
                UiNodeKind::Text,
                "workspace-status-compact",
                json!({ "text": compact }),
            ),
        }
    }

    fn connection_alert(&self) -> Option<UiNode> {
        let connection_error = self.connection_error.as_ref()?;
        Some(node(
            UiNodeKind::Text,
            "workspace-connection-alert",
            json!({
                "text": format!(
                    "Connection unavailable: {connection_error} · Expected protocol: {PROTOCOL}."
                )
            }),
        ))
    }

    fn workspace_toolbar(&self) -> UiNode {
        let selected = self.selected_session_row();
        let selected_is_attached = selected
            .is_some_and(|session| self.attached_session_id() == Some(session.session_id.as_str()));
        let selected_is_attachable = selected.is_some_and(SessionRow::is_attachable);
        let selected_is_removable = selected.is_some_and(|session| {
            !session.pending && !matches!(session.lifecycle.as_str(), "running" | "pending")
        });
        let attach_is_primary = selected_is_attachable && !selected_is_attached;
        let detach_is_primary = self.attached.is_some() && !attach_is_primary;
        let spawn_is_primary = !attach_is_primary && !detach_is_primary;
        let payload = json!({ "session_id": self.selected_session });

        let mut actions = vec![child(workspace_button(
            "tui-spawn",
            "Spawn",
            "botster.tui.spawn",
            json!({}),
            if spawn_is_primary { "never" } else { "auto" },
            None,
        ))];
        if attach_is_primary {
            actions.push(child(workspace_button(
                "workspace-attach",
                "Attach",
                "botster.tui.attach",
                payload.clone(),
                "never",
                None,
            )));
        }
        if self.attached.is_some() {
            actions.push(child(workspace_button(
                "tui-detach",
                "Detach",
                "botster.tui.detach",
                json!({}),
                if detach_is_primary { "never" } else { "auto" },
                None,
            )));
        }
        actions.extend([
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
                None,
            )),
            child(workspace_button(
                "workspace-refresh",
                "Refresh",
                "botster.tui.refresh",
                json!({}),
                "auto",
                None,
            )),
        ]);
        if selected_is_attachable {
            actions.push(child(workspace_button(
                "workspace-shutdown",
                "Shutdown",
                "botster.tui.session.shutdown",
                payload.clone(),
                "auto",
                Some("danger"),
            )));
        }
        if selected_is_removable {
            actions.push(child(workspace_button(
                "workspace-remove",
                "Remove",
                "botster.tui.session.remove",
                payload,
                "auto",
                Some("danger"),
            )));
        }

        let mut toolbar = node(UiNodeKind::Toolbar, "workspace-toolbar", json!({}));
        toolbar.slots.insert("actions".to_string(), actions);
        toolbar
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
        let attached = self.attached_session_id() == Some(session.session_id.as_str());
        let state = if session.pending {
            "pending spawn"
        } else if attached && session.is_attachable() {
            "attached"
        } else {
            session.lifecycle.as_str()
        };
        let mut label = format!("{} · {state}", session.session_id);
        if let Some(session_type_id) = &session.session_type_id {
            label.push_str(&format!(" · type={session_type_id}"));
        }
        if let Some(source) = &session.session_type_source {
            label.push_str(&format!(" · source={source}"));
        }
        if let Some(role) = &session.role {
            label.push_str(&format!(" · role={role}"));
        }
        if let Some(interaction) = &session.interaction {
            label.push_str(&format!(" · interaction={interaction}"));
        }
        if !session.traits.is_empty() {
            label.push_str(&format!(" · traits={}", session.traits.join(",")));
        }
        if let Some(lifecycle) = &session.session_type_lifecycle {
            label.push_str(&format!(" · type_lifecycle={lifecycle}"));
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
                    "id": "botster.tui.attach",
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
        let mut body = node(
            UiNodeKind::Stack,
            "workspace-focused-session",
            json!({ "direction": "vertical" }),
        );
        if let Some(error) = &self.error {
            body.children.push(child(node(
                UiNodeKind::Text,
                "workspace-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        body.children.push(child(self.terminal_panel()));
        body
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
                None,
            )),
            child(workspace_button(
                "workspace-confirm-accept",
                verb,
                "botster.tui.confirm.accept",
                json!({}),
                "never",
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
                "tui-hub-software",
                json!({ "text": self.hub_software_text() }),
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
        children.extend(self.session_types_section_nodes().into_iter().map(child));
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

    /// Renders authoritative Hub identity from `DaemonStatus.software` alone.
    ///
    /// An absent `build_revision` is omitted rather than filled with a
    /// placeholder, and a Hub that has not reported status reads as unknown —
    /// the same convention [`TuiApp::compatibility_text`] uses for
    /// `schema_version`. No value here is ever derived from a package row.
    fn hub_software_text(&self) -> String {
        match &self.software {
            Some(software) => {
                let mut text = format!(
                    "hub software: {} {} ({})",
                    software.product_name, software.version, software.product_id
                );
                if let Some(build_revision) = &software.build_revision {
                    text.push_str(&format!("; build {build_revision}"));
                }
                text
            }
            None => "hub software: unknown".to_string(),
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

    fn session_types_section_nodes(&self) -> Vec<UiNode> {
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-session-types-heading",
            json!({ "text": "Session types" }),
        )];
        if !self.session_types_supported {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-session-types-unsupported",
                json!({
                    "text": "This hub does not provide session_type_entity_subscriptions."
                }),
            ));
            return nodes;
        }
        if let Some(error) = &self.session_type_subscription_error {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-session-types-subscription-error",
                json!({ "text": format!("session type subscription: {error}") }),
            ));
        }
        nodes.push(button(
            "tui-session-type-create",
            "Add session type",
            "botster.tui.session_type.create",
            json!({}),
        ));
        if let Some(form) = &self.session_type_form {
            nodes.extend(self.session_type_form_nodes(form));
        }
        let mut by_source: BTreeMap<String, Vec<&DaemonSessionType>> = BTreeMap::new();
        for entity in self.session_type_entities.ordered() {
            by_source
                .entry(entity.source.clone())
                .or_default()
                .push(entity);
        }
        if by_source.is_empty() {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-session-types-empty",
                json!({ "text": "session types: none reported" }),
            ));
        } else {
            for (source, rows) in by_source {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-session-type-source-{source}"),
                    json!({ "text": format!("source: {source}") }),
                ));
                for entity in rows {
                    nodes.extend(self.session_type_row_nodes(entity));
                }
            }
        }
        if let Some(selected_id) = &self.selected_session_type_id
            && let Some(entity) = self.session_type_entities.entities.get(selected_id)
        {
            nodes.push(self.session_type_detail_node(entity));
        }
        nodes
    }

    fn session_type_row_nodes(&self, entity: &DaemonSessionType) -> Vec<UiNode> {
        let selected =
            self.selected_session_type_id.as_deref() == Some(entity.session_type_id.as_str());
        let availability = if entity.available {
            "available"
        } else {
            "unavailable"
        };
        let editable = if entity.editable {
            "editable"
        } else {
            "read-only"
        };
        let mut label = format!(
            "{} · {} · {} · {} · {availability} · {editable}",
            entity.label, entity.role, entity.interaction, entity.lifecycle
        );
        if !entity.traits.is_empty() {
            label.push_str(&format!(" · traits={}", entity.traits.join(",")));
        }
        if !entity.diagnostics.is_empty() {
            label.push_str(&format!(" · {}", entity.diagnostics.join("; ")));
        }
        let mut nodes = vec![
            node(
                UiNodeKind::Text,
                &format!("tui-session-type-{}-label", entity.session_type_id),
                json!({ "text": label }),
            ),
            button(
                &format!("tui-session-type-{}", entity.session_type_id),
                "Select",
                "botster.tui.session_type.select",
                json!({ "session_type_id": entity.session_type_id, "selected": selected }),
            ),
        ];
        if entity.editable {
            nodes.push(button(
                &format!("tui-session-type-{}-edit", entity.session_type_id),
                "Edit",
                "botster.tui.session_type.edit",
                json!({ "session_type_id": entity.session_type_id }),
            ));
            nodes.push(button(
                &format!("tui-session-type-{}-delete", entity.session_type_id),
                "Delete",
                "botster.tui.session_type.delete",
                json!({ "session_type_id": entity.session_type_id }),
            ));
        }
        nodes
    }

    fn session_type_detail_node(&self, entity: &DaemonSessionType) -> UiNode {
        let mut detail = node(
            UiNodeKind::Stack,
            "tui-session-type-detail",
            json!({ "direction": "vertical" }),
        );
        let override_chain = entity
            .overridden_sources
            .iter()
            .map(|source| format!("{}:{}", source.kind, source.name))
            .collect::<Vec<_>>()
            .join(", ");
        let lines = [
            format!("session_type_id: {}", entity.session_type_id),
            format!("id: {}", entity.id),
            format!("source: {} ({})", entity.source, entity.source_name),
            format!(
                "execution: {}",
                match &entity.execution {
                    DaemonSessionTypeExecution::RelativeExecutable => "relative_executable",
                    DaemonSessionTypeExecution::ShellCommand => "shell_command",
                }
            ),
            format!("command: {} {:?}", entity.command, entity.args),
            format!(
                "working_directory_policy: {}",
                entity.working_directory_policy
            ),
            format!(
                "allowed_environment_overrides: {}",
                entity.allowed_environment_overrides.join(", ")
            ),
            format!("context_keys: {}", entity.context_keys.join(", ")),
            format!("target_id: {}", entity.target_id),
            format!("override_chain: {override_chain}"),
            format!("role: {}", entity.role),
            format!("interaction: {}", entity.interaction),
            format!("traits: {}", entity.traits.join(", ")),
            format!("lifecycle: {}", entity.lifecycle),
        ];
        detail.children = lines
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                child(node(
                    UiNodeKind::Text,
                    &format!("tui-session-type-detail-{index}"),
                    json!({ "text": text }),
                ))
            })
            .collect();
        detail
    }

    fn session_type_form_nodes(&self, form: &SessionTypeFormDraft) -> Vec<UiNode> {
        let title = match form.mode {
            SessionTypeFormMode::Create => "Create session type",
            SessionTypeFormMode::Edit => "Edit session type",
        };
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-session-type-form-title",
            json!({ "text": title }),
        )];
        if let Some(error) = &form.error {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-session-type-form-error",
                json!({ "text": format!("form error: {error}") }),
            ));
        }
        let fields = [
            ("session_type_source", "source", form.source.as_str()),
            (
                "session_type_source_target_id",
                "source target id",
                form.source_target_id.as_str(),
            ),
            ("session_type_id", "id", form.id.as_str()),
            ("session_type_label", "label", form.label.as_str()),
            (
                "session_type_description",
                "description",
                form.description.as_str(),
            ),
            ("session_type_role", "role", form.role.as_str()),
            (
                "session_type_interaction",
                "interaction",
                form.interaction.as_str(),
            ),
            ("session_type_traits", "traits", form.traits.as_str()),
            (
                "session_type_lifecycle",
                "lifecycle",
                form.lifecycle.as_str(),
            ),
            ("session_type_command", "command", form.command.as_str()),
            ("session_type_args", "args", form.args.as_str()),
            (
                "session_type_working_directory_policy",
                "working directory policy",
                form.working_directory_policy.as_str(),
            ),
            (
                "session_type_working_directory_path",
                "working directory path",
                form.working_directory_path.as_str(),
            ),
            (
                "session_type_environment",
                "environment",
                form.environment.as_str(),
            ),
            (
                "session_type_allowed_environment_overrides",
                "allowed environment overrides",
                form.allowed_environment_overrides.as_str(),
            ),
            (
                "session_type_context_keys",
                "context keys",
                form.context_keys.as_str(),
            ),
        ];
        for (name, label, value) in fields {
            let displayed = self
                .drafts
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or(value);
            // Render label+value as text so System details always shows the draft,
            // plus a TextInput for keyboard editing.
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-session-type-field-text-{name}"),
                json!({ "text": format!("{label}: {displayed}") }),
            ));
            nodes.push(node(
                UiNodeKind::TextInput,
                &format!("tui-session-type-field-{name}"),
                json!({
                    "name": name,
                    "label": label,
                    "value": displayed
                }),
            ));
        }
        let execution = self
            .drafts
            .get("session_type_execution")
            .and_then(Value::as_str)
            .unwrap_or(&form.execution);
        nodes.push(node(
            UiNodeKind::Text,
            "tui-session-type-field-text-session_type_execution",
            json!({ "text": format!("execution: {execution}") }),
        ));
        let mut execution_select = node(
            UiNodeKind::Select,
            "tui-session-type-field-session_type_execution",
            json!({
                "name": "session_type_execution",
                "label": "execution",
                "selected": execution
            }),
        );
        execution_select.slots.insert(
            "options".to_string(),
            [
                ("relative_executable", "Relative executable"),
                ("shell_command", "Shell command"),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (value, label))| {
                child(node(
                    UiNodeKind::SelectOption,
                    &format!("tui-session-type-execution-option-{index}"),
                    json!({ "value": value, "label": label }),
                ))
            })
            .collect(),
        );
        nodes.push(execution_select);
        nodes.push(button(
            "tui-session-type-form-cancel",
            "Cancel",
            "botster.tui.session_type.form.cancel",
            json!({}),
        ));
        nodes.push(button(
            "tui-session-type-form-submit",
            "Save",
            "botster.tui.session_type.form.submit",
            json!({}),
        ));
        nodes
    }

    fn target_first_spawn_nodes(&self, flow: &TargetFirstSpawnFlow) -> Vec<UiNode> {
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-target-first-spawn-title",
            json!({ "text": "Target-first spawn" }),
        )];
        match &flow.step {
            TargetFirstSpawnStep::PickTarget => {
                nodes.push(node(
                    UiNodeKind::Text,
                    "tui-target-first-spawn-help",
                    json!({ "text": "Select a launch target first" }),
                ));
                for target in self.launch_target_options() {
                    nodes.push(button(
                        &format!("tui-spawn-target-{}", target.target_id),
                        &format!("{} ({})", target.label, target.target_id),
                        "botster.tui.spawn.pick_target",
                        json!({ "target_id": target.target_id }),
                    ));
                }
            }
            TargetFirstSpawnStep::PickSessionType {
                target_id,
                target_label,
                session_types,
            } => {
                nodes.push(node(
                    UiNodeKind::Text,
                    "tui-target-first-spawn-target",
                    json!({ "text": format!("Target: {target_label} ({target_id})") }),
                ));
                // Membership comes only from Hub list-for-target rows stored on
                // the flow — never from entity.target_id equality filtering.
                if session_types.is_empty() {
                    nodes.push(node(
                        UiNodeKind::Text,
                        "tui-target-first-spawn-empty",
                        json!({ "text": "No session types for this target" }),
                    ));
                } else {
                    for session_type in session_types {
                        let label = if session_type.available {
                            format!("{} · {}", session_type.label, session_type.session_type_id)
                        } else {
                            format!(
                                "{} · {} · unavailable · {}",
                                session_type.label,
                                session_type.session_type_id,
                                session_type.diagnostics.join("; ")
                            )
                        };
                        if session_type.available {
                            nodes.push(button(
                                &format!("tui-spawn-session-type-{}", session_type.session_type_id),
                                &label,
                                "botster.tui.spawn.pick_session_type",
                                json!({ "session_type_id": session_type.session_type_id }),
                            ));
                        } else {
                            nodes.push(node(
                                UiNodeKind::Text,
                                &format!("tui-spawn-session-type-{}", session_type.session_type_id),
                                json!({ "text": label }),
                            ));
                        }
                    }
                }
            }
            TargetFirstSpawnStep::Prompt {
                target_label,
                session_type_id,
                prompt,
                ..
            } => {
                nodes.push(node(
                    UiNodeKind::Text,
                    "tui-target-first-spawn-prompt-meta",
                    json!({
                        "text": format!("Target {target_label} · type {session_type_id}")
                    }),
                ));
                let displayed_prompt = self
                    .drafts
                    .get("spawn_prompt")
                    .and_then(Value::as_str)
                    .unwrap_or(prompt);
                nodes.push(node(
                    UiNodeKind::TextInput,
                    "tui-spawn-prompt",
                    json!({
                        "name": "spawn_prompt",
                        "label": "prompt",
                        "value": displayed_prompt
                    }),
                ));
                nodes.push(button(
                    "tui-spawn-submit",
                    "Start session",
                    "botster.tui.spawn.submit",
                    json!({}),
                ));
            }
        }
        nodes.push(button(
            "tui-spawn-cancel",
            "Cancel spawn",
            "botster.tui.spawn.cancel",
            json!({}),
        ));
        nodes
    }

    fn target_first_spawn_dialog(&self) -> UiNode {
        let flow = self
            .target_first_spawn
            .as_ref()
            .expect("target-first spawn dialog requires active flow");
        let mut body = node(
            UiNodeKind::Stack,
            "tui-target-first-spawn-body",
            json!({ "direction": "vertical" }),
        );
        body.children = self
            .target_first_spawn_nodes(flow)
            .into_iter()
            .map(child)
            .collect();
        let mut dialog = node(
            UiNodeKind::Dialog,
            "tui-target-first-spawn",
            json!({ "title": "Target-first spawn", "presentation": "auto" }),
        );
        dialog.slots.insert("body".to_string(), vec![child(body)]);
        dialog
    }

    fn terminal_panel(&self) -> UiNode {
        let mut terminal = node(
            UiNodeKind::TerminalView,
            "tui-terminal",
            json!({
                "title": self.terminal_title(),
                "session_id": self.attached_session_id().map(str::to_string)
                    .or_else(|| self.attach_hydration.as_ref().map(|hydration| hydration.session_id.clone()))
                    .unwrap_or_else(|| "not attached".to_string())
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
        match (
            self.attached_session_id(),
            self.attach_hydration.as_ref(),
            &self.selected_session,
        ) {
            (None, Some(hydration), _) => {
                format!("Terminal · {} · attaching", hydration.session_id)
            }
            (Some(attached), _, _) => format!("Terminal · {attached}"),
            (None, None, Some(selected)) => format!("Terminal · {selected} · detached"),
            (None, None, None) => "Terminal".to_string(),
        }
    }

    fn terminal_content(&self) -> String {
        // When a Ghostty projection is installed, styled paint is authoritative.
        // Kit Text child is chrome placeholder only — not ReadScreen authority.
        if self.ghostty_projection.is_some() || self.ghostty_viewport_cache.is_some() {
            if self.attached.is_some()
                || self
                    .attach_hydration
                    .as_ref()
                    .is_some_and(|hydration| hydration.snapshot_ready)
            {
                return String::new();
            }
            return "Detached · Ghostty projection retained for scrollback.".to_string();
        }
        if self.attached.is_some() {
            return "Waiting for terminal projection.".to_string();
        }
        match self.selected_session_row() {
            Some(session) if session.pending => {
                "This session is pending; attachment is unavailable.".to_string()
            }
            Some(session) if session.is_attachable() => {
                "Activate this session to open its terminal.".to_string()
            }
            Some(session) => format!(
                "This session is {}; attachment is unavailable{}.",
                session.lifecycle,
                session
                    .failure_reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            ),
            None if self.connection_error.is_some() => {
                "Hub unavailable. Reconnect from System details.".to_string()
            }
            None => "Choose a session, or Spawn to create one.".to_string(),
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
    Detach {
        session_id: String,
        subscription_id: String,
    },
    ShutdownSession(String),
    RemoveSession(String),
    ReadScreen(String),
    ReadModeFlags(String),
    CaptureSnapshot(String),
    ListSpawnTargets,
    ListSessionTypesForTarget {
        target_id: String,
    },
    ShowSessionTypeDefinition(String),
    CreateSessionType,
    UpdateSessionType,
    DeleteSessionType {
        source: DaemonSessionTypeMutationSource,
        session_type_id: String,
    },
    SpawnSessionType {
        session_type_id: String,
        session_id: String,
        target_id: Option<String>,
    },
    Spawn {
        session_id: String,
        command: String,
    },
    SubscribeEvents {
        subscription_id: String,
        owner: String,
        name: String,
        subjects: Vec<String>,
    },
    UnsubscribeEvents {
        subscription_id: String,
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
const WORKSPACES_SPAWN_OPENER_ACTION: &str = "botster_workspaces.open_spawn";
const WORKSPACES_ADD_SESSION_ACTION: &str = "botster_workspaces.add_session";
const WORKSPACES_ADD_SESSION_FIELD: &str = "session_id";
const WORKSPACES_ADD_SESSION_NODE: &str = "botster-workspaces-add-session-id";
const WORKSPACES_MEMBERSHIP_FAMILY: &str = "botster-workspaces.membership";

fn run_workspaces_acceptance(args: AppArgs, config: AcceptanceConfig) -> io::Result<()> {
    let mut evidence = EvidenceWriter::create(&config.evidence_path, SCHEMA)?;
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

    let mut app = TuiApp::new_with_runtime_context(Some(endpoint), None, true, HubIo::new());
    app.connect();
    let connect_deadline = Instant::now() + ACCEPTANCE_TIMEOUT;
    app.pump_until(connect_deadline, |app| {
        app.is_connected() || app.connection_error.is_some()
    });
    if !app.is_connected() {
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
    if !app.handle_tui_owned_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), None) {
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
    let opener = activate_acceptance_action(
        app,
        router,
        WORKSPACES_SPAWN_OPENER_ACTION,
        |_| true,
        evidence,
        Some(&case.case_id),
        diagnostics,
    )?;
    if payload_field(&opener.payload, "selected_workspace") != Some(workspace_id) {
        return invalid_acceptance(format!(
            "case {:?} rendered Spawn opener payload did not identify workspace {workspace_id:?}",
            case.case_id
        ));
    }
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
        "single eligible session type and keyboard-typed requested branch",
    );
    select_only_acceptance_value(
        app,
        router,
        "session_type_id",
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
    let observed = app.pump_until(deadline, |app| {
        diagnostics.observe_app(app);
        ready(app, diagnostics)
    });
    if observed {
        return Ok(());
    }
    invalid_acceptance(format!("timed out waiting for {expectation}"))
}

fn run_workspaces_claim_acceptance(args: AppArgs, config: ClaimConfig) -> io::Result<()> {
    let mut evidence = EvidenceWriter::create(&config.evidence_path, CLAIM_SCHEMA)?;
    let mut diagnostics = AcceptanceDiagnostics {
        last_observation: json!({}),
        ..AcceptanceDiagnostics::default()
    };
    diagnostics.stage(
        "pin_ledger",
        None,
        "fail-closed Hub/Workspaces/TUI pin ancestry and Available sessions form",
    );
    let result = drive_workspaces_claim_acceptance(args, &config, &mut evidence, &mut diagnostics);
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = evidence.failure(&diagnostics.failure_context(), &error.to_string());
            Err(error)
        }
    }
}

fn drive_workspaces_claim_acceptance(
    args: AppArgs,
    config: &ClaimConfig,
    evidence: &mut EvidenceWriter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    let scenario = &config.scenario;
    let pin_ledger = verify_claim_pins(scenario)?;
    evidence.event(
        "pin_ledger",
        None,
        serde_json::to_value(&pin_ledger).map_err(io::Error::other)?,
    )?;

    if let Some(error) = args.connection_error.as_deref() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid Hub connection configuration: {error}"),
        ));
    }
    let endpoint = args.daemon_endpoint().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotConnected,
            "claim acceptance requires BOTSTER_HUB_CONNECTION",
        )
    })?;
    let data_dir = args.hub_data_dir.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "claim acceptance requires BOTSTER_HUB_DATA_DIR (or BOTSTER_LIVE_DATA_DIR alias)",
        )
    })?;
    if !data_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "injected Hub data directory is not a directory",
        ));
    }

    diagnostics.stage(
        "connect",
        None,
        "caller-injected Hub connection and data directory",
    );
    let mut app = TuiApp::new_with_runtime_context(Some(endpoint), None, true, HubIo::new());
    app.connect();
    let connect_deadline = Instant::now() + ACCEPTANCE_TIMEOUT;
    app.pump_until(connect_deadline, |app| {
        app.is_connected() || app.connection_error.is_some()
    });
    if !app.is_connected() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            app.connection_error
                .clone()
                .unwrap_or_else(|| "claim driver could not connect to the Hub".to_string()),
        ));
    }
    app.acceptance_audit = Some(AcceptanceRequestAudit::default());

    diagnostics.stage(
        "baseline",
        None,
        "authoritative /session baseline with exact session_uuid and lifecycle_class=current",
    );
    wait_for_acceptance_state(
        &mut app,
        diagnostics,
        "authoritative current session baseline with exact session_uuid",
        |app, _| claim_session_is_current(app, &scenario.session_uuid),
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
    let baseline_entity = app
        .session_entities
        .entities
        .get(&scenario.session_uuid)
        .expect("wait proved exact current session");
    evidence.event(
        "ready",
        None,
        json!({
            "workspace_id": scenario.workspace_id,
            "session_uuid": scenario.session_uuid
        }),
    )?;
    evidence.event(
        "baseline",
        None,
        json!({
            "subscription_id": app.session_entities.subscription_id,
            "snapshot_seq": app.session_entities.snapshot_seq,
            "has_snapshot": app.session_entities.has_snapshot,
            "session_uuid": scenario.session_uuid,
            "lifecycle_class": baseline_entity.lifecycle_class,
            "lifecycle": baseline_entity.lifecycle
        }),
    )?;

    let mut router = InputRouter::new(renderer::action_request_context());
    diagnostics.stage(
        "initial_surface_open",
        None,
        "realized Workspaces navigation and exact workspace detail",
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
        &scenario.workspace_id,
        evidence,
        diagnostics,
    )?;

    diagnostics.stage(
        "add_dialog_open",
        None,
        "realized Add existing session control for exact workspace",
    );
    let dialog = format!("add:{}", scenario.workspace_id);
    let workspace_id = scenario.workspace_id.clone();
    activate_acceptance_action(
        &mut app,
        &mut router,
        "botster_workspaces.open",
        move |payload| {
            payload_field(payload, "selected_workspace") == Some(workspace_id.as_str())
                && payload_field(payload, "dialog") == Some(dialog.as_str())
        },
        evidence,
        None,
        diagnostics,
    )?;

    diagnostics.stage(
        "option_present",
        None,
        "Available sessions entity_options includes exact session_uuid",
    );
    wait_for_acceptance_state(
        &mut app,
        diagnostics,
        "Available sessions option for exact session_uuid",
        |app, diagnostics| {
            acceptance_field_has_option(
                app,
                &mut router,
                WORKSPACES_ADD_SESSION_FIELD,
                Some(WORKSPACES_ADD_SESSION_NODE),
                &scenario.session_uuid,
                diagnostics,
            )
            .unwrap_or(false)
        },
    )?;
    let option_count = acceptance_field_option_count(
        &mut app,
        &mut router,
        WORKSPACES_ADD_SESSION_FIELD,
        Some(WORKSPACES_ADD_SESSION_NODE),
        diagnostics,
    )?;
    evidence.event(
        "option_present",
        None,
        json!({
            "field": WORKSPACES_ADD_SESSION_FIELD,
            "node_id": WORKSPACES_ADD_SESSION_NODE,
            "session_uuid": scenario.session_uuid,
            "option_count": option_count
        }),
    )?;

    // Plan C2.5: held-open Available sessions must reflect a Hub lifecycle patch without
    // reopening the dialog or using PluginSurfaceRender as synchronization.
    diagnostics.stage(
        "lifecycle_live_update",
        None,
        "held-open Available sessions option updates lifecycle without reopen or surface refresh",
    );
    let surface_renders_before_lifecycle = app
        .acceptance_audit
        .as_ref()
        .expect("acceptance audit enabled")
        .surface_renders
        .len();
    let lifecycle_before =
        claim_option_lifecycle(&app, &scenario.session_uuid).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "option present without projected lifecycle metadata",
            )
        })?;
    let label_before = claim_option_dedicated_label(&app, &scenario.session_uuid);
    let projected_before =
        claim_option_compact_label(&app, &scenario.session_uuid).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "option present without projected compact label",
            )
        })?;
    app.submit_apply(DaemonRequest::ShutdownSession {
        session_id: scenario.session_uuid.clone(),
    });
    wait_for_acceptance_state(
        &mut app,
        diagnostics,
        "held-open Available sessions lifecycle change without reopen",
        |app, diagnostics| {
            // Form must remain open (no reopen path).
            if !claim_add_form_open(app, &mut router, &scenario.workspace_id, diagnostics) {
                return false;
            }
            let Some(lifecycle_after) = claim_option_lifecycle(app, &scenario.session_uuid) else {
                return false;
            };
            let Some(projected_after) = claim_option_compact_label(app, &scenario.session_uuid)
            else {
                return false;
            };
            lifecycle_after != lifecycle_before
                && projected_after != projected_before
                && lifecycle_token_terminal(&lifecycle_after)
        },
    )?;
    let lifecycle_after =
        claim_option_lifecycle(&app, &scenario.session_uuid).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "lifecycle wait completed without projected lifecycle",
            )
        })?;
    let projected_after =
        claim_option_compact_label(&app, &scenario.session_uuid).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "lifecycle wait completed without projected compact label",
            )
        })?;
    let label_after = claim_option_dedicated_label(&app, &scenario.session_uuid);
    if app
        .acceptance_audit
        .as_ref()
        .expect("acceptance audit enabled")
        .surface_renders
        .len()
        != surface_renders_before_lifecycle
    {
        return invalid_acceptance(
            "held-open lifecycle update issued PluginSurfaceRender as synchronization",
        );
    }
    if !claim_add_form_open(&mut app, &mut router, &scenario.workspace_id, diagnostics) {
        return invalid_acceptance("held-open lifecycle update reopened or closed the Add form");
    }
    let label_live_update = match (&label_before, &label_after) {
        (Some(before), Some(after)) if before != after => true,
        (Some(_), Some(_)) => false,
        _ => false,
    };
    evidence.event(
        "lifecycle_live_update",
        None,
        json!({
            "field": WORKSPACES_ADD_SESSION_FIELD,
            "node_id": WORKSPACES_ADD_SESSION_NODE,
            "session_uuid": scenario.session_uuid,
            "lifecycle_before": lifecycle_before,
            "lifecycle_after": lifecycle_after,
            "projected_label_before": projected_before,
            "projected_label_after": projected_after,
            "dedicated_label_before": label_before,
            "dedicated_label_after": label_after,
            "label_live_update": label_live_update,
            "label_field_present": label_before.is_some() || label_after.is_some(),
            "reopened": false,
            "surface_render_delta": 0
        }),
    )?;

    diagnostics.stage(
        "keyboard_select",
        None,
        "production keyboard select of exact session_uuid",
    );
    select_acceptance_value(
        &mut app,
        &mut router,
        WORKSPACES_ADD_SESSION_FIELD,
        &scenario.session_uuid,
        evidence,
        "claim",
        diagnostics,
    )?;

    diagnostics.stage(
        "claim_submit",
        None,
        "realized botster_workspaces.add_session with exact session_uuid",
    );
    let request = activate_acceptance_action(
        &mut app,
        &mut router,
        WORKSPACES_ADD_SESSION_ACTION,
        |_| true,
        evidence,
        None,
        diagnostics,
    )?;
    let request_uuid = request
        .values
        .as_ref()
        .and_then(|values| values.0.get(WORKSPACES_ADD_SESSION_FIELD))
        .and_then(Value::as_str);
    if request_uuid != Some(scenario.session_uuid.as_str()) {
        return invalid_acceptance(format!(
            "add_session request.values.session_id must equal exact session_uuid {:?}; values={:?}",
            scenario.session_uuid, request.values
        ));
    }

    // Surface-render budget for membership join + option exclusion: entity frames
    // alone must update options. Add-dialog reopen is keyboard PluginSurfaceAction
    // only and must not issue PluginSurfaceRender.
    let surface_renders_before_reconciliation = app
        .acceptance_audit
        .as_ref()
        .expect("acceptance audit enabled")
        .surface_renders
        .len();

    // Action accepted is supporting only — membership entity is the join oracle.
    // Owner replacement may close the dialog and drop the exclude-family demand;
    // reopen Add once so `/botster-workspaces.membership` is demanded again and
    // late frames can apply into the entity-options store.
    ensure_membership_family_demanded(
        &mut app,
        &mut router,
        &scenario.workspace_id,
        evidence,
        diagnostics,
    )?;
    diagnostics.stage(
        "membership_join",
        None,
        "authoritative /botster-workspaces.membership row with exact workspace_id and session_uuid",
    );
    wait_for_acceptance_state(
        &mut app,
        diagnostics,
        "membership entity row for exact workspace and session",
        |app, _| membership_entity_contains(app, &scenario.workspace_id, &scenario.session_uuid),
    )?;
    let membership = membership_entity_row(&app, &scenario.workspace_id, &scenario.session_uuid)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "membership join wait completed without exact row",
            )
        })?;
    evidence.event(
        "membership_join",
        None,
        json!({
            "family": WORKSPACES_MEMBERSHIP_FAMILY,
            "session_uuid": scenario.session_uuid,
            "workspace_id": scenario.workspace_id,
            "membership_id": membership.0,
            "snapshot_seq": membership.1
        }),
    )?;

    diagnostics.stage(
        "option_excluded",
        None,
        "Available sessions excludes claimed session without list_sessions or surface refresh sync",
    );
    let reopened = ensure_claim_option_exclusion(
        &mut app,
        &mut router,
        &scenario.workspace_id,
        &scenario.session_uuid,
        evidence,
        diagnostics,
    )?;
    let excluded_count = acceptance_field_option_count(
        &mut app,
        &mut router,
        WORKSPACES_ADD_SESSION_FIELD,
        Some(WORKSPACES_ADD_SESSION_NODE),
        diagnostics,
    )
    .unwrap_or(0);
    evidence.event(
        "option_excluded",
        None,
        json!({
            "field": WORKSPACES_ADD_SESSION_FIELD,
            "node_id": WORKSPACES_ADD_SESSION_NODE,
            "session_uuid": scenario.session_uuid,
            "option_count": excluded_count,
            "reopened": reopened
        }),
    )?;

    let audit = app
        .acceptance_audit
        .as_ref()
        .expect("acceptance audit enabled");
    if audit.list_sessions != 0 {
        return invalid_acceptance(format!(
            "claim path must not issue session-list reads; count={}",
            audit.list_sessions
        ));
    }
    if audit.surface_renders.len() != surface_renders_before_reconciliation {
        return invalid_acceptance(format!(
            "membership join / option exclusion issued PluginSurfaceRender as synchronization: before={surface_renders_before_reconciliation} after={}",
            audit.surface_renders.len()
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
        json!({
            "workspace_id": scenario.workspace_id,
            "session_uuid": scenario.session_uuid,
            "action_id": WORKSPACES_ADD_SESSION_ACTION
        }),
    )
}

fn ensure_membership_family_demanded(
    app: &mut TuiApp,
    router: &mut InputRouter,
    workspace_id: &str,
    evidence: &mut EvidenceWriter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<()> {
    if app
        .entity_options
        .family(WORKSPACES_MEMBERSHIP_FAMILY)
        .is_some()
        || app
            .entity_options_subscriptions
            .contains(WORKSPACES_MEMBERSHIP_FAMILY)
    {
        return Ok(());
    }
    let dialog = format!("add:{workspace_id}");
    let workspace = workspace_id.to_string();
    if !acceptance_has_action(
        app,
        router,
        "botster_workspaces.open",
        |payload| {
            payload_field(payload, "selected_workspace") == Some(workspace.as_str())
                && payload_field(payload, "dialog") == Some(dialog.as_str())
        },
        diagnostics,
    )? {
        return Ok(());
    }
    activate_acceptance_action(
        app,
        router,
        "botster_workspaces.open",
        |payload| {
            payload_field(payload, "selected_workspace") == Some(workspace.as_str())
                && payload_field(payload, "dialog") == Some(dialog.as_str())
        },
        evidence,
        None,
        diagnostics,
    )?;
    Ok(())
}

fn claim_session_is_current(app: &TuiApp, session_uuid: &str) -> bool {
    app.session_entities.has_snapshot
        && app
            .session_entities
            .entities
            .get(session_uuid)
            .is_some_and(|entity| entity.lifecycle_class == "current")
}

fn membership_entity_contains(app: &TuiApp, workspace_id: &str, session_uuid: &str) -> bool {
    membership_entity_row(app, workspace_id, session_uuid).is_some()
}

fn membership_entity_row(
    app: &TuiApp,
    workspace_id: &str,
    session_uuid: &str,
) -> Option<(String, Option<u64>)> {
    let family = app.entity_options.family(WORKSPACES_MEMBERSHIP_FAMILY)?;
    if !family.has_snapshot && family.records.is_empty() {
        return None;
    }
    for (id, fields) in &family.records {
        let row_session = fields
            .get("session_uuid")
            .and_then(Value::as_str)
            .or_else(|| fields.get("id").and_then(Value::as_str));
        let row_workspace = fields.get("workspace_id").and_then(Value::as_str);
        if row_session == Some(session_uuid) && row_workspace == Some(workspace_id) {
            return Some((id.clone(), family.snapshot_seq));
        }
    }
    None
}

fn acceptance_field_has_option(
    app: &mut TuiApp,
    router: &mut InputRouter,
    field_name: &str,
    node_id: Option<&str>,
    expected: &str,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<bool> {
    let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
    let target = Value::String(expected.to_string());
    Ok(hit_map.regions().iter().any(|region| {
        let node_ok = node_id.is_none_or(|id| region.node_id == id);
        node_ok
            && region.field.as_ref().is_some_and(|field| {
                field.name == field_name && field.options.iter().any(|value| value == &target)
            })
    }))
}

fn acceptance_field_option_count(
    app: &mut TuiApp,
    router: &mut InputRouter,
    field_name: &str,
    node_id: Option<&str>,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<usize> {
    let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
    let field = hit_map.regions().iter().find_map(|region| {
        let node_ok = node_id.is_none_or(|id| region.node_id == id);
        if node_ok {
            region
                .field
                .as_ref()
                .filter(|field| field.name == field_name)
                .cloned()
        } else {
            None
        }
    });
    Ok(field.map(|field| field.options.len()).unwrap_or(0))
}

/// Prove claimed session is absent from Available sessions options.
/// Returns whether the Add dialog was reopened after owner replacement closed it.
fn ensure_claim_option_exclusion(
    app: &mut TuiApp,
    router: &mut InputRouter,
    workspace_id: &str,
    session_uuid: &str,
    evidence: &mut EvidenceWriter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<bool> {
    let target = Value::String(session_uuid.to_string());
    let mut reopened = false;
    let deadline = Instant::now() + ACCEPTANCE_TIMEOUT;
    while Instant::now() < deadline {
        let (_, hit_map) = acceptance_frame(app, router, diagnostics)?;
        let field = hit_map.regions().iter().find_map(|region| {
            (region.node_id == WORKSPACES_ADD_SESSION_NODE)
                .then_some(region.field.as_ref())
                .flatten()
                .filter(|field| field.name == WORKSPACES_ADD_SESSION_FIELD)
                .cloned()
        });
        if let Some(field) = field {
            if !field.options.iter().any(|value| value == &target) {
                return Ok(reopened);
            }
        } else if membership_entity_contains(app, workspace_id, session_uuid)
            && !session_option_projected(app, session_uuid)
        {
            // Owner replacement closed the dialog; reopen once and re-check options.
            let dialog = format!("add:{workspace_id}");
            let workspace = workspace_id.to_string();
            if acceptance_has_action(
                app,
                router,
                "botster_workspaces.open",
                |payload| {
                    payload_field(payload, "selected_workspace") == Some(workspace.as_str())
                        && payload_field(payload, "dialog") == Some(dialog.as_str())
                },
                diagnostics,
            )? {
                activate_acceptance_action(
                    app,
                    router,
                    "botster_workspaces.open",
                    |payload| {
                        payload_field(payload, "selected_workspace") == Some(workspace.as_str())
                            && payload_field(payload, "dialog") == Some(dialog.as_str())
                    },
                    evidence,
                    None,
                    diagnostics,
                )?;
                reopened = true;
                wait_for_acceptance_state(
                    app,
                    diagnostics,
                    "reopened Available sessions without claimed session",
                    |app, diagnostics| {
                        let (_, hit_map) = match acceptance_frame(app, router, diagnostics) {
                            Ok(frame) => frame,
                            Err(_) => return false,
                        };
                        hit_map.regions().iter().any(|region| {
                            region.node_id == WORKSPACES_ADD_SESSION_NODE
                                && region.field.as_ref().is_some_and(|field| {
                                    field.name == WORKSPACES_ADD_SESSION_FIELD
                                        && !field.options.iter().any(|value| value == &target)
                                })
                        })
                    },
                )?;
                return Ok(reopened);
            }
        }
        app.pump_once(Instant::now() + Duration::from_millis(50));
    }
    invalid_acceptance(format!(
        "timed out waiting for Available sessions to exclude {session_uuid}"
    ))
}

fn session_option_projected(app: &TuiApp, session_uuid: &str) -> bool {
    let store = app.entity_options_projection_store();
    let session_records = store.get("session");
    let membership_records = store.get(WORKSPACES_MEMBERSHIP_FAMILY);
    let Some(sessions) = session_records else {
        return false;
    };
    if !sessions.contains_key(session_uuid) {
        return false;
    }
    if let Some(membership) = membership_records {
        for fields in membership.values() {
            let claimed = fields
                .get("session_uuid")
                .and_then(Value::as_str)
                .or_else(|| fields.get("id").and_then(Value::as_str));
            if claimed == Some(session_uuid) {
                return false;
            }
        }
    }
    true
}

/// Display fields authored on Workspaces Available sessions entity_options.
const CLAIM_SESSION_DISPLAY_FIELDS: &[&str] = &[
    "label",
    "session_uuid",
    "lifecycle",
    "lifecycle_class",
    "session_type_id",
    "spawn_point",
];

fn claim_session_option_fields(
    app: &TuiApp,
    session_uuid: &str,
) -> Option<serde_json::Map<String, Value>> {
    // Session family is process-wide: projection injects session_entities, not entity_options.
    let store = app.entity_options_projection_store();
    store
        .get("session")
        .and_then(|records| records.get(session_uuid).cloned())
        .or_else(|| {
            app.session_entities
                .entities
                .get(session_uuid)
                .and_then(|entity| match serde_json::to_value(entity) {
                    Ok(Value::Object(fields)) => Some(fields),
                    _ => None,
                })
        })
}

fn json_field_string(fields: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match fields.get(key)? {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn claim_option_lifecycle(app: &TuiApp, session_uuid: &str) -> Option<String> {
    claim_session_option_fields(app, session_uuid)
        .and_then(|fields| json_field_string(&fields, "lifecycle"))
        .or_else(|| {
            app.session_entities
                .entities
                .get(session_uuid)
                .and_then(|entity| entity.lifecycle.clone())
        })
}

fn claim_option_dedicated_label(app: &TuiApp, session_uuid: &str) -> Option<String> {
    claim_session_option_fields(app, session_uuid)
        .and_then(|fields| json_field_string(&fields, "label"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != session_uuid)
}

fn claim_option_compact_label(app: &TuiApp, session_uuid: &str) -> Option<String> {
    let fields = claim_session_option_fields(app, session_uuid)?;
    let mut metadata = std::collections::BTreeMap::new();
    for field in CLAIM_SESSION_DISPLAY_FIELDS {
        if let Some(value) = json_field_string(&fields, field) {
            metadata.insert((*field).to_string(), value);
        }
    }
    // Always include session_uuid so the compact label is non-empty when the option exists.
    metadata
        .entry("session_uuid".to_string())
        .or_insert_with(|| session_uuid.to_string());
    let option = botster_ui_contract::EntityOption {
        value: session_uuid.to_string(),
        label: metadata
            .get("label")
            .cloned()
            .unwrap_or_else(|| session_uuid.to_string()),
        metadata,
    };
    let display_fields: Vec<String> = CLAIM_SESSION_DISPLAY_FIELDS
        .iter()
        .map(|field| (*field).to_string())
        .collect();
    Some(crate::entity_options::compact_entity_option_label(
        &option,
        &display_fields,
    ))
}

fn lifecycle_token_terminal(lifecycle: &str) -> bool {
    matches!(
        lifecycle.to_ascii_lowercase().as_str(),
        "exited" | "ended" | "failed" | "stopping" | "stale"
    )
}

fn claim_add_form_open(
    app: &mut TuiApp,
    router: &mut InputRouter,
    workspace_id: &str,
    diagnostics: &mut AcceptanceDiagnostics,
) -> bool {
    let form_id = format!("botster-workspaces-add-form-{workspace_id}");
    acceptance_frame(app, router, diagnostics)
        .map(|(_, hit_map)| {
            hit_map.regions().iter().any(|region| {
                region.node_id == form_id || region.node_id == WORKSPACES_ADD_SESSION_NODE
            })
        })
        .unwrap_or(false)
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
    let (expected_node_id, expected_action) =
        unique_acceptance_action(&hit_map, action_id, payload_matches, &lines)?;
    focus_acceptance_node(router, &hit_map, &expected_node_id)?;
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
    if !app.settle(Instant::now() + ACCEPTANCE_TIMEOUT) {
        return invalid_acceptance(format!(
            "action {action_id} request did not complete within the acceptance timeout"
        ));
    }
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

fn unique_acceptance_action(
    hit_map: &HitMap,
    action_id: &str,
    payload_matches: impl Fn(&Option<Value>) -> bool,
    lines: &[String],
) -> io::Result<(String, botster_ui_contract::UiAction)> {
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
            focusable_ids(hit_map),
            lines
                .iter()
                .map(|line| line.trim())
                .filter(|line| !line.is_empty())
                .take(30)
                .collect::<Vec<_>>()
                .join(" | ")
        ));
    }
    Ok(matches.into_iter().next().expect("one match"))
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
    app.connect();
    let connect_deadline = Instant::now() + HEADLESS_CONNECT_TIMEOUT;
    app.pump_until(connect_deadline, |app| {
        app.is_connected() || app.connection_error.is_some()
    });
    if !app.is_connected() {
        eprintln!(
            "headless-live-runtime-error: {}",
            app.connection_error
                .clone()
                .unwrap_or_else(|| "hub connection did not complete".to_string())
        );
        return Err(DaemonTransportError::NotRunning);
    }
    // Harness smoke only: freeform Spawn seeds a shell session so contract-matrix /
    // attach paths can run without writing into Hub's device session-types root.
    // Product launch remains target-first SpawnSessionType (toolbar dialog).
    let session_id = format!("btui-{}", short_suffix());
    app.pending_sessions
        .insert(session_id.clone(), SessionRow::pending(session_id.clone()));
    app.selected_session = Some(session_id.clone());
    app.rebuild_session_rows();
    app.action_feedback = Some(format!("spawn pending: {session_id}"));
    app.submit(
        DaemonRequest::Spawn {
            session_id: session_id.clone(),
            command: DEFAULT_COMMAND.to_string(),
        },
        PendingReply::Spawn {
            session_id: session_id.clone(),
        },
        REQUEST_DEADLINE,
    );
    if !app.settle(Instant::now() + REQUEST_DEADLINE) {
        eprintln!("headless-live-runtime-error: spawn request did not complete");
        return Err(DaemonTransportError::Protocol(
            "headless spawn request did not complete",
        ));
    }
    if let Some(error) = &app.error {
        eprintln!("headless-live-runtime-error: {error}");
        return Err(DaemonTransportError::Protocol(
            "headless live runtime app error",
        ));
    }
    #[cfg(test)]
    {
        let rendered = render_app_to_lines(&app, 200, 48, &RenderState::default())
            .0
            .join("\n");
        assert!(rendered.contains("pending spawn"));
        assert_eq!(app.attached, None);
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
    // The live path already sent RESIZE. The smoke input is typed as KEY frames.
    for character in HEADLESS_INPUT.chars() {
        let code = if character == '\n' {
            KeyCode::Enter
        } else {
            KeyCode::Char(character)
        };
        app.send_key(KeyEvent::new(code, KeyModifiers::NONE));
    }
    wait_for_app_output(&mut app, HEADLESS_OUTPUT)?;
    #[cfg(test)]
    {
        let (lines, hit_map) = render_app_to_lines(&app, 200, 48, &RenderState::default());
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
            FEATURE_PACKAGE_NAVIGATION,
            FEATURE_PLUGIN_SURFACE_RENDER,
            FEATURE_PLUGIN_SURFACE_ACTION,
            FEATURE_SESSION_ENTITY_SUBSCRIPTIONS,
            FEATURE_UNIX_TERMINAL_ADAPTER,
            FEATURE_TERMINAL_SUBSCRIPTION_CLOSED,
            FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS,
        ] {
            assert!(
                compatibility
                    .features
                    .iter()
                    .any(|feature| feature == required_feature)
            );
        }
        assert!(rendered.contains("Sessions"));
        assert!(rendered.contains("Terminal ·"));
        assert!(rendered.contains(HEADLESS_OUTPUT));
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-terminal-output")
        );
    }
    println!("terminal-output: {HEADLESS_OUTPUT}");
    app.submit_apply(DaemonRequest::ShutdownSession { session_id });
    let _ = app.settle(Instant::now() + REQUEST_DEADLINE);
    app.shutdown();
    Ok(())
}

fn wait_for_authoritative_session(app: &mut TuiApp, session_id: &str) -> DaemonTransportResult<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let ready = app.pump_until(deadline, |app| {
        app.sessions.iter().any(|session| {
            session.session_id == session_id && session.is_attachable() && !session.pending
        })
    });
    if ready {
        return Ok(());
    }
    eprintln!(
        "authoritative-session-timeout: id={session_id} error={:?} connection_error={:?} status={} sessions={:?} pending={:?} has_snapshot={} sub={:?}",
        app.error,
        app.connection_error,
        app.status,
        app.sessions
            .iter()
            .map(|session| format!(
                "{}:{}:pending={}",
                session.session_id, session.lifecycle, session.pending
            ))
            .collect::<Vec<_>>(),
        app.pending_sessions.keys().cloned().collect::<Vec<_>>(),
        app.session_entities.has_snapshot,
        app.session_entities.subscription_id,
    );
    Err(DaemonTransportError::Protocol(
        "timed out waiting for authoritative session entity",
    ))
}

/// Wait until the projected viewport contains `needle`.
fn wait_for_app_output(app: &mut TuiApp, needle: &str) -> DaemonTransportResult<()> {
    let deadline = Instant::now() + Duration::from_secs(8);
    if app.pump_until(deadline, |app| app.viewport_text().contains(needle)) {
        return Ok(());
    }
    let observed_prefix = app.viewport_text().chars().take(256).collect::<String>();
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

fn workspace_button(
    id: &str,
    label: &str,
    action_id: &str,
    payload: Value,
    toolbar_overflow: &str,
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

fn admit_terminal_hello(ack: &DaemonHelloAck) -> DaemonTransportResult<()> {
    let requirement = tui_terminal_compatibility_requirement();
    let Some(terminal_compatibility) = ack.terminal_compatibility.as_ref() else {
        return Err(terminal_hello_error(
            "hello ack omitted terminal_compatibility",
        ));
    };
    ensure_terminal_compatible(&requirement, terminal_compatibility)
        .map_err(|error| terminal_hello_error_with_diagnostic(error.diagnostic))
}

fn terminal_hello_error(reason: &str) -> DaemonTransportError {
    terminal_hello_error_with_diagnostic(format!(
        "botster-tui is incompatible with the terminal protocol: {reason}"
    ))
}

fn terminal_hello_error_with_diagnostic(diagnostic: String) -> DaemonTransportError {
    DaemonTransportError::Compatibility(DaemonCompatibilityError {
        diagnostic: diagnostic.clone(),
        diagnostics: vec![DaemonDiagnostic::compatibility_mismatch(diagnostic)],
    })
}

fn tui_compatibility_requirement() -> DaemonCompatibilityRequirement {
    DaemonCompatibilityRequirement {
        protocol: PROTOCOL.to_string(),
        protocol_version: botster_hub_client::PROTOCOL_VERSION,
        required_features: vec![
            FEATURE_SESSIONS.to_string(),
            FEATURE_PACKAGE_NAVIGATION.to_string(),
            FEATURE_PLUGIN_SURFACE_RENDER.to_string(),
            FEATURE_PLUGIN_SURFACE_ACTION.to_string(),
            FEATURE_TERMINAL_READBACK.to_string(),
            FEATURE_SESSION_ENTITY_SUBSCRIPTIONS.to_string(),
            FEATURE_UNIX_TERMINAL_ADAPTER.to_string(),
            FEATURE_TERMINAL_SUBSCRIPTION_CLOSED.to_string(),
            FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS.to_string(),
        ],
        minimum_conformance_fixture_revision: MINIMUM_CONFORMANCE_FIXTURE_REVISION,
        client_name: "botster-tui".to_string(),
    }
}

fn tui_terminal_compatibility_requirement() -> TerminalCompatibilityRequirement {
    let mut requirement = TerminalCompatibilityRequirement::for_ready_then_history_attach();
    requirement.client_name = "botster-tui".to_string();
    requirement
}

fn diagnostic_text(diagnostic: &DaemonDiagnostic) -> String {
    let label = match diagnostic.kind {
        DaemonDiagnosticKind::Connected => "connected",
        DaemonDiagnosticKind::Disconnected => "disconnected",
        DaemonDiagnosticKind::CompatibilityMismatch => "compatibility_mismatch",
        DaemonDiagnosticKind::UnsupportedFeature => "unsupported_feature",
        DaemonDiagnosticKind::TerminalStreamUnavailable => "terminal_stream_unavailable",
        DaemonDiagnosticKind::WorkerCompatibility => "worker_compatibility",
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
    // Authored validation owns binding context and descendant-key diagnostics.
    // Renderer capabilities still inspect only concrete trees because bound prop
    // sentinels are materialized in plugin_surface_render_root.
    surface.body.validate().map_err(|error| {
        format!(
            "plugin surface {}:{} failed UiNode validate: {error}",
            surface.package_name, surface.surface_id
        )
    })?;
    if !node_requires_binding_materialization(&surface.body) {
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
    entity_options_store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid_entity_option_fields: &BTreeSet<String>,
) -> Result<UiNode, String> {
    let mut materialized = if node_requires_binding_materialization(root) {
        let rows = session_entities.binding_rows()?;
        materialize_binding_node(root, &rows, None, None, false)?
    } else {
        root.clone()
    };
    // Realize entity-backed selects before kit validation / hit-map render.
    let mut draft_view = drafts.clone();
    materialize_entity_options_selects(&mut materialized, entity_options_store, &mut draft_view)?;
    // Re-apply invalid-field errors for fields already cleared by reconcile.
    stamp_entity_option_invalid_errors(&mut materialized, invalid_entity_option_fields);
    reject_duplicate_realized_node_ids(&materialized)?;
    Ok(materialized)
}

fn stamp_entity_option_invalid_errors(node: &mut UiNode, invalid_fields: &BTreeSet<String>) {
    if node.kind == UiNodeKind::Select
        && let Some(name) = node.props.get("name").and_then(Value::as_str)
        && invalid_fields.contains(name)
        && !node.props.contains_key("error")
    {
        node.props.insert(
            "error".to_string(),
            Value::String("Selected value is no longer available".to_string()),
        );
    }
    for child in &mut node.children {
        stamp_entity_option_invalid_errors_child(child, invalid_fields);
    }
    for children in node.slots.values_mut() {
        for child in children {
            stamp_entity_option_invalid_errors_child(child, invalid_fields);
        }
    }
}

fn stamp_entity_option_invalid_errors_child(
    child: &mut UiChild,
    invalid_fields: &BTreeSet<String>,
) {
    match child {
        UiChild::Node(node)
        | UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. }) => {
            stamp_entity_option_invalid_errors(node, invalid_fields);
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            stamp_entity_option_invalid_errors(item_template, invalid_fields);
            if let Some(template) = empty_template {
                stamp_entity_option_invalid_errors(template, invalid_fields);
            }
        }
    }
}

fn collect_invalid_entity_option_fields(
    node: &UiNode,
    store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid: &mut BTreeSet<String>,
) {
    if node.kind == UiNodeKind::Select
        && let Some(source) = node.props.get("options_source")
        && let Ok(descriptor) =
            serde_json::from_value::<botster_ui_contract::UiEntityOptionsSource>(source.clone())
    {
        let field_name = node
            .props
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !field_name.is_empty() {
            let selection = drafts.get(field_name).and_then(Value::as_str);
            let projection = botster_ui_contract::project_entity_options_from_store(
                &descriptor,
                store,
                selection,
            );
            if !projection.selection_valid {
                invalid.insert(field_name.to_string());
            }
        }
    }
    for child in &node.children {
        collect_invalid_entity_option_fields_child(child, store, drafts, invalid);
    }
    for children in node.slots.values() {
        for child in children {
            collect_invalid_entity_option_fields_child(child, store, drafts, invalid);
        }
    }
}

fn collect_invalid_entity_option_fields_child(
    child: &UiChild,
    store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid: &mut BTreeSet<String>,
) {
    match child {
        UiChild::Node(node)
        | UiChild::Conditional(UiConditional::When { node, .. })
        | UiChild::Conditional(UiConditional::Hidden { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { node, .. })
        | UiChild::BindIf(botster_ui_contract::UiBindIf::BindIf { node, .. }) => {
            collect_invalid_entity_option_fields(node, store, drafts, invalid);
        }
        UiChild::BindList(botster_ui_contract::UiBindList::BindList {
            item_template,
            empty_template,
            ..
        }) => {
            collect_invalid_entity_option_fields(item_template, store, drafts, invalid);
            if let Some(template) = empty_template {
                collect_invalid_entity_option_fields(template, store, drafts, invalid);
            }
        }
    }
}

fn node_requires_binding_materialization(node: &UiNode) -> bool {
    matches!(
        node.id,
        Some(UiAuthoredNodeId::Bind(_) | UiAuthoredNodeId::BindListDescendant(_))
    ) || node.props.values().any(value_contains_binding)
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
    row_id: Option<&UiNodeId>,
    bound_id_allowed: bool,
) -> Result<UiNode, String> {
    let mut node = source.clone();
    let mut descendant_row_id = row_id.cloned();
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
            let id = UiNodeId(id.to_string());
            descendant_row_id = Some(id.clone());
            Some(UiAuthoredNodeId::Literal(id))
        }
        Some(UiAuthoredNodeId::BindListDescendant(descendant_id)) => {
            let row_id = descendant_row_id.as_ref().ok_or_else(|| {
                "bound list descendant id requires a realized item template root id".to_string()
            })?;
            let id = realize_bind_list_descendant_id(&row_id.0, descendant_id.key())
                .map_err(|error| format!("bound list descendant id failed: {error}"))?;
            Some(UiAuthoredNodeId::Literal(id))
        }
    };
    for value in node.props.values_mut() {
        *value = materialize_binding_value(value, item)?;
    }
    node.children = materialize_binding_children(
        &source.children,
        session_rows,
        item,
        descendant_row_id.as_ref(),
    )?;
    node.slots = source
        .slots
        .iter()
        .map(|(name, children)| {
            materialize_binding_children(children, session_rows, item, descendant_row_id.as_ref())
                .map(|children| (name.clone(), children))
        })
        .collect::<Result<_, _>>()?;
    Ok(node)
}

fn materialize_binding_children(
    children: &[UiChild],
    session_rows: &[Value],
    item: Option<&Value>,
    row_id: Option<&UiNodeId>,
) -> Result<Vec<UiChild>, String> {
    let mut materialized = Vec::new();
    for child in children {
        match child {
            UiChild::Node(node) => materialized.push(UiChild::Node(Box::new(
                materialize_binding_node(node, session_rows, item, row_id, false)?,
            ))),
            UiChild::Conditional(UiConditional::When { condition, node }) => {
                materialized.push(UiChild::Conditional(UiConditional::When {
                    condition: condition.clone(),
                    node: Box::new(materialize_binding_node(
                        node,
                        session_rows,
                        item,
                        row_id,
                        false,
                    )?),
                }));
            }
            UiChild::Conditional(UiConditional::Hidden { condition, node }) => {
                materialized.push(UiChild::Conditional(UiConditional::Hidden {
                    condition: condition.clone(),
                    node: Box::new(materialize_binding_node(
                        node,
                        session_rows,
                        item,
                        row_id,
                        false,
                    )?),
                }));
            }
            UiChild::BindIf(botster_ui_contract::UiBindIf::PresentationIf { predicate, node }) => {
                materialized.push(UiChild::BindIf(
                    botster_ui_contract::UiBindIf::PresentationIf {
                        predicate: predicate.clone(),
                        node: Box::new(materialize_binding_node(
                            node,
                            session_rows,
                            item,
                            row_id,
                            false,
                        )?),
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
                        row_id,
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
                            None,
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
    entity_options_store: &EntityFamilyStore,
    drafts: &BTreeMap<String, Value>,
    invalid_entity_option_fields: &BTreeSet<String>,
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
    let mut root = match materialize_plugin_surface(
        &root,
        session_entities,
        entity_options_store,
        drafts,
        invalid_entity_option_fields,
    ) {
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
    validated_materialized_plugin_surface_node(surface, root)
}

fn validated_materialized_plugin_surface_node(
    surface: &DaemonPluginSurface,
    root: UiNode,
) -> UiNode {
    if let Err(error) = root.validate_realized() {
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
    if let Err(error) = renderer::tui_capabilities().validate_realized_node(&root) {
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
    use botster_hub_client::TerminalCompatibility;
    use botster_terminal_protocol_client::mode_bits;

    use botster_ui_contract::{
        UiActionId, UiActionKind, UiActionRequest, UiActionRequestId, UiSurfaceId,
    };

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

        for (width, height, horizontal) in [
            (240, 50, true),
            (140, 42, true),
            (96, 30, true),
            (72, 24, false),
        ] {
            let (lines, hit_map) =
                render_app_to_lines(&app, width, height, &RenderState::default());
            let rendered = lines.join("\n");
            let navigator = hit_map
                .regions()
                .iter()
                .find(|region| region.node_id == "workspace-session-navigator")
                .expect("session navigator should render");
            let terminal = hit_map
                .regions()
                .iter()
                .find(|region| region.node_id == "tui-terminal")
                .expect("focused terminal should render");

            if width >= 120 {
                assert!(rendered.contains("Botster · Hub: connected"), "{rendered}");
            } else {
                assert!(rendered.contains("Botster · connected"), "{rendered}");
            }
            if width == 72 {
                assert!(!rendered.contains("Selected:"), "{rendered}");
            } else {
                assert!(rendered.contains("Selected: session-alpha"), "{rendered}");
            }
            assert!(!rendered.contains("protocol:"), "{rendered}");
            assert!(rendered.contains("session-alpha"), "{rendered}");
            assert!(
                rendered.contains("Activate this session to open"),
                "{rendered}"
            );
            assert_eq!(navigator.rect.y, 2, "{rendered}");
            if horizontal {
                assert_eq!(navigator.rect.y, terminal.rect.y);
                assert_ne!(navigator.rect.x, terminal.rect.x);
                assert!(navigator.rect.width < terminal.rect.width, "{rendered}");
            } else {
                assert_eq!(navigator.rect.x, terminal.rect.x);
                assert!(terminal.rect.y > navigator.rect.y);
            }
        }
    }

    #[test]
    fn compact_workspace_reserves_usable_terminal_height() {
        let panes = workspace_panes(Rect::new(0, 0, 60, 12), 20);

        assert_eq!(panes.len(), 2);
        assert!(panes[1].height >= 6, "{panes:?}");
    }

    #[test]
    fn short_compact_workspace_keeps_session_navigation_reachable() {
        let app = workspace_fixture();
        let (_lines, hit_map) = render_app_to_lines(&app, 60, 10, &RenderState::default());

        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id.starts_with("tui-session-session-")),
            "short compact layout should retain a focusable session row"
        );
    }

    #[test]
    fn workspace_hides_transient_action_feedback() {
        let mut app = workspace_fixture();
        app.action_feedback = Some("detach requested: session-alpha".to_string());

        let rendered = render_app_to_lines(&app, 140, 42, &RenderState::default())
            .0
            .join("\n");

        assert!(!rendered.contains("action:"), "{rendered}");
        assert!(rendered.contains("session-alpha · running"), "{rendered}");
    }

    #[test]
    fn unavailable_attach_yields_to_spawn_and_cannot_dispatch() {
        let mut app = workspace_fixture();
        app.sessions = vec![SessionRow::pending("session-pending")];
        app.selected_session = Some("session-pending".to_string());
        let (lines, hit_map) = render_app_to_lines(&app, 96, 30, &RenderState::default());
        assert!(lines.iter().all(|line| !line.contains("disabled: Attach")));
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-spawn")
        );
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "workspace-attach")
        );
    }

    #[test]
    fn session_navigator_scrolls_without_hidden_row_hit_regions() {
        let mut app = workspace_fixture();
        app.sessions = (0..20)
            .map(|index| SessionRow::running(format!("session-{index:02}")))
            .collect();
        app.selected_session = Some("session-00".to_string());

        let (_lines, hit_map) = render_app_to_lines(&app, 72, 16, &RenderState::default());
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
        app.observed_terminal_inputs.clear();

        app.handle_action(
            "botster.tui.session.shutdown".to_string(),
            None,
            Some(json!({ "session_id": "session-alpha" })),
        );
        let (lines, hit_map) = render_app_to_lines(&app, 96, 30, &RenderState::default());
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
        let (_lines, confirm_hits) = render_app_to_lines(&app, 96, 30, &RenderState::default());
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
    fn session_type_form_fields_render_in_system_details() {
        let mut app = TuiApp::new(None);
        app.system_details_visible = true;
        app.session_types_supported = true;
        let mut form = SessionTypeFormDraft::create_default();
        form.id = "shell".to_string();
        form.label = "Shell".to_string();
        form.command = "printf draft".to_string();
        app.session_type_form = Some(form);

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Create session type"), "{rendered}");
        assert!(rendered.contains("printf draft"), "{rendered}");
        assert!(
            rendered.contains("execution: relative_executable"),
            "{rendered}"
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-session-type-form-submit")
        );
    }

    #[test]
    fn session_type_form_renders_explicit_execution_control() {
        let app = TuiApp::new(None);
        let form = SessionTypeFormDraft::create_default();
        let nodes = app.session_type_form_nodes(&form);
        let execution = nodes
            .iter()
            .find(|node| {
                node.kind == UiNodeKind::Select
                    && node.props.get("name") == Some(&json!("session_type_execution"))
            })
            .expect("execution select renders");

        assert_eq!(
            execution.props.get("selected"),
            Some(&json!("relative_executable"))
        );
        let options = execution.slots.get("options").expect("options render");
        assert_eq!(options.len(), 2);
        let UiChild::Node(relative) = &options[0] else {
            panic!("relative executable option renders as a node");
        };
        let UiChild::Node(shell) = &options[1] else {
            panic!("shell command option renders as a node");
        };
        assert_eq!(
            relative.props.get("value"),
            Some(&json!("relative_executable"))
        );
        assert_eq!(shell.props.get("value"), Some(&json!("shell_command")));
    }

    #[test]
    fn blank_target_first_spawn_validation_renders_visible_error_state() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.begin_target_first_spawn();

        assert_eq!(
            app.error.as_deref(),
            Some("no launch targets available (no enabled admitted spawn targets)")
        );
        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        assert!(
            lines
                .join("\n")
                .contains("error: no launch targets available (no enabled admitted spawn targets)")
        );
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
    fn missing_hub_connection_fails_closed_for_shared_profile() {
        let (connection, error) = parse_hub_connection(None);
        assert_eq!(connection, None);
        assert_eq!(error.as_deref(), Some("BOTSTER_HUB_CONNECTION is required"));
    }

    #[test]
    fn malformed_hub_connection_json_fails_closed() {
        let (connection, error) = parse_hub_connection(Some("not-json".into()));
        assert_eq!(connection, None);
        assert!(
            error
                .as_deref()
                .is_some_and(|message| message.contains("BOTSTER_HUB_CONNECTION is malformed")),
            "{error:?}"
        );
    }

    #[test]
    fn missing_shared_session_id_fails_closed() {
        assert_eq!(
            parse_shared_session_id(None).unwrap_err(),
            "BOTSTER_SHARED_SESSION_ID is required"
        );
        assert_eq!(
            parse_shared_session_id(Some("".into())).unwrap_err(),
            "BOTSTER_SHARED_SESSION_ID is required"
        );
        assert_eq!(
            parse_shared_session_id(Some("   ".into())).unwrap_err(),
            "BOTSTER_SHARED_SESSION_ID is required"
        );
    }

    #[test]
    fn terminal_hello_still_requires_core_mechanism_tokens() {
        let requirement = tui_terminal_compatibility_requirement();
        for terminal_feature in [
            botster_terminal_protocol_client::FEATURE_TERMINAL_STREAMING,
            botster_terminal_protocol_client::FEATURE_RESIZE,
            botster_terminal_protocol_client::FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY,
        ] {
            assert!(
                requirement
                    .required_features
                    .iter()
                    .any(|feature| feature == terminal_feature),
                "terminal Hello must require {terminal_feature}"
            );
        }
    }

    #[test]
    fn session_reducer_reports_matching_subscription_errors_and_ignores_foreign_ones() {
        let mut state = SessionEntityState::default();
        state.begin_generation("generation-error".to_string());

        let error = state
            .apply(DaemonEntityFrame::Error {
                subscription_id: "generation-error".to_string(),
                entity_type: "session".to_string(),
                code: "subscription_failed".to_string(),
                message: "hub dropped the session projection".to_string(),
            })
            .expect_err("a matching subscription error surfaces as a diagnostic");
        assert!(error.contains("subscription_failed"));
        assert!(error.contains("hub dropped the session projection"));

        assert!(
            !state
                .apply(DaemonEntityFrame::Error {
                    subscription_id: "generation-other".to_string(),
                    entity_type: "session".to_string(),
                    code: "subscription_failed".to_string(),
                    message: "unrelated subscription".to_string(),
                })
                .expect("a non-matching subscription error is ignored")
        );
    }

    #[test]
    fn session_binding_reference_row_exposes_every_session_type_key() {
        let reference = session_binding_reference_row();

        for key in [
            "session_type_id",
            "session_type_source",
            "role",
            "traits",
            "interaction",
            "session_type_lifecycle",
        ] {
            assert!(
                reference.contains_key(key),
                "bind-list templates must observe the {key} key"
            );
        }
    }

    #[test]
    fn compatibility_error_branch_renders_distinct_compatibility_diagnostic() {
        let mut app = TuiApp::new(None);
        let mut requirement = tui_compatibility_requirement();
        requirement
            .required_features
            .push("botster-tui-future-feature".to_string());
        let mut compatibility = DaemonCompatibility::current();
        compatibility.features.push(
            botster_terminal_protocol_client::FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY
                .to_string(),
        );
        let error = botster_hub_client::ensure_compatible(&requirement, &compatibility)
            .expect_err("unsatisfied requirement should produce compatibility error");

        app.apply_link_failure(DaemonTransportError::Compatibility(error));

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
    fn daemon_status_renders_authoritative_hub_software_identity() {
        let mut app = TuiApp::new(None);

        app.apply_response(status_response("running", 7));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("hub software: Botster Hub 9.9.9-test (botster-hub)"));
        assert!(rendered.contains("build test-build-revision"));
    }

    #[test]
    fn hub_software_identity_is_never_sourced_from_an_installed_package_row() {
        let mut app = TuiApp::new(None);

        app.apply_response(status_response_with_package_counts("running", 7, 2, 1));
        app.apply_response(packages_response(vec![
            package(
                "botster-hub",
                "0.0.1-package-row",
                "first-party",
                "enabled",
                Vec::new(),
                false,
            ),
            package(
                "workspaces",
                "0.4.2",
                "first-party",
                "installed",
                Vec::new(),
                false,
            ),
        ]));

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 64);
        let rendered = lines.join("\n");
        let software_line = rendered
            .lines()
            .find(|line| line.contains("hub software:"))
            .expect("hub software identity is rendered");

        // A package row literally named `botster-hub` carries a different version.
        // Hub identity must still come from `DaemonStatus.software`.
        assert!(software_line.contains("9.9.9-test"));
        assert!(!software_line.contains("0.0.1-package-row"));
    }

    #[test]
    fn hub_software_omits_absent_build_revision_rather_than_fabricating_one() {
        let mut app = TuiApp::new(None);
        let mut response = status_response("running", 7);
        response
            .status
            .as_mut()
            .expect("status fixture carries a status")
            .software
            .build_revision = None;

        app.apply_response(response);

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");
        let software_line = rendered
            .lines()
            .find(|line| line.contains("hub software:"))
            .expect("hub software identity is rendered");

        assert!(software_line.contains("Botster Hub 9.9.9-test"));
        assert!(!software_line.contains("build"));
    }

    #[test]
    fn hub_software_reads_unknown_before_any_status_response() {
        let mut app = TuiApp::new(None);
        app.system_details_visible = true;

        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
        let rendered = lines.join("\n");

        assert!(rendered.contains("hub software: unknown"));
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

        app.apply_link_failure(DaemonTransportError::Compatibility(error));
        app.apply_link_failure(DaemonTransportError::ClientDisconnected);
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
        app.session_types_supported = true;
        app.system_details_visible = true;
        app.begin_target_first_spawn();
        assert_eq!(
            app.error.as_deref(),
            Some("no launch targets available (no enabled admitted spawn targets)")
        );

        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        app.error = None;
        app.begin_target_first_spawn();

        let (lines, _) = renderer::render_to_lines(&app.surface(), 160, 70);
        let rendered = lines.join("\n");
        assert!(
            !rendered
                .contains("error: no launch targets available (no enabled admitted spawn targets)")
        );
        assert!(rendered.contains("Target-first spawn"));
        assert!(app.target_first_spawn.is_some());
    }

    #[test]
    fn not_running_path_is_not_reported_as_compatibility_mismatch() {
        let mut app = TuiApp::new(None);

        app.apply_link_failure(DaemonTransportError::NotRunning);

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
    fn activating_session_list_row_attaches_that_session() {
        let mut app = TuiApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-alpha".to_string());
        app.observed_requests.clear();
        let (_lines, hit_map) = render_app_to_lines(&app, 120, 48, &RenderState::default());
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

        assert!(app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::Attach { session_id, .. } if session_id == "session-beta"
        )));
    }

    #[test]
    fn session_click_cancels_when_redraw_reorders_another_row_under_release() {
        let mut app = TuiApp::new(None);
        app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "running")]);
        app.selected_session = Some("session-alpha".to_string());
        let (_lines, frame_n_hit_map) = render_app_to_lines(&app, 120, 48, &RenderState::default());
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
        let (_lines, frame_n_plus_one_hit_map) =
            render_app_to_lines(&app, 120, 48, &RenderState::default());
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
    fn missing_terminal_snapshot_delivery_on_hello_ack_fails_before_attach() {
        let mut compatibility = TerminalCompatibility::current();
        compatibility.features.retain(|feature| {
            feature
                != botster_terminal_protocol_client::FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY
        });
        let ack = DaemonHelloAck {
            protocol: PROTOCOL.to_string(),
            compatibility: {
                let mut host = DaemonCompatibility::current();
                host.features.push(
                    botster_terminal_protocol_client::FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY
                        .to_string(),
                );
                host.features
                    .push(FEATURE_UNIX_TERMINAL_ADAPTER.to_string());
                host.features
                    .push(FEATURE_TERMINAL_SUBSCRIPTION_CLOSED.to_string());
                host
            },
            terminal_compatibility: Some(compatibility),
            diagnostics: Vec::new(),
        };
        let error = admit_terminal_hello(&ack)
            .expect_err("missing terminal snapshot_delivery must fail before Attach");
        let diagnostic = match error {
            DaemonTransportError::Compatibility(error) => error.diagnostic,
            other => panic!("expected compatibility error, got {other}"),
        };
        assert!(
            diagnostic.contains("snapshot_delivery=ready_then_history"),
            "{diagnostic}"
        );
    }

    #[test]
    fn missing_terminal_compatibility_ack_field_fails_before_attach() {
        let ack = DaemonHelloAck {
            protocol: PROTOCOL.to_string(),
            compatibility: DaemonCompatibility::current(),
            terminal_compatibility: None,
            diagnostics: Vec::new(),
        };
        admit_terminal_hello(&ack)
            .expect_err("omitted terminal_compatibility must fail before Attach");
    }

    #[test]
    fn terminal_subscription_ids_advance_the_per_app_sequence() {
        let mut app = TuiApp::new(None);
        assert_eq!(app.next_terminal_subscription_sequence, 1);

        let first = app.mint_subscription_id();
        let second = app.mint_subscription_id();

        assert_ne!(first, second);
        assert!(first.ends_with("-1"));
        assert!(second.ends_with("-2"));
        assert_eq!(app.next_terminal_subscription_sequence, 3);
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
            session_type_id: None,
            session_type_source: None,
            role: None,
            traits: Vec::new(),
            interaction: None,
            session_type_lifecycle: None,
        }];
        app.selected_session = Some("session-beta".to_string());
        let (_lines, hit_map) = render_app_to_lines(&app, 120, 48, &RenderState::default());
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
        let (lines, _) = render_app_to_lines(&app, 120, 48, &RenderState::default());
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
    fn terminal_focus_does_not_attempt_to_attach_non_running_session() {
        let mut app = TuiApp::new(None);
        app.sessions = vec![SessionRow {
            session_id: "session-beta".to_string(),
            lifecycle: "stopped".to_string(),
            failure_reason: None,
            pending: false,
            session_type_id: None,
            session_type_source: None,
            role: None,
            traits: Vec::new(),
            interaction: None,
            session_type_lifecycle: None,
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
        assert_eq!(app.error, None);
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
                ObservedRequest::ListSpawnTargets,
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
    fn spawn_opener_selection_uses_realized_semantic_action_not_visible_copy() {
        let workspace_id = "workspace-semantic-action";
        let semantic_node_id = "opaque-producer-node-7f3a";
        let semantic_payload = json!({
            "selected_workspace": workspace_id,
            "dialog": "spawn-target:workspace-semantic-action"
        });
        let mut root = node(UiNodeKind::Stack, "semantic-action-fixture", json!({}));
        root.children = vec![
            child(button(
                semantic_node_id,
                "Create session",
                "botster_workspaces.open_spawn",
                semantic_payload.clone(),
            )),
            child(button(
                "visible-spawn-generic-decoy",
                "Spawn",
                "botster_workspaces.open",
                json!({
                    "selected_workspace": workspace_id,
                    "dialog": "spawn-target:workspace-semantic-action"
                }),
            )),
        ];
        let mut router = InputRouter::new(renderer::action_request_context_for(WORKSPACES_SURFACE));
        let (lines, hit_map) = botster_tui_kit::render_to_lines_with_presentation_state(
            &root,
            120,
            48,
            &router.render_state(),
            &Default::default(),
        )
        .expect("render semantic action fixture through the real frame backend");
        assert!(lines.join("\n").contains("Spawn"));

        let (selected_node_id, selected_action) =
            unique_acceptance_action(&hit_map, WORKSPACES_SPAWN_OPENER_ACTION, |_| true, &lines)
                .expect("select the unique semantic Spawn opener");
        assert_eq!(selected_node_id, semantic_node_id);
        assert_eq!(selected_action.payload, Some(semantic_payload.clone()));

        focus_acceptance_node(&mut router, &hit_map, &selected_node_id)
            .expect("focus semantic action with keyboard traversal");
        let dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            &hit_map,
        );
        let InputDispatch::Action(request) = dispatch else {
            panic!("focused semantic action must dispatch through InputRouter");
        };
        assert_eq!(
            request.node_id,
            Some(UiNodeId(semantic_node_id.to_string()))
        );
        assert_eq!(request.action_id.0, "botster_workspaces.open_spawn");
        assert_eq!(request.payload, Some(semantic_payload));
        assert_ne!(
            request.node_id,
            Some(UiNodeId("visible-spawn-generic-decoy".to_string()))
        );
        assert_ne!(request.action_id.0, "botster_workspaces.open");
    }

    /// Hermetic: contract-matrix mode must fail closed when its fixture env is
    /// missing, independent of any Workspaces profile path.
    #[test]
    fn contract_matrix_mode_requires_its_fixture_env_var() {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../script/test-live-hub");
        let contract_matrix = std::process::Command::new(&script)
            .arg("contract-matrix")
            .env_remove("BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE")
            .output()
            .expect("contract-matrix mode runs");
        let stderr = String::from_utf8_lossy(&contract_matrix.stderr);
        assert!(
            !contract_matrix.status.success(),
            "contract-matrix without fixture must exit non-zero"
        );
        assert!(
            stderr.contains("BOTSTER_PLUGIN_CONTRACT_MATRIX_FIXTURE is required"),
            "contract-matrix must reach its own validation; stderr was: {stderr}"
        );
    }

    #[test]
    fn ghostty_shared_wrapper_fails_closed_without_caller_injectors() {
        let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../script/test-live-hub");
        let missing_connection = std::process::Command::new(&script)
            .arg("ghostty-shared")
            .env_remove("BOTSTER_HUB_CONNECTION")
            .env_remove("BOTSTER_HUB_BIN")
            .env("BOTSTER_SHARED_SESSION_ID", "north-star-shared")
            .output()
            .expect("ghostty-shared missing connection");
        let stderr = String::from_utf8_lossy(&missing_connection.stderr);
        assert!(!missing_connection.status.success());
        assert!(
            stderr.contains("BOTSTER_HUB_CONNECTION is required"),
            "stderr was: {stderr}"
        );
        assert!(
            !stderr.contains("BOTSTER_HUB_BIN"),
            "shared wrapper must not require Hub binaries; stderr was: {stderr}"
        );

        let missing_session = std::process::Command::new(&script)
            .arg("ghostty-shared-exit")
            .env(
                "BOTSTER_HUB_CONNECTION",
                r#"{"transport":{"type":"unix_socket","path":"/tmp/hub.sock"}}"#,
            )
            .env_remove("BOTSTER_SHARED_SESSION_ID")
            .env_remove("BOTSTER_HUB_BIN")
            .output()
            .expect("ghostty-shared-exit missing session");
        let stderr = String::from_utf8_lossy(&missing_session.stderr);
        assert!(!missing_session.status.success());
        assert!(
            stderr.contains("BOTSTER_SHARED_SESSION_ID is required"),
            "stderr was: {stderr}"
        );

        let malformed = std::process::Command::new(&script)
            .arg("ghostty-shared")
            .env("BOTSTER_HUB_CONNECTION", "not-json")
            .env("BOTSTER_SHARED_SESSION_ID", "north-star-shared")
            .env_remove("BOTSTER_HUB_BIN")
            .output()
            .expect("ghostty-shared malformed connection");
        let stderr = String::from_utf8_lossy(&malformed.stderr);
        assert!(!malformed.status.success());
        assert!(
            stderr.contains("BOTSTER_HUB_CONNECTION is malformed"),
            "stderr was: {stderr}"
        );
    }

    /// Default-gate cold-cut invariant: the installed Workspaces spawn-form driver
    /// must key the form field as session_type_id, never the retired template
    /// field name. Live lanes prove the field works end-to-end; this scan keeps a
    /// silent revert from surviving `script/test`.
    #[test]
    fn workspaces_spawn_acceptance_uses_session_type_id_field_key() {
        let source = source_without_line_comments();
        let call_site = concat!(
            "select_only_acceptance_value(\n",
            "        app,\n",
            "        router,\n",
            "        \"",
            "session",
            "_type_id\",\n"
        );
        assert!(
            source.contains(call_site),
            "acceptance spawn-form selector must pass the session type field name"
        );

        let forbidden_field = concat!("template", "_id");
        assert!(
            !source.contains(forbidden_field),
            "acceptance source must not retain the retired spawn form field key"
        );

        let fixture = include_str!("../fixtures/workspaces-spawn-driver-v1.evidence.jsonl");
        assert!(
            !fixture.contains(forbidden_field),
            "checked-in spawn-driver evidence example must not teach the retired field key"
        );
        assert!(
            fixture.contains(concat!("\"", "session", "_type_id\"")),
            "checked-in spawn-driver evidence example must use the session type field key"
        );
    }

    #[test]
    fn claim_session_baseline_requires_lifecycle_class_current() {
        let mut app = TuiApp::new(None);
        app.session_entities.has_snapshot = true;
        let session_uuid = "00000000-0000-4000-8000-0000000000ee";
        let mut ended = session_entity(session_uuid, Some("exited"));
        ended.lifecycle_class = "ended".to_string();
        app.session_entities
            .entities
            .insert(session_uuid.to_string(), ended);
        assert!(
            !claim_session_is_current(&app, session_uuid),
            "ended lifecycle_class must not satisfy claim baseline"
        );
        app.session_entities.entities.insert(
            session_uuid.to_string(),
            session_entity(session_uuid, Some("running")),
        );
        assert!(
            claim_session_is_current(&app, session_uuid),
            "current lifecycle_class must satisfy claim baseline"
        );
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
                session_type_id: None,
                session_type_source: None,
                role: None,
                traits: Vec::new(),
                interaction: None,
                session_type_lifecycle: None,
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
            session_type_id: None,
            session_type_source: None,
            role: None,
            traits: Vec::new(),
            interaction: None,
            session_type_lifecycle: None,
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
            retention: None,
            compatibility: DaemonCompatibility {
                protocol: PROTOCOL.to_string(),
                protocol_version: 1,
                features: vec![
                    FEATURE_SESSIONS.to_string(),
                    botster_terminal_protocol_client::FEATURE_TERMINAL_STREAMING.to_string(),
                    botster_terminal_protocol_client::FEATURE_RESIZE.to_string(),
                    FEATURE_PACKAGE_NAVIGATION.to_string(),
                    FEATURE_TERMINAL_READBACK.to_string(),
                ],
                conformance_fixture_revision: 1,
            },
            software: botster_hub_client::DaemonSoftwareIdentity {
                product_id: "botster-hub".to_string(),
                product_name: "Botster Hub".to_string(),
                version: "9.9.9-test".to_string(),
                build_revision: Some("test-build-revision".to_string()),
            },
            installation: botster_hub_client::DaemonInstallationIdentity {
                mode: botster_hub_client::DaemonInstallationMode::Development,
                provenance: "test".to_string(),
                release_channel: None,
                provider: None,
                diagnostics: Vec::new(),
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
            live_attach_occupancy: Vec::new(),
            observability: botster_hub_client::DaemonObservabilityCounters::default(),
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
            notice_reactions: Vec::new(),
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

    fn sample_session_type(
        session_type_id: &str,
        source: &str,
        editable: bool,
    ) -> DaemonSessionType {
        DaemonSessionType {
            session_type_id: session_type_id.to_string(),
            source_name: source.to_string(),
            id: session_type_id
                .rsplit_once('/')
                .map(|(_, id)| id.to_string())
                .unwrap_or_else(|| session_type_id.to_string()),
            source: source.to_string(),
            editable,
            overridden_sources: Vec::new(),
            diagnostics: Vec::new(),
            label: format!("label-{session_type_id}"),
            description: None,
            icon: None,
            role: "botster.agent".to_string(),
            interaction: "interactive".to_string(),
            traits: vec!["namespaced.trait".to_string()],
            lifecycle: "task".to_string(),
            execution: DaemonSessionTypeExecution::RelativeExecutable,
            command: "/bin/echo".to_string(),
            args: vec!["hello".to_string()],
            working_directory_policy: "package_root".to_string(),
            allowed_environment_overrides: Vec::new(),
            context_keys: Vec::new(),
            target_id: "repo-a".to_string(),
            available: true,
        }
    }

    #[test]
    fn session_types_render_package_read_only_and_unknown_literals() {
        let mut app = TuiApp::new(None);
        app.system_details_visible = true;
        app.session_types_supported = true;
        app.session_type_entities.begin_generation("st".to_string());
        let mut package = sample_session_type("package.demo/init", "package", false);
        package.role = "custom.role.token".to_string();
        package.traits = vec!["unknown.trait.token".to_string()];
        package.interaction = "service".to_string();
        app.session_type_entities
            .entities
            .insert(package.session_type_id.clone(), package.clone());
        app.session_type_entities
            .entity_order
            .push(package.session_type_id.clone());
        app.selected_session_type_id = Some(package.session_type_id.clone());

        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 220, 70);
        let rendered = lines.join("\n");
        assert!(rendered.contains("custom.role.token"), "{rendered}");
        assert!(rendered.contains("unknown.trait.token"), "{rendered}");
        assert!(rendered.contains("read-only"), "{rendered}");
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id.contains("-edit")),
            "package rows must not expose edit"
        );
    }

    #[test]
    fn authoring_seed_and_wholesale_definition_preserve_path_and_environment() {
        let editable = DaemonSessionTypeEditableDefinition {
            session_type_id: "device/shell".to_string(),
            source: DaemonSessionTypeMutationSource::Device,
            definition: DaemonSessionTypeDefinition {
                id: "shell".to_string(),
                label: "Shell".to_string(),
                description: None,
                icon: None,
                role: "botster.agent".to_string(),
                interaction: "interactive".to_string(),
                traits: vec!["keep.trait".to_string()],
                lifecycle: "task".to_string(),
                execution: DaemonSessionTypeExecution::ShellCommand,
                command: "shell.sh".to_string(),
                args: Vec::new(),
                working_directory: DaemonSessionTypeWorkingDirectory::Relative {
                    path: "nested/path".to_string(),
                },
                environment: BTreeMap::from([("KEEP".to_string(), "yes".to_string())]),
                allowed_environment_overrides: Vec::new(),
                context: Vec::new(),
                target_id: None,
            },
        };
        let form = SessionTypeFormDraft::from_authoring(editable);
        assert_eq!(form.working_directory_path, "nested/path");
        assert!(form.environment.contains("KEEP=yes"));
        let mut form = form;
        form.label = "Shell 2".to_string();
        let definition = definition_from_session_type_form(&form).expect("form reconstructs");
        assert_eq!(
            definition.working_directory,
            DaemonSessionTypeWorkingDirectory::Relative {
                path: "nested/path".to_string()
            }
        );
        assert_eq!(
            definition.environment.get("KEEP").map(String::as_str),
            Some("yes")
        );
        assert_eq!(definition.label, "Shell 2");
        assert_eq!(definition.traits, vec!["keep.trait".to_string()]);
        assert_eq!(
            definition.execution,
            DaemonSessionTypeExecution::ShellCommand
        );
    }

    #[test]
    fn session_type_create_form_defaults_to_relative_executable() {
        let mut form = SessionTypeFormDraft::create_default();
        form.label = "Shell".to_string();
        form.command = "printf ready && exec agent".to_string();
        form.args = "first, second".to_string();

        let definition = definition_from_session_type_form(&form).expect("form reconstructs");

        assert_eq!(form.execution, "relative_executable");
        assert_eq!(
            definition.execution,
            DaemonSessionTypeExecution::RelativeExecutable
        );
        assert_eq!(definition.command, "printf ready && exec agent");
        assert_eq!(definition.args, vec!["first", "second"]);
    }

    #[test]
    fn session_type_create_form_preserves_selected_execution_and_separate_args() {
        let mut app = TuiApp::new(None);
        app.session_type_form = Some(SessionTypeFormDraft::create_default());
        app.apply_session_type_form_values(&UiFormValues(serde_json::Map::from_iter([
            ("session_type_execution".to_string(), json!("shell_command")),
            (
                "session_type_command".to_string(),
                json!("printf '%s' \"$1\""),
            ),
            ("session_type_args".to_string(), json!("first, second")),
        ])));
        let form = app.session_type_form.expect("form remains open");

        let definition = definition_from_session_type_form(&form).expect("form reconstructs");

        assert_eq!(
            definition.execution,
            DaemonSessionTypeExecution::ShellCommand
        );
        assert_eq!(definition.command, "printf '%s' \"$1\"");
        assert_eq!(definition.args, vec!["first", "second"]);
    }

    #[test]
    fn session_type_edit_form_preserves_shell_command_and_args() {
        let editable = DaemonSessionTypeEditableDefinition {
            session_type_id: "device/shell".to_string(),
            source: DaemonSessionTypeMutationSource::Device,
            definition: DaemonSessionTypeDefinition {
                id: "shell".to_string(),
                label: "Shell".to_string(),
                description: None,
                icon: None,
                role: "botster.agent".to_string(),
                interaction: "interactive".to_string(),
                traits: Vec::new(),
                lifecycle: "task".to_string(),
                execution: DaemonSessionTypeExecution::ShellCommand,
                command: "printf '%s' \"$1\"".to_string(),
                args: vec!["first value".to_string(), "second".to_string()],
                working_directory: DaemonSessionTypeWorkingDirectory::PackageRoot,
                environment: BTreeMap::new(),
                allowed_environment_overrides: Vec::new(),
                context: Vec::new(),
                target_id: None,
            },
        };

        let form = SessionTypeFormDraft::from_authoring(editable);
        let definition = definition_from_session_type_form(&form).expect("form reconstructs");

        assert_eq!(form.execution, "shell_command");
        assert_eq!(
            definition.execution,
            DaemonSessionTypeExecution::ShellCommand
        );
        assert_eq!(definition.command, "printf '%s' \"$1\"");
        assert_eq!(definition.args, vec!["first value", "second"]);
    }

    #[test]
    fn session_type_execution_modes_round_trip_through_form_reconstruction() {
        for execution in [
            DaemonSessionTypeExecution::RelativeExecutable,
            DaemonSessionTypeExecution::ShellCommand,
        ] {
            let editable = DaemonSessionTypeEditableDefinition {
                session_type_id: "device/round-trip".to_string(),
                source: DaemonSessionTypeMutationSource::Device,
                definition: DaemonSessionTypeDefinition {
                    id: "round-trip".to_string(),
                    label: "Round trip".to_string(),
                    description: None,
                    icon: None,
                    role: "botster.agent".to_string(),
                    interaction: "interactive".to_string(),
                    traits: Vec::new(),
                    lifecycle: "task".to_string(),
                    execution: execution.clone(),
                    command: "bin/agent --literal-text".to_string(),
                    args: vec!["one value".to_string()],
                    working_directory: DaemonSessionTypeWorkingDirectory::PackageRoot,
                    environment: BTreeMap::new(),
                    allowed_environment_overrides: Vec::new(),
                    context: Vec::new(),
                    target_id: None,
                },
            };

            let form = SessionTypeFormDraft::from_authoring(editable);
            let reconstructed =
                definition_from_session_type_form(&form).expect("form reconstructs");

            assert_eq!(reconstructed.execution, execution);
            assert_eq!(reconstructed.command, "bin/agent --literal-text");
            assert_eq!(reconstructed.args, vec!["one value"]);
        }
    }

    #[test]
    fn omitted_session_type_execution_defaults_to_relative_executable() {
        let definition: DaemonSessionTypeDefinition = serde_json::from_value(json!({
            "id": "defaulted",
            "label": "Defaulted",
            "role": "botster.agent",
            "interaction": "interactive",
            "lifecycle": "task",
            "command": "bin/defaulted"
        }))
        .expect("definition decodes");

        assert_eq!(
            definition.execution,
            DaemonSessionTypeExecution::RelativeExecutable
        );
    }

    #[test]
    fn clearing_token_list_fields_emits_empty_collections_on_update() {
        let editable = DaemonSessionTypeEditableDefinition {
            session_type_id: "device/shell".to_string(),
            source: DaemonSessionTypeMutationSource::Device,
            definition: DaemonSessionTypeDefinition {
                id: "shell".to_string(),
                label: "Shell".to_string(),
                description: None,
                icon: None,
                role: "botster.agent".to_string(),
                interaction: "interactive".to_string(),
                traits: vec!["a.trait".to_string()],
                lifecycle: "task".to_string(),
                execution: DaemonSessionTypeExecution::RelativeExecutable,
                command: "shell.sh".to_string(),
                args: vec!["--flag".to_string()],
                working_directory: DaemonSessionTypeWorkingDirectory::PackageRoot,
                environment: BTreeMap::from([("KEEP".to_string(), "yes".to_string())]),
                allowed_environment_overrides: vec!["KEEP".to_string()],
                context: vec!["prompt".to_string()],
                target_id: None,
            },
        };
        let mut form = SessionTypeFormDraft::from_authoring(editable);
        form.traits.clear();
        form.args.clear();
        form.environment.clear();
        form.allowed_environment_overrides.clear();
        form.context_keys.clear();
        let definition = definition_from_session_type_form(&form).expect("form reconstructs");
        assert!(definition.traits.is_empty());
        assert!(definition.args.is_empty());
        assert!(definition.environment.is_empty());
        assert!(definition.allowed_environment_overrides.is_empty());
        assert!(definition.context.is_empty());
    }

    #[test]
    fn launch_targets_are_enabled_admitted_spawn_targets_only() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.spawn_targets = vec![
            DaemonSpawnTarget {
                target_id: "repo-a".to_string(),
                label: "Repo A".to_string(),
                root: std::path::PathBuf::from("/tmp/repo-a"),
                enabled: true,
                kind: "git".to_string(),
                base_ref: None,
                metadata: BTreeMap::new(),
            },
            DaemonSpawnTarget {
                target_id: "repo-disabled".to_string(),
                label: "Disabled".to_string(),
                root: std::path::PathBuf::from("/tmp/repo-disabled"),
                enabled: false,
                kind: "git".to_string(),
                base_ref: None,
                metadata: BTreeMap::new(),
            },
        ];
        let mut device = sample_session_type("device/shell", "device", true);
        device.target_id = "device:local".to_string();
        app.session_type_entities.begin_generation("st".to_string());
        app.session_type_entities
            .entities
            .insert(device.session_type_id.clone(), device.clone());
        app.session_type_entities
            .entity_order
            .push(device.session_type_id.clone());
        let options = app.launch_target_options();
        assert_eq!(
            options
                .iter()
                .map(|option| option.target_id.as_str())
                .collect::<Vec<_>>(),
            vec!["repo-a"]
        );
        assert!(
            !options
                .iter()
                .any(|option| option.target_id == "device:local"
                    || option.target_id.starts_with("package:")),
            "must not synthesize device:local/package launch targets: {options:?}"
        );
    }

    #[test]
    fn toolbar_spawn_dialog_is_reachable_without_system_details() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.system_details_visible = false;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        app.handle_action("botster.tui.spawn".to_string(), None, None);
        assert!(app.target_first_spawn.is_some());
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
        let rendered = lines.join("\n");
        assert!(rendered.contains("Target-first spawn"), "{rendered}");
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-spawn-cancel")
        );
        app.handle_action("botster.tui.spawn.cancel".to_string(), None, None);
        assert!(app.target_first_spawn.is_none());
    }

    #[test]
    fn session_type_form_draft_keystrokes_render_before_submit() {
        let mut app = TuiApp::new(None);
        app.system_details_visible = true;
        app.session_types_supported = true;
        let mut form = SessionTypeFormDraft::create_default();
        form.command = String::new();
        app.session_type_form = Some(form);
        let (_lines, hit_map) = render_app_to_lines(&app, 220, 80, &RenderState::default());
        let field = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-session-type-field-session_type_command")
            .expect("command field hit region");
        let mut router = InputRouter::new(renderer::action_request_context());
        let column = field.rect.x;
        let row = field.rect.y;
        app.handle_dispatch(router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        ));
        app.handle_dispatch(router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        ));
        for ch in ['z', 's', 'h'] {
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            let dispatch = router.dispatch_event(Event::Key(key), &hit_map);
            app.handle_dispatch(dispatch);
            app.set_drafts(router.draft_values());
        }
        assert_eq!(
            app.drafts
                .get("session_type_command")
                .and_then(Value::as_str),
            Some("zsh")
        );
        let (lines, _) = render_app_to_lines(&app, 220, 80, &RenderState::default());
        let rendered = lines.join("\n");
        assert!(rendered.contains("zsh"), "{rendered}");
    }

    #[test]
    fn product_toolbar_spawn_emits_spawn_session_type_request() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.system_details_visible = false;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        let mut global = sample_session_type("device/shell", "device", true);
        global.target_id = "repo-a".to_string();
        global.available = true;
        app.list_for_target_stub = Some(ListForTargetStub::Ok(vec![global]));
        app.observed_requests.clear();
        app.handle_action("botster.tui.spawn".to_string(), None, None);
        app.handle_action(
            "botster.tui.spawn.pick_target".to_string(),
            None,
            Some(json!({ "target_id": "repo-a" })),
        );
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::ListSessionTypesForTarget { target_id }
                    if target_id == "repo-a"
            )),
            "pick target must observe ListSessionTypesForTarget: {:?}",
            app.observed_requests
        );
        app.handle_action(
            "botster.tui.spawn.pick_session_type".to_string(),
            None,
            Some(json!({ "session_type_id": "device/shell" })),
        );
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::SpawnSessionType {
                    session_type_id,
                    target_id: Some(target_id),
                    ..
                } if session_type_id == "device/shell" && target_id == "repo-a"
            )),
            "{:?}",
            app.observed_requests
        );
        assert!(
            !app.observed_requests
                .iter()
                .any(|request| matches!(request, ObservedRequest::Spawn { .. }))
        );
    }

    #[test]
    fn delete_session_type_uses_source_name_for_repo_mutation_source() {
        let mut app = TuiApp::new(None);
        let mut entity = sample_session_type("shared-git/type-a", "repo", true);
        entity.source_name = "shared-git".to_string();
        entity.target_id = "authored-other-target".to_string();
        entity.id = "type-a".to_string();
        app.session_type_entities.begin_generation("st".to_string());
        app.session_type_entities
            .entities
            .insert(entity.session_type_id.clone(), entity);
        app.session_type_entities
            .entity_order
            .push("shared-git/type-a".to_string());
        app.observed_requests.clear();
        app.delete_session_type("shared-git/type-a");
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::DeleteSessionType {
                    source: DaemonSessionTypeMutationSource::Repo { target_id },
                    session_type_id,
                } if target_id == "shared-git" && session_type_id == "type-a"
            )),
            "{:?}",
            app.observed_requests
        );
    }

    #[test]
    fn pinned_session_plugin_binding_fixture_is_conformance_40() {
        let scenario = botster_hub_test_support::session_plugin_binding_conformance_scenario();
        assert_eq!(
            scenario.conformance_fixture_revision, 49,
            "hub-test-support pin must publish fixture revision 49"
        );
        assert!(scenario.conformance_fixture_revision >= MINIMUM_CONFORMANCE_FIXTURE_REVISION);
    }

    #[test]
    fn product_toolbar_spawn_opens_target_first_flow_not_freeform_spawn() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.system_details_visible = true;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        app.observed_requests.clear();
        app.handle_action("botster.tui.spawn".to_string(), None, None);
        assert!(app.target_first_spawn.is_some());
        assert!(
            !app.observed_requests
                .iter()
                .any(|request| matches!(request, ObservedRequest::Spawn { .. })),
            "toolbar spawn must not emit freeform Spawn"
        );
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
        let rendered = lines.join("\n");
        assert!(
            rendered.contains("Select a launch target first"),
            "{rendered}"
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-spawn-target-repo-a")
        );
    }

    #[test]
    fn session_types_unsupported_surface_when_feature_missing() {
        let mut app = TuiApp::new(None);
        app.system_details_visible = true;
        app.session_types_supported = false;
        let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 60);
        let rendered = lines.join("\n");
        assert!(
            rendered.contains("does not provide session_type_entity_subscriptions"),
            "{rendered}"
        );
    }

    #[test]
    fn target_first_spawn_picker_uses_list_rows_not_entity_target_equality() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        // Entity catalog has a type whose target_id equals repo-a, plus a Global
        // that would never match entity equality for real admitted T.
        let mut repo_entity = sample_session_type("repo-a/svc", "repo", true);
        repo_entity.target_id = "repo-a".to_string();
        let mut global_entity = sample_session_type("device/shell", "device", true);
        global_entity.target_id = "device:local".to_string();
        app.session_type_entities.begin_generation("st".to_string());
        for entity in [&repo_entity, &global_entity] {
            app.session_type_entities
                .entities
                .insert(entity.session_type_id.clone(), entity.clone());
            app.session_type_entities
                .entity_order
                .push(entity.session_type_id.clone());
        }
        // Hub list-for-target projects Global with list-context target_id = T.
        let mut listed_global = sample_session_type("device/shell", "device", true);
        listed_global.target_id = "repo-a".to_string();
        listed_global.available = true;
        app.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickSessionType {
                target_id: "repo-a".to_string(),
                target_label: "Repo A".to_string(),
                session_types: vec![listed_global],
            },
        });
        let nodes = app.target_first_spawn_nodes(app.target_first_spawn.as_ref().unwrap());
        let buttons: Vec<String> = nodes
            .iter()
            .filter(|node| node.kind == UiNodeKind::Button)
            .filter_map(|node| {
                node.id
                    .as_ref()
                    .and_then(UiAuthoredNodeId::as_literal)
                    .map(|id| id.0.clone())
            })
            .collect();
        assert!(
            buttons
                .iter()
                .any(|id| id.contains("tui-spawn-session-type-device/shell")),
            "Global from Hub list must appear for admitted T: {buttons:?}"
        );
        assert!(
            !buttons
                .iter()
                .any(|id| id.contains("tui-spawn-session-type-repo-a/svc")),
            "entity-only rows must not appear when absent from Hub list: {buttons:?}"
        );
    }

    #[test]
    fn product_pick_target_list_failure_keeps_flow_recoverable_without_stale_rows() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        // Seed a prior successful list so a failed re-pick must not leave those rows.
        let prior = sample_session_type("device/stale", "device", true);
        app.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickSessionType {
                target_id: "repo-a".to_string(),
                target_label: "Repo A".to_string(),
                session_types: vec![prior],
            },
        });
        app.list_for_target_stub = Some(ListForTargetStub::OperatorError {
            code: "target_unavailable".to_string(),
            operation: "list_session_types_for_target".to_string(),
            message: "spawn target is not eligible".to_string(),
        });
        app.observed_requests.clear();
        app.handle_action(
            "botster.tui.spawn.pick_target".to_string(),
            None,
            Some(json!({ "target_id": "repo-a" })),
        );
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::ListSessionTypesForTarget { target_id }
                    if target_id == "repo-a"
            )),
            "{:?}",
            app.observed_requests
        );
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("spawn target is not eligible")),
            "{:?}",
            app.error
        );
        match app.target_first_spawn.as_ref().map(|flow| &flow.step) {
            Some(TargetFirstSpawnStep::PickTarget) => {}
            other => panic!("failed list must stay on PickTarget, got {other:?}"),
        }
        let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
        let rendered = lines.join("\n");
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id.contains("tui-spawn-session-type-")),
            "no selectable session-type rows after failed list: {:?}",
            hit_map
                .regions()
                .iter()
                .map(|r| &r.node_id)
                .collect::<Vec<_>>()
        );
        assert!(
            hit_map
                .regions()
                .iter()
                .any(|region| region.node_id == "tui-spawn-cancel"),
            "flow must remain cancellable: {rendered}"
        );
        // Recovery: re-pick after a successful list.
        let mut recovered = sample_session_type("device/shell", "device", true);
        recovered.target_id = "repo-a".to_string();
        app.list_for_target_stub = Some(ListForTargetStub::Ok(vec![recovered]));
        app.error = None;
        app.handle_action(
            "botster.tui.spawn.pick_target".to_string(),
            None,
            Some(json!({ "target_id": "repo-a" })),
        );
        assert_eq!(app.error, None);
        match &app.target_first_spawn.as_ref().unwrap().step {
            TargetFirstSpawnStep::PickSessionType { session_types, .. } => {
                assert_eq!(session_types.len(), 1);
                assert_eq!(session_types[0].session_type_id, "device/shell");
            }
            other => panic!("recovery should reach PickSessionType, got {other:?}"),
        }
    }

    #[test]
    fn product_pick_target_transport_error_keeps_no_stale_selectable_rows() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        let prior = sample_session_type("device/stale", "device", true);
        app.target_first_spawn = Some(TargetFirstSpawnFlow {
            step: TargetFirstSpawnStep::PickSessionType {
                target_id: "repo-a".to_string(),
                target_label: "Repo A".to_string(),
                session_types: vec![prior],
            },
        });
        app.list_for_target_stub = Some(ListForTargetStub::TransportError);
        app.handle_action(
            "botster.tui.spawn.pick_target".to_string(),
            None,
            Some(json!({ "target_id": "repo-a" })),
        );
        match app.target_first_spawn.as_ref().map(|flow| &flow.step) {
            Some(TargetFirstSpawnStep::PickTarget) => {}
            other => panic!("transport failure must stay on PickTarget, got {other:?}"),
        }
        let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
        assert!(
            !hit_map
                .regions()
                .iter()
                .any(|region| region.node_id.contains("tui-spawn-session-type-")),
            "transport failure must not leave selectable session-type rows"
        );
    }

    #[test]
    fn product_spawn_list_and_pick_are_reachable_through_input_router() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.system_details_visible = false;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        let mut listed = sample_session_type("device/shell", "device", true);
        listed.target_id = "repo-a".to_string();
        app.list_for_target_stub = Some(ListForTargetStub::Ok(vec![listed]));
        app.handle_action("botster.tui.spawn".to_string(), None, None);
        let mut router = InputRouter::new(renderer::action_request_context());
        // Mouse path (hit-map click activation).
        let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &RenderState::default());
        let target_region = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-spawn-target-repo-a")
            .expect("launch target hit region");
        let column = target_region.rect.x;
        let row = target_region.rect.y;
        app.handle_dispatch(router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        ));
        app.handle_dispatch(router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        ));
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::ListSessionTypesForTarget { target_id }
                    if target_id == "repo-a"
            )),
            "{:?}",
            app.observed_requests
        );
        let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &RenderState::default());
        let type_region = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-spawn-session-type-device/shell")
            .expect("session type hit region from Hub list");
        let column = type_region.rect.x;
        let row = type_region.rect.y;
        app.handle_dispatch(router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        ));
        app.handle_dispatch(router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        ));
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::SpawnSessionType {
                    session_type_id,
                    target_id: Some(target_id),
                    ..
                } if session_type_id == "device/shell" && target_id == "repo-a"
            )),
            "{:?}",
            app.observed_requests
        );
    }

    #[test]
    fn product_spawn_list_and_pick_are_reachable_through_keyboard_input_router() {
        let mut app = TuiApp::new(None);
        app.session_types_supported = true;
        app.system_details_visible = false;
        app.spawn_targets = vec![DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        }];
        let mut listed = sample_session_type("device/shell", "device", true);
        listed.target_id = "repo-a".to_string();
        app.list_for_target_stub = Some(ListForTargetStub::Ok(vec![listed]));
        app.observed_requests.clear();
        app.handle_action("botster.tui.spawn".to_string(), None, None);
        let mut router = InputRouter::new(renderer::action_request_context());

        // Keyboard path: Tab focus admitted T, Enter → ListSessionTypesForTarget.
        let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &router.render_state());
        focus_hit_map_node_by_tab(&mut router, &hit_map, "tui-spawn-target-repo-a");
        activate_focused_action_with_enter(&mut app, &mut router, &hit_map);
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::ListSessionTypesForTarget { target_id }
                    if target_id == "repo-a"
            )),
            "keyboard pick target must observe ListSessionTypesForTarget: {:?}",
            app.observed_requests
        );
        assert_eq!(app.error, None, "keyboard list-for-target should succeed");

        // Keyboard path: Tab focus Hub-listed Global, Enter → SpawnSessionType target_id=T.
        let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &router.render_state());
        focus_hit_map_node_by_tab(&mut router, &hit_map, "tui-spawn-session-type-device/shell");
        activate_focused_action_with_enter(&mut app, &mut router, &hit_map);
        assert!(
            app.observed_requests.iter().any(|request| matches!(
                request,
                ObservedRequest::SpawnSessionType {
                    session_type_id,
                    target_id: Some(target_id),
                    ..
                } if session_type_id == "device/shell" && target_id == "repo-a"
            )),
            "keyboard spawn must carry target_id=T: {:?}",
            app.observed_requests
        );
    }

    fn focus_hit_map_node_by_tab(router: &mut InputRouter, hit_map: &HitMap, node_id: &str) {
        router.reconcile(hit_map);
        let attempts = hit_map.focusable_regions().count().saturating_add(2);
        for _ in 0..attempts {
            if router.focused_node_id() == Some(node_id) {
                return;
            }
            router.dispatch_event(
                Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                hit_map,
            );
        }
        panic!(
            "Tab traversal could not focus {node_id}; focused={:?}; focusable={:?}",
            router.focused_node_id(),
            hit_map
                .focusable_regions()
                .map(|region| region.node_id.as_str())
                .collect::<Vec<_>>()
        );
    }

    fn activate_focused_action_with_enter(
        app: &mut TuiApp,
        router: &mut InputRouter,
        hit_map: &HitMap,
    ) {
        let dispatch = router.dispatch_event(
            Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            hit_map,
        );
        match &dispatch {
            InputDispatch::Action(request) => {
                assert!(
                    request.action_id.0.starts_with("botster.tui.spawn"),
                    "expected product spawn action, got {:?}",
                    request.action_id
                );
            }
            other => panic!("Enter on focused spawn control must dispatch Action, got {other:?}"),
        }
        app.handle_dispatch(dispatch);
    }

    #[test]
    fn production_spawn_picker_source_does_not_filter_entities_by_target_id_equality() {
        let source = source_without_line_comments();
        // Build forbidden needles without embedding the production anti-patterns as
        // contiguous literals (this test body is itself scanned).
        let entity_eq_filter = format!("entity.target_id {} {}", "!=", "*target_id");
        let device_local_synth = format!("entity.target_id {} \"{}\"", "==", "device:local");
        assert!(
            !source.contains(&entity_eq_filter),
            "spawn picker must not re-filter entities by target_id equality"
        );
        assert!(
            !source.contains(&device_local_synth),
            "launch targets must not synthesize device:local from entities"
        );
        assert!(
            source.contains("ListSessionTypesForTarget"),
            "product path must call Hub ListSessionTypesForTarget"
        );
    }

    #[test]
    fn session_type_real_input_create_button_dispatches_through_input_router() {
        let mut app = TuiApp::new(None);
        app.system_details_visible = true;
        app.session_types_supported = true;
        let (lines, hit_map) = render_app_to_lines(&app, 220, 70, &RenderState::default());
        let _ = lines;
        let region = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-session-type-create")
            .expect("create button hit region");
        let mut router = InputRouter::new(renderer::action_request_context());
        let column = region.rect.x;
        let row = region.rect.y;
        let down = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        app.handle_dispatch(down);
        let up = router.dispatch_event(
            mouse_event(
                crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
                column,
                row,
            ),
            &hit_map,
        );
        app.handle_dispatch(up);
        assert!(app.session_type_form.is_some());
    }

    const MATRIX_OWNER: &str = "botster.plugin-contract-matrix";

    const MATRIX_EVENT: &str = "contract.ready";

    fn matrix_descriptor(ttl_ms: u32) -> PackageNoticeReactionDescriptor {
        PackageNoticeReactionDescriptor {
            owner: MATRIX_OWNER.to_string(),
            name: MATRIX_EVENT.to_string(),
            subject_scope: botster_ui_contract::PackageNoticeSubjectScope::Session,
            text_pointer: "/notice".to_string(),
            ttl_ms,
            severity: botster_ui_contract::PackageNoticeSeverity::Info,
        }
    }

    fn activate_notice(app: &mut TuiApp, subscription_id: &str, ttl_ms: u32, subject: &str) {
        let descriptor = matrix_descriptor(ttl_ms);
        let key = (descriptor.owner.clone(), descriptor.name.clone());
        app.notice_subscription_by_id
            .insert(subscription_id.to_string(), key.clone());
        app.notice_subscriptions.insert(
            key,
            NoticeSubscriptionEntry {
                descriptor,
                subject: subject.to_string(),
                state: EventSubscriptionState::Active(subscription_id.to_string()),
            },
        );
    }

    fn rendered_workspace(app: &TuiApp) -> String {
        render_app_to_lines(app, 140, 42, &RenderState::default())
            .0
            .join("\n")
    }

    #[test]
    fn tui_requires_package_event_subscriptions_at_floor_49() {
        let requirement = tui_compatibility_requirement();
        assert_eq!(requirement.minimum_conformance_fixture_revision, 49);
        assert!(
            requirement
                .required_features
                .iter()
                .any(|feature| feature == FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS)
        );
        let terminal = tui_terminal_compatibility_requirement();
        assert!(
            !terminal
                .required_features
                .iter()
                .any(|feature| feature == FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS)
        );
    }

    #[test]
    fn reconnect_clears_transient_notice_and_event_subscription_state() {
        let mut app = workspace_fixture();
        activate_notice(&mut app, "old-sub", 5_000, "session-alpha");
        app.transient_notice = Some(TransientNotice {
            text: "old".to_string(),
            deadline: Instant::now() + Duration::from_secs(5),
        });
        app.force_reconnect();
        assert!(app.notice_subscriptions.is_empty());
        assert!(app.notice_subscription_by_id.is_empty());
        assert!(app.transient_notice.is_none());
        assert!(!rendered_workspace(&app).contains("old"));
    }

    fn base_response(kind: DaemonResponseKind) -> DaemonResponse {
        DaemonResponse {
            kind,
            status: None,
            sessions: Vec::new(),
            session_types: Vec::new(),
            session_type_definition: None,
            resolved_session_type: None,
            hub_update: None,
            hub_update_execution: None,
            session_context: None,
            read_screen: None,
            mode_flags: None,
            terminal_reservation: None,
            subscription_reservation: None,
            capture_snapshot: None,
            snapshot_page: None,
            terminal_attach: None,
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
            plugin_resource_counters: None,
            error: None,
            diagnostics: Vec::new(),
        }
    }

    // ── Wake-driven contracts (v9 host control, scheme 2 terminal stream) ──

    fn routed(
        route: &str,
        generation: u64,
        frame: botster_terminal_protocol_client::TerminalFrame,
    ) -> AppWake {
        routed_at_epoch(route, generation, 0, frame)
    }

    fn routed_at_epoch(
        route: &str,
        generation: u64,
        stream_epoch: u32,
        frame: botster_terminal_protocol_client::TerminalFrame,
    ) -> AppWake {
        AppWake::Terminal(RoutedTerminalFrame {
            route: RouteId::new(route).expect("route id"),
            generation,
            stream_epoch,
            frame,
        })
    }

    fn resync_frame(
        from_epoch: u32,
        to_epoch: u32,
    ) -> botster_terminal_protocol_client::TerminalFrame {
        botster_terminal_protocol_client::encode_route_resync(from_epoch, to_epoch)
            .expect("resync frame")
    }

    /// Complete the Attach request for `route` with the trusted generation.
    fn complete_attach(app: &mut TuiApp, session_id: &str, route: &str, generation: u64) {
        let mut response = base_response(DaemonResponseKind::TerminalAttached);
        response.terminal_attach = Some(botster_hub_client::DaemonTerminalAttach::new(
            session_id, route, generation,
        ));
        app.apply_completion(
            PendingReply::Attach {
                session_id: session_id.to_string(),
                route: route.to_string(),
            },
            response,
        );
    }

    #[test]
    fn generation_comes_only_from_the_attach_response_and_parked_frames_replay() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        // Frames before the response never set the reservation; they wait.
        app.apply_wake(routed(
            "route-1",
            9,
            attach_state_frame(AttachStateCode::Attached),
        ));
        app.apply_wake(routed(
            "route-1",
            4,
            attach_state_frame(AttachStateCode::Attached),
        ));
        app.apply_wake(routed("route-1", 4, modes_frame(mode_bits::MOUSE_NORMAL)));
        assert_eq!(app.route_generation, None);
        assert_eq!(
            app.attach_hydration
                .as_ref()
                .map(|hydration| hydration.pending_frames.len()),
            Some(3)
        );
        complete_attach(&mut app, "session-alpha", "route-1", 4);
        assert_eq!(app.route_generation, Some(4));
        assert_eq!(app.route_epoch, Some(0));
        let hydration = app.attach_hydration.as_ref().expect("campaign continues");
        assert!(hydration.attached_seen);
        assert!(hydration.pending_frames.is_empty());
        assert_eq!(
            app.terminal_modes
                .as_ref()
                .map(|state| state.modes.mode_bits),
            Some(mode_bits::MOUSE_NORMAL)
        );
        // Another generation is dropped after the response, including a stale
        // ATTACH_STATE attached; another epoch on a data frame is dropped.
        app.terminal_modes = None;
        app.apply_wake(routed(
            "route-1",
            5,
            attach_state_frame(AttachStateCode::Attached),
        ));
        app.apply_wake(routed("route-1", 3, modes_frame(mode_bits::MOUSE_NORMAL)));
        app.apply_wake(routed_at_epoch(
            "route-1",
            4,
            1,
            modes_frame(mode_bits::MOUSE_NORMAL),
        ));
        assert_eq!(app.route_generation, Some(4));
        assert!(app.terminal_modes.is_none());
        // Frames for a foreign route never touch the campaign.
        app.apply_wake(routed(
            "route-9",
            4,
            attach_state_frame(AttachStateCode::Detached),
        ));
        assert!(app.attach_hydration.is_some());
    }

    #[test]
    fn frames_before_the_attach_response_share_the_pending_budget() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        for _ in 0..crate::hub_io::MAX_PENDING_WAKE_ITEMS {
            app.apply_wake(routed("route-1", 1, modes_frame(0)));
        }
        assert!(app.attach_hydration.is_some());
        assert!(!app.hub_io.try_retain(0));
        app.apply_wake(routed("route-1", 1, modes_frame(0)));
        assert!(app.retired_subscription_ids.contains("route-1"));
        assert!(app.attach_recovery_used);
        // The failed campaign released every parked frame from the budget.
        assert!(app.hub_io.try_retain(0));
        app.hub_io.release_retained(0);
    }

    #[test]
    fn route_resync_adopts_to_epoch_and_restarts_hydration() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        complete_attach(&mut app, "session-alpha", "route-1", 1);
        app.apply_wake(routed(
            "route-1",
            1,
            attach_state_frame(AttachStateCode::Attached),
        ));
        // Stale transition (wrong from_epoch), envelope epoch not to_epoch, and
        // a transition that does not change the epoch are all dropped.
        app.apply_wake(routed_at_epoch("route-1", 1, 2, resync_frame(1, 2)));
        app.apply_wake(routed_at_epoch("route-1", 1, 0, resync_frame(0, 1)));
        app.apply_wake(routed_at_epoch("route-1", 1, 0, resync_frame(0, 0)));
        assert_eq!(app.route_epoch, Some(0));
        app.apply_wake(routed_at_epoch("route-1", 1, 1, resync_frame(0, 1)));
        assert_eq!(app.route_generation, Some(1));
        assert_eq!(app.route_epoch, Some(1));
        let hydration = app
            .attach_hydration
            .as_ref()
            .expect("hydration restarts at SNAPSHOT_READY");
        assert!(hydration.attached_seen);
        assert!(!hydration.snapshot_ready);
        assert!(app.attached.is_none());
        assert!(app.ghostty_projection.is_none());
        // Data from the previous epoch is stale after the transition.
        app.apply_wake(routed_at_epoch(
            "route-1",
            1,
            0,
            modes_frame(mode_bits::MOUSE_NORMAL),
        ));
        assert!(app.terminal_modes.is_none());
        app.apply_wake(routed_at_epoch(
            "route-1",
            1,
            1,
            modes_frame(mode_bits::MOUSE_NORMAL),
        ));
        assert!(app.terminal_modes.is_some());
    }

    #[test]
    fn input_results_are_correlated_by_operation_on_every_epoch() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        complete_attach(&mut app, "session-alpha", "route-1", 1);
        app.apply_wake(routed(
            "route-1",
            1,
            attach_state_frame(AttachStateCode::Attached),
        ));
        app.attached = Some(AttachedRoute {
            session_id: "session-alpha".to_string(),
            route: "route-1".to_string(),
        });
        app.attach_hydration = None;
        app.terminal_modes = Some(TerminalModeState {
            route: "route-1".to_string(),
            modes: ModesBody::default(),
        });
        let operation_id = app.input_window.next_operation_id().expect("id");
        app.input_window
            .admit(operation_id, false, vec![vec![1]])
            .expect("admitted");
        app.apply_wake(routed_at_epoch("route-1", 1, 1, resync_frame(0, 1)));
        let result = InputResultBody {
            operation_id,
            outcome: InputOutcome::Written,
            accepted_payload_bytes: Some(1),
            written_pty_bytes: Some(1),
            mode_bits: mode_bits::KITTY_KEYBOARD,
            detail: String::new(),
        };
        app.apply_wake(routed_at_epoch(
            "route-1",
            1,
            0,
            botster_terminal_protocol_client::encode_input_result(&result).expect("result frame"),
        ));
        assert_eq!(app.input_window.in_flight_len(), 0);
        // Mode bits from a result never become current renderer state.
        assert_eq!(app.current_mode_bits(), 0);
    }

    #[test]
    fn closing_a_route_resolves_in_flight_operations_as_unknown() {
        let mut app = workspace_fixture();
        app.attached = Some(AttachedRoute {
            session_id: "session-alpha".to_string(),
            route: "route-1".to_string(),
        });
        let operation_id = app.input_window.next_operation_id().expect("id");
        app.input_window
            .admit(operation_id, false, vec![vec![1]])
            .expect("admitted");
        app.retire_subscription("route-1");
        assert_eq!(app.input_window.in_flight_len(), 0);
        assert!(
            app.action_feedback.as_deref().is_some_and(
                |feedback| feedback.contains("1 terminal input operation(s) unresolved")
            )
        );
    }

    fn attach_state_frame(
        state: AttachStateCode,
    ) -> botster_terminal_protocol_client::TerminalFrame {
        botster_terminal_protocol_client::encode_attach_state(state).expect("attach state frame")
    }

    fn modes_frame(mode_bits: u32) -> botster_terminal_protocol_client::TerminalFrame {
        botster_terminal_protocol_client::encode_modes(ModesBody {
            mode_bits,
            rows: 24,
            cols: 80,
        })
        .expect("modes frame")
    }

    #[test]
    fn attach_completion_adopts_generation_from_terminal_attach() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        let mut response = base_response(DaemonResponseKind::TerminalAttached);
        response.terminal_attach = Some(botster_hub_client::DaemonTerminalAttach::new(
            "session-alpha",
            "route-1",
            6,
        ));
        app.apply_completion(
            PendingReply::Attach {
                session_id: "session-alpha".to_string(),
                route: "route-1".to_string(),
            },
            response,
        );
        assert_eq!(app.route_generation, Some(6));
        assert!(app.attach_hydration.is_some());
        assert!(app.error.is_none());
    }

    #[test]
    fn attach_completion_without_terminal_attach_closes_the_campaign() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        app.apply_completion(
            PendingReply::Attach {
                session_id: "session-alpha".to_string(),
                route: "route-1".to_string(),
            },
            base_response(DaemonResponseKind::TerminalAttached),
        );
        assert!(app.attach_hydration.is_none());
        assert!(app.retired_subscription_ids.contains("route-1"));
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("omitted the terminal attachment"))
        );
    }

    #[test]
    fn attach_failed_before_ready_recovers_once_with_a_fresh_route() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        complete_attach(&mut app, "session-alpha", "route-1", 1);
        app.apply_wake(routed(
            "route-1",
            1,
            attach_state_frame(AttachStateCode::Attached),
        ));
        app.apply_wake(routed(
            "route-1",
            1,
            attach_state_frame(AttachStateCode::Failed),
        ));
        assert!(app.retired_subscription_ids.contains("route-1"));
        assert!(app.attach_recovery_used);
        let replacement = app
            .attach_hydration
            .as_ref()
            .map(|hydration| hydration.route.clone())
            .expect("one fresh attach campaign");
        assert_ne!(replacement, "route-1");
        complete_attach(&mut app, "session-alpha", &replacement, 2);
        app.apply_wake(routed(
            &replacement,
            2,
            attach_state_frame(AttachStateCode::Attached),
        ));
        app.apply_wake(routed(
            &replacement,
            2,
            attach_state_frame(AttachStateCode::Failed),
        ));
        assert!(app.attach_hydration.is_none());
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("failed closed after recovery"))
        );
    }

    #[test]
    fn history_unavailable_before_ready_is_a_phase_gap_and_after_ready_keeps_the_route() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        complete_attach(&mut app, "session-alpha", "route-1", 1);
        app.apply_wake(routed(
            "route-1",
            1,
            attach_state_frame(AttachStateCode::Attached),
        ));
        app.apply_wake(routed(
            "route-1",
            1,
            botster_terminal_protocol_client::encode_history_unavailable(
                HistoryUnavailableReason::CaptureFailed,
            )
            .expect("history unavailable frame"),
        ));
        assert!(app.retired_subscription_ids.contains("route-1"));
        assert!(app.attach_recovery_used);
        let replacement = app
            .attach_hydration
            .as_ref()
            .map(|hydration| hydration.route.clone())
            .expect("recovery campaign");
        complete_attach(&mut app, "session-alpha", &replacement, 2);
        app.apply_wake(routed(
            &replacement,
            2,
            attach_state_frame(AttachStateCode::Attached),
        ));
        if let Some(hydration) = app.attach_hydration.as_mut() {
            hydration.snapshot_ready = true;
        }
        app.apply_wake(routed(
            &replacement,
            2,
            botster_terminal_protocol_client::encode_history_unavailable(
                HistoryUnavailableReason::CaptureFailed,
            )
            .expect("history unavailable frame"),
        ));
        let hydration = app
            .attach_hydration
            .as_ref()
            .expect("post-READY history unavailable keeps the campaign");
        assert!(!hydration.snapshot_finished);
        assert!(
            app.action_feedback
                .as_deref()
                .is_some_and(|feedback| feedback.contains("history unavailable"))
        );
    }

    #[test]
    fn package_events_before_event_subscribed_are_parked_then_promoted() {
        let mut app = workspace_fixture();
        let descriptor = matrix_descriptor(5_000);
        let key = (descriptor.owner.clone(), descriptor.name.clone());
        app.subscribe_notice_entry(NoticeSubscriptionEntry {
            descriptor: descriptor.clone(),
            subject: "session-alpha".to_string(),
            state: EventSubscriptionState::Idle,
        });
        let subscription_id = app.notice_subscriptions[&key]
            .state
            .candidate_id()
            .expect("candidate")
            .to_string();
        app.handle_package_event(
            subscription_id.clone(),
            descriptor.owner.clone(),
            descriptor.name.clone(),
            json!({ "notice": "early" }),
        );
        assert!(app.transient_notice.is_none());
        assert_eq!(app.notice_parked[&subscription_id].events.len(), 1);
        app.complete_notice_subscription(
            &key,
            &subscription_id,
            base_response(DaemonResponseKind::EventSubscribed),
        );
        assert_eq!(
            app.transient_notice
                .as_ref()
                .map(|notice| notice.text.as_str()),
            Some("early")
        );
        assert!(app.notice_parked.is_empty());
        assert_eq!(
            app.notice_subscriptions[&key].state.active_id(),
            Some(subscription_id.as_str())
        );
    }

    #[test]
    fn input_result_for_unknown_operation_is_reported_without_touching_modes() {
        let mut app = workspace_fixture();
        app.attached = Some(AttachedRoute {
            session_id: "session-alpha".to_string(),
            route: "route-1".to_string(),
        });
        app.route_generation = Some(1);
        app.terminal_modes = Some(TerminalModeState {
            route: "route-1".to_string(),
            modes: ModesBody::default(),
        });
        let result = InputResultBody {
            operation_id: 9,
            outcome: InputOutcome::RejectedLaneFull,
            accepted_payload_bytes: None,
            written_pty_bytes: None,
            mode_bits: mode_bits::KITTY_KEYBOARD,
            detail: "lane".to_string(),
        };
        app.apply_wake(routed(
            "route-1",
            1,
            botster_terminal_protocol_client::encode_input_result(&result).expect("result frame"),
        ));
        assert_eq!(app.current_mode_bits(), 0);
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("input lane full"))
        );
    }

    #[test]
    fn request_failures_route_through_the_pending_reply() {
        let mut app = workspace_fixture();
        app.submit_apply(DaemonRequest::Status);
        assert_eq!(app.pending_requests.len(), 1);
        let wake = app
            .try_next_wake()
            .expect("completion queued without a link");
        app.apply_wake(wake);
        assert!(app.pending_requests.is_empty());
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("request failed"))
        );
    }

    #[test]
    fn disconnect_wake_schedules_a_reconnect_and_resets_route_state() {
        let mut app = workspace_fixture();
        app.endpoint = Some(DaemonEndpoint::new("/tmp/botster-tui-test-none.sock"));
        let generation = app.hub_io.generation();
        app.connected_generation = Some(generation);
        app.attached = Some(AttachedRoute {
            session_id: "session-alpha".to_string(),
            route: "route-1".to_string(),
        });
        app.apply_wake(AppWake::Disconnected {
            generation,
            error: DaemonTransportError::ClientDisconnected,
        });
        assert!(app.attached.is_none());
        assert!(app.connected_generation.is_none());
        assert_eq!(app.reconnect_failures, 1);
        assert!(app.reconnect_at.is_some());
        assert!(app.next_deadline().is_some());
        assert!(app.status.contains("reconnecting"));
    }

    #[test]
    fn pending_input_is_bounded_while_attaching() {
        let mut app = workspace_fixture();
        app.begin_attach_hydration("session-alpha", "route-1");
        app.queue_pending_input(PendingTerminalInput::Paste(vec![
            0;
            MAX_PENDING_HYDRATION_INPUT_BYTES
        ]));
        assert!(app.error.is_none());
        app.queue_pending_input(PendingTerminalInput::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        )));
        assert!(
            app.error
                .as_deref()
                .is_some_and(|error| error.contains("attach bound"))
        );
        assert_eq!(
            app.attach_hydration
                .as_ref()
                .map(|hydration| hydration.pending_input.len()),
            Some(1)
        );
    }
}
