use super::*;

#[test]
fn session_type_form_fields_render_in_system_details() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = true;
    let mut form = SessionTypeFormDraft::create_default();
    form.id = "shell".to_string();
    form.label = "Shell".to_string();
    form.command = "printf draft".to_string();
    app.session_type_form = Some(form);

    let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
    let rendered = lines.join("\n");
    assert!(rendered.contains("Create session type"), "{rendered}");
    assert!(rendered.contains("printf draft"), "{rendered}");
    assert!(
        rendered.contains("execution: relative_executable"),
        "{rendered}"
    );
    assert!(
        hit_map
            .regions()
            .iter()
            .any(|region| region.node_id == "tui-session-type-form-submit")
    );
}

#[test]
fn session_type_form_renders_explicit_execution_control() {
    let app = TuiApp::new(None);
    let form = SessionTypeFormDraft::create_default();
    let nodes = app.session_type_form_nodes(&form);
    let execution = nodes
        .iter()
        .find(|node| {
            node.kind == UiNodeKind::Select
                && node.props.get("name") == Some(&json!("session_type_execution"))
        })
        .expect("execution select renders");

    assert_eq!(
        execution.props.get("selected"),
        Some(&json!("relative_executable"))
    );
    let options = execution.slots.get("options").expect("options render");
    assert_eq!(options.len(), 2);
    let UiChild::Node(relative) = &options[0] else {
        panic!("relative executable option renders as a node");
    };
    let UiChild::Node(shell) = &options[1] else {
        panic!("shell command option renders as a node");
    };
    assert_eq!(
        relative.props.get("value"),
        Some(&json!("relative_executable"))
    );
    assert_eq!(shell.props.get("value"), Some(&json!("shell_command")));
}

#[test]
fn spawn_before_targets_load_opens_the_dialog_and_fills_it_on_arrival() {
    let mut app = TuiApp::new(None);
    app.begin_target_first_spawn();
    assert_eq!(app.error, None, "an unloaded list is not an empty list");
    assert!(app.target_first_spawn.is_some());
    let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
    assert!(lines.join("\n").contains("Loading launch targets"));

    let mut response = base_response(DaemonResponseKind::SpawnTargets);
    response.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    app.apply_response(response);
    let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
    let rendered = lines.join("\n");
    assert!(rendered.contains("Repo A (repo-a)"), "{rendered}");
    assert!(!rendered.contains("Loading launch targets"));
}

#[test]
fn spawn_target_state_is_per_connection_and_failures_leave_loading() {
    let mut app = TuiApp::new(None);
    app.spawn_targets_loaded = true;
    app.drop_connection_state();
    app.begin_target_first_spawn();
    assert_eq!(app.error, None, "a reconnect is loading, not empty");
    let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
    assert!(lines.join("\n").contains("Loading launch targets"));

    app.apply_request_failure(
        PendingReply::SpawnTargets,
        DaemonRequestError::DeadlineExpired,
    );
    let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
    let rendered = lines.join("\n");
    assert!(
        rendered.contains("Launch targets failed to load"),
        "{rendered}"
    );
    assert!(!rendered.contains("Loading launch targets"));

    app.target_first_spawn = None;
    app.begin_target_first_spawn();
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.starts_with("launch targets failed to load"))
    );
    assert!(app.target_first_spawn.is_none());
}

#[test]
fn blank_target_first_spawn_validation_renders_visible_error_state() {
    let mut app = TuiApp::new(None);
    app.spawn_targets_loaded = true;
    app.begin_target_first_spawn();

    assert_eq!(
        app.error.as_deref(),
        Some("no launch targets available (no enabled admitted spawn targets)")
    );
    let (lines, _) = renderer::render_to_lines(&app.surface(), 200, 48);
    assert!(
        lines
            .join("\n")
            .contains("error: no launch targets available (no enabled admitted spawn targets)")
    );
}

#[test]
fn session_reducer_reports_matching_subscription_errors_and_ignores_foreign_ones() {
    let mut state = SessionEntityState::default();
    state.begin_generation("generation-error".to_string());

    let error = state
        .apply(DaemonEntityFrame::Error {
            subscription_id: "generation-error".to_string(),
            entity_type: "session".to_string(),
            code: "subscription_failed".to_string(),
            message: "hub dropped the session projection".to_string(),
        })
        .expect_err("a matching subscription error surfaces as a diagnostic");
    assert!(error.contains("subscription_failed"));
    assert!(error.contains("hub dropped the session projection"));

    assert!(
        !state
            .apply(DaemonEntityFrame::Error {
                subscription_id: "generation-other".to_string(),
                entity_type: "session".to_string(),
                code: "subscription_failed".to_string(),
                message: "unrelated subscription".to_string(),
            })
            .expect("a non-matching subscription error is ignored")
    );
}

