use super::*;

#[test]
fn generation_comes_only_from_the_attach_response() {
    let mut app = workspace_fixture();
    app.begin_attach_hydration("session-alpha", "route-1");
    // A frame before the response never sets the reservation, and is dropped:
    // the route's socket opens after the response.
    app.apply_wake(routed(
        "route-1",
        9,
        attach_state_frame(AttachStateCode::Attached),
    ));
    assert_eq!(app.route_generation, None);
    assert_eq!(app.route_epoch, None);
    complete_attach(&mut app, "session-alpha", "route-1", 4);
    assert_eq!(app.route_generation, Some(4));
    app.apply_wake(routed(
        "route-1",
        4,
        attach_state_frame(AttachStateCode::Attached),
    ));
    app.apply_wake(routed("route-1", 4, modes_frame(mode_bits::MOUSE_NORMAL)));
    assert_eq!(app.route_epoch, Some(0));
    let hydration = app.attach_hydration.as_ref().expect("campaign continues");
    assert!(hydration.attached_seen);
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
fn route_resync_snapshot_replaces_the_screen_and_reopens_the_live_path() {
    let mut app = workspace_fixture();
    app.begin_attach_hydration("session-alpha", "route-1");
    complete_attach(&mut app, "session-alpha", "route-1", 1);
    app.apply_wake(routed(
        "route-1",
        1,
        attach_state_frame(AttachStateCode::Attached),
    ));
    for frame in ghostsnp_snapshot_frames("OLD-SCREEN-MARKER\r\n") {
        app.apply_wake(routed("route-1", 1, frame));
    }
    assert_eq!(app.attached_session_id(), Some("session-alpha"));
    assert!(projection_text(&mut app).contains("OLD-SCREEN-MARKER"));

    // A stalled bound route resyncs: the old screen is dropped at once.
    app.apply_wake(routed_at_epoch("route-1", 1, 1, resync_frame(0, 1)));
    assert!(app.ghostty_projection.is_none());
    assert!(app.attached.is_none());

    for frame in ghostsnp_snapshot_frames("NEW-SCREEN-MARKER\r\n") {
        app.apply_wake(routed_at_epoch("route-1", 1, 1, frame));
    }
    assert_eq!(
        app.attached_session_id(),
        Some("session-alpha"),
        "the live path reopens after the replacement snapshot"
    );
    let screen = projection_text(&mut app);
    assert!(screen.contains("NEW-SCREEN-MARKER"), "{screen}");
    assert!(!screen.contains("OLD-SCREEN-MARKER"), "{screen}");

    app.apply_wake(routed_at_epoch(
        "route-1",
        1,
        1,
        botster_terminal_protocol_client::encode_output(b"LIVE-AFTER-RESYNC")
            .expect("output frame"),
    ));
    assert!(projection_text(&mut app).contains("LIVE-AFTER-RESYNC"));
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
        app.action_feedback
            .as_deref()
            .is_some_and(|feedback| feedback.contains("1 terminal input operation(s) unresolved"))
    );
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
fn a_completed_recovery_restores_one_recovery_for_the_next_close() {
    let mut app = workspace_fixture();
    app.begin_attach_hydration("session-alpha", "route-1");
    hydrate_fully(&mut app, "route-1", 1);
    assert!(app.attached_matches_route("route-1"));

    close_route(&mut app, "route-1", 1);
    assert!(app.attach_recovery_used);
    let second = replacement_route(&app);
    hydrate_fully(&mut app, &second, 2);
    assert!(app.attached_matches_route(&second));
    assert!(
        !app.attach_recovery_used,
        "a completed recovery restores the allowance"
    );
    assert_eq!(
        app.recovery_notice,
        Some(RecoveryNotice {
            count: 1,
            cause: "core_adapter_closed".to_string()
        })
    );
    // Sends set or clear the error line; they never touch the notice.
    app.send_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(app.recovery_notice.is_some());

    // An independent later close recovers again instead of failing closed.
    close_route(&mut app, &second, 2);
    let third = replacement_route(&app);
    assert_ne!(third, second);
    assert!(app.attach_recovery_used);
    assert!(
        !app.error
            .as_deref()
            .is_some_and(|error| error.contains("failed closed after recovery"))
    );
    hydrate_fully(&mut app, &third, 3);
    let rendered = render_app_to_lines(&app, 140, 42, &RenderState::default())
        .0
        .join("\n");
    assert!(
        rendered.contains("reconnected 2× after core_adapter_closed"),
        "{rendered}"
    );

    // A user detach ends the attachment and its notice.
    app.detach_attached();
    assert_eq!(app.recovery_notice, None);
}

#[test]
fn a_resync_during_a_recovery_keeps_it_a_recovery() {
    let mut app = workspace_fixture();
    app.begin_attach_hydration("session-alpha", "route-1");
    hydrate_fully(&mut app, "route-1", 1);
    close_route(&mut app, "route-1", 1);
    let second = replacement_route(&app);
    complete_attach(&mut app, "session-alpha", &second, 2);
    app.apply_wake(routed(
        &second,
        2,
        attach_state_frame(AttachStateCode::Attached),
    ));
    let ready = ghostsnp_snapshot_frames("SCREEN\r\n")
        .into_iter()
        .next()
        .expect("READY frame");
    app.apply_wake(routed(&second, 2, ready));

    // The recovery's route resyncs before its snapshot finishes.
    app.apply_wake(routed_at_epoch(&second, 2, 1, resync_frame(0, 1)));
    assert!(
        app.attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.recovery_cause.is_some()),
        "the resync hydration keeps the recovery cause"
    );
    for frame in ghostsnp_snapshot_frames("SCREEN\r\n") {
        app.apply_wake(routed_at_epoch(&second, 2, 1, frame));
    }
    assert!(app.attached_matches_route(&second));
    assert!(
        !app.attach_recovery_used,
        "the completed recovery restores it"
    );
    assert_eq!(
        app.recovery_notice,
        Some(RecoveryNotice {
            count: 1,
            cause: "core_adapter_closed".to_string()
        }),
        "counted once"
    );

    // A later independent close recovers.
    close_route(&mut app, &second, 2);
    assert_ne!(replacement_route(&app), second);
    assert!(
        !app.error
            .as_deref()
            .is_some_and(|error| error.contains("failed closed after recovery"))
    );
}

