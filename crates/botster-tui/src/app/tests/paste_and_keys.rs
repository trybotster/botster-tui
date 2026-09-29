use super::*;

#[test]
fn quit_keys_match_documented_exit_path() {
    assert!(should_quit(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(should_quit(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::NONE
    )));
    assert!(should_quit(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL
    )));
    assert!(!should_quit(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::NONE
    )));
}

#[test]
fn terminal_paste_before_attach_renders_stream_unavailable_error() {
    let mut app = TuiApp::new(None);
    app.sessions = vec![SessionRow::running("session-alpha")];
    app.selected_session = Some("session-alpha".to_string());
    app.status = "connected".to_string();
    let mut router = InputRouter::new(renderer::action_request_context());
    let (_, hit_map) = render_app_to_lines(&app, 120, 48, &router.render_state());
    focus_hit_map_node_by_tab(&mut router, &hit_map, "tui-terminal");

    assert!(route_input_event(
        &mut app,
        &mut router,
        &hit_map,
        Event::Paste("x".to_string()),
    ));

    let (lines, _) = render_app_to_lines(&app, 120, 48, &router.render_state());
    let rendered = lines.join("\n");
    assert!(rendered.contains("terminal stream unavailable"));
    assert!(app.observed_terminal_inputs.is_empty());
}

