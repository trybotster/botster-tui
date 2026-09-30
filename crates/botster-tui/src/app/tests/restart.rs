use super::*;

/// A workspace whose selected session ended and is restartable.
fn ended_restartable_app() -> TuiApp {
    let mut app = workspace_fixture();
    let beta = app
        .sessions
        .iter_mut()
        .find(|row| row.session_id == "session-beta")
        .expect("beta row");
    beta.restartable = true;
    app.selected_session = Some("session-beta".to_string());
    app
}

#[test]
fn restart_is_offered_only_when_the_hub_offers_it_and_the_session_is_restartable() {
    let mut app = ended_restartable_app();
    assert!(
        !rendered_workspace(&app).contains("[ Restart ]"),
        "Hub offer missing"
    );
    app.hub_offers_restart = true;
    assert!(rendered_workspace(&app).contains("[ Restart ]"));
    // A session the Hub cannot restart gets no button.
    app.sessions
        .iter_mut()
        .find(|row| row.session_id == "session-beta")
        .expect("beta row")
        .restartable = false;
    assert!(
        !rendered_workspace(&app).contains("[ Restart ]"),
        "not restartable"
    );
    // A running session is not restartable either.
    app.selected_session = Some("session-alpha".to_string());
    assert!(!rendered_workspace(&app).contains("[ Restart ]"));
}

#[test]
fn the_hello_feature_list_decides_whether_the_hub_offers_restart() {
    let mut compatibility = DaemonCompatibility::current();
    compatibility
        .features
        .retain(|f| f != FEATURE_SESSION_RESTART);
    assert!(!host_offers_restart(&compatibility));
    compatibility
        .features
        .push(FEATURE_SESSION_RESTART.to_string());
    assert!(host_offers_restart(&compatibility));
}

#[test]
fn restart_sends_the_request_shows_restarting_and_clears_on_the_answer() {
    let mut app = ended_restartable_app();
    app.hub_offers_restart = true;
    app.observed_requests.clear();
    app.handle_action(
        "botster.tui.session.restart".to_string(),
        None,
        Some(json!({ "session_id": "session-beta" })),
    );
    assert!(
        app.observed_requests.iter().any(
            |request| matches!(request, ObservedRequest::RestartSession(id) if id == "session-beta")
        ),
        "{:?}",
        app.observed_requests
    );
    assert!(app.restarting_sessions.contains("session-beta"));
    let rendered = rendered_workspace(&app);
    assert!(rendered.contains("session-beta · restarting"), "{rendered}");
    assert!(
        !rendered.contains("[ Restart ]"),
        "no second restart while one runs"
    );
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("restart request pending")
        .0;
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Ok(Box::new(base_response(DaemonResponseKind::Spawned))),
    });
    assert!(app.restarting_sessions.is_empty());
    assert!(app.error.is_none(), "{:?}", app.error);
    assert!(
        app.action_feedback
            .as_deref()
            .is_some_and(|text| text.contains("restart accepted: session-beta")),
        "{:?}",
        app.action_feedback
    );
}

#[test]
fn a_refused_restart_shows_the_hubs_code_and_what_to_do() {
    let mut app = ended_restartable_app();
    app.hub_offers_restart = true;
    app.handle_action(
        "botster.tui.session.restart".to_string(),
        None,
        Some(json!({ "session_id": "session-beta" })),
    );
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("restart request pending")
        .0;
    let mut refusal = operator_error_response("the previous process group is alive");
    refusal.error.as_mut().expect("error").code = "restart_not_ready".to_string();
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Ok(Box::new(refusal)),
    });
    assert!(app.restarting_sessions.is_empty());
    let error = app.error.as_deref().expect("a refusal is shown");
    assert!(error.contains("restart_not_ready"), "{error}");
    assert!(error.contains("retry shortly"), "{error}");
}

#[test]
fn restart_without_the_hub_offer_sends_nothing() {
    let mut app = ended_restartable_app();
    app.observed_requests.clear();
    app.handle_action(
        "botster.tui.session.restart".to_string(),
        None,
        Some(json!({ "session_id": "session-beta" })),
    );
    assert!(
        app.observed_requests.is_empty(),
        "{:?}",
        app.observed_requests
    );
    assert!(app.restarting_sessions.is_empty());
    assert!(app.error.is_some());
}
