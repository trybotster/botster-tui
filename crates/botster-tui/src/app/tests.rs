mod attach_route;
mod connection_args;
mod package_ui;
mod paste_and_keys;
mod session_selection;
mod session_types_spawn;
mod status_quarantine_logs;
mod workspace_layout;

use super::*;
use botster_hub_client::{DaemonUiTreeSnapshot, TerminalCompatibility};
use botster_terminal_protocol_client::mode_bits;
use botster_ui_contract::{
    UiActionId, UiActionKind, UiActionRequest, UiActionRequestId, UiActionResultState, UiSurfaceId,
};

fn plugin_surface_fixture(body: UiNode) -> DaemonPluginSurface {
    DaemonPluginSurface {
        package_name: "plugin.test".to_string(),
        surface_id: "test.surface".to_string(),
        ui_tree_snapshot: DaemonUiTreeSnapshot {
            package_name: "plugin.test".to_string(),
            surface_id: "test.surface".to_string(),
            body,
        },
    }
}

fn plugin_surface_response(surface: DaemonPluginSurface) -> DaemonResponse {
    let mut response = base_response(DaemonResponseKind::PluginSurface);
    response.plugin_surface = Some(surface);
    response
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

fn workspace_fixture() -> TuiApp {
    let mut app = TuiApp::new(None);
    app.workspace_test_mode = true;
    app.status = "connected".to_string();
    app.connection_error = None;
    app.sessions = session_rows([("session-alpha", "running"), ("session-beta", "exited")]);
    app.selected_session = Some("session-alpha".to_string());
    app
}

fn quarantined_status() -> DaemonResponse {
    let mut response = status_response("running", 1);
    response.status.as_mut().expect("status").quarantines = vec![
        DaemonQuarantine::Package {
            package_name: "workflow.plugin".to_string(),
            original: "enable failed".to_string(),
            compensation: "rollback failed".to_string(),
            durable: false,
            loaded: true,
            quarantined_at_ms: 1_790_000_000_000,
        },
        DaemonQuarantine::RepositorySessionTypes {
            root: std::path::PathBuf::from("/repo/one"),
            cause: "write_outcome_unknown".to_string(),
            detail: "rename interrupted".to_string(),
            quarantined_at_ms: 1_790_000_000_001,
        },
    ];
    response
        .status
        .as_mut()
        .expect("status")
        .observability
        .events_stranded = 3;
    response
}

fn plugin_log_page(count: u64) -> DaemonPluginLogs {
    DaemonPluginLogs {
        package_name: "workflow.plugin".to_string(),
        records: (1..=count)
            .map(|seq| botster_hub_client::DaemonPluginLogRecord {
                seq,
                generation: 1,
                at_ms: 1_790_000_000_000 + seq,
                level: "info".to_string(),
                message: format!("message {seq}"),
                fields_json: None,
                dropped_before: if seq == 12 { 4 } else { 0 },
            })
            .collect(),
        next_seq: count + 1,
        first_available_seq: 1,
    }
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
        quarantines: Vec::new(),
        local_webrtc_terminal_records: Vec::new(),
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
        route_path: "/packages/botster.plugin-contract-matrix/surfaces/contract.app".to_string(),
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
        route_path: "/packages/botster.plugin-contract-matrix/surfaces/contract.app".to_string(),
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

fn package_diagnostic(kind: &str, message: &str) -> botster_hub_client::DaemonPackageDiagnostic {
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

fn sample_session_type(session_type_id: &str, source: &str, editable: bool) -> DaemonSessionType {
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
        plugin_logs: None,
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

fn resync_frame(from_epoch: u32, to_epoch: u32) -> botster_terminal_protocol_client::TerminalFrame {
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

/// Encode one real GHOSTSNP export the way the Hub sends it: READY, one
/// SNAPSHOT_HISTORY per History or Finish page, then SNAPSHOT_FINISH.
fn ghostsnp_snapshot_frames(text: &str) -> Vec<botster_terminal_protocol_client::TerminalFrame> {
    use botster_terminal_ghostty::{GhosttySnapshotFrameKind, GhosttyTerminal};
    use botster_terminal_protocol_client::{
        encode_snapshot_finish, encode_snapshot_history, encode_snapshot_ready,
    };
    let mut source =
        GhosttyTerminal::new(TerminalScreenSize::new(24, 80)).expect("producer terminal");
    source.write_output_bytes(text.as_bytes());
    let mut frames = Vec::new();
    source
        .export_snapshot_frames(|frame| {
            let encoded = match frame.kind {
                GhosttySnapshotFrameKind::Ready => encode_snapshot_ready(&frame.bytes),
                GhosttySnapshotFrameKind::History | GhosttySnapshotFrameKind::Finish => {
                    encode_snapshot_history(&frame.bytes)
                }
            };
            frames.push(encoded.expect("snapshot frame"));
            true
        })
        .expect("export GHOSTSNP frames");
    frames.push(encode_snapshot_finish().expect("snapshot finish"));
    frames
}

fn projection_text(app: &mut TuiApp) -> String {
    let projection = app
        .ghostty_projection
        .as_mut()
        .expect("projection installed")
        .project_viewport()
        .expect("project viewport");
    projection
        .cells
        .iter()
        .map(|cell| cell.grapheme.as_str())
        .collect()
}

fn attach_state_frame(state: AttachStateCode) -> botster_terminal_protocol_client::TerminalFrame {
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

/// Attach `route` fully: the response, `attached`, and a whole snapshot.
fn hydrate_fully(app: &mut TuiApp, route: &str, generation: u64) {
    complete_attach(app, "session-alpha", route, generation);
    app.apply_wake(routed(
        route,
        generation,
        attach_state_frame(AttachStateCode::Attached),
    ));
    for frame in ghostsnp_snapshot_frames("SCREEN\r\n") {
        app.apply_wake(routed(route, generation, frame));
    }
}

/// Close `route` the way the Hub reports a Core reader-deadline close.
fn close_route(app: &mut TuiApp, route: &str, generation: u64) {
    app.handle_terminal_subscription_closed(
        "session-alpha".to_string(),
        route.to_string(),
        generation,
        "core_adapter_closed".to_string(),
    );
}

fn replacement_route(app: &TuiApp) -> String {
    app.attach_hydration
        .as_ref()
        .map(|hydration| hydration.route.clone())
        .expect("a recovery attach is hydrating")
}

fn paste_awaiting_result(payload: &[u8]) -> (TuiApp, u64) {
    let mut app = workspace_fixture();
    app.hub_io.capture_terminal_frames();
    app.attached = Some(AttachedRoute {
        session_id: "session-alpha".to_string(),
        route: "route-1".to_string(),
    });
    app.subscription_id = "route-1".to_string();
    app.route_generation = Some(7);
    app.route_epoch = Some(0);
    app.send_paste(payload.to_vec());
    let operation_id = match app.pending_unsafe_paste.as_ref() {
        Some(PendingUnsafePaste::AwaitingResult { operation_id, .. }) => *operation_id,
        other => panic!("paste payload was not retained: {other:?}"),
    };
    (app, operation_id)
}

fn apply_paste_result(
    app: &mut TuiApp,
    operation_id: u64,
    outcome: InputOutcome,
    accepted_payload_bytes: Option<u64>,
    written_pty_bytes: Option<u64>,
) {
    app.apply_terminal_input_result(
        "route-1",
        InputResultBody {
            operation_id,
            outcome,
            accepted_payload_bytes,
            written_pty_bytes,
            mode_bits: 0,
            detail: String::new(),
        },
    );
}

fn attached_workspace_with_focused_terminal() -> (TuiApp, InputRouter, HitMap) {
    let mut app = workspace_fixture();
    app.sessions = session_rows([
        ("session-alpha", "running"),
        ("session-beta", "running"),
        ("session-gamma", "running"),
    ]);
    app.hub_io.capture_terminal_frames();
    app.attached = Some(AttachedRoute {
        session_id: "session-alpha".to_string(),
        route: "route-1".to_string(),
    });
    app.subscription_id = "route-1".to_string();
    app.route_generation = Some(7);
    app.route_epoch = Some(0);
    let mut router = InputRouter::new(renderer::action_request_context());
    let (_, map) = render_app_to_lines(&app, 140, 40, &router.render_state());
    focus_hit_map_node_by_tab(&mut router, &map, "tui-terminal");
    (app, router, map)
}
