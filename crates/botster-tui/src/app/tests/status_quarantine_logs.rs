use super::*;

#[test]
fn status_quarantines_render_with_resolve_actions_and_a_workspace_band() {
    let mut app = workspace_fixture();
    app.apply_completion(PendingReply::Apply, quarantined_status());
    assert_eq!(app.quarantines.len(), 2);

    let workspace = render_app_to_lines(&app, 140, 42, &RenderState::default())
        .0
        .join("\n");
    assert!(
        workspace.contains("2 quarantines awaiting resolution (System details)"),
        "{workspace}"
    );

    app.system_details_visible = true;
    let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 80);
    let details = lines.join("\n");
    assert!(
        details.contains(
            "quarantine: package workflow.plugin: enable failed; compensation failed: rollback failed · loaded but inert · not durable: lasts until the Hub restarts"
        ),
        "{details}"
    );
    assert!(
        details.contains(
            "quarantine: session types at /repo/one: write_outcome_unknown: rename interrupted"
        ),
        "{details}"
    );
    assert_eq!(details.matches("Resolve").count(), 2, "{details}");
    assert!(
        details.contains("hub counters: events_stranded=3"),
        "{details}"
    );
}

#[test]
fn resolve_sends_the_listed_target_and_a_resolution_rereads_status() {
    let mut app = workspace_fixture();
    app.apply_completion(PendingReply::Apply, quarantined_status());
    let nodes = app.quarantine_nodes();
    let payload = nodes
        .iter()
        .filter_map(|node| node.props.get("action"))
        .filter_map(|action| action.get("payload"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(payload.len(), 2, "one Resolve payload per quarantine");
    for payload in payload {
        app.handle_action(
            "botster.tui.quarantine.resolve".to_string(),
            None,
            Some(payload),
        );
    }
    app.handle_action(
        "botster.tui.quarantine.resolve".to_string(),
        None,
        Some(json!({ "kind": "unknown" })),
    );
    assert_eq!(
        app.observed_requests,
        vec![
            ObservedRequest::ResolveQuarantine(DaemonQuarantineTarget::Package {
                package_name: "workflow.plugin".to_string()
            }),
            ObservedRequest::ResolveQuarantine(DaemonQuarantineTarget::RepositorySessionTypes {
                root: std::path::PathBuf::from("/repo/one")
            }),
        ]
    );
    assert_eq!(
        app.error.as_deref(),
        Some("resolve: invalid quarantine target")
    );

    // A repository root's resolution replies without a package list:
    // the package model stays.
    app.packages = vec![package_with_configuration()];
    app.observed_requests.clear();
    app.apply_completion(
        PendingReply::ResolveQuarantine {
            target: DaemonQuarantineTarget::RepositorySessionTypes {
                root: std::path::PathBuf::from("/repo/one"),
            },
        },
        base_response(DaemonResponseKind::QuarantineResolved),
    );
    assert_eq!(app.packages.len(), 1, "the package model survives");
    assert_eq!(app.observed_requests, vec![ObservedRequest::Status]);
    assert_eq!(app.action_feedback.as_deref(), Some("quarantine resolved"));

    // A package resolution replies with the package list, which replaces
    // the model.
    app.observed_requests.clear();
    app.apply_completion(
        PendingReply::ResolveQuarantine {
            target: DaemonQuarantineTarget::Package {
                package_name: "workflow.plugin".to_string(),
            },
        },
        base_response(DaemonResponseKind::QuarantineResolved),
    );
    assert!(app.packages.is_empty(), "the reply's package list applies");
    assert_eq!(app.observed_requests, vec![ObservedRequest::Status]);

    // The re-read Status without quarantines clears the list and band.
    app.apply_completion(PendingReply::Apply, status_response("running", 1));
    assert!(app.quarantines.is_empty());
    assert!(app.quarantine_band().is_none());
}

#[test]
fn quarantine_creating_errors_reread_status_and_others_do_not() {
    let mut app = workspace_fixture();
    for (code, operation, rereads) in [
        (
            "package_compensation_failed",
            "package_mutation_compensation",
            true,
        ),
        (
            "repo_session_type_publication_uncertain",
            "repo_session_type",
            true,
        ),
        ("package_not_found", "package_mutation", false),
    ] {
        app.observed_requests.clear();
        let mut response = base_response(DaemonResponseKind::OperatorError);
        response.error = Some(botster_hub_client::DaemonOperatorError {
            code: code.to_string(),
            request_id: "request".to_string(),
            operation: operation.to_string(),
            message: "failed".to_string(),
            diagnostics: Vec::new(),
        });
        app.apply_completion(PendingReply::Apply, response);
        assert_eq!(
            app.observed_requests.contains(&ObservedRequest::Status),
            rereads,
            "{code} / {operation}"
        );
    }
}

#[test]
fn a_dropped_connection_forgets_its_quarantines_and_counters() {
    let mut app = workspace_fixture();
    app.apply_completion(PendingReply::Apply, quarantined_status());
    assert!(app.quarantine_band().is_some());
    assert!(hub_counters_text(&app.hub_counters).is_some());
    app.drop_connection_state();
    assert!(app.quarantines.is_empty());
    assert!(app.quarantine_band().is_none());
    assert!(app.quarantine_nodes().is_empty());
    assert_eq!(hub_counters_text(&app.hub_counters), None);
}

#[test]
fn plugin_logs_are_read_from_the_start_and_show_the_last_records() {
    let mut app = workspace_fixture();
    app.packages = vec![package_with_configuration()];
    let package_name = app.packages[0].package_name.clone();
    app.observed_requests.clear();
    app.handle_action(
        "botster.tui.package.logs".to_string(),
        None,
        Some(json!({ "package_name": package_name })),
    );
    assert_eq!(
        app.observed_requests,
        vec![ObservedRequest::ReadPluginLogs {
            package_name: package_name.clone(),
            after_seq: 0,
        }]
    );

    let mut page = plugin_log_page(12);
    page.package_name = package_name.clone();
    let mut response = base_response(DaemonResponseKind::PluginLogs);
    response.plugin_logs = Some(page);
    app.apply_completion(PendingReply::Apply, response);

    app.system_details_visible = true;
    let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 120);
    let details = lines.join("\n");
    assert!(details.contains("[ Logs ]"), "{details}");
    assert!(
        details.contains(&format!(
            "logs: {package_name} · 12 records read · showing the last 10 · next_seq=13 first_available_seq=1"
        )),
        "{details}"
    );
    assert!(
        !details.contains("log #2 info"),
        "only the last 10: {details}"
    );
    assert!(details.contains("log #3 info: message 3"), "{details}");
    assert!(
        details.contains("log #12 info: message 12 (4 dropped before)"),
        "{details}"
    );

    app.drop_connection_state();
    assert!(app.plugin_logs.is_empty(), "a reconnect forgets the logs");
}

#[test]
fn plugin_logs_fit_the_live_terminal_size() {
    // The live tests drive a 140x40 terminal.
    let mut app = workspace_fixture();
    app.packages = vec![package_with_configuration()];
    let mut page = plugin_log_page(12);
    page.package_name = app.packages[0].package_name.clone();
    let mut response = base_response(DaemonResponseKind::PluginLogs);
    response.plugin_logs = Some(page);
    app.apply_completion(PendingReply::Apply, response);
    app.system_details_visible = true;
    let (lines, _) = renderer::render_to_lines(&app.surface(), 140, 40);
    let details = lines.join("\n");
    assert!(details.contains("[ Logs ]"), "{details}");
    assert!(details.contains("showing the last 10"), "{details}");
    assert!(details.contains("log #3 info: message 3"), "{details}");
    assert!(
        details.contains("log #12 info: message 12 (4 dropped before)"),
        "{details}"
    );
}

#[test]
fn hub_counters_show_only_nonzero_protocol_11_counters() {
    assert_eq!(
        hub_counters_text(&DaemonObservabilityCounters::default()),
        None
    );
    let mut counters = DaemonObservabilityCounters::default();
    counters.event_stage_overlaps = 1;
    counters.package_quarantines_not_durable = 2;
    assert_eq!(
        hub_counters_text(&counters).as_deref(),
        Some("hub counters: event_stage_overlaps=1 package_quarantines_not_durable=2")
    );
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
        botster_terminal_protocol_client::FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY.to_string(),
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
    assert!(rendered.contains("features sessions,terminal_streaming,resize,package_navigation"));
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
fn not_running_path_is_not_reported_as_compatibility_mismatch() {
    let mut app = TuiApp::new(None);

    app.apply_link_failure(DaemonTransportError::NotRunning);

    let (lines, _) = renderer::render_to_lines(&app.surface(), 120, 48);
    let rendered = lines.join("\n");
    assert!(rendered.contains("hub unavailable; reconnecting"));
    assert!(!rendered.contains("compatibility mismatch"));
}