#[test]
fn session_binding_reference_row_exposes_every_session_type_key() {
    let reference = session_binding_reference_row();

    for key in [
        "session_type_id",
        "session_type_source",
        "role",
        "traits",
        "interaction",
        "session_type_lifecycle",
    ] {
        assert!(
            reference.contains_key(key),
            "bind-list templates must observe the {key} key"
        );
    }
}

#[test]
fn spawn_opener_selection_uses_realized_semantic_action_not_visible_copy() {
    let workspace_id = "workspace-semantic-action";
    let semantic_node_id = "opaque-producer-node-7f3a";
    let semantic_payload = json!({
        "selected_workspace": workspace_id,
        "dialog": "spawn-target:workspace-semantic-action"
    });
    let mut root = node(UiNodeKind::Stack, "semantic-action-fixture", json!({}));
    root.children = vec![
        child(button(
            semantic_node_id,
            "Create session",
            "botster_workspaces.open_spawn",
            semantic_payload.clone(),
        )),
        child(button(
            "visible-spawn-generic-decoy",
            "Spawn",
            "botster_workspaces.open",
            json!({
                "selected_workspace": workspace_id,
                "dialog": "spawn-target:workspace-semantic-action"
            }),
        )),
    ];
    let mut router = InputRouter::new(renderer::action_request_context_for(WORKSPACES_SURFACE));
    let (lines, hit_map) = botster_tui_kit::render_to_lines_with_presentation_state(
        &root,
        120,
        48,
        &router.render_state(),
        &Default::default(),
    )
    .expect("render semantic action fixture through the real frame backend");
    assert!(lines.join("\n").contains("Spawn"));

    let (selected_node_id, selected_action) =
        unique_acceptance_action(&hit_map, WORKSPACES_SPAWN_OPENER_ACTION, |_| true, &lines)
            .expect("select the unique semantic Spawn opener");
    assert_eq!(selected_node_id, semantic_node_id);
    assert_eq!(selected_action.payload, Some(semantic_payload.clone()));

    focus_acceptance_node(&mut router, &hit_map, &selected_node_id)
        .expect("focus semantic action with keyboard traversal");
    let dispatch = router.dispatch_event(
        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
        &hit_map,
    );
    let InputDispatch::Action(request) = dispatch else {
        panic!("focused semantic action must dispatch through InputRouter");
    };
    assert_eq!(
        request.node_id,
        Some(UiNodeId(semantic_node_id.to_string()))
    );
    assert_eq!(request.action_id.0, "botster_workspaces.open_spawn");
    assert_eq!(request.payload, Some(semantic_payload));
    assert_ne!(
        request.node_id,
        Some(UiNodeId("visible-spawn-generic-decoy".to_string()))
    );
    assert_ne!(request.action_id.0, "botster_workspaces.open");
}

#[test]
fn session_type_entity_error_keeps_the_subscription_until_a_replacement_snapshot() {
    let mut app = TuiApp::new(None);
    app.session_type_entities.begin_generation("st".to_string());
    let entity = |id: &str| {
        serde_json::to_value(sample_session_type(id, "device", true)).expect("entity json")
    };
    app.apply_entity_frame(DaemonEntityFrame::Snapshot {
        subscription_id: "st".to_string(),
        entity_type: "session_type".to_string(),
        snapshot_seq: 1,
        items: vec![entity("device/old")],
        resync_reason: None,
    });
    let pending_before = app.pending_requests.len();

    app.apply_entity_frame(DaemonEntityFrame::Error {
        subscription_id: "st".to_string(),
        entity_type: "session_type".to_string(),
        code: "invalid_repo_session_types".to_string(),
        message: "repo catalog is invalid".to_string(),
    });

    assert_eq!(
        app.session_type_entities.subscription_id.as_deref(),
        Some("st")
    );
    assert_eq!(
        app.pending_requests.len(),
        pending_before,
        "no unsubscribe or resubscribe"
    );
    assert!(
        app.session_type_subscription_error
            .as_deref()
            .is_some_and(|error| error.contains("invalid_repo_session_types"))
    );
    assert!(
        app.session_type_entities
            .entities
            .contains_key("device/old")
    );

    app.apply_entity_frame(DaemonEntityFrame::Upsert {
        subscription_id: "st".to_string(),
        entity_type: "session_type".to_string(),
        snapshot_seq: 2,
        id: "device/delta".to_string(),
        entity: entity("device/delta"),
    });
    assert!(
        !app.session_type_entities
            .entities
            .contains_key("device/delta"),
        "deltas wait for the replacement snapshot"
    );

    app.apply_entity_frame(DaemonEntityFrame::Snapshot {
        subscription_id: "st".to_string(),
        entity_type: "session_type".to_string(),
        snapshot_seq: 3,
        items: vec![entity("device/new")],
        resync_reason: None,
    });

    assert_eq!(
        app.session_type_entities
            .entities
            .keys()
            .collect::<Vec<_>>(),
        vec!["device/new"],
        "the snapshot replaces the whole set"
    );
    assert_eq!(app.session_type_subscription_error, None);
    assert_eq!(
        app.session_type_entities.subscription_id.as_deref(),
        Some("st")
    );
    assert_eq!(app.pending_requests.len(), pending_before);
}

