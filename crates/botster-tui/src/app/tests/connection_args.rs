use super::*;

#[test]
fn parses_typed_hub_connection_and_data_dir() {
    let Ok(ParsedCommand::Run(args)) = AppArgs::parse_with_environment(
        [],
        Some(
            botster_core_test_support::fixtures::runnable_entrypoint_hub_connection::VALID_UNIX_SOCKET_JSON
                .into(),
        ),
        Some("target/hub-data".into()),
    ) else {
        panic!("expected run arguments");
    };

    assert_eq!(
        args.daemon_endpoint().map(|endpoint| endpoint.socket_path),
        Some(PathBuf::from("/var/run/botster/hub.sock"))
    );
    assert_eq!(args.connection_error, None);
    assert_eq!(args.hub_data_dir, Some(PathBuf::from("target/hub-data")));
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
fn missing_hub_connection_is_reported() {
    let Ok(ParsedCommand::Run(args)) = AppArgs::parse_with_environment([], None, None) else {
        panic!("expected run arguments");
    };

    assert_eq!(args.hub_connection, None);
    assert_eq!(args.daemon_endpoint(), None);
    assert_eq!(
        args.connection_error.as_deref(),
        Some("BOTSTER_HUB_CONNECTION is required")
    );
}

#[test]
fn cli_rejects_unknown_options_and_answers_help_and_version() {
    let parse = |args: &[&str]| {
        AppArgs::parse_with_environment(args.iter().map(ToString::to_string), None, None)
    };
    assert_eq!(
        parse(&["--hub-socket", "/tmp/retired.sock"]),
        Err("unknown option: --hub-socket".to_string())
    );
    assert_eq!(parse(&["--help"]), Ok(ParsedCommand::Help));
    assert_eq!(parse(&["-h"]), Ok(ParsedCommand::Help));
    assert_eq!(parse(&["--version"]), Ok(ParsedCommand::Version));
    assert_eq!(parse(&["-V"]), Ok(ParsedCommand::Version));
    assert!(matches!(
        parse(&["--smoke"]),
        Ok(ParsedCommand::Run(AppArgs { smoke: true, .. }))
    ));
}

#[test]
fn missing_hub_connection_renders_connection_diagnostic() {
    let mut app =
        TuiApp::new_with_connection(None, Some("BOTSTER_HUB_CONNECTION is required".to_string()));
    app.connect();

    let (lines, _) = render_app_to_lines(&app, 120, 48, &RenderState::default());
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
fn missing_terminal_snapshot_delivery_on_hello_ack_fails_before_attach() {
    let mut compatibility = TerminalCompatibility::current();
    compatibility.features.retain(|feature| {
        feature != botster_terminal_protocol_client::FEATURE_SNAPSHOT_DELIVERY_READY_THEN_HISTORY
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
        terminal_generation: None,
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
        terminal_generation: None,
    };
    admit_terminal_hello(&ack).expect_err("omitted terminal_compatibility must fail before Attach");
}

#[test]
fn pinned_session_plugin_binding_fixture_is_conformance_52() {
    let scenario = botster_hub_test_support::session_plugin_binding_conformance_scenario();
    assert_eq!(
        scenario.conformance_fixture_revision, 52,
        "hub-test-support pin must publish fixture revision 52"
    );
    assert!(scenario.conformance_fixture_revision >= MINIMUM_CONFORMANCE_FIXTURE_REVISION);
}

#[test]
fn tui_requires_package_event_subscriptions_at_floor_50() {
    let requirement = tui_compatibility_requirement();
    assert_eq!(requirement.minimum_conformance_fixture_revision, 50);
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