#[test]
fn unsafe_paste_consent_requires_an_exact_zero_byte_rejection() {
    let (mut app, operation_id) = paste_awaiting_result(b"echo one\necho two\n");
    apply_paste_result(
        &mut app,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    assert!(matches!(
        app.pending_unsafe_paste,
        Some(PendingUnsafePaste::AwaitingConsent {
            stage: UnsafePasteConsentStage::Review,
            ..
        })
    ));
    assert_eq!(app.input_window.retained_bytes(), 18);

    for (outcome, accepted, written) in [
        (InputOutcome::PartialWrite, Some(18), Some(1)),
        (InputOutcome::OutcomeUnknown, None, None),
        (InputOutcome::RejectedUnsafePaste, None, Some(0)),
        (InputOutcome::RejectedUnsafePaste, Some(1), Some(0)),
    ] {
        let (mut app, operation_id) = paste_awaiting_result(b"echo one\necho two\n");
        apply_paste_result(&mut app, operation_id, outcome, accepted, written);
        assert!(app.pending_unsafe_paste.is_none());
        assert_eq!(app.input_window.retained_bytes(), 0);
    }
}

#[test]
fn unsafe_paste_confirmation_uses_a_new_id_once() {
    let (mut app, operation_id) = paste_awaiting_result(b"echo one\necho two\n");
    apply_paste_result(
        &mut app,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    app.handle_action("botster.tui.unsafe_paste.review".to_string(), None, None);
    app.handle_action("botster.tui.unsafe_paste.confirm".to_string(), None, None);
    let paste_begins = app
        .observed_terminal_inputs
        .iter()
        .filter_map(|command| match command {
            TerminalInputCommand::PasteBegin {
                operation_id,
                allow_unsafe,
                ..
            } => Some((*operation_id, *allow_unsafe)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(paste_begins, vec![(1, false), (2, true)]);
    assert!(app.pending_unsafe_paste.is_none());
    assert_eq!(app.input_window.retained_bytes(), 0);

    app.handle_action("botster.tui.unsafe_paste.confirm".to_string(), None, None);
    assert_eq!(app.observed_terminal_inputs.len(), 6);
}

#[test]
fn unsafe_paste_confirmation_refusal_releases_the_payload() {
    let payload = vec![b'\n'; botster_terminal_protocol_client::MAX_PASTE_BYTES];
    let (mut app, operation_id) = paste_awaiting_result(&payload);
    apply_paste_result(
        &mut app,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    for _ in 0..crate::terminal_input::MAX_IN_FLIGHT_OPERATIONS {
        let id = app.input_window.next_operation_id().expect("id");
        app.input_window
            .admit(id, false, vec![vec![1]])
            .expect("fill the input window");
    }
    app.handle_action("botster.tui.unsafe_paste.review".to_string(), None, None);
    app.handle_action("botster.tui.unsafe_paste.confirm".to_string(), None, None);
    assert!(app.pending_unsafe_paste.is_none());
    assert_eq!(app.input_window.retained_bytes(), 0);
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("client bound"))
    );
}

#[test]
fn unsafe_paste_cancel_expiry_and_resync_drop_only_the_retry() {
    let (mut cancelled, operation_id) = paste_awaiting_result(b"one\ntwo");
    apply_paste_result(
        &mut cancelled,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    cancelled.handle_action("botster.tui.unsafe_paste.cancel".to_string(), None, None);
    assert!(cancelled.pending_unsafe_paste.is_none());
    assert_eq!(cancelled.input_window.retained_bytes(), 0);

    let (mut expired, operation_id) = paste_awaiting_result(b"one\ntwo");
    apply_paste_result(
        &mut expired,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    if let Some(PendingUnsafePaste::AwaitingConsent { deadline, .. }) =
        expired.pending_unsafe_paste.as_mut()
    {
        *deadline = Instant::now();
    }
    expired.prepare_paint();
    assert!(expired.pending_unsafe_paste.is_none());
    assert_eq!(expired.input_window.retained_bytes(), 0);

    let (mut resync, operation_id) = paste_awaiting_result(b"one\ntwo");
    apply_paste_result(
        &mut resync,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    let key_id = resync.input_window.next_operation_id().expect("id");
    resync
        .input_window
        .admit(key_id, false, vec![vec![1]])
        .expect("unrelated input remains admitted");
    resync.begin_route_resync();
    assert!(resync.pending_unsafe_paste.is_none());
    assert_eq!(resync.input_window.in_flight_len(), 1);
    assert_eq!(resync.input_window.retained_bytes(), 0);
}

#[test]
fn a_new_paste_replaces_consent_without_replaying_the_old_payload() {
    let (mut app, operation_id) = paste_awaiting_result(b"old\npayload");
    apply_paste_result(
        &mut app,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    assert!(app.handle_focused_terminal_paste("new\npayload", Some("tui-terminal")));
    assert!(matches!(
        app.pending_unsafe_paste,
        Some(PendingUnsafePaste::AwaitingResult {
            operation_id: 2,
            ..
        })
    ));
    let paste_begins = app
        .observed_terminal_inputs
        .iter()
        .filter_map(|command| match command {
            TerminalInputCommand::PasteBegin {
                operation_id,
                allow_unsafe,
                ..
            } => Some((*operation_id, *allow_unsafe)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(paste_begins, vec![(1, false), (2, false)]);
}

#[test]
fn host_keys_are_the_reserved_set_and_exclude_shift_tab() {
    let key = |code, modifiers| KeyEvent::new(code, modifiers);
    let control = KeyModifiers::CONTROL;
    let shift = KeyModifiers::SHIFT;
    assert_eq!(
        host_key(key(KeyCode::Char('p'), control)),
        Some(HostKey::Menu)
    );
    assert_eq!(
        host_key(key(KeyCode::Char('j'), control)),
        Some(HostKey::NextSession)
    );
    assert_eq!(
        host_key(key(KeyCode::Char('k'), control)),
        Some(HostKey::PreviousSession)
    );
    for (code, scroll) in [
        (KeyCode::PageUp, HostScroll::PageUp),
        (KeyCode::PageDown, HostScroll::PageDown),
        (KeyCode::Home, HostScroll::Top),
        (KeyCode::End, HostScroll::Bottom),
    ] {
        assert_eq!(host_key(key(code, shift)), Some(HostKey::Scroll(scroll)));
    }
    for free in [
        key(KeyCode::BackTab, shift),
        key(KeyCode::Tab, KeyModifiers::NONE),
        key(KeyCode::Char('p'), KeyModifiers::NONE),
        key(KeyCode::Char('c'), control),
        key(KeyCode::PageUp, KeyModifiers::NONE),
    ] {
        assert_eq!(host_key(free), None, "{free:?} belongs to the session");
    }
}

#[test]
fn reserved_chord_releases_are_consumed_without_acting() {
    let (mut app, mut router, map) = attached_workspace_with_focused_terminal();
    for (code, modifiers) in [
        (KeyCode::PageUp, KeyModifiers::SHIFT),
        (KeyCode::Char('p'), KeyModifiers::CONTROL),
        (KeyCode::Char('j'), KeyModifiers::CONTROL),
    ] {
        let mut release = KeyEvent::new(code, modifiers);
        release.kind = KeyEventKind::Release;
        assert!(route_input_event(
            &mut app,
            &mut router,
            &map,
            Event::Key(release)
        ));
    }
    assert!(app.observed_terminal_inputs.is_empty());
    assert_eq!(router.focused_node_id(), Some("tui-terminal"));
    assert_eq!(app.selected_session.as_deref(), Some("session-alpha"));
}

#[test]
fn ctrl_p_moves_focus_from_the_terminal_to_the_toolbar() {
    let (mut app, mut router, map) = attached_workspace_with_focused_terminal();
    let ctrl_p = Event::Key(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));

    assert!(route_input_event(&mut app, &mut router, &map, ctrl_p));

    assert_eq!(router.focused_node_id(), Some(WORKSPACE_MENU_NODE));
    assert!(app.observed_terminal_inputs.is_empty());
    let q = Event::Key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
    assert!(
        !route_input_event(&mut app, &mut router, &map, q),
        "after Ctrl+P a keyboard user can quit"
    );
}

#[test]
fn ctrl_j_and_ctrl_k_select_sessions_without_attaching_or_typing() {
    let (mut app, mut router, map) = attached_workspace_with_focused_terminal();
    let chord = |c| Event::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));

    assert!(route_input_event(&mut app, &mut router, &map, chord('j')));
    assert_eq!(app.selected_session.as_deref(), Some("session-beta"));
    assert_eq!(router.focused_node_id(), Some("tui-session-session-beta"));
    assert!(route_input_event(&mut app, &mut router, &map, chord('j')));
    assert_eq!(app.selected_session.as_deref(), Some("session-gamma"));
    assert!(route_input_event(&mut app, &mut router, &map, chord('k')));
    assert!(route_input_event(&mut app, &mut router, &map, chord('k')));
    assert_eq!(app.selected_session.as_deref(), Some("session-alpha"));
    assert!(route_input_event(&mut app, &mut router, &map, chord('k')));
    assert_eq!(
        app.selected_session.as_deref(),
        Some("session-gamma"),
        "wraps"
    );

    assert_eq!(
        app.attached_session_id(),
        Some("session-alpha"),
        "selection never attaches"
    );
    assert!(app.observed_terminal_inputs.is_empty());
}

#[test]
fn unshifted_page_and_ctrl_home_end_keys_reach_the_focused_session() {
    let (mut app, mut router, map) = attached_workspace_with_focused_terminal();
    // With a projection installed these keys used to scroll it instead.
    app.ensure_ghostty_projection("session-alpha");
    assert!(app.ghostty_projection.is_some());
    for (code, modifiers) in [
        (KeyCode::PageUp, KeyModifiers::NONE),
        (KeyCode::PageDown, KeyModifiers::NONE),
        (KeyCode::Home, KeyModifiers::CONTROL),
        (KeyCode::End, KeyModifiers::CONTROL),
    ] {
        let event = Event::Key(KeyEvent::new(code, modifiers));
        assert!(route_input_event(&mut app, &mut router, &map, event));
    }
    assert_eq!(
        app.observed_terminal_inputs.len(),
        4,
        "each key is forwarded to the session as input"
    );
    assert_eq!(router.focused_node_id(), Some("tui-terminal"));
}

#[test]
fn shift_page_keys_scroll_and_never_reach_the_session() {
    let (mut app, mut router, map) = attached_workspace_with_focused_terminal();
    for code in [
        KeyCode::PageUp,
        KeyCode::PageDown,
        KeyCode::Home,
        KeyCode::End,
    ] {
        let event = Event::Key(KeyEvent::new(code, KeyModifiers::SHIFT));
        assert!(route_input_event(&mut app, &mut router, &map, event));
    }
    assert!(app.observed_terminal_inputs.is_empty());
    assert_eq!(router.focused_node_id(), Some("tui-terminal"));
}

#[test]
fn queued_enter_cannot_confirm_unsafe_paste() {
    let (mut app, operation_id) = paste_awaiting_result(b"SECRET_UNSAFE_PASTE\nSECOND_SECRET_LINE");
    let mut router = InputRouter::new(renderer::action_request_context());
    let (_, workspace_map) = render_app_to_lines(&app, 96, 30, &router.render_state());
    focus_hit_map_node_by_tab(&mut router, &workspace_map, "tui-terminal");
    apply_paste_result(
        &mut app,
        operation_id,
        InputOutcome::RejectedUnsafePaste,
        Some(0),
        Some(0),
    );
    let (review_lines, review_map) = render_app_to_lines(&app, 96, 30, &router.render_state());
    assert!(!review_lines.join("\n").contains("SECRET_UNSAFE_PASTE"));
    app.apply_wake(routed(
        "route-1",
        7,
        botster_terminal_protocol_client::encode_output(b"terminal output continues")
            .expect("output frame"),
    ));
    assert_eq!(
        app.applied_live_payloads,
        vec![b"terminal output continues"]
    );
    router.reconcile(&review_map);
    assert_eq!(
        router.focused_node_id(),
        Some("workspace-unsafe-paste-primary")
    );
    let enter = || Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(route_input_event(
        &mut app,
        &mut router,
        &review_map,
        enter()
    ));
    assert!(route_input_event(
        &mut app,
        &mut router,
        &review_map,
        enter()
    ));
    assert!(matches!(
        app.pending_unsafe_paste,
        Some(PendingUnsafePaste::AwaitingConsent {
            stage: UnsafePasteConsentStage::Armed,
            ..
        })
    ));
    assert_eq!(app.observed_terminal_inputs.len(), 3);

    let (_, armed_map) = render_app_to_lines(&app, 96, 30, &router.render_state());
    router.reconcile(&armed_map);
    assert_eq!(
        router.focused_node_id(),
        Some("workspace-unsafe-paste-primary")
    );
    assert!(route_input_event(
        &mut app,
        &mut router,
        &armed_map,
        enter()
    ));
    assert!(app.pending_unsafe_paste.is_none());
    assert_eq!(app.observed_terminal_inputs.len(), 3);
}