#[test]
fn session_types_render_package_read_only_and_unknown_literals() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = true;
    app.session_type_entities.begin_generation("st".to_string());
    let mut package = sample_session_type("package.demo/init", "package", false);
    package.role = "custom.role.token".to_string();
    package.traits = vec!["unknown.trait.token".to_string()];
    package.interaction = "service".to_string();
    app.session_type_entities
        .entities
        .insert(package.session_type_id.clone(), package.clone());
    app.session_type_entities
        .entity_order
        .push(package.session_type_id.clone());
    app.selected_session_type_id = Some(package.session_type_id.clone());

    let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 220, 70);
    let rendered = lines.join("\n");
    assert!(rendered.contains("custom.role.token"), "{rendered}");
    assert!(rendered.contains("unknown.trait.token"), "{rendered}");
    assert!(rendered.contains("read-only"), "{rendered}");
    assert!(
        !hit_map
            .regions()
            .iter()
            .any(|region| region.node_id.contains("-edit")),
        "package rows must not expose edit"
    );
}

#[test]
fn authoring_seed_and_wholesale_definition_preserve_path_and_environment() {
    let editable = DaemonSessionTypeEditableDefinition {
        session_type_id: "device/shell".to_string(),
        source: DaemonSessionTypeMutationSource::Device,
        definition: DaemonSessionTypeDefinition {
            id: "shell".to_string(),
            label: "Shell".to_string(),
            description: None,
            icon: None,
            role: "botster.agent".to_string(),
            interaction: "interactive".to_string(),
            traits: vec!["keep.trait".to_string()],
            lifecycle: "task".to_string(),
            execution: DaemonSessionTypeExecution::ShellCommand,
            command: "shell.sh".to_string(),
            args: Vec::new(),
            working_directory: DaemonSessionTypeWorkingDirectory::Relative {
                path: "nested/path".to_string(),
            },
            environment: BTreeMap::from([("KEEP".to_string(), "yes".to_string())]),
            allowed_environment_overrides: Vec::new(),
            context: Vec::new(),
            target_id: None,
        },
    };
    let form = SessionTypeFormDraft::from_authoring(editable);
    assert_eq!(form.working_directory_path, "nested/path");
    assert!(form.environment.contains("KEEP=yes"));
    let mut form = form;
    form.label = "Shell 2".to_string();
    let definition = definition_from_session_type_form(&form).expect("form reconstructs");
    assert_eq!(
        definition.working_directory,
        DaemonSessionTypeWorkingDirectory::Relative {
            path: "nested/path".to_string()
        }
    );
    assert_eq!(
        definition.environment.get("KEEP").map(String::as_str),
        Some("yes")
    );
    assert_eq!(definition.label, "Shell 2");
    assert_eq!(definition.traits, vec!["keep.trait".to_string()]);
    assert_eq!(
        definition.execution,
        DaemonSessionTypeExecution::ShellCommand
    );
}

#[test]
fn session_type_create_form_defaults_to_relative_executable() {
    let mut form = SessionTypeFormDraft::create_default();
    form.label = "Shell".to_string();
    form.command = "printf ready && exec agent".to_string();
    form.args = "first, second".to_string();

    let definition = definition_from_session_type_form(&form).expect("form reconstructs");

    assert_eq!(form.execution, "relative_executable");
    assert_eq!(
        definition.execution,
        DaemonSessionTypeExecution::RelativeExecutable
    );
    assert_eq!(definition.command, "printf ready && exec agent");
    assert_eq!(definition.args, vec!["first", "second"]);
}

#[test]
fn session_type_create_form_preserves_selected_execution_and_separate_args() {
    let mut app = TuiApp::new(None);
    app.session_type_form = Some(SessionTypeFormDraft::create_default());
    app.apply_session_type_form_values(&UiFormValues(serde_json::Map::from_iter([
        ("session_type_execution".to_string(), json!("shell_command")),
        (
            "session_type_command".to_string(),
            json!("printf '%s' \"$1\""),
        ),
        ("session_type_args".to_string(), json!("first, second")),
    ])));
    let form = app.session_type_form.expect("form remains open");

    let definition = definition_from_session_type_form(&form).expect("form reconstructs");

    assert_eq!(
        definition.execution,
        DaemonSessionTypeExecution::ShellCommand
    );
    assert_eq!(definition.command, "printf '%s' \"$1\"");
    assert_eq!(definition.args, vec!["first", "second"]);
}

