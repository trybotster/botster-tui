use super::*;

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
            request_id: botster_ui_contract::UiActionRequestId("req-terminal-focus".to_string()),
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
fn detached_title_appears_only_after_a_confirmed_detach_response() {
    let reply = |route: &str| PendingReply::Detach {
        session_id: "session-alpha".to_string(),
        route: route.to_string(),
    };
    let mut app = workspace_fixture();
    app.attached = None;
    app.selected_session = Some("session-alpha".to_string());

    app.send_bounded_detach("session-alpha".to_string(), "route-1".to_string());
    assert_eq!(app.terminal_title(), "Terminal · session-alpha · detaching");

    // A stale answer for an older route changes nothing.
    app.apply_completion(reply("route-0"), base_response(DaemonResponseKind::Events));
    assert_eq!(app.terminal_title(), "Terminal · session-alpha · detaching");

    app.apply_completion(reply("route-1"), base_response(DaemonResponseKind::Events));
    assert_eq!(app.terminal_title(), "Terminal · session-alpha · detached");

    // An operator error is a failed detach, never a release.
    app.send_bounded_detach("session-alpha".to_string(), "route-2".to_string());
    app.apply_completion(reply("route-2"), operator_error_response("detach refused"));
    assert_eq!(
        app.terminal_title(),
        "Terminal · session-alpha · detach failed"
    );
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("detach refused"))
    );

    // So is expiry.
    app.send_bounded_detach("session-alpha".to_string(), "route-3".to_string());
    app.apply_request_failure(reply("route-3"), DaemonRequestError::DeadlineExpired);
    assert_eq!(
        app.terminal_title(),
        "Terminal · session-alpha · detach failed"
    );
}

#[test]
fn a_new_running_session_is_never_attached_without_activation() {
    let mut app = workspace_fixture();
    app.attached = None;
    app.session_entities
        .begin_generation("sessions".to_string());
    let entity =
        |id: &str| serde_json::to_value(session_entity(id, Some("running"))).expect("session json");
    app.apply_entity_frame(DaemonEntityFrame::Snapshot {
        subscription_id: "sessions".to_string(),
        entity_type: "session".to_string(),
        snapshot_seq: 1,
        items: vec![entity("session-stale")],
        resync_reason: None,
    });
    app.apply_entity_frame(DaemonEntityFrame::Upsert {
        subscription_id: "sessions".to_string(),
        entity_type: "session".to_string(),
        snapshot_seq: 2,
        id: "session-fresh".to_string(),
        entity: entity("session-fresh"),
    });
    assert!(
        app.sessions
            .iter()
            .any(|session| session.session_id == "session-fresh"),
        "the fresh session is listed"
    );
    assert!(
        !app.observed_requests
            .iter()
            .any(|request| matches!(request, ObservedRequest::Attach { .. })),
        "listing a session must not submit an Attach: {:?}",
        app.observed_requests
    );
    assert!(app.attach_hydration.is_none());
}

#[test]
fn reconnect_clears_transient_notice_and_event_subscription_state() {
    let mut app = workspace_fixture();
    activate_notice(&mut app, "old-sub", 5_000, "session-alpha");
    app.transient_notice = Some(TransientNotice {
        text: "old".to_string(),
        // timer: ui-lifetime — test notice lifetime
        deadline: Instant::now() + Duration::from_secs(5),
    });
    app.force_reconnect();
    assert!(app.notice_subscriptions.is_empty());
    assert!(app.notice_subscription_by_id.is_empty());
    assert!(app.transient_notice.is_none());
    assert!(!rendered_workspace(&app).contains("old"));
}