#[test]
fn a_worker_lost_close_ends_the_route_without_a_recovery() {
    let mut app = workspace_fixture();
    app.begin_attach_hydration("session-alpha", "route-1");
    hydrate_fully(&mut app, "route-1", 1);
    app.handle_terminal_subscription_closed(
        "session-alpha".to_string(),
        "route-1".to_string(),
        1,
        "worker_lost".to_string(),
    );
    assert!(app.attached.is_none());
    assert!(app.attach_hydration.is_none(), "no re-attach is attempted");
    assert!(!app.attach_recovery_used, "the recovery is not spent");
    assert!(app.recovery_notice.is_none());
    assert!(app.retired_subscription_ids.contains("route-1"));
    assert_eq!(
        app.error.as_deref(),
        Some("session session-alpha crashed: its worker was lost (worker_lost)")
    );
}

#[test]
fn a_crashed_session_renders_distinctly_from_an_exited_one() {
    let mut app = workspace_fixture();
    let alpha = app
        .sessions
        .iter_mut()
        .find(|row| row.session_id == "session-alpha")
        .expect("alpha row");
    alpha.lifecycle = "failed".to_string();
    alpha.failure_reason = Some("worker_lost".to_string());
    assert!(!alpha.is_attachable());

    let rendered = render_app_to_lines(&app, 140, 42, &RenderState::default())
        .0
        .join("\n");
    assert!(rendered.contains("session-alpha · crashed"), "{rendered}");
    assert!(!rendered.contains("· worker_lost"), "{rendered}");
    assert!(rendered.contains("session-beta · exited"), "{rendered}");
    assert!(
        rendered.contains("Terminal · session-alpha · crashed"),
        "{rendered}"
    );
    assert!(
        rendered.contains("This session crashed: its worker was lost."),
        "{rendered}"
    );
    assert!(
        rendered.contains("[ Remove ]"),
        "the crashed session can be removed"
    );

    // Another failure reason is not a crash.
    let alpha = app
        .sessions
        .iter_mut()
        .find(|row| row.session_id == "session-alpha")
        .expect("alpha row");
    alpha.failure_reason = Some("spawn_failed".to_string());
    let rendered = render_app_to_lines(&app, 140, 42, &RenderState::default())
        .0
        .join("\n");
    assert!(rendered.contains("session-alpha · failed"), "{rendered}");
    assert!(!rendered.contains("crashed"), "{rendered}");
}

#[test]
fn a_not_attached_refusal_says_nothing_reached_the_session() {
    let mut app = workspace_fixture();
    let mut response = base_response(DaemonResponseKind::OperatorError);
    response.error = Some(botster_hub_client::DaemonOperatorError {
        code: "not_attached".to_string(),
        request_id: "request".to_string(),
        operation: "resize".to_string(),
        message: "client c is not subscribed to session session-alpha".to_string(),
        diagnostics: Vec::new(),
    });
    app.apply_completion(PendingReply::Apply, response);
    assert_eq!(
        app.error.as_deref(),
        Some(
            "not attached: client c is not subscribed to session session-alpha (operation=resize); nothing reached the session"
        )
    );
}