#[test]
fn session_type_edit_form_preserves_shell_command_and_args() {
    let editable = DaemonSessionTypeEditableDefinition {
        session_type_id: "device/shell".to_string(),
        source: DaemonSessionTypeMutationSource::Device,
        definition: DaemonSessionTypeDefinition {
            id: "shell".to_string(),
            label: "Shell".to_string(),
            description: None,
            icon: None,
            role: "botster.agent".to_string(),
            interaction: "interactive".to_string(),
            traits: Vec::new(),
            lifecycle: "task".to_string(),
            execution: DaemonSessionTypeExecution::ShellCommand,
            command: "printf '%s' \"$1\"".to_string(),
            args: vec!["first value".to_string(), "second".to_string()],
            working_directory: DaemonSessionTypeWorkingDirectory::PackageRoot,
            environment: BTreeMap::new(),
            allowed_environment_overrides: Vec::new(),
            context: Vec::new(),
            target_id: None,
        },
    };

    let form = SessionTypeFormDraft::from_authoring(editable);
    let definition = definition_from_session_type_form(&form).expect("form reconstructs");

    assert_eq!(form.execution, "shell_command");
    assert_eq!(
        definition.execution,
        DaemonSessionTypeExecution::ShellCommand
    );
    assert_eq!(definition.command, "printf '%s' \"$1\"");
    assert_eq!(definition.args, vec!["first value", "second"]);
}

#[test]
fn session_type_execution_modes_round_trip_through_form_reconstruction() {
    for execution in [
        DaemonSessionTypeExecution::RelativeExecutable,
        DaemonSessionTypeExecution::ShellCommand,
    ] {
        let editable = DaemonSessionTypeEditableDefinition {
            session_type_id: "device/round-trip".to_string(),
            source: DaemonSessionTypeMutationSource::Device,
            definition: DaemonSessionTypeDefinition {
                id: "round-trip".to_string(),
                label: "Round trip".to_string(),
                description: None,
                icon: None,
                role: "botster.agent".to_string(),
                interaction: "interactive".to_string(),
                traits: Vec::new(),
                lifecycle: "task".to_string(),
                execution: execution.clone(),
                command: "bin/agent --literal-text".to_string(),
                args: vec!["one value".to_string()],
                working_directory: DaemonSessionTypeWorkingDirectory::PackageRoot,
                environment: BTreeMap::new(),
                allowed_environment_overrides: Vec::new(),
                context: Vec::new(),
                target_id: None,
            },
        };

        let form = SessionTypeFormDraft::from_authoring(editable);
        let reconstructed = definition_from_session_type_form(&form).expect("form reconstructs");

        assert_eq!(reconstructed.execution, execution);
        assert_eq!(reconstructed.command, "bin/agent --literal-text");
        assert_eq!(reconstructed.args, vec!["one value"]);
    }
}

#[test]
fn omitted_session_type_execution_defaults_to_relative_executable() {
    let definition: DaemonSessionTypeDefinition = serde_json::from_value(json!({
        "id": "defaulted",
        "label": "Defaulted",
        "role": "botster.agent",
        "interaction": "interactive",
        "lifecycle": "task",
        "command": "bin/defaulted"
    }))
    .expect("definition decodes");

    assert_eq!(
        definition.execution,
        DaemonSessionTypeExecution::RelativeExecutable
    );
}

#[test]
fn clearing_token_list_fields_emits_empty_collections_on_update() {
    let editable = DaemonSessionTypeEditableDefinition {
        session_type_id: "device/shell".to_string(),
        source: DaemonSessionTypeMutationSource::Device,
        definition: DaemonSessionTypeDefinition {
            id: "shell".to_string(),
            label: "Shell".to_string(),
            description: None,
            icon: None,
            role: "botster.agent".to_string(),
            interaction: "interactive".to_string(),
            traits: vec!["a.trait".to_string()],
            lifecycle: "task".to_string(),
            execution: DaemonSessionTypeExecution::RelativeExecutable,
            command: "shell.sh".to_string(),
            args: vec!["--flag".to_string()],
            working_directory: DaemonSessionTypeWorkingDirectory::PackageRoot,
            environment: BTreeMap::from([("KEEP".to_string(), "yes".to_string())]),
            allowed_environment_overrides: vec!["KEEP".to_string()],
            context: vec!["prompt".to_string()],
            target_id: None,
        },
    };
    let mut form = SessionTypeFormDraft::from_authoring(editable);
    form.traits.clear();
    form.args.clear();
    form.environment.clear();
    form.allowed_environment_overrides.clear();
    form.context_keys.clear();
    let definition = definition_from_session_type_form(&form).expect("form reconstructs");
    assert!(definition.traits.is_empty());
    assert!(definition.args.is_empty());
    assert!(definition.environment.is_empty());
    assert!(definition.allowed_environment_overrides.is_empty());
    assert!(definition.context.is_empty());
}

