use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum EventSubscriptionState {
    #[default]
    Idle,
    Candidate(String),
    Active(String),
}

impl EventSubscriptionState {
    pub(super) fn active_id(&self) -> Option<&str> {
        match self {
            Self::Active(id) => Some(id.as_str()),
            Self::Idle | Self::Candidate(_) => None,
        }
    }

    pub(super) fn candidate_id(&self) -> Option<&str> {
        match self {
            Self::Candidate(id) => Some(id.as_str()),
            Self::Idle | Self::Active(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TransientNotice {
    pub(super) text: String,
    pub(super) deadline: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NoticeSubscriptionEntry {
    pub(super) descriptor: PackageNoticeReactionDescriptor,
    pub(super) subject: String,
    pub(super) state: EventSubscriptionState,
}

pub(super) type NoticeSubscriptionKey = (String, String);

#[derive(Clone, Debug)]
pub(super) struct EntityOptionsRetryState {
    pub(super) consecutive_failures: u32,
    pub(super) next_attempt_at: Instant,
}

pub(super) fn entity_options_backoff_delay(consecutive_failures: u32) -> Duration {
    let shift = consecutive_failures.saturating_sub(1).min(6);
    let millis = ENTITY_OPTIONS_BACKOFF_INITIAL
        .as_millis()
        .saturating_mul(1u128 << shift);
    let capped = millis.min(ENTITY_OPTIONS_BACKOFF_CAP.as_millis());
    Duration::from_millis(capped as u64)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SessionRow {
    pub(super) session_id: String,
    pub(super) lifecycle: String,
    pub(super) failure_reason: Option<String>,
    pub(super) pending: bool,
    pub(super) session_type_id: Option<String>,
    pub(super) session_type_source: Option<String>,
    pub(super) role: Option<String>,
    pub(super) traits: Vec<String>,
    pub(super) interaction: Option<String>,
    pub(super) session_type_lifecycle: Option<String>,
}

/// One attach campaign between the Attach request and the open live path.
///
/// Scheme 2 ordering per route: ATTACH_STATE attached, MODES, SNAPSHOT_READY,
/// live OUTPUT interleaved with SNAPSHOT_HISTORY, SNAPSHOT_FINISH, then OUTPUT.
/// Live OUTPUT that arrives before SNAPSHOT_FINISH is retained here (bounded)
/// and applied after the last history page.
#[derive(Clone, Debug)]
pub(super) struct AttachHydration {
    pub(super) session_id: String,
    pub(super) route: String,
    pub(super) buffered_live_output: Vec<u8>,
    pub(super) pending_input: Vec<PendingTerminalInput>,
    pub(super) pending_input_bytes: usize,
    pub(super) pending_resize: Option<TerminalScreenSize>,
    /// True after the incremental decoder validates READY.
    pub(super) snapshot_ready: bool,
    /// True after SNAPSHOT_FINISH or HISTORY_UNAVAILABLE after READY.
    pub(super) snapshot_finished: bool,
    /// True after the matching attached state arrives.
    pub(super) attached_seen: bool,
    /// True when this hydration restarts a route that was already live
    /// (ROUTE_RESYNC): the attachment and its input window survive.
    pub(super) resync: bool,
    /// Set when this hydration re-attaches after a route failure, with the
    /// failure's short cause. Only its completion restores the campaign's one
    /// recovery and counts toward the recovery notice.
    pub(super) recovery_cause: Option<String>,
}

impl AttachHydration {
    pub(super) fn new(session_id: &str, route: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            route: route.to_string(),
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
pub(super) struct RecoveryNotice {
    pub(super) count: u32,
    pub(super) cause: String,
}

/// Input captured while a route is still attaching. Operation ids are
/// assigned when the live path opens so they stay strictly increasing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PendingTerminalInput {
    Key(KeyEvent),
    Focus(bool),
    Paste(Vec<u8>),
}

impl PendingTerminalInput {
    /// Bytes retained by the client for this pending input.
    pub(super) fn retained_bytes(&self) -> usize {
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
pub(super) const WORKSPACE_MENU_NODE: &str = "tui-spawn";

pub(super) fn is_terminal_node(node_id: Option<&str>) -> bool {
    matches!(node_id, Some("tui-terminal" | "tui-terminal-output"))
}

/// Entity family of one subscription frame.
pub(super) fn entity_frame_type(frame: &DaemonEntityFrame) -> &str {
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
pub(super) struct AttachedRoute {
    pub(super) session_id: String,
    pub(super) route: String,
}

/// Last MODES frame for the current route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TerminalModeState {
    pub(super) route: String,
    pub(super) modes: ModesBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DestructiveAction {
    Shutdown(String),
    Remove(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UnsafePasteConsentStage {
    Review,
    Armed,
}

#[derive(Debug)]
pub(super) enum PendingUnsafePaste {
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
    pub(super) fn payload_len(&self) -> usize {
        match self {
            Self::AwaitingResult { payload, .. } | Self::AwaitingConsent { payload, .. } => {
                payload.len()
            }
        }
    }
}

/// The Hub's answer to one Detach, as far as this connection knows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DetachState {
    /// Sent; no correlated response yet.
    Pending,
    /// A correlated Events response with no operator error arrived.
    Confirmed,
    /// An operator error, an unexpected response, or request failure/expiry.
    Failed(String),
}

/// What the application does with one host-control completion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PendingReply {
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
pub(super) struct ParkedNoticeEvents {
    pub(super) events: VecDeque<(String, String, Value)>,
    pub(super) gap: bool,
}

impl SessionRow {
    #[cfg(test)]
    pub(super) fn running(session_id: impl Into<String>) -> Self {
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

    pub(super) fn pending(session_id: impl Into<String>) -> Self {
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

    pub(super) fn from_entity(entity: &DaemonSessionEntity) -> Self {
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

    pub(super) fn is_attachable(&self) -> bool {
        !self.pending && self.lifecycle == "running"
    }

    /// The session's worker died without an exit report: a crash, not an
    /// exit.
    pub(super) fn crashed(&self) -> bool {
        self.lifecycle == "failed"
            && self.failure_reason.as_deref() == Some(TERMINAL_SUBSCRIPTION_CLOSED_WORKER_LOST)
    }
}

#[derive(Default)]
pub(super) struct SessionEntityState {
    pub(super) subscription_id: Option<String>,
    pub(super) has_snapshot: bool,
    pub(super) snapshot_seq: Option<u64>,
    pub(super) entity_order: Vec<String>,
    pub(super) entities: BTreeMap<String, DaemonSessionEntity>,
}

impl SessionEntityState {
    pub(super) fn begin_generation(&mut self, subscription_id: String) {
        self.subscription_id = Some(subscription_id);
        self.has_snapshot = false;
        self.snapshot_seq = None;
        self.entity_order.clear();
        self.entities.clear();
    }

    pub(super) fn apply(&mut self, frame: DaemonEntityFrame) -> Result<bool, String> {
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

    pub(super) fn matches(&self, subscription_id: &str, entity_type: &str) -> bool {
        entity_type == "session" && self.subscription_id.as_deref() == Some(subscription_id)
    }

    pub(super) fn accepts_delta(
        &self,
        subscription_id: &str,
        entity_type: &str,
        snapshot_seq: u64,
    ) -> bool {
        self.has_snapshot
            && self.matches(subscription_id, entity_type)
            && self
                .snapshot_seq
                .is_none_or(|current| snapshot_seq > current)
    }

    pub(super) fn binding_rows(&self) -> Result<Vec<Value>, String> {
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
pub(super) enum SessionTypeFrameOutcome {
    /// The frame belongs to another generation or is stale.
    Ignored,
    /// The entity set changed. `replaced` is true for a full Snapshot.
    Applied { replaced: bool },
    /// Hub reported a catalog error. The subscription stays open, and the
    /// next Snapshot replaces the whole entity set.
    HubError(String),
}

#[derive(Default)]
pub(super) struct SessionTypeEntityState {
    pub(super) subscription_id: Option<String>,
    pub(super) has_snapshot: bool,
    pub(super) snapshot_seq: Option<u64>,
    pub(super) entity_order: Vec<String>,
    pub(super) entities: BTreeMap<String, DaemonSessionType>,
}

impl SessionTypeEntityState {
    pub(super) fn begin_generation(&mut self, subscription_id: String) {
        self.subscription_id = Some(subscription_id);
        self.has_snapshot = false;
        self.snapshot_seq = None;
        self.entity_order.clear();
        self.entities.clear();
    }

    pub(super) fn apply(
        &mut self,
        frame: DaemonEntityFrame,
    ) -> Result<SessionTypeFrameOutcome, String> {
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

    pub(super) fn matches(&self, subscription_id: &str, entity_type: &str) -> bool {
        entity_type == "session_type" && self.subscription_id.as_deref() == Some(subscription_id)
    }

    pub(super) fn accepts_delta(
        &self,
        subscription_id: &str,
        entity_type: &str,
        snapshot_seq: u64,
    ) -> bool {
        self.has_snapshot
            && self.matches(subscription_id, entity_type)
            && self
                .snapshot_seq
                .is_none_or(|current| snapshot_seq > current)
    }

    pub(super) fn ordered(&self) -> Vec<&DaemonSessionType> {
        self.entity_order
            .iter()
            .filter_map(|id| self.entities.get(id))
            .collect()
    }
}

pub(super) fn decode_session_type_entity(entity: Value) -> Result<DaemonSessionType, String> {
    serde_json::from_value(entity)
        .map_err(|error| format!("session type entity failed to decode: {error}"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SessionTypeFormMode {
    Create,
    Edit,
}

/// Draft for create/edit. Edit is seeded only from ShowSessionTypeDefinition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SessionTypeFormDraft {
    pub(super) mode: SessionTypeFormMode,
    pub(super) source: String,
    pub(super) source_target_id: String,
    /// Effective Hub session_type_id while editing; unused for create.
    pub(super) session_type_id: Option<String>,
    /// Lossless seed retained for edit wholesale replacement.
    pub(super) seed_definition: Option<DaemonSessionTypeDefinition>,
    pub(super) seed_source: Option<DaemonSessionTypeMutationSource>,
    pub(super) id: String,
    pub(super) label: String,
    pub(super) description: String,
    pub(super) icon: String,
    pub(super) role: String,
    pub(super) interaction: String,
    pub(super) traits: String,
    pub(super) lifecycle: String,
    pub(super) execution: String,
    pub(super) command: String,
    pub(super) args: String,
    pub(super) working_directory_policy: String,
    pub(super) working_directory_path: String,
    pub(super) environment: String,
    pub(super) allowed_environment_overrides: String,
    pub(super) context_keys: String,
    /// Preserved authored collections when text controls are left untouched.
    pub(super) seeded_traits: Option<Vec<String>>,
    pub(super) seeded_args: Option<Vec<String>>,
    pub(super) seeded_context: Option<Vec<String>>,
    pub(super) seeded_allowed_environment_overrides: Option<Vec<String>>,
    pub(super) seeded_environment: Option<BTreeMap<String, String>>,
    pub(super) definition_target_id: String,
    pub(super) error: Option<String>,
}

impl SessionTypeFormDraft {
    pub(super) fn create_default() -> Self {
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

    pub(super) fn from_authoring(editable: DaemonSessionTypeEditableDefinition) -> Self {
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
pub(super) enum TargetFirstSpawnStep {
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
pub(super) struct TargetFirstSpawnFlow {
    pub(super) step: TargetFirstSpawnStep,
}

/// One pickable launch target: an enabled admitted Hub spawn target only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LaunchTargetOption {
    pub(super) target_id: String,
    pub(super) label: String,
}

pub(super) fn join_tokens(values: &[String]) -> String {
    values.join(", ")
}

pub(super) fn parse_token_list(input: &str, seeded: Option<&Vec<String>>) -> Vec<String> {
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

pub(super) fn format_environment(environment: &BTreeMap<String, String>) -> String {
    environment
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn parse_environment(
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

pub(super) fn definition_from_session_type_form(
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

pub(super) fn mutation_source_from_form(
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
pub(super) fn decode_session_entity(entity: Value) -> Result<DaemonSessionEntity, String> {
    serde_json::from_value(entity)
        .map_err(|error| format!("session entity failed to decode: {error}"))
}

/// Builds an intentionally exhaustive session-entity row so bind-list templates
/// observe every key, including those the Hub omits when absent.
///
/// The values are deliberately reference-shaped placeholders: only
/// [`session_binding_reference_row`]'s keys are consumed, and the TUI must not
/// imply ownership of the Hub's role/interaction/lifecycle vocabulary.
pub(super) fn session_binding_reference_row() -> serde_json::Map<String, Value> {
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