#[test]
fn a_recovery_that_reaches_only_ready_keeps_the_allowance_spent() {
    let mut app = workspace_fixture();
    app.begin_attach_hydration("session-alpha", "route-1");
    hydrate_fully(&mut app, "route-1", 1);
    close_route(&mut app, "route-1", 1);
    let second = replacement_route(&app);
    complete_attach(&mut app, "session-alpha", &second, 2);
    app.apply_wake(routed(
        &second,
        2,
        attach_state_frame(AttachStateCode::Attached),
    ));
    // READY only: the snapshot never finishes, so the attach never opens.
    let ready = ghostsnp_snapshot_frames("SCREEN\r\n")
        .into_iter()
        .next()
        .expect("READY frame");
    app.apply_wake(routed(&second, 2, ready));
    assert!(
        app.attach_hydration
            .as_ref()
            .is_some_and(|h| h.snapshot_ready)
    );
    assert!(app.attached.is_none());
    assert!(app.attach_recovery_used, "READY alone restores nothing");
    assert_eq!(app.recovery_notice, None, "READY alone is no recovery");

    close_route(&mut app, &second, 2);
    assert!(app.attach_hydration.is_none(), "no second recovery");
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

#[test]
fn released_queued_frames_are_sent_on_the_same_attachment_during_resync() {
    let mut app = workspace_fixture();
    app.hub_io.capture_terminal_frames();
    app.begin_attach_hydration("session-alpha", "route-1");
    complete_attach(&mut app, "session-alpha", "route-1", 3);
    app.apply_wake(routed(
        "route-1",
        3,
        attach_state_frame(AttachStateCode::Attached),
    ));
    app.attached = Some(AttachedRoute {
        session_id: "session-alpha".to_string(),
        route: "route-1".to_string(),
    });
    app.attach_hydration = None;
    for _ in 0..crate::terminal_input::MAX_IN_FLIGHT_OPERATIONS {
        let id = app.input_window.next_operation_id().expect("id");
        app.input_window
            .admit(id, false, vec![vec![1]])
            .expect("admitted");
    }
    let queued_id = app.input_window.next_operation_id().expect("id");
    app.input_window
        .admit(queued_id, false, vec![vec![0xAB]])
        .expect("queued");
    app.apply_wake(routed_at_epoch("route-1", 3, 1, resync_frame(0, 1)));
    assert!(app.attached.is_none());
    assert!(
        app.attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.resync)
    );
    let result = InputResultBody {
        operation_id: 1,
        outcome: InputOutcome::Written,
        accepted_payload_bytes: Some(1),
        written_pty_bytes: Some(1),
        mode_bits: 0,
        detail: String::new(),
    };
    app.apply_wake(routed_at_epoch(
        "route-1",
        3,
        1,
        botster_terminal_protocol_client::encode_input_result(&result).expect("result frame"),
    ));
    let sent = app.hub_io.take_captured_terminal_frames();
    assert_eq!(
        sent.len(),
        1,
        "the released queued frame is written on the attachment"
    );
    assert_eq!(sent[0].0, "route-1");
    assert_eq!(sent[0].1, 3);
    assert_eq!(sent[0].2, vec![0xAB]);
    assert_eq!(
        app.input_window.in_flight_len(),
        crate::terminal_input::MAX_IN_FLIGHT_OPERATIONS
    );
    assert!(app.error.is_none());
}

#[test]
fn input_results_are_rejected_after_the_attachment_is_lost_or_replaced() {
    let mut app = workspace_fixture();
    app.hub_io.capture_terminal_frames();
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
    let id = app.input_window.next_operation_id().expect("id");
    app.input_window
        .admit(id, false, vec![vec![1]])
        .expect("admitted");
    let result = InputResultBody {
        operation_id: id,
        outcome: InputOutcome::Written,
        accepted_payload_bytes: Some(1),
        written_pty_bytes: Some(1),
        mode_bits: 0,
        detail: String::new(),
    };
    let result_frame =
        || botster_terminal_protocol_client::encode_input_result(&result).expect("result frame");
    // Lost attachment: the route is retired, the operation is resolved as
    // unknown, and a late result for it is dropped without effect.
    app.retire_subscription("route-1");
    app.attached = None;
    app.error = None;
    app.apply_wake(routed("route-1", 1, result_frame()));
    assert_eq!(app.input_window.in_flight_len(), 0);
    assert!(app.error.is_none());
    // Replaced attachment: a fresh campaign before its live path is not a
    // valid target for results or released frames.
    app.begin_attach_hydration("session-alpha", "route-2");
    complete_attach(&mut app, "session-alpha", "route-2", 2);
    app.apply_wake(routed(
        "route-2",
        2,
        attach_state_frame(AttachStateCode::Attached),
    ));
    assert!(!app.attachment_matches_route("route-2"));
    app.apply_wake(routed("route-2", 2, result_frame()));
    assert!(app.error.is_none());
    app.send_encoded_frames(vec![vec![1]]);
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("no attached route"))
    );
    assert!(app.hub_io.take_captured_terminal_frames().is_empty());
}

