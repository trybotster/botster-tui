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

mod args;
pub use args::{AppArgs, ParsedCommand, smoke_message, usage};
mod state;
use state::*;
mod shell;
pub use shell::run;
use shell::*;

mod actions;
mod attach;
mod connection;
mod diagnostics;
mod entities;
mod ghostty;
mod input_out;
mod lifecycle;
mod notices;
mod requests_out;
mod responses;
mod route;
mod spawn;
mod surface;

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
