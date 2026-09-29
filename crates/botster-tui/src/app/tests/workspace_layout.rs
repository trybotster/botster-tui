use super::*;

#[test]
fn smoke_message_names_the_workspace() {
    assert_eq!(smoke_message(), "botster-tui smoke ok");
}

#[test]
fn workspaces_profile_is_explicit_and_ledgers_fail_closed() {
    assert_eq!(
        WorkspacesProfile::parse("plumbing"),
        Ok(WorkspacesProfile::Plumbing)
    );
    assert_eq!(
        WorkspacesProfile::parse("lifecycle"),
        Ok(WorkspacesProfile::Lifecycle)
    );
    assert!(WorkspacesProfile::parse("").is_err());
    assert!(WorkspacesProfile::parse("auto").is_err());

    for profile in [WorkspacesProfile::Plumbing, WorkspacesProfile::Lifecycle] {
        let required = match profile {
            WorkspacesProfile::Plumbing => WorkspacesStage::plumbing().to_vec(),
            WorkspacesProfile::Lifecycle => WorkspacesStage::lifecycle(),
        };
        let omitted = required[required.len() / 2];
        let mut ledger = WorkspacesLedger::new(profile);
        for stage in required {
            if stage != omitted {
                ledger.record(stage);
            }
        }
        let error = ledger
            .assert_complete()
            .expect_err("each profile must reject an incomplete ledger");
        assert!(error.contains(&format!("{omitted:?}")), "{error}");
    }
}

#[test]
fn workspaces_lifecycle_ledger_is_a_strict_plumbing_superset() {
    let plumbing = WorkspacesStage::plumbing()
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    let lifecycle = WorkspacesStage::lifecycle()
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    assert!(plumbing.is_subset(&lifecycle));
    assert!(lifecycle.len() > plumbing.len());
}

#[test]
fn workspaces_reference_order_uses_realized_traversal_not_prop_text() {
    let mut group = node(UiNodeKind::Stack, "group", json!({}));
    let mut first_wrapper = node(UiNodeKind::Stack, "first-wrapper", json!({}));
    first_wrapper
        .children
        .push(child(node(UiNodeKind::Text, "first-root", json!({}))));
    let mut second_wrapper = node(UiNodeKind::Stack, "second-wrapper", json!({}));
    second_wrapper
        .children
        .push(child(node(UiNodeKind::Text, "second-root", json!({}))));
    group.children = vec![child(first_wrapper), child(second_wrapper)];
    let mut materialized = node(
        UiNodeKind::Stack,
        "surface",
        json!({ "producer_metadata": "second-root" }),
    );
    materialized.children.push(child(group));

    assert_realized_roots_follow_reference_order(
        &materialized,
        ["first-root".to_string(), "second-root".to_string()],
    );
}

#[test]
fn workspace_uses_semantic_widths_for_wide_regular_and_compact_layouts() {
    let app = workspace_fixture();

    for (width, height, horizontal) in [
        (240, 50, true),
        (140, 42, true),
        (96, 30, true),
        (72, 24, false),
    ] {
        let (lines, hit_map) = render_app_to_lines(&app, width, height, &RenderState::default());
        let rendered = lines.join("\n");
        let navigator = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "workspace-session-navigator")
            .expect("session navigator should render");
        let terminal = hit_map
            .regions()
            .iter()
            .find(|region| region.node_id == "tui-terminal")
            .expect("focused terminal should render");

        if width >= 120 {
            assert!(rendered.contains("Botster · Hub: connected"), "{rendered}");
        } else {
            assert!(rendered.contains("Botster · connected"), "{rendered}");
        }
        if width == 72 {
            assert!(!rendered.contains("Selected:"), "{rendered}");
        } else {
            assert!(rendered.contains("Selected: session-alpha"), "{rendered}");
        }
        assert!(!rendered.contains("protocol:"), "{rendered}");
        assert!(rendered.contains("session-alpha"), "{rendered}");
        assert!(
            rendered.contains("Activate this session to open"),
            "{rendered}"
        );
        assert_eq!(navigator.rect.y, 2, "{rendered}");
        if horizontal {
            assert_eq!(navigator.rect.y, terminal.rect.y);
            assert_ne!(navigator.rect.x, terminal.rect.x);
            assert!(navigator.rect.width < terminal.rect.width, "{rendered}");
        } else {
            assert_eq!(navigator.rect.x, terminal.rect.x);
            assert!(terminal.rect.y > navigator.rect.y);
        }
    }
}