#[test]
fn launch_targets_are_enabled_admitted_spawn_targets_only() {
    let mut app = TuiApp::new(None);
    app.spawn_targets = vec![
        DaemonSpawnTarget {
            target_id: "repo-a".to_string(),
            label: "Repo A".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-a"),
            enabled: true,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        },
        DaemonSpawnTarget {
            target_id: "repo-disabled".to_string(),
            label: "Disabled".to_string(),
            root: std::path::PathBuf::from("/tmp/repo-disabled"),
            enabled: false,
            kind: "git".to_string(),
            base_ref: None,
            metadata: BTreeMap::new(),
        },
    ];
    let mut device = sample_session_type("device/shell", "device", true);
    device.target_id = "device:local".to_string();
    app.session_type_entities.begin_generation("st".to_string());
    app.session_type_entities
        .entities
        .insert(device.session_type_id.clone(), device.clone());
    app.session_type_entities
        .entity_order
        .push(device.session_type_id.clone());
    let options = app.launch_target_options();
    assert_eq!(
        options
            .iter()
            .map(|option| option.target_id.as_str())
            .collect::<Vec<_>>(),
        vec!["repo-a"]
    );
    assert!(
        !options
            .iter()
            .any(|option| option.target_id == "device:local"
                || option.target_id.starts_with("package:")),
        "must not synthesize device:local/package launch targets: {options:?}"
    );
}

#[test]
fn toolbar_spawn_dialog_is_reachable_without_system_details() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = false;
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    app.handle_action("botster.tui.spawn".to_string(), None, None);
    assert!(app.target_first_spawn.is_some());
    let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
    let rendered = lines.join("\n");
    assert!(rendered.contains("Target-first spawn"), "{rendered}");
    assert!(
        hit_map
            .regions()
            .iter()
            .any(|region| region.node_id == "tui-spawn-cancel")
    );
    app.handle_action("botster.tui.spawn.cancel".to_string(), None, None);
    assert!(app.target_first_spawn.is_none());
}

#[test]
fn session_type_form_draft_keystrokes_render_before_submit() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = true;
    let mut form = SessionTypeFormDraft::create_default();
    form.command = String::new();
    app.session_type_form = Some(form);
    let (_lines, hit_map) = render_app_to_lines(&app, 220, 80, &RenderState::default());
    let field = hit_map
        .regions()
        .iter()
        .find(|region| region.node_id == "tui-session-type-field-session_type_command")
        .expect("command field hit region");
    let mut router = InputRouter::new(renderer::action_request_context());
    let column = field.rect.x;
    let row = field.rect.y;
    app.handle_dispatch(router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    ));
    app.handle_dispatch(router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    ));
    for ch in ['z', 's', 'h'] {
        let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
        let dispatch = router.dispatch_event(Event::Key(key), &hit_map);
        app.handle_dispatch(dispatch);
        app.set_drafts(router.draft_values());
    }
    assert_eq!(
        app.drafts
            .get("session_type_command")
            .and_then(Value::as_str),
        Some("zsh")
    );
    let (lines, _) = render_app_to_lines(&app, 220, 80, &RenderState::default());
    let rendered = lines.join("\n");
    assert!(rendered.contains("zsh"), "{rendered}");
}

#[test]
fn product_toolbar_spawn_emits_spawn_session_type_request() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = false;
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    let mut global = sample_session_type("device/shell", "device", true);
    global.target_id = "repo-a".to_string();
    global.available = true;
    app.observed_requests.clear();
    app.handle_action("botster.tui.spawn".to_string(), None, None);
    app.handle_action(
        "botster.tui.spawn.pick_target".to_string(),
        None,
        Some(json!({ "target_id": "repo-a" })),
    );
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::ListSessionTypesForTarget { target_id }
                if target_id == "repo-a"
        )),
        "pick target must observe ListSessionTypesForTarget: {:?}",
        app.observed_requests
    );
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("list-for-target request should be pending")
        .0;
    let mut response = base_response(DaemonResponseKind::SessionTypes);
    response.session_types = vec![global];
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Ok(Box::new(response)),
    });
    app.handle_action(
        "botster.tui.spawn.pick_session_type".to_string(),
        None,
        Some(json!({ "session_type_id": "device/shell" })),
    );
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::SpawnSessionType {
                session_type_id,
                target_id: Some(target_id),
                ..
            } if session_type_id == "device/shell" && target_id == "repo-a"
        )),
        "{:?}",
        app.observed_requests
    );
    assert!(
        !app.observed_requests
            .iter()
            .any(|request| matches!(request, ObservedRequest::Spawn { .. }))
    );
}