#[test]
fn drawn_terminal_pane_size_reaches_the_hub_once_per_change() {
    let mut app = workspace_fixture();
    app.hub_io.capture_terminal_frames();
    app.attached = Some(AttachedRoute {
        session_id: "session-alpha".to_string(),
        route: "route-1".to_string(),
    });
    app.subscription_id = "route-1".to_string();
    app.route_generation = Some(7);
    app.route_epoch = Some(0);
    app.terminal_viewport_size = TerminalScreenSize::new(24, 80);
    let router = InputRouter::new(renderer::action_request_context());
    let resizes = |app: &TuiApp| {
        app.observed_terminal_inputs
            .iter()
            .filter_map(|command| match command {
                TerminalInputCommand::Resize { rows, cols, .. } => Some((*rows, *cols)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let pane = |map: &HitMap| {
        let inner = botster_tui_kit::terminal_inner_rect(
            tui_terminal_region(map).expect("terminal pane drawn"),
        );
        (inner.height, inner.width)
    };

    let (_, first) = render_app_to_lines(&app, 140, 40, &router.render_state());
    app.sync_terminal_pane_size(&first);
    app.sync_terminal_pane_size(&first);
    assert_eq!(
        resizes(&app),
        vec![pane(&first)],
        "unchanged size sends nothing"
    );

    let (_, second) = render_app_to_lines(&app, 160, 48, &router.render_state());
    assert_ne!(pane(&first), pane(&second));
    app.sync_terminal_pane_size(&second);
    assert_eq!(resizes(&app), vec![pane(&first), pane(&second)]);
    assert_eq!(
        app.terminal_viewport_size,
        TerminalScreenSize::new(pane(&second).0, pane(&second).1)
    );
}

#[test]
fn refused_resize_keeps_the_local_size_and_retries_on_the_next_draw() {
    let mut app = workspace_fixture();
    app.hub_io.capture_terminal_frames();
    app.attached = Some(AttachedRoute {
        session_id: "session-alpha".to_string(),
        route: "route-1".to_string(),
    });
    app.subscription_id = "route-1".to_string();
    app.route_generation = Some(7);
    app.route_epoch = Some(0);
    let stale = TerminalScreenSize::new(24, 80);
    app.terminal_viewport_size = stale;
    // Fill every in-flight slot, then the local queue to its byte bound.
    let mut in_flight = Vec::new();
    for _ in 0..terminal_input::MAX_IN_FLIGHT_OPERATIONS {
        let id = app.input_window.next_operation_id().expect("operation id");
        app.input_window
            .admit(id, false, vec![vec![0; 8]])
            .expect("in-flight slot");
        in_flight.push(id);
    }
    let id = app.input_window.next_operation_id().expect("operation id");
    app.input_window
        .admit(
            id,
            false,
            vec![vec![0; terminal_input::MAX_QUEUED_INPUT_BYTES]],
        )
        .expect("queue fills to its bound");
    let router = InputRouter::new(renderer::action_request_context());
    let (_, map) = render_app_to_lines(&app, 140, 40, &router.render_state());
    let inner = botster_tui_kit::terminal_inner_rect(
        tui_terminal_region(&map).expect("terminal pane drawn"),
    );
    let pane = TerminalScreenSize::new(inner.height, inner.width);
    assert_ne!(pane, stale);

    app.sync_terminal_pane_size(&map);
    assert_eq!(
        app.terminal_viewport_size, stale,
        "a refused RESIZE is not applied"
    );
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("byte client bound"))
    );

    // An INPUT_RESULT frees a slot: the queued bulk operation moves into
    // flight, and the next draw retries the same size.
    app.input_window.complete(in_flight[0]);
    assert_eq!(app.input_window.queued_bytes(), 0);
    app.sync_terminal_pane_size(&map);
    assert_eq!(app.terminal_viewport_size, pane);
    assert!(
        app.input_window.queued_bytes() > 0,
        "the retried RESIZE is admitted behind in-flight operations"
    );
}
