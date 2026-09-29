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
    DaemonEntityFrame, DaemonEvent, DaemonHelloAck, DaemonObservabilityCounters, DaemonPackage,
    DaemonPackageAvailabilityReason, DaemonPackageAvailabilityState, DaemonPackageInstallPlan,
    DaemonPackageNavigationEntry, DaemonPackagePin, DaemonPackageRouteDescriptor,
    DaemonPackageUpdateStatus, DaemonPluginLogs, DaemonPluginSurface, DaemonQuarantine,
    DaemonQuarantineTarget, DaemonRequest, DaemonRequestError, DaemonResponse, DaemonResponseKind,
    DaemonSessionEntity, DaemonSessionType, DaemonSessionTypeDefinition,
    DaemonSessionTypeEditableDefinition, DaemonSessionTypeExecution,
    DaemonSessionTypeMutationSource, DaemonSessionTypeRequest, DaemonSessionTypeWorkingDirectory,
    DaemonSoftwareIdentity, DaemonSpawnTarget, DaemonTransportError, DaemonTransportResult,
    FEATURE_PACKAGE_EVENT_SUBSCRIPTIONS, FEATURE_PACKAGE_NAVIGATION, FEATURE_PLUGIN_SURFACE_ACTION,
    FEATURE_PLUGIN_SURFACE_RENDER, FEATURE_SESSION_ENTITY_SUBSCRIPTIONS,
    FEATURE_SESSION_TYPE_ENTITY_SUBSCRIPTIONS, FEATURE_SESSIONS, FEATURE_TERMINAL_READBACK,
    FEATURE_TERMINAL_SUBSCRIPTION_CLOSED, FEATURE_UNIX_TERMINAL_ADAPTER, PROTOCOL,
    TERMINAL_SUBSCRIPTION_CLOSED_WORKER_LOST, TerminalCompatibilityRequirement,
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
    AcceptanceMode, CLAIM_SCHEMA, ClaimConfig, EvidenceWriter, FailureContext, SCHEMA,
    ScenarioCase, SpawnConfig, verify_claim_pins,
};
use crate::projection_paint::tui_terminal_region;
use crate::renderer::{self, HitMap, InputDispatch, InputRouter, RenderState};

mod acceptance_drive;
use acceptance_drive::*;
mod package_config;
use package_config::*;
mod ui_nodes;
use ui_nodes::*;
mod hub_text;
use hub_text::*;
mod package_ui;
use package_ui::*;
mod plugin_surface;
use plugin_surface::*;

const PACKAGE_CONFIG_FIELD_PREFIX: &str = "package-config";
const SMOKE_MESSAGE: &str = "botster-tui smoke ok";
const MINIMUM_CONFORMANCE_FIXTURE_REVISION: u16 = 50;
/// Absolute deadline for an ordinary host-control request.
const REQUEST_DEADLINE: Duration = Duration::from_secs(10);
/// Absolute deadline for Detach and for connection teardown.
const DETACH_ON_DISCONNECT_BOUND: Duration = Duration::from_secs(2);
/// Bound for stopping the I/O owner at exit.
const SHUTDOWN_BOUND: Duration = Duration::from_secs(2);
const RECONNECT_BACKOFF_INITIAL: Duration = Duration::from_millis(750);
const RECONNECT_BACKOFF_CAP: Duration = Duration::from_secs(8);
const UNSAFE_PASTE_CONSENT_TIMEOUT: Duration = Duration::from_secs(30);
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
}

/// What the command line asks the binary to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParsedCommand {
    Run(AppArgs),
    Help,
    Version,
}

pub const fn usage() -> &'static str {
    "usage: botster-tui [--smoke]\n\n\
     The Hub host supplies the connection in BOTSTER_HUB_CONNECTION.\n\n\
     Options:\n\
     \x20 --smoke        Print a startup smoke message and exit\n\
     \x20 -h, --help     Show this help\n\
     \x20 -V, --version  Show the version"
}

impl AppArgs {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<ParsedCommand, String> {
        // Parent claim-stack prose uses BOTSTER_LIVE_DATA_DIR; prefer the established
        // BOTSTER_HUB_DATA_DIR injector when both are present.
        let hub_data_dir = std::env::var_os("BOTSTER_HUB_DATA_DIR")
            .or_else(|| std::env::var_os(crate::acceptance::LIVE_DATA_DIR_ENV));
        Self::parse_with_environment(
            args,
            std::env::var_os("BOTSTER_HUB_CONNECTION"),
            hub_data_dir,
        )
    }

    fn parse_with_environment(
        args: impl IntoIterator<Item = String>,
        hub_connection: Option<std::ffi::OsString>,
        hub_data_dir: Option<std::ffi::OsString>,
    ) -> Result<ParsedCommand, String> {
        let mut parsed = Self::default();
        for arg in args {
            match arg.as_str() {
                "--smoke" => parsed.smoke = true,
                "-h" | "--help" => return Ok(ParsedCommand::Help),
                "-V" | "--version" => return Ok(ParsedCommand::Version),
                _ => return Err(format!("unknown option: {arg}")),
            }
        }
        let (connection, connection_error) = parse_hub_connection(hub_connection);
        parsed.hub_connection = connection;
        parsed.connection_error = connection_error;
        parsed.hub_data_dir = hub_data_dir.map(PathBuf::from);
        Ok(ParsedCommand::Run(parsed))
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
    /// True when this hydration restarts a route that was already live
    /// (ROUTE_RESYNC): the attachment and its input window survive.
    resync: bool,
    /// Set when this hydration re-attaches after a route failure, with the
    /// failure's short cause. Only its completion restores the campaign's one
    /// recovery and counts toward the recovery notice.
    recovery_cause: Option<String>,
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
            resync: false,
            recovery_cause: None,
        }
    }
}