#[test]
fn delete_session_type_uses_source_name_for_repo_mutation_source() {
    let mut app = TuiApp::new(None);
    let mut entity = sample_session_type("shared-git/type-a", "repo", true);
    entity.source_name = "shared-git".to_string();
    entity.target_id = "authored-other-target".to_string();
    entity.id = "type-a".to_string();
    app.session_type_entities.begin_generation("st".to_string());
    app.session_type_entities
        .entities
        .insert(entity.session_type_id.clone(), entity);
    app.session_type_entities
        .entity_order
        .push("shared-git/type-a".to_string());
    app.observed_requests.clear();
    app.delete_session_type("shared-git/type-a");
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::DeleteSessionType {
                source: DaemonSessionTypeMutationSource::Repo { target_id },
                session_type_id,
            } if target_id == "shared-git" && session_type_id == "type-a"
        )),
        "{:?}",
        app.observed_requests
    );
}

#[test]
fn product_toolbar_spawn_opens_target_first_flow_not_freeform_spawn() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = true;
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    app.observed_requests.clear();
    app.handle_action("botster.tui.spawn".to_string(), None, None);
    assert!(app.target_first_spawn.is_some());
    assert!(
        !app.observed_requests
            .iter()
            .any(|request| matches!(request, ObservedRequest::Spawn { .. })),
        "toolbar spawn must not emit freeform Spawn"
    );
    let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
    let rendered = lines.join("\n");
    assert!(
        rendered.contains("Select a launch target first"),
        "{rendered}"
    );
    assert!(
        hit_map
            .regions()
            .iter()
            .any(|region| region.node_id == "tui-spawn-target-repo-a")
    );
}

#[test]
fn target_first_spawn_picker_uses_list_rows_not_entity_target_equality() {
    let mut app = TuiApp::new(None);
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    // Entity catalog has a type whose target_id equals repo-a, plus a Global
    // that would never match entity equality for real admitted T.
    let mut repo_entity = sample_session_type("repo-a/svc", "repo", true);
    repo_entity.target_id = "repo-a".to_string();
    let mut global_entity = sample_session_type("device/shell", "device", true);
    global_entity.target_id = "device:local".to_string();
    app.session_type_entities.begin_generation("st".to_string());
    for entity in [&repo_entity, &global_entity] {
        app.session_type_entities
            .entities
            .insert(entity.session_type_id.clone(), entity.clone());
        app.session_type_entities
            .entity_order
            .push(entity.session_type_id.clone());
    }
    // Hub list-for-target projects Global with list-context target_id = T.
    let mut listed_global = sample_session_type("device/shell", "device", true);
    listed_global.target_id = "repo-a".to_string();
    listed_global.available = true;
    app.target_first_spawn = Some(TargetFirstSpawnFlow {
        step: TargetFirstSpawnStep::PickSessionType {
            target_id: "repo-a".to_string(),
            target_label: "Repo A".to_string(),
            session_types: vec![listed_global],
        },
    });
    let nodes = app.target_first_spawn_nodes(app.target_first_spawn.as_ref().unwrap());
    let buttons: Vec<String> = nodes
        .iter()
        .filter(|node| node.kind == UiNodeKind::Button)
        .filter_map(|node| {
            node.id
                .as_ref()
                .and_then(UiAuthoredNodeId::as_literal)
                .map(|id| id.0.clone())
        })
        .collect();
    assert!(
        buttons
            .iter()
            .any(|id| id.contains("tui-spawn-session-type-device/shell")),
        "Global from Hub list must appear for admitted T: {buttons:?}"
    );
    assert!(
        !buttons
            .iter()
            .any(|id| id.contains("tui-spawn-session-type-repo-a/svc")),
        "entity-only rows must not appear when absent from Hub list: {buttons:?}"
    );
}