#[test]
fn compact_workspace_reserves_usable_terminal_height() {
    let panes = workspace_panes(Rect::new(0, 0, 60, 12), 20);

    assert_eq!(panes.len(), 2);
    assert!(panes[1].height >= 6, "{panes:?}");
}

#[test]
fn short_compact_workspace_keeps_session_navigation_reachable() {
    let app = workspace_fixture();
    let (_lines, hit_map) = render_app_to_lines(&app, 60, 10, &RenderState::default());

    assert!(
        hit_map
            .regions()
            .iter()
            .any(|region| region.node_id.starts_with("tui-session-session-")),
        "short compact layout should retain a focusable session row"
    );
}

#[test]
fn workspace_hides_transient_action_feedback() {
    let mut app = workspace_fixture();
    app.action_feedback = Some("detach requested: session-alpha".to_string());

    let rendered = render_app_to_lines(&app, 140, 42, &RenderState::default())
        .0
        .join("\n");

    assert!(!rendered.contains("action:"), "{rendered}");
    assert!(rendered.contains("session-alpha · running"), "{rendered}");
}

#[test]
fn unavailable_attach_yields_to_spawn_and_cannot_dispatch() {
    let mut app = workspace_fixture();
    app.sessions = vec![SessionRow::pending("session-pending")];
    app.selected_session = Some("session-pending".to_string());
    let (lines, hit_map) = render_app_to_lines(&app, 96, 30, &RenderState::default());
    assert!(lines.iter().all(|line| !line.contains("disabled: Attach")));
    assert!(
        hit_map
            .regions()
            .iter()
            .any(|region| region.node_id == "tui-spawn")
    );
    assert!(
        !hit_map
            .regions()
            .iter()
            .any(|region| region.node_id == "workspace-attach")
    );
}

#[test]
fn session_navigator_scrolls_without_hidden_row_hit_regions() {
    let mut app = workspace_fixture();
    app.sessions = (0..20)
        .map(|index| SessionRow::running(format!("session-{index:02}")))
        .collect();
    app.selected_session = Some("session-00".to_string());

    let (_lines, hit_map) = render_app_to_lines(&app, 72, 16, &RenderState::default());
    let bounds = hit_map
        .scroll_bounds("tui-session-list")
        .expect("session navigator should expose scroll bounds");
    let visible_rows = hit_map
        .regions()
        .iter()
        .filter(|region| region.node_id.starts_with("tui-session-session-"))
        .count();

    assert!(bounds.max_offset > 0);
    assert!(visible_rows < app.sessions.len());
}

#[test]
fn destructive_confirmation_isolates_workspace_and_dispatches_only_after_confirm() {
    let mut app = workspace_fixture();
    app.observed_terminal_inputs.clear();

    app.handle_action(
        "botster.tui.session.shutdown".to_string(),
        None,
        Some(json!({ "session_id": "session-alpha" })),
    );
    let (lines, hit_map) = render_app_to_lines(&app, 96, 30, &RenderState::default());
    let rendered = lines.join("\n");
    assert!(rendered.contains("Shut down session session-alpha?"));
    assert!(
        hit_map
            .regions()
            .iter()
            .all(|region| region.node_id != "tui-session-session-alpha")
    );
    assert!(
        hit_map
            .regions()
            .iter()
            .any(|region| region.node_id == "workspace-confirm-accept")
    );

    app.handle_action("botster.tui.confirm.cancel".to_string(), None, None);
    assert!(app.observed_requests.is_empty());

    app.handle_action(
        "botster.tui.session.shutdown".to_string(),
        None,
        Some(json!({ "session_id": "session-alpha" })),
    );
    let (_lines, confirm_hits) = render_app_to_lines(&app, 96, 30, &RenderState::default());
    let confirm = confirm_hits
        .regions()
        .iter()
        .find(|region| region.node_id == "workspace-confirm-accept")
        .expect("confirm button should be clickable");
    let mut router = InputRouter::new(renderer::action_request_context());
    let down = router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            confirm.rect.x,
            confirm.rect.y,
        ),
        &confirm_hits,
    );
    app.handle_dispatch(down);
    let up = router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
            confirm.rect.x,
            confirm.rect.y,
        ),
        &confirm_hits,
    );
    app.handle_dispatch(up);
    assert_eq!(
        app.observed_requests,
        vec![ObservedRequest::ShutdownSession(
            "session-alpha".to_string()
        )]
    );
}