/// How often the current attachment recovered automatically, and why last.
#[derive(Clone, Debug, PartialEq, Eq)]
struct RecoveryNotice {
    count: u32,
    cause: String,
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
/// Ctrl+P focus target: the always-present primary toolbar action.
const WORKSPACE_MENU_NODE: &str = "tui-spawn";

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UnsafePasteConsentStage {
    Review,
    Armed,
}

#[derive(Debug)]
enum PendingUnsafePaste {
    AwaitingResult {
        operation_id: u64,
        route: String,
        generation: u64,
        payload: Vec<u8>,
    },
    AwaitingConsent {
        route: String,
        generation: u64,
        payload: Vec<u8>,
        deadline: Instant,
        stage: UnsafePasteConsentStage,
    },
}

impl PendingUnsafePaste {
    fn payload_len(&self) -> usize {
        match self {
            Self::AwaitingResult { payload, .. } | Self::AwaitingConsent { payload, .. } => {
                payload.len()
            }
        }
    }
}

/// The Hub's answer to one Detach, as far as this connection knows.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DetachState {
    /// Sent; no correlated response yet.
    Pending,
    /// A correlated Events response with no operator error arrived.
    Confirmed,
    /// An operator error, an unexpected response, or request failure/expiry.
    Failed(String),
}

