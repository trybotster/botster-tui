use super::*;

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
        rendered.contains("path=/packages/botster.plugin-contract-matrix/surfaces/contract.app")
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
fn plugin_surface_renders_only_the_identity_matched_snapshot_body() {
    let body = node(
        UiNodeKind::Text,
        "canonical-snapshot-body",
        json!({ "text": "canonical snapshot" }),
    );
    let mut app = TuiApp::new(None);

    app.apply_response(plugin_surface_response(plugin_surface_fixture(
        body.clone(),
    )));

    let surface = app
        .plugin_surface
        .as_ref()
        .expect("accepted plugin surface");
    assert_eq!(plugin_surface_body_node(surface), Ok(body));
    assert_eq!(app.error, None);
}

#[test]
fn plugin_surface_rejects_snapshot_identity_mismatch() {
    let mut surface = plugin_surface_fixture(node(
        UiNodeKind::Text,
        "mismatched-snapshot-body",
        json!({ "text": "mismatched snapshot" }),
    ));
    surface.ui_tree_snapshot.surface_id = "wrong.surface".to_string();
    let mut app = TuiApp::new(None);

    app.apply_response(plugin_surface_response(surface));

    assert!(app.plugin_surface.is_none());
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("ui_tree_snapshot identity mismatch")),
        "{:?}",
        app.error
    );
}

#[test]
fn plugin_action_replacement_updates_the_single_snapshot_body() {
    let initial = node(
        UiNodeKind::Text,
        "initial-snapshot-body",
        json!({ "text": "initial snapshot" }),
    );
    let replacement = node(
        UiNodeKind::Text,
        "replacement-snapshot-body",
        json!({ "text": "replacement snapshot" }),
    );
    let request = UiActionRequest {
        request_id: UiActionRequestId("request-replace".to_string()),
        surface_id: UiSurfaceId("test.surface".to_string()),
        action_id: UiActionId("plugin.replace".to_string()),
        node_id: Some(UiNodeId("replace-button".to_string())),
        kind: UiActionKind::Submit,
        values: None,
        payload: None,
    };
    let result = UiActionResult {
        request_id: request.request_id.clone(),
        surface_id: request.surface_id.clone(),
        action_id: request.action_id.clone(),
        node_id: request.node_id.clone(),
        state: UiActionResultState::Accepted,
        field_errors: BTreeMap::new(),
        form_errors: Vec::new(),
        warnings: Vec::new(),
        normalized_values: None,
        presentation: None,
        replacement: Some(Box::new(replacement.clone())),
        payload: None,
        error: None,
    };
    let mut app = TuiApp::new(None);
    app.plugin_surface = Some(plugin_surface_fixture(initial));
    app.pending_plugin_request = Some(request);

    app.apply_plugin_action_result(result);

    let surface = app.plugin_surface.as_ref().expect("active plugin surface");
    assert_eq!(surface.ui_tree_snapshot.body, replacement);
    assert_eq!(surface.ui_tree_snapshot.package_name, surface.package_name);
    assert_eq!(surface.ui_tree_snapshot.surface_id, surface.surface_id);
    assert_eq!(app.error, None);
}

#[test]
fn plugin_surface_without_required_snapshot_fails_deserialization() {
    let error = serde_json::from_value::<DaemonPluginSurface>(json!({
        "package_name": "plugin.test",
        "surface_id": "test.surface"
    }))
    .expect_err("required ui_tree_snapshot must fail closed");

    assert!(error.to_string().contains("ui_tree_snapshot"), "{error}");
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
    assert!(rendered.contains(
        "feature blocked: reason=missing_provider action=install_provider package=cloud.provider"
    ));
    assert!(rendered.contains(
        "feature blocked: reason=missing_capability action=grant_capability capability=http:egress"
    ));
    assert!(rendered.contains(
        "feature blocked: reason=package_disabled action=enable_package package=workflow.plugin"
    ));
    assert!(rendered.contains(
        "feature blocked: reason=invalid_configuration action=fix_configuration requirement=mode"
    ));
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
    assert!(rendered.contains("install effect: write_manifest:registry entry will be installed"));
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
    assert!(rendered.contains("API token secret (redacted; Space marks write-only update): [ ]"));
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
    app.system_details_visible = true;
    app.spawn_targets_loaded = true;
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
