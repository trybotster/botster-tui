use super::*;

#[derive(Default)]
pub(super) struct AcceptanceRequestAudit {
    pub(super) surface_renders: Vec<(String, String)>,
    pub(super) surface_actions: Vec<UiActionRequest>,
    pub(super) list_sessions: usize,
}

impl AcceptanceRequestAudit {
    pub(super) fn record(&mut self, request: &DaemonRequest) {
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
pub(super) struct AcceptanceDiagnostics {
    pub(super) case_id: Option<String>,
    pub(super) phase: String,
    pub(super) expected_condition: String,
    pub(super) subscription_id: Option<String>,
    pub(super) snapshot_seq: Option<u64>,
    pub(super) surface_render_count: usize,
    pub(super) focusable_ids: Vec<String>,
    pub(super) last_observation: Value,
}

impl AcceptanceDiagnostics {
    pub(super) fn stage(&mut self, phase: &str, case_id: Option<&str>, expected_condition: &str) {
        self.phase = phase.to_string();
        self.case_id = case_id.map(ToOwned::to_owned);
        self.expected_condition = expected_condition.to_string();
    }

    pub(super) fn observe_app(&mut self, app: &TuiApp) {
        self.subscription_id = app.session_entities.subscription_id.clone();
        self.snapshot_seq = app.session_entities.snapshot_seq;
        self.surface_render_count = app
            .acceptance_audit
            .as_ref()
            .map_or(0, |audit| audit.surface_renders.len());
    }

    pub(super) fn observe_frame(&mut self, app: &TuiApp, hit_map: &HitMap) {
        self.observe_app(app);
        self.focusable_ids = focusable_ids(hit_map);
    }

    pub(super) fn observe_request(&mut self, request: &UiActionRequest) {
        self.last_observation = json!({
            "kind": "action_request",
            "request_id": request.request_id,
            "surface_id": request.surface_id,
            "action_id": request.action_id,
            "node_id": request.node_id
        });
    }

    pub(super) fn observe_result(&mut self, result: &UiActionResult) {
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

    pub(super) fn failure_context(&self) -> FailureContext {
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

pub(super) const ACCEPTANCE_WIDTH: u16 = 500;
pub(super) const ACCEPTANCE_HEIGHT: u16 = 240;
pub(super) const ACCEPTANCE_TIMEOUT: Duration = Duration::from_secs(12);
pub(super) const WORKSPACES_PACKAGE: &str = "botster-workspaces";
pub(super) const WORKSPACES_SURFACE: &str = "workspaces";
pub(super) const WORKSPACES_SPAWN_OPENER_ACTION: &str = "botster_workspaces.open_spawn";
pub(super) const WORKSPACES_ADD_SESSION_ACTION: &str = "botster_workspaces.add_session";
pub(super) const WORKSPACES_ADD_SESSION_FIELD: &str = "session_id";
pub(super) const WORKSPACES_ADD_SESSION_NODE: &str = "botster-workspaces-add-session-id";
pub(super) const WORKSPACES_MEMBERSHIP_FAMILY: &str = "botster-workspaces.membership";

pub(super) fn run_workspaces_acceptance(args: AppArgs, config: SpawnConfig) -> io::Result<()> {
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

pub(super) fn drive_workspaces_acceptance(
    args: AppArgs,
    config: &SpawnConfig,
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
    // timer: deadline — acceptance step budget; expiry fails the acceptance run
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

pub(super) fn open_workspaces_surface(
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

pub(super) fn drive_spawn_case(
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

pub(super) fn wait_for_acceptance_state(
    app: &mut TuiApp,
    diagnostics: &mut AcceptanceDiagnostics,
    expectation: &str,
    mut ready: impl FnMut(&mut TuiApp, &mut AcceptanceDiagnostics) -> bool,
) -> io::Result<()> {
    // timer: deadline — acceptance step budget; expiry fails the acceptance run
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

pub(super) fn run_workspaces_claim_acceptance(
    args: AppArgs,
    config: ClaimConfig,
) -> io::Result<()> {
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

pub(super) fn drive_workspaces_claim_acceptance(
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
    // timer: deadline — acceptance step budget; expiry fails the acceptance run
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

pub(super) fn ensure_membership_family_demanded(
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

pub(super) fn claim_session_is_current(app: &TuiApp, session_uuid: &str) -> bool {
    app.session_entities.has_snapshot
        && app
            .session_entities
            .entities
            .get(session_uuid)
            .is_some_and(|entity| entity.lifecycle_class == "current")
}

pub(super) fn membership_entity_contains(
    app: &TuiApp,
    workspace_id: &str,
    session_uuid: &str,
) -> bool {
    membership_entity_row(app, workspace_id, session_uuid).is_some()
}

pub(super) fn membership_entity_row(
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

pub(super) fn acceptance_field_has_option(
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

pub(super) fn acceptance_field_option_count(
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
pub(super) fn ensure_claim_option_exclusion(
    app: &mut TuiApp,
    router: &mut InputRouter,
    workspace_id: &str,
    session_uuid: &str,
    evidence: &mut EvidenceWriter,
    diagnostics: &mut AcceptanceDiagnostics,
) -> io::Result<bool> {
    let target = Value::String(session_uuid.to_string());
    let mut reopened = false;
    // timer: deadline — acceptance step budget; expiry fails the acceptance run
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
        app.pump_once(deadline);
    }
    invalid_acceptance(format!(
        "timed out waiting for Available sessions to exclude {session_uuid}"
    ))
}

pub(super) fn session_option_projected(app: &TuiApp, session_uuid: &str) -> bool {
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
pub(super) const CLAIM_SESSION_DISPLAY_FIELDS: &[&str] = &[
    "label",
    "session_uuid",
    "lifecycle",
    "lifecycle_class",
    "session_type_id",
    "spawn_point",
];

pub(super) fn claim_session_option_fields(
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

pub(super) fn json_field_string(
    fields: &serde_json::Map<String, Value>,
    key: &str,
) -> Option<String> {
    match fields.get(key)? {
        Value::String(value) if !value.is_empty() => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn claim_option_lifecycle(app: &TuiApp, session_uuid: &str) -> Option<String> {
    claim_session_option_fields(app, session_uuid)
        .and_then(|fields| json_field_string(&fields, "lifecycle"))
        .or_else(|| {
            app.session_entities
                .entities
                .get(session_uuid)
                .and_then(|entity| entity.lifecycle.clone())
        })
}

pub(super) fn claim_option_dedicated_label(app: &TuiApp, session_uuid: &str) -> Option<String> {
    claim_session_option_fields(app, session_uuid)
        .and_then(|fields| json_field_string(&fields, "label"))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value != session_uuid)
}

pub(super) fn claim_option_compact_label(app: &TuiApp, session_uuid: &str) -> Option<String> {
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

pub(super) fn lifecycle_token_terminal(lifecycle: &str) -> bool {
    matches!(
        lifecycle.to_ascii_lowercase().as_str(),
        "exited" | "ended" | "failed" | "stopping" | "stale"
    )
}

pub(super) fn claim_add_form_open(
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

pub(super) fn acceptance_frame(
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

pub(super) fn acceptance_has_action(
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

pub(super) fn activate_acceptance_action(
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
    // timer: deadline — acceptance step budget; expiry fails the acceptance run
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

pub(super) fn unique_acceptance_action(
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

pub(super) fn focus_acceptance_node(
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

pub(super) fn select_acceptance_value(
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

pub(super) fn select_only_acceptance_value(
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

pub(super) fn type_acceptance_text(
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

pub(super) fn payload_field<'a>(payload: &'a Option<Value>, field: &str) -> Option<&'a str> {
    payload.as_ref()?.get(field)?.as_str()
}

pub(super) fn focusable_ids(hit_map: &HitMap) -> Vec<String> {
    hit_map
        .focusable_regions()
        .take(24)
        .map(|region| region.node_id.clone())
        .collect()
}

pub(super) fn invalid_acceptance<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}