#[test]
fn product_pick_target_list_failure_keeps_flow_recoverable_without_stale_rows() {
    let mut app = TuiApp::new(None);
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    // Seed a prior successful list so a failed re-pick must not leave those rows.
    let prior = sample_session_type("device/stale", "device", true);
    app.target_first_spawn = Some(TargetFirstSpawnFlow {
        step: TargetFirstSpawnStep::PickSessionType {
            target_id: "repo-a".to_string(),
            target_label: "Repo A".to_string(),
            session_types: vec![prior],
        },
    });
    app.observed_requests.clear();
    app.handle_action(
        "botster.tui.spawn.pick_target".to_string(),
        None,
        Some(json!({ "target_id": "repo-a" })),
    );
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::ListSessionTypesForTarget { target_id }
                if target_id == "repo-a"
        )),
        "{:?}",
        app.observed_requests
    );
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("list-for-target request should be pending")
        .0;
    let mut response = operator_error_response("spawn target is not eligible");
    let error = response.error.as_mut().expect("operator error payload");
    error.code = "target_unavailable".to_string();
    error.operation = "list_session_types_for_target".to_string();
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Ok(Box::new(response)),
    });
    assert!(
        app.error
            .as_deref()
            .is_some_and(|error| error.contains("spawn target is not eligible")),
        "{:?}",
        app.error
    );
    match app.target_first_spawn.as_ref().map(|flow| &flow.step) {
        Some(TargetFirstSpawnStep::PickTarget) => {}
        other => panic!("failed list must stay on PickTarget, got {other:?}"),
    }
    let (lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
    let rendered = lines.join("\n");
    assert!(
        !hit_map
            .regions()
            .iter()
            .any(|region| region.node_id.contains("tui-spawn-session-type-")),
        "no selectable session-type rows after failed list: {:?}",
        hit_map
            .regions()
            .iter()
            .map(|r| &r.node_id)
            .collect::<Vec<_>>()
    );
    assert!(
        hit_map
            .regions()
            .iter()
            .any(|region| region.node_id == "tui-spawn-cancel"),
        "flow must remain cancellable: {rendered}"
    );
    // Recovery: re-pick after a successful list.
    let mut recovered = sample_session_type("device/shell", "device", true);
    recovered.target_id = "repo-a".to_string();
    app.error = None;
    app.handle_action(
        "botster.tui.spawn.pick_target".to_string(),
        None,
        Some(json!({ "target_id": "repo-a" })),
    );
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("recovery list-for-target request should be pending")
        .0;
    let mut response = base_response(DaemonResponseKind::SessionTypes);
    response.session_types = vec![recovered];
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Ok(Box::new(response)),
    });
    assert_eq!(app.error, None);
    match &app.target_first_spawn.as_ref().unwrap().step {
        TargetFirstSpawnStep::PickSessionType { session_types, .. } => {
            assert_eq!(session_types.len(), 1);
            assert_eq!(session_types[0].session_type_id, "device/shell");
        }
        other => panic!("recovery should reach PickSessionType, got {other:?}"),
    }
}

#[test]
fn product_pick_target_transport_error_keeps_no_stale_selectable_rows() {
    let mut app = TuiApp::new(None);
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    let prior = sample_session_type("device/stale", "device", true);
    app.target_first_spawn = Some(TargetFirstSpawnFlow {
        step: TargetFirstSpawnStep::PickSessionType {
            target_id: "repo-a".to_string(),
            target_label: "Repo A".to_string(),
            session_types: vec![prior],
        },
    });
    app.observed_requests.clear();
    app.handle_action(
        "botster.tui.spawn.pick_target".to_string(),
        None,
        Some(json!({ "target_id": "repo-a" })),
    );
    assert!(app.observed_requests.iter().any(|request| matches!(
        request,
        ObservedRequest::ListSessionTypesForTarget { target_id }
            if target_id == "repo-a"
    )));
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("list-for-target request should be pending")
        .0;
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Err(DaemonRequestError::ConnectionClosed),
    });
    match app.target_first_spawn.as_ref().map(|flow| &flow.step) {
        Some(TargetFirstSpawnStep::PickTarget) => {}
        other => panic!("transport failure must stay on PickTarget, got {other:?}"),
    }
    let (_lines, hit_map) = renderer::render_to_lines(&app.surface(), 200, 60);
    assert!(
        !hit_map
            .regions()
            .iter()
            .any(|region| region.node_id.contains("tui-spawn-session-type-")),
        "transport failure must not leave selectable session-type rows"
    );
}

#[test]
fn product_spawn_list_and_pick_are_reachable_through_input_router() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = false;
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    let mut listed = sample_session_type("device/shell", "device", true);
    listed.target_id = "repo-a".to_string();
    app.handle_action("botster.tui.spawn".to_string(), None, None);
    let mut router = InputRouter::new(renderer::action_request_context());
    // Mouse path (hit-map click activation).
    let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &RenderState::default());
    let target_region = hit_map
        .regions()
        .iter()
        .find(|region| region.node_id == "tui-spawn-target-repo-a")
        .expect("launch target hit region");
    let column = target_region.rect.x;
    let row = target_region.rect.y;
    app.handle_dispatch(router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    ));
    app.handle_dispatch(router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    ));
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::ListSessionTypesForTarget { target_id }
                if target_id == "repo-a"
        )),
        "{:?}",
        app.observed_requests
    );
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("list-for-target request should be pending")
        .0;
    let mut response = base_response(DaemonResponseKind::SessionTypes);
    response.session_types = vec![listed];
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Ok(Box::new(response)),
    });
    let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &RenderState::default());
    let type_region = hit_map
        .regions()
        .iter()
        .find(|region| region.node_id == "tui-spawn-session-type-device/shell")
        .expect("session type hit region from Hub list");
    let column = type_region.rect.x;
    let row = type_region.rect.y;
    app.handle_dispatch(router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    ));
    app.handle_dispatch(router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    ));
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::SpawnSessionType {
                session_type_id,
                target_id: Some(target_id),
                ..
            } if session_type_id == "device/shell" && target_id == "repo-a"
        )),
        "{:?}",
        app.observed_requests
    );
}