/// What the application does with one host-control completion.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingReply {
    /// Apply the response to read models and diagnostics.
    Apply,
    /// ResolveQuarantine. Only a package resolution replies with the package
    /// list; a repository root's reply carries none.
    ResolveQuarantine { target: DaemonQuarantineTarget },
    /// The connection's spawn-target list; a failure is kept for the dialog.
    SpawnTargets,
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
    /// Detach for a retired route. Its correlated response is the only proof
    /// that the Hub released the route.
    Detach { session_id: String, route: String },
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

    /// The session's worker died without an exit report: a crash, not an
    /// exit.
    fn crashed(&self) -> bool {
        self.lifecycle == "failed"
            && self.failure_reason.as_deref() == Some(TERMINAL_SUBSCRIPTION_CLOSED_WORKER_LOST)
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

/// Outcome of one `session_type` entity frame for the current generation.
#[derive(Debug, PartialEq, Eq)]
enum SessionTypeFrameOutcome {
    /// The frame belongs to another generation or is stale.
    Ignored,
    /// The entity set changed. `replaced` is true for a full Snapshot.
    Applied { replaced: bool },
    /// Hub reported a catalog error. The subscription stays open, and the
    /// next Snapshot replaces the whole entity set.
    HubError(String),
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

    fn apply(&mut self, frame: DaemonEntityFrame) -> Result<SessionTypeFrameOutcome, String> {
        match frame {
            DaemonEntityFrame::Snapshot {
                subscription_id,
                entity_type,
                snapshot_seq,
                items,
                ..
            } => {
                if !self.matches(&subscription_id, &entity_type) {
                    return Ok(SessionTypeFrameOutcome::Ignored);
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
                Ok(SessionTypeFrameOutcome::Applied { replaced: true })
            }
            DaemonEntityFrame::Upsert {
                subscription_id,
                entity_type,
                snapshot_seq,
                id,
                entity,
            } => {
                if !self.accepts_delta(&subscription_id, &entity_type, snapshot_seq) {
                    return Ok(SessionTypeFrameOutcome::Ignored);
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
                Ok(SessionTypeFrameOutcome::Applied { replaced: false })
            }
            DaemonEntityFrame::Patch {
                subscription_id,
                entity_type,
                ..
            } => {
                if !self.matches(&subscription_id, &entity_type) {
                    return Ok(SessionTypeFrameOutcome::Ignored);
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
                    return Ok(SessionTypeFrameOutcome::Ignored);
                }
                self.entities.remove(&id);
                self.entity_order.retain(|entity_id| entity_id != &id);
                self.snapshot_seq = Some(snapshot_seq);
                Ok(SessionTypeFrameOutcome::Applied { replaced: false })
            }
            DaemonEntityFrame::Error {
                subscription_id,
                entity_type,
                code,
                message,
            } => {
                if !self.matches(&subscription_id, &entity_type) {
                    return Ok(SessionTypeFrameOutcome::Ignored);
                }
                // Non-terminal: refuse deltas until the replacement Snapshot.
                self.has_snapshot = false;
                Ok(SessionTypeFrameOutcome::HubError(format!(
                    "code={code} message={message}"
                )))
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
        app.sync_terminal_pane_size(&hit_map);
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
    if !app.shutdown() {
        return Err(io::Error::other(
            "the Hub link or input thread did not stop within the shutdown bound",
        ));
    }
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

/// Chords the TUI keeps for itself. They never reach a session, whatever has focus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostKey {
    /// Ctrl+P: move focus out of the terminal to the workspace toolbar.
    Menu,
    /// Ctrl+J: select the next session row.
    NextSession,
    /// Ctrl+K: select the previous session row.
    PreviousSession,
    /// Shift+PageUp / PageDown / Home / End: scroll the terminal projection.
    Scroll(HostScroll),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostScroll {
    PageUp,
    PageDown,
    Top,
    Bottom,
}

/// Matches every key kind, so a reserved chord's release never reaches a
/// session either. Release reporting needs keyboard enhancement flags, which
/// the TUI does not enable today.
fn host_key(key: KeyEvent) -> Option<HostKey> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(HostKey::Menu),
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => Some(HostKey::NextSession),
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Some(HostKey::PreviousSession),
        (KeyCode::PageUp, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::PageUp)),
        (KeyCode::PageDown, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::PageDown)),
        (KeyCode::Home, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::Top)),
        (KeyCode::End, KeyModifiers::SHIFT) => Some(HostKey::Scroll(HostScroll::Bottom)),
        _ => None,
    }
}

fn apply_host_key(app: &mut TuiApp, router: &mut InputRouter, hit_map: &HitMap, host_key: HostKey) {
    match host_key {
        HostKey::Menu => {
            let focused = matches!(
                router.focus_node(WORKSPACE_MENU_NODE, hit_map),
                InputDispatch::Focus { .. }
            );
            // The toolbar can overflow at narrow widths; still leave the terminal.
            if !focused && is_terminal_node(router.focused_node_id()) {
                router.focus_next(hit_map);
            }
        }
        HostKey::NextSession | HostKey::PreviousSession => {
            let count = app.sessions.len();
            if count == 0 {
                return;
            }
            let current = app.selected_session.as_deref().and_then(|id| {
                app.sessions
                    .iter()
                    .position(|session| session.session_id == id)
            });
            let next = match (host_key, current) {
                (HostKey::NextSession, Some(index)) => (index + 1) % count,
                (HostKey::NextSession, None) => 0,
                (_, Some(index)) => (index + count - 1) % count,
                (_, None) => count - 1,
            };
            let session_id = app.sessions[next].session_id.clone();
            // Focus the row so Enter attaches it. A modal hides the list, and
            // then the chord changes nothing.
            if matches!(
                router.focus_node(&format!("tui-session-{session_id}"), hit_map),
                InputDispatch::Focus { .. }
            ) {
                app.set_selected_session(Some(session_id));
            }
        }
        HostKey::Scroll(scroll) => {
            let page = i32::from(app.terminal_viewport_size.rows);
            app.scroll_projection(match scroll {
                HostScroll::PageUp => ScrollOp::Delta(-page),
                HostScroll::PageDown => ScrollOp::Delta(page),
                HostScroll::Top => ScrollOp::Top,
                HostScroll::Bottom => ScrollOp::Bottom,
            });
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
        Event::Key(key) if let Some(host_key) = host_key(key) => {
            if key.kind != KeyEventKind::Release {
                apply_host_key(app, router, hit_map, host_key);
            }
        }
        Event::Key(key) if key.kind == KeyEventKind::Press && app.handle_tui_owned_key(key) => {}
        Event::Key(key) if app.handle_focused_terminal_key(key, router.focused_node_id()) => {}
        Event::Paste(ref text)
            if app.handle_focused_terminal_paste(text, router.focused_node_id()) => {}
        Event::Mouse(mouse)
            if app.handle_focused_terminal_mouse(mouse, router.focused_node_id(), hit_map) => {}
        // The next draw measures the new pane; `sync_terminal_pane_size`
        // sends it. The router would report the previous frame's pane.
        Event::Resize(..) => {}
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
    let recovery = app.recovery_notice_line();
    let quarantine = app.quarantine_band();
    let toolbar = app.workspace_toolbar();
    let navigator = app.session_navigator();
    let focused_session = app.focused_session_panel();
    for node in [
        Some(&status),
        alert.as_ref(),
        notice.as_ref(),
        recovery.as_ref(),
        quarantine.as_ref(),
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
    for band in [
        alert.as_ref(),
        notice.as_ref(),
        recovery.as_ref(),
        quarantine.as_ref(),
    ]
    .into_iter()
    .flatten()
    {
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
    /// Hub quarantines awaiting operator resolution, from the latest Status.
    quarantines: Vec<DaemonQuarantine>,
    /// Hub observability counters from the latest Status.
    hub_counters: DaemonObservabilityCounters,
    /// The latest plugin log page read per package (protocol 12).
    plugin_logs: BTreeMap<String, DaemonPluginLogs>,
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
    spawn_targets: Vec<DaemonSpawnTarget>,
    /// Whether this connection's ListSpawnTargets reply has arrived; before
    /// it, an empty list means "not loaded yet", not "no targets".
    spawn_targets_loaded: bool,
    /// Why this connection's ListSpawnTargets request failed, if it did.
    spawn_targets_failure: Option<String>,
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
    /// Completed automatic recoveries of the current attachment, shown as an
    /// informational line until the next user attach or detach.
    recovery_notice: Option<RecoveryNotice>,
    /// Retired route ids whose late frames and close events must be ignored.
    retired_subscription_ids: BTreeSet<String>,
    /// Close-event evidence from `TerminalSubscriptionClosed` (generation, reason).
    terminal_close_evidence: Option<(u64, String)>,
    /// The newest Detach per session on this connection: its route and
    /// whether the Hub confirmed it. The pane title shows the outcome.
    detaches: BTreeMap<String, (String, DetachState)>,
    /// Last MODES frame for the current route.
    terminal_modes: Option<TerminalModeState>,
    /// Client-side input window for the current route generation.
    input_window: InputWindow,
    /// One raw paste retained only until its result or explicit consent ends.
    pending_unsafe_paste: Option<PendingUnsafePaste>,
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
    #[cfg(test)]
    fn new(endpoint: Option<DaemonEndpoint>) -> Self {
        Self::new_with_connection(endpoint, None)
    }

    #[cfg(test)]
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
            quarantines: Vec::new(),
            hub_counters: DaemonObservabilityCounters::default(),
            plugin_logs: BTreeMap::new(),
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
            spawn_targets: Vec::new(),
            spawn_targets_loaded: false,
            spawn_targets_failure: None,
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
            recovery_notice: None,
            retired_subscription_ids: BTreeSet::new(),
            terminal_close_evidence: None,
            detaches: BTreeMap::new(),
            terminal_modes: None,
            input_window: InputWindow::new(),
            pending_unsafe_paste: None,
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
            self.pending_unsafe_paste
                .as_ref()
                .and_then(|pending| match pending {
                    PendingUnsafePaste::AwaitingConsent { deadline, .. } => Some(*deadline),
                    PendingUnsafePaste::AwaitingResult { .. } => None,
                }),
        );
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
            AppWake::Completed { request_id, result } => {
                self.complete_request(request_id, result.map(|response| *response))
            }
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
        self.expire_unsafe_paste_consent(now);
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
        self.expire_unsafe_paste_consent(Instant::now());
        if self.projection_dirty {
            self.refresh_ghostty_viewport_cache();
        }
    }

    /// Stop the I/O owner within the shutdown bound. Returns whether every
    /// I/O thread confirmed its stop.
    fn shutdown(self) -> bool {
        let mut app = self;
        app.detach_owner_if_writable();
        app.hub_io.shutdown(SHUTDOWN_BOUND)
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
            | InputDispatch::HostKey(_)
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
        // Every new paste event invalidates a prior retry, even when a modal
        // currently owns focus. The current event is never replayed.
        self.invalidate_unsafe_paste();
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

    /// Keys the TUI handles before terminal forwarding: Esc for dialogs and
    /// plugin content. PageUp/PageDown and Ctrl+Home/End reach a focused
    /// session; the Shift variants scroll as reserved host keys.
    fn handle_tui_owned_key(&mut self, key: KeyEvent) -> bool {
        if key.code != KeyCode::Esc || key.modifiers != KeyModifiers::NONE {
            return false;
        }
        if matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { .. })
        ) {
            self.invalidate_unsafe_paste();
            return true;
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
                    // The accepted owner replacement is canonical, including
                    // confirmation trees that drop entity-option producers.
                    surface.ui_tree_snapshot.body = replacement;
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
            "botster.tui.unsafe_paste.review" => {
                if let Some(PendingUnsafePaste::AwaitingConsent { stage, .. }) =
                    self.pending_unsafe_paste.as_mut()
                {
                    *stage = UnsafePasteConsentStage::Armed;
                }
            }
            "botster.tui.unsafe_paste.cancel" => self.invalidate_unsafe_paste(),
            "botster.tui.unsafe_paste.confirm" => self.confirm_unsafe_paste(),
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
            "botster.tui.quarantine.resolve" => {
                match payload.map(serde_json::from_value::<DaemonQuarantineTarget>) {
                    Some(Ok(target)) => {
                        self.action_feedback = Some(format!(
                            "resolve requested: {}",
                            quarantine_target_text(&target)
                        ));
                        self.submit(
                            DaemonRequest::ResolveQuarantine {
                                target: target.clone(),
                            },
                            PendingReply::ResolveQuarantine { target },
                            REQUEST_DEADLINE,
                        );
                    }
                    _ => self.error = Some("resolve: invalid quarantine target".to_string()),
                }
            }
            "botster.tui.package.logs" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("logs requested: {package_name}"));
                    self.submit_apply(DaemonRequest::ReadPluginLogs {
                        package_name,
                        after_seq: 0,
                    });
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
        // timer: backoff — failed Hub connect or lost link; capped exponential reconnect delay
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
        if !self.hub_io.disconnect(DETACH_ON_DISCONNECT_BOUND) {
            self.error =
                Some("the previous Hub link did not close within the detach bound".to_string());
        }
        self.connected_generation = None;
        self.pending_requests.clear();
        self.reset_active_plugin_surface();
        self.invalidate_session_generation();
        self.invalidate_session_type_generation();
        self.drop_entity_options_subscriptions();
        self.clear_event_subscription_state();
        self.clear_route_state();
        self.attach_recovery_used = false;
        self.recovery_notice = None;
        // Quarantines and counters describe the Hub of the dropped
        // connection; the next Status fills them again.
        self.quarantines.clear();
        self.hub_counters = DaemonObservabilityCounters::default();
        self.plugin_logs.clear();
        self.terminal_close_evidence = None;
        // Detach answers and spawn targets are per connection.
        self.detaches.clear();
        self.spawn_targets.clear();
        self.spawn_targets_loaded = false;
        self.spawn_targets_failure = None;
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
        self.invalidate_unsafe_paste();
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
            self.hub_io.disconnect_now();
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
        self.start_session_type_subscription();
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
        // timer: deadline — host-control request expiry; expiry completes the request with DeadlineExpired
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
            PendingReply::Apply | PendingReply::Unsubscribe => {
                self.apply_response(response);
            }
            PendingReply::ResolveQuarantine { target } => {
                let resolved = response.error.is_none()
                    && matches!(response.kind, DaemonResponseKind::QuarantineResolved);
                if resolved && matches!(target, DaemonQuarantineTarget::Package { .. }) {
                    self.packages = response.packages.clone();
                    self.sync_notice_subscriptions();
                }
                self.apply_response(response);
            }
            PendingReply::Detach { session_id, route } => {
                // Success is a correlated Events response with no operator
                // error; anything else is a failed detach, never a release.
                let state = match &response.error {
                    None if response.kind == DaemonResponseKind::Events => DetachState::Confirmed,
                    Some(error) => {
                        DetachState::Failed(format!("{} (code={})", error.message, error.code))
                    }
                    None => DetachState::Failed(format!("unexpected {:?} response", response.kind)),
                };
                self.finish_detach(&session_id, &route, state);
                self.apply_response(response);
            }
            PendingReply::SpawnTargets => {
                self.spawn_targets_failure = response
                    .error
                    .as_ref()
                    .map(|error| format!("{} (code={})", error.message, error.code));
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
            PendingReply::Apply | PendingReply::ResolveQuarantine { .. } => {
                self.error = Some(format!("request failed: {message}"));
            }
            PendingReply::SpawnTargets => self.spawn_targets_failure = Some(message),
            PendingReply::Unsubscribe => {}
            PendingReply::Detach { session_id, route } => {
                self.finish_detach(&session_id, &route, DetachState::Failed(message));
            }
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
        self.submit(
            DaemonRequest::ListSpawnTargets,
            PendingReply::SpawnTargets,
            REQUEST_DEADLINE,
        );
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
                Ok(SessionTypeFrameOutcome::Applied { replaced }) => {
                    if replaced {
                        self.session_type_subscription_error = None;
                    }
                    if self
                        .selected_session_type_id
                        .as_ref()
                        .is_some_and(|id| !self.session_type_entities.entities.contains_key(id))
                    {
                        self.selected_session_type_id = None;
                    }
                }
                Ok(SessionTypeFrameOutcome::Ignored) => {}
                Ok(SessionTypeFrameOutcome::HubError(error)) => {
                    self.session_type_subscription_error = Some(error);
                }
                Err(error) => {
                    self.session_type_subscription_error = Some(error.clone());
                    self.error = Some(format!("session type sync: {error}"));
                    self.invalidate_session_type_generation();
                    self.session_type_subscription_error = Some(error);
                    if self.is_connected() {
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
                // timer: backoff — entity-options subscribe admission failure; capped exponential retry delay
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
        if let Some(failure) = &self.spawn_targets_failure {
            self.error = Some(format!("launch targets failed to load: {failure}"));
            return;
        }
        // Before the target list loads, the dialog opens and fills in when the
        // ListSpawnTargets reply arrives.
        if self.spawn_targets_loaded && self.launch_target_options().is_empty() {
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
                demanded_entity_option_families(&surface.ui_tree_snapshot.body)
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
        collect_invalid_entity_option_fields(
            &surface.ui_tree_snapshot.body,
            &store,
            &self.drafts,
            &mut invalid,
        );
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
        self.recovery_notice = None;
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
        self.detaches
            .insert(session_id.clone(), (route.clone(), DetachState::Pending));
        self.submit(
            DaemonRequest::Detach {
                session_id: session_id.clone(),
                subscription_id: route.clone(),
            },
            PendingReply::Detach { session_id, route },
            DETACH_ON_DISCONNECT_BOUND,
        );
    }

    /// Record the Hub's answer to one Detach. A stale answer for a route the
    /// session no longer owns changes nothing.
    fn finish_detach(&mut self, session_id: &str, route: &str, state: DetachState) {
        let Some((current, slot)) = self.detaches.get_mut(session_id) else {
            return;
        };
        if current != route {
            return;
        }
        if let DetachState::Failed(reason) = &state {
            self.error = Some(format!("detach of {session_id} failed: {reason}"));
        }
        *slot = state;
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
        self.recovery_notice = None;
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
                    // timer: ui-lifetime — transient notice, server-supplied ttl_ms; one wake at expiry via next_deadline
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

    /// Informational, not an error: the terminal re-attached on its own.
    fn recovery_notice_line(&self) -> Option<UiNode> {
        let notice = self.recovery_notice.as_ref()?;
        Some(node(
            UiNodeKind::Text,
            "workspace-recovery-notice",
            json!({ "text": format!("reconnected {}× after {}", notice.count, notice.cause) }),
        ))
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
    /// reset, never continued. The attachment itself survives: Core keeps the
    /// route attached across a resync and Hub never gates input, so the
    /// hydration is marked `resync` and the input window keeps its slots.
    /// Input captured so far stays queued and is never replayed.
    fn begin_route_resync(&mut self) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            return;
        };
        self.invalidate_unsafe_paste();
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_ghostty_projection();
        let was_attached = self.attached.take().is_some();
        let _ = self.take_parked_frames();
        let mut hydration = AttachHydration::new(&session_id, &route);
        if let Some(previous) = self.attach_hydration.take() {
            hydration.attached_seen = previous.attached_seen;
            hydration.resync = previous.resync;
            hydration.pending_input = previous.pending_input;
            hydration.pending_input_bytes = previous.pending_input_bytes;
            hydration.pending_resize = previous.pending_resize;
            // A resync during a recovery's hydration is still that recovery.
            hydration.recovery_cause = previous.recovery_cause;
        }
        hydration.attached_seen |= was_attached;
        hydration.resync |= was_attached;
        self.attach_hydration = Some(hydration);
        self.action_feedback = Some(format!("terminal route resync: {session_id}"));
    }

    /// Whether `route` names the current valid attachment for input purposes:
    /// the live attached route, or a resync hydration of a route that was
    /// live, with the trusted attachment generation known. Initial hydration
    /// before the Attach response and retired or replaced routes never match.
    fn attachment_matches_route(&self, route: &str) -> bool {
        if self.route_generation.is_none() {
            return false;
        }
        self.attached_matches_route(route)
            || self
                .attach_hydration
                .as_ref()
                .is_some_and(|hydration| hydration.resync && hydration.route == route)
    }

    fn send_encoded_frames(&mut self, frames: Vec<Vec<u8>>) {
        if frames.is_empty() {
            return;
        }
        let Some((_, route)) = self.current_owner_pair() else {
            self.error = Some("terminal stream unavailable: no attached route".to_string());
            return;
        };
        if !self.attachment_matches_route(&route) {
            self.error = Some("terminal stream unavailable: no attached route".to_string());
            return;
        }
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

    /// INPUT_RESULT is correlated by operation id within the current
    /// attachment, including a resync hydration of that attachment. Results
    /// for a lost or replaced attachment never reach the window.
    fn apply_terminal_input_result(&mut self, route: &str, result: InputResultBody) {
        if !self.attachment_matches_route(route) {
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
        let retry_matches = matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingResult {
                operation_id,
                route: pending_route,
                generation,
                ..
            }) if *operation_id == result.operation_id
                && pending_route == route
                && self.route_generation == Some(*generation)
        );
        if retry_matches {
            let may_retry = completed.as_ref().is_some_and(|operation| operation.paste)
                && result.outcome == InputOutcome::RejectedUnsafePaste
                && result.accepted_payload_bytes == Some(0)
                && result.written_pty_bytes == Some(0);
            if may_retry {
                let Some(PendingUnsafePaste::AwaitingResult {
                    route,
                    generation,
                    payload,
                    ..
                }) = self.pending_unsafe_paste.take()
                else {
                    unreachable!("matching unsafe paste state changed")
                };
                self.pending_unsafe_paste = Some(PendingUnsafePaste::AwaitingConsent {
                    route,
                    generation,
                    payload,
                    // timer: deadline — unsafe-paste consent expires; expiry cancels the retry
                    deadline: Instant::now() + UNSAFE_PASTE_CONSENT_TIMEOUT,
                    stage: UnsafePasteConsentStage::Review,
                });
            } else {
                self.invalidate_unsafe_paste();
            }
        }
        match result.outcome {
            InputOutcome::Written => {
                if completed.is_some() {
                    self.error = None;
                }
            }
            _ => self.error = Some(input_outcome_message(&result)),
        }
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
        if reason == TERMINAL_SUBSCRIPTION_CLOSED_WORKER_LOST {
            // The worker is gone: a re-attach can only fail. End the route
            // and report the crash instead of recovering.
            self.end_route_after_worker_lost(&session_id, &subscription_id);
            return;
        }
        self.recover_current_subscription(
            &session_id,
            &subscription_id,
            &format!("terminal subscription closed ({reason})"),
            &reason,
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
        self.recover_current_subscription(&session_id, &route, reason, reason);
    }

    /// The session's worker died: retire the route with a bounded Detach and
    /// leave the session to its failed entity row.
    fn end_route_after_worker_lost(&mut self, session_id: &str, route: &str) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_route_state();
        self.retire_subscription(route);
        if self.is_connected() {
            self.send_bounded_detach(session_id.to_string(), route.to_string());
        }
        self.recovery_notice = None;
        self.error = Some(format!(
            "session {session_id} crashed: its worker was lost (worker_lost)"
        ));
    }

    fn recover_from_decode_or_phase_gap(&mut self, reason: &str) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            self.error = Some(reason.to_string());
            return;
        };
        self.recover_current_subscription(&session_id, &route, reason, reason);
    }

    /// Retire the current route with a bounded Detach and re-attach with a
    /// fresh route. A campaign has one recovery at a time: a failure during
    /// the recovery's own hydration fails closed, and a completed recovery
    /// restores it, so each later independent failure recovers once.
    ///
    /// `reason` is the error line while the recovery runs; `cause` is the
    /// short cause the recovery notice names once it completes.
    fn recover_current_subscription(
        &mut self,
        session_id: &str,
        route: &str,
        reason: &str,
        cause: &str,
    ) {
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
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.recovery_cause = Some(cause.to_string());
        }
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
        if let Some(cause) = hydration.recovery_cause.clone() {
            self.attach_recovery_used = false;
            let count = self
                .recovery_notice
                .as_ref()
                .map_or(0, |notice| notice.count)
                + 1;
            self.recovery_notice = Some(RecoveryNotice { count, cause });
        }
        if !hydration.buffered_live_output.is_empty() {
            self.apply_live_terminal_output(&hydration.buffered_live_output);
        }
        let size = hydration
            .pending_resize
            .unwrap_or(self.terminal_viewport_size);
        if self.send_resize(size) {
            self.apply_local_resize(size);
        }
        for input in hydration.pending_input {
            match input {
                PendingTerminalInput::Key(key) => self.send_key(key),
                PendingTerminalInput::Focus(focused) => self.send_focus(focused),
                PendingTerminalInput::Paste(data) => self.send_paste(data),
            }
        }
    }

    /// Keep the Hub PTY size equal to the terminal pane in the frame just drawn.
    ///
    /// Attach, outer resize, and layout changes all converge here whether or
    /// not the pane has focus. An unchanged size sends nothing. A RESIZE the
    /// input window refuses (for example QueueFull) leaves the local size
    /// unchanged, so the next draw retries; draws follow wakes such as the
    /// INPUT_RESULT that frees window capacity. There is no timer.
    fn sync_terminal_pane_size(&mut self, hit_map: &HitMap) {
        let Some(outer) = tui_terminal_region(hit_map) else {
            return;
        };
        let inner = botster_tui_kit::terminal_inner_rect(outer);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let size = TerminalScreenSize::new(inner.height, inner.width);
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.pending_resize = Some(size);
            return;
        }
        if self.attached.is_none() || self.terminal_viewport_size == size {
            return;
        }
        if self.send_resize(size) {
            self.apply_local_resize(size);
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

    /// Encode one typed command, admit it into the window, and write what the
    /// window releases. Returns whether the window admitted the command.
    fn send_command(&mut self, operation_id: u64, command: TerminalInputCommand) -> bool {
        let frame = match encode_terminal_input(&command) {
            Ok(frame) => frame,
            Err(error) => {
                self.error = Some(error.to_string());
                return false;
            }
        };
        #[cfg(test)]
        self.observed_terminal_inputs.push(command);
        self.send_operation_frames(operation_id, false, vec![frame.into_bytes()])
    }

    /// Admit one operation of encoded frames into the window and write what
    /// the window releases, in order.
    ///
    /// Returns whether the window admitted the operation. Admission means the
    /// frames were written now or queued behind in-flight operations; it does
    /// not confirm transport delivery or the Hub's INPUT_RESULT.
    fn send_operation_frames(
        &mut self,
        operation_id: u64,
        paste: bool,
        frames: Vec<Vec<u8>>,
    ) -> bool {
        match self.input_window.admit(operation_id, paste, frames) {
            Ok(ready) => {
                self.send_encoded_frames(ready);
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
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

    /// Returns whether the input window admitted the RESIZE (written now or
    /// queued). A refusal leaves the caller's local size unchanged, so the
    /// next draw retries.
    fn send_resize(&mut self, size: TerminalScreenSize) -> bool {
        if self.attached.is_none() {
            return false;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            return false;
        };
        let command = terminal_input::resize_command(size.rows, size.cols, operation_id);
        self.send_command(operation_id, command)
    }

    fn send_paste(&mut self, data: Vec<u8>) {
        if self.input_window.has_paste() {
            self.error = Some("terminal paste unavailable: another paste is in flight".to_string());
            return;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let Some((_, route)) = self.current_owner_pair() else {
            self.error = Some("terminal stream unavailable: no attached route".to_string());
            return;
        };
        let Some(generation) = self.route_generation else {
            self.error = Some("terminal stream unavailable: route generation unknown".to_string());
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
        match self
            .input_window
            .admit_retaining(operation_id, true, frames, data.len())
        {
            Ok(ready) => {
                self.pending_unsafe_paste = Some(PendingUnsafePaste::AwaitingResult {
                    operation_id,
                    route,
                    generation,
                    payload: data,
                });
                self.send_encoded_frames(ready);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn invalidate_unsafe_paste(&mut self) {
        if let Some(pending) = self.pending_unsafe_paste.take() {
            self.input_window.release_retained(pending.payload_len());
        }
    }

    fn expire_unsafe_paste_consent(&mut self, now: Instant) {
        let expired = matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { deadline, .. }) if *deadline <= now
        );
        if expired {
            self.invalidate_unsafe_paste();
        }
    }

    fn confirm_unsafe_paste(&mut self) {
        let ready = matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent {
                stage: UnsafePasteConsentStage::Armed,
                ..
            })
        );
        if !ready {
            return;
        }
        let Some(PendingUnsafePaste::AwaitingConsent {
            route,
            generation,
            payload,
            deadline,
            ..
        }) = self.pending_unsafe_paste.take()
        else {
            return;
        };
        let payload_bytes = payload.len();
        if deadline <= Instant::now()
            || self.route_generation != Some(generation)
            || !self.attachment_matches_route(&route)
        {
            self.input_window.release_retained(payload_bytes);
            return;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            self.input_window.release_retained(payload_bytes);
            return;
        };
        let frames = match encode_paste(operation_id, true, &payload) {
            Ok(frames) => frames,
            Err(error) => {
                self.input_window.release_retained(payload_bytes);
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
        match self.input_window.admit(operation_id, true, frames) {
            Ok(ready) => {
                self.input_window.release_retained(payload_bytes);
                self.send_encoded_frames(ready);
            }
            Err(error) => {
                self.input_window.release_retained(payload_bytes);
                self.error = Some(error.to_string());
            }
        }
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

    #[cfg(test)]
    fn record_request(&mut self, request: &DaemonRequest) {
        match request {
            DaemonRequest::Status => self.observed_requests.push(ObservedRequest::Status),
            DaemonRequest::ReadPluginLogs {
                package_name,
                after_seq,
            } => self
                .observed_requests
                .push(ObservedRequest::ReadPluginLogs {
                    package_name: package_name.clone(),
                    after_seq: *after_seq,
                }),
            DaemonRequest::ResolveQuarantine { target } => self
                .observed_requests
                .push(ObservedRequest::ResolveQuarantine(target.clone())),
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
            let quarantine_changed = error_may_create_quarantine(&error.code, &error.operation);
            self.record_diagnostics(error.diagnostics);
            self.error = Some(if error.code == "not_attached" {
                // Core refused input, resize or a guarded write from a client
                // with no attachment; nothing reached the session.
                format!(
                    "not attached: {} (operation={}); nothing reached the session",
                    error.message, error.operation
                )
            } else {
                format!(
                    "{} (code={} operation={})",
                    error.message, error.code, error.operation
                )
            });
            if quarantine_changed {
                // Only Status lists quarantines; show the new one now.
                self.refresh_status();
            }
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
            self.quarantines = status.quarantines;
            self.hub_counters = status.observability;
        }

        if matches!(response.kind, DaemonResponseKind::PluginLogs)
            && let Some(logs) = response.plugin_logs.clone()
        {
            self.plugin_logs.insert(logs.package_name.clone(), logs);
        }
        if matches!(response.kind, DaemonResponseKind::QuarantineResolved) {
            // The quarantine list lives only in Status, so read it again. A
            // package resolution's package list is applied by its reply.
            self.action_feedback = Some("quarantine resolved".to_string());
            self.refresh_status();
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
            self.spawn_targets_loaded = true;
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
        if matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { .. })
        ) {
            let root = self.unsafe_paste_consent_surface();
            root.validate()
                .expect("unsafe paste consent UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("unsafe paste consent UiNode should fit TUI renderer capabilities");
            return root;
        }
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
        if let Some(notice) = self.recovery_notice_line() {
            root.children.push(child(notice));
        }
        if let Some(notice) = self.quarantine_band() {
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
        if matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { .. })
        ) || self.confirmation.is_some()
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
        } else if session.crashed() {
            // The pane names the lost worker; the row stays short enough for
            // the navigator.
            "crashed"
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
        if let Some(reason) = session
            .failure_reason
            .as_ref()
            .filter(|_| !session.crashed())
        {
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

    fn unsafe_paste_consent_surface(&self) -> UiNode {
        let stage = match self.pending_unsafe_paste.as_ref() {
            Some(PendingUnsafePaste::AwaitingConsent { stage, .. }) => *stage,
            _ => panic!("unsafe paste consent surface requires pending consent"),
        };
        let mut actions = node(
            UiNodeKind::Inline,
            "workspace-unsafe-paste-actions",
            json!({}),
        );
        actions.children.push(child(workspace_button(
            "workspace-unsafe-paste-primary",
            if stage == UnsafePasteConsentStage::Review {
                "Review paste"
            } else {
                "Cancel"
            },
            if stage == UnsafePasteConsentStage::Review {
                "botster.tui.unsafe_paste.review"
            } else {
                "botster.tui.unsafe_paste.cancel"
            },
            json!({}),
            "never",
            None,
        )));
        if stage == UnsafePasteConsentStage::Armed {
            actions.children.push(child(workspace_button(
                "workspace-unsafe-paste-confirm",
                "Paste anyway",
                "botster.tui.unsafe_paste.confirm",
                json!({}),
                "never",
                Some("danger"),
            )));
        }
        let mut body = node(
            UiNodeKind::Stack,
            "workspace-unsafe-paste-body",
            json!({ "direction": "vertical" }),
        );
        body.children = vec![
            child(node(
                UiNodeKind::Text,
                "workspace-unsafe-paste-warning",
                json!({ "text": "This paste contains multiple lines or terminal control characters. It can run commands or change terminal state." }),
            )),
            child(actions),
        ];
        let mut dialog = node(
            UiNodeKind::Dialog,
            "workspace-unsafe-paste-consent",
            json!({ "title": "Unsafe paste blocked", "presentation": "auto" }),
        );
        dialog.slots.insert("body".to_string(), vec![child(body)]);
        dialog
    }

    /// One line and one Resolve action per Hub quarantine.
    fn quarantine_nodes(&self) -> Vec<UiNode> {
        let mut nodes = Vec::new();
        for (index, quarantine) in self.quarantines.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-quarantine-{index}"),
                json!({ "text": format!("quarantine: {}", quarantine_text(quarantine)) }),
            ));
            let target = quarantine_target(quarantine);
            nodes.push(button(
                &format!("tui-quarantine-{index}-resolve"),
                "Resolve",
                "botster.tui.quarantine.resolve",
                serde_json::to_value(&target).unwrap_or(Value::Null),
            ));
        }
        nodes
    }

    /// Informational band while the Hub holds quarantines.
    fn quarantine_band(&self) -> Option<UiNode> {
        let count = self.quarantines.len();
        if count == 0 {
            return None;
        }
        let noun = if count == 1 {
            "quarantine"
        } else {
            "quarantines"
        };
        Some(node(
            UiNodeKind::Text,
            "workspace-quarantine-notice",
            json!({ "text": format!("{count} {noun} awaiting resolution (System details)") }),
        ))
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
        children.extend(self.quarantine_nodes().into_iter().map(child));
        if let Some(counters) = hub_counters_text(&self.hub_counters) {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-hub-counters",
                json!({ "text": counters }),
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
                if let Some(logs) = self.plugin_logs.get(&package.package_name) {
                    children.extend(plugin_log_nodes(logs, index).into_iter().map(child));
                }
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
            nodes.extend(action_state_nodes(
                &app.actions,
                "app action",
                &format!("tui-app-{app_index}"),
            ));
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
                let options = self.launch_target_options();
                let help = if !options.is_empty() {
                    "Select a launch target first".to_string()
                } else if let Some(failure) = &self.spawn_targets_failure {
                    format!("Launch targets failed to load: {failure}")
                } else if self.spawn_targets_loaded {
                    "No launch targets available".to_string()
                } else {
                    "Loading launch targets…".to_string()
                };
                nodes.push(node(
                    UiNodeKind::Text,
                    "tui-target-first-spawn-help",
                    json!({ "text": help }),
                ));
                for target in options {
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
            (None, None, Some(selected))
                if self.selected_session_row().is_some_and(SessionRow::crashed) =>
            {
                format!("Terminal · {selected} · crashed")
            }
            (None, None, Some(selected)) => match self.detaches.get(selected) {
                Some((_, DetachState::Pending)) => format!("Terminal · {selected} · detaching"),
                Some((_, DetachState::Failed(_))) => {
                    format!("Terminal · {selected} · detach failed")
                }
                Some((_, DetachState::Confirmed)) | None => {
                    format!("Terminal · {selected} · detached")
                }
            },
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
            Some(session) if session.crashed() => {
                "This session crashed: its worker was lost. Remove it, or Spawn a new session."
                    .to_string()
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
    ReadPluginLogs {
        package_name: String,
        after_seq: u64,
    },
    ResolveQuarantine(DaemonQuarantineTarget),
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

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

pub(crate) fn short_suffix() -> u64 {
    (unique_suffix() % 1_000_000_000_000) as u64
}

#[cfg(test)]
mod tests;