#[test]
fn product_spawn_list_and_pick_are_reachable_through_keyboard_input_router() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = false;
    app.spawn_targets = vec![DaemonSpawnTarget {
        target_id: "repo-a".to_string(),
        label: "Repo A".to_string(),
        root: std::path::PathBuf::from("/tmp/repo-a"),
        enabled: true,
        kind: "git".to_string(),
        base_ref: None,
        metadata: BTreeMap::new(),
    }];
    let mut listed = sample_session_type("device/shell", "device", true);
    listed.target_id = "repo-a".to_string();
    app.observed_requests.clear();
    app.handle_action("botster.tui.spawn".to_string(), None, None);
    let mut router = InputRouter::new(renderer::action_request_context());

    // Keyboard path: Tab focus admitted T, Enter → ListSessionTypesForTarget.
    let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &router.render_state());
    focus_hit_map_node_by_tab(&mut router, &hit_map, "tui-spawn-target-repo-a");
    activate_focused_action_with_enter(&mut app, &mut router, &hit_map);
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::ListSessionTypesForTarget { target_id }
                if target_id == "repo-a"
        )),
        "keyboard pick target must observe ListSessionTypesForTarget: {:?}",
        app.observed_requests
    );
    assert_eq!(app.error, None, "keyboard list-for-target should succeed");
    let request_id = *app
        .pending_requests
        .last_key_value()
        .expect("list-for-target request should be pending")
        .0;
    let mut response = base_response(DaemonResponseKind::SessionTypes);
    response.session_types = vec![listed];
    app.apply_wake(AppWake::Completed {
        request_id,
        result: Ok(Box::new(response)),
    });

    // Keyboard path: Tab focus Hub-listed Global, Enter → SpawnSessionType target_id=T.
    let (_lines, hit_map) = render_app_to_lines(&app, 220, 70, &router.render_state());
    focus_hit_map_node_by_tab(&mut router, &hit_map, "tui-spawn-session-type-device/shell");
    activate_focused_action_with_enter(&mut app, &mut router, &hit_map);
    assert!(
        app.observed_requests.iter().any(|request| matches!(
            request,
            ObservedRequest::SpawnSessionType {
                session_type_id,
                target_id: Some(target_id),
                ..
            } if session_type_id == "device/shell" && target_id == "repo-a"
        )),
        "keyboard spawn must carry target_id=T: {:?}",
        app.observed_requests
    );
}

#[test]
fn production_spawn_picker_source_does_not_filter_entities_by_target_id_equality() {
    let source = source_without_line_comments();
    // Build forbidden needles without embedding the production anti-patterns as
    // contiguous literals (this test body is itself scanned).
    let entity_eq_filter = format!("entity.target_id {} {}", "!=", "*target_id");
    let device_local_synth = format!("entity.target_id {} \"{}\"", "==", "device:local");
    assert!(
        !source.contains(&entity_eq_filter),
        "spawn picker must not re-filter entities by target_id equality"
    );
    assert!(
        !source.contains(&device_local_synth),
        "launch targets must not synthesize device:local from entities"
    );
    assert!(
        source.contains("ListSessionTypesForTarget"),
        "product path must call Hub ListSessionTypesForTarget"
    );
}

#[test]
fn session_type_real_input_create_button_dispatches_through_input_router() {
    let mut app = TuiApp::new(None);
    app.system_details_visible = true;
    let (lines, hit_map) = render_app_to_lines(&app, 220, 70, &RenderState::default());
    let _ = lines;
    let region = hit_map
        .regions()
        .iter()
        .find(|region| region.node_id == "tui-session-type-create")
        .expect("create button hit region");
    let mut router = InputRouter::new(renderer::action_request_context());
    let column = region.rect.x;
    let row = region.rect.y;
    let down = router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    );
    app.handle_dispatch(down);
    let up = router.dispatch_event(
        mouse_event(
            crossterm::event::MouseEventKind::Up(crossterm::event::MouseButton::Left),
            column,
            row,
        ),
        &hit_map,
    );
    app.handle_dispatch(up);
    assert!(app.session_type_form.is_some());
}
