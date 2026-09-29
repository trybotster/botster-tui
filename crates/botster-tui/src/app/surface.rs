use super::*;

impl TuiApp {
    pub(super) fn surface(&self) -> UiNode {
        if matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { .. })
        ) {
            let root = self.unsafe_paste_consent_surface();
            root.validate()
                .expect("unsafe paste consent UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("unsafe paste consent UiNode should fit TUI renderer capabilities");
            return root;
        }
        if self.confirmation.is_some() {
            let root = self.confirmation_surface();
            root.validate()
                .expect("confirmation UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("confirmation UiNode should fit TUI renderer capabilities");
            return root;
        }

        if self.target_first_spawn.is_some() {
            let root = self.target_first_spawn_dialog();
            root.validate()
                .expect("target-first spawn UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("target-first spawn UiNode should fit TUI renderer capabilities");
            return root;
        }

        if self.plugin_surface.is_some() {
            let root = self.plugin_shell_surface();
            root.validate()
                .expect("plugin shell UiNode should satisfy the UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("plugin shell UiNode should fit TUI renderer capabilities");
            return root;
        }

        #[cfg(test)]
        if !self.workspace_test_mode && self.legacy_test_needs_system_details() {
            let root = self.system_details_panel();
            root.validate()
                .expect("system details UiNode should satisfy the core UI contract");
            renderer::tui_capabilities()
                .validate_node(&root)
                .expect("system details UiNode should fit TUI renderer capabilities");
            return root;
        }

        let mut root = node(
            UiNodeKind::Stack,
            "workspace-root",
            json!({ "direction": "vertical" }),
        );
        root.children = self.status_summary_children();
        if let Some(alert) = self.connection_alert() {
            root.children.push(child(alert));
        }
        if let Some(notice) = self.transient_notice_band() {
            root.children.push(child(notice));
        }
        if let Some(notice) = self.recovery_notice_line() {
            root.children.push(child(notice));
        }
        if let Some(notice) = self.quarantine_band() {
            root.children.push(child(notice));
        }
        root.children.push(child(self.workspace_toolbar()));
        if self.system_details_visible {
            root.children.push(child(self.system_details_panel()));
        } else {
            root.children.push(child(self.session_navigator()));
            root.children.push(child(self.focused_session_panel()));
        }
        root.validate()
            .expect("workspace UiNode should satisfy the core UI contract");
        renderer::tui_capabilities()
            .validate_node(&root)
            .expect("workspace UiNode should fit TUI renderer capabilities");
        root
    }

    pub(super) fn uses_workspace_shell(&self) -> bool {
        if matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { .. })
        ) || self.confirmation.is_some()
            || self.target_first_spawn.is_some()
            || self.plugin_surface.is_some()
            || self.system_details_visible
        {
            return false;
        }
        #[cfg(test)]
        if !self.workspace_test_mode && self.legacy_test_needs_system_details() {
            return false;
        }
        true
    }

    pub(super) fn plugin_shell_surface(&self) -> UiNode {
        let surface = self
            .plugin_surface
            .as_ref()
            .expect("plugin shell requires an active surface");
        let mut root = node(
            UiNodeKind::Stack,
            "plugin-shell",
            json!({ "direction": "vertical" }),
        );
        root.children = self.status_summary_children();
        root.children.push(child(node(
            UiNodeKind::Text,
            "plugin-shell-owner",
            json!({
                "text": format!(
                    "Plugin: {} / {} | Esc returns to System",
                    surface.package_name, surface.surface_id
                )
            }),
        )));
        if let Some(error) = &self.connection_error {
            root.children.push(child(node(
                UiNodeKind::Text,
                "plugin-shell-connection-error",
                json!({ "text": format!("connection: {error}") }),
            )));
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            root.children.push(child(node(
                UiNodeKind::Text,
                &format!("plugin-shell-diagnostic-{index}"),
                json!({ "text": format!("diagnostic: {}", diagnostic_text(diagnostic)) }),
            )));
        }
        if let Some(feedback) = &self.action_feedback {
            root.children.push(child(node(
                UiNodeKind::Text,
                "plugin-shell-action-feedback",
                json!({ "text": format!("action: {feedback}") }),
            )));
        }
        if let Some(error) = &self.error {
            root.children.push(child(node(
                UiNodeKind::Text,
                "plugin-shell-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        root.children.push(child(plugin_surface_render_root(
            surface,
            self.plugin_action_result.as_ref(),
            &self.session_entities,
            &self.entity_options_projection_store(),
            &self.drafts,
            &self.entity_options_invalid_fields,
        )));
        root
    }

    #[cfg(test)]
    pub(super) fn legacy_test_needs_system_details(&self) -> bool {
        self.compatibility.is_some()
            || !self.diagnostics.is_empty()
            || !self.apps.is_empty()
            || !self.package_navigation.is_empty()
            || !self.packages.is_empty()
            || !self.available_packages.is_empty()
            || self.install_plan.is_some()
            || self.update_status.is_some()
            || self.package_decision.is_some()
            || !self.drafts.is_empty()
    }

    pub(super) fn status_summary_children(&self) -> Vec<UiChild> {
        [
            UiWidthClass::Expanded,
            UiWidthClass::Regular,
            UiWidthClass::Compact,
        ]
        .into_iter()
        .map(|width| responsive_child(width, self.status_summary_node(width)))
        .collect()
    }

    pub(super) fn status_summary_node(&self, width: UiWidthClass) -> UiNode {
        let selected = self.selected_session.as_deref().unwrap_or("none");
        let attached = self.attached_session_id().unwrap_or("none");
        let session_count = match self.sessions.len() {
            1 => "1 session".to_string(),
            count => format!("{count} sessions"),
        };
        let compact = match self.attached_session_id() {
            Some(attached) => format!("Botster · {} · attached: {attached}", self.status),
            None => format!("Botster · {} · {session_count}", self.status),
        };
        match width {
            UiWidthClass::Expanded => node(
                UiNodeKind::Text,
                "workspace-status-expanded",
                json!({
                    "text": format!(
                        "Botster · Hub: {} · {session_count} · Selected: {selected} · Attached: {attached}",
                        self.status,
                    )
                }),
            ),
            UiWidthClass::Regular => node(
                UiNodeKind::Text,
                "workspace-status-regular",
                json!({
                    "text": format!(
                        "Botster · {} · {session_count} · Selected: {selected} · Attached: {attached}",
                        self.status,
                    )
                }),
            ),
            UiWidthClass::Compact => node(
                UiNodeKind::Text,
                "workspace-status-compact",
                json!({ "text": compact }),
            ),
        }
    }

    pub(super) fn connection_alert(&self) -> Option<UiNode> {
        let connection_error = self.connection_error.as_ref()?;
        Some(node(
            UiNodeKind::Text,
            "workspace-connection-alert",
            json!({
                "text": format!(
                    "Connection unavailable: {connection_error} · Expected protocol: {PROTOCOL}."
                )
            }),
        ))
    }

    pub(super) fn workspace_toolbar(&self) -> UiNode {
        let selected = self.selected_session_row();
        let selected_is_attached = selected
            .is_some_and(|session| self.attached_session_id() == Some(session.session_id.as_str()));
        let selected_is_attachable = selected.is_some_and(SessionRow::is_attachable);
        let selected_is_removable = selected.is_some_and(|session| {
            !session.pending && !matches!(session.lifecycle.as_str(), "running" | "pending")
        });
        let attach_is_primary = selected_is_attachable && !selected_is_attached;
        let detach_is_primary = self.attached.is_some() && !attach_is_primary;
        let spawn_is_primary = !attach_is_primary && !detach_is_primary;
        let payload = json!({ "session_id": self.selected_session });

        let mut actions = vec![child(workspace_button(
            "tui-spawn",
            "Spawn",
            "botster.tui.spawn",
            json!({}),
            if spawn_is_primary { "never" } else { "auto" },
            None,
        ))];
        if attach_is_primary {
            actions.push(child(workspace_button(
                "workspace-attach",
                "Attach",
                "botster.tui.attach",
                payload.clone(),
                "never",
                None,
            )));
        }
        if self.attached.is_some() {
            actions.push(child(workspace_button(
                "tui-detach",
                "Detach",
                "botster.tui.detach",
                json!({}),
                if detach_is_primary { "never" } else { "auto" },
                None,
            )));
        }
        actions.extend([
            child(workspace_button(
                "workspace-system-details",
                if self.system_details_visible {
                    "Workspace"
                } else {
                    "System details"
                },
                "botster.tui.system.toggle",
                json!({}),
                "auto",
                None,
            )),
            child(workspace_button(
                "workspace-refresh",
                "Refresh",
                "botster.tui.refresh",
                json!({}),
                "auto",
                None,
            )),
        ]);
        if selected_is_attachable {
            actions.push(child(workspace_button(
                "workspace-shutdown",
                "Shutdown",
                "botster.tui.session.shutdown",
                payload.clone(),
                "auto",
                Some("danger"),
            )));
        }
        if selected_is_removable {
            actions.push(child(workspace_button(
                "workspace-remove",
                "Remove",
                "botster.tui.session.remove",
                payload,
                "auto",
                Some("danger"),
            )));
        }

        let mut toolbar = node(UiNodeKind::Toolbar, "workspace-toolbar", json!({}));
        toolbar.slots.insert("actions".to_string(), actions);
        toolbar
    }

    pub(super) fn session_navigator(&self) -> UiNode {
        let mut panel = node(
            UiNodeKind::Panel,
            "workspace-session-navigator",
            json!({ "title": "Sessions" }),
        );
        let mut scroll = node(UiNodeKind::ScrollArea, "tui-session-list", json!({}));
        if self.sessions.is_empty() {
            scroll.children = vec![
                child(node(
                    UiNodeKind::Text,
                    "workspace-empty-title",
                    json!({ "text": "No sessions yet" }),
                )),
                child(node(
                    UiNodeKind::Text,
                    "workspace-empty-help",
                    json!({ "text": "Spawn starts a session; selection never attaches automatically." }),
                )),
            ];
        } else {
            scroll.children = self
                .sessions
                .iter()
                .map(|session| child(self.session_navigation_row(session)))
                .collect();
        }
        panel.slots.insert("body".to_string(), vec![child(scroll)]);
        panel
    }

    pub(super) fn session_navigation_row(&self, session: &SessionRow) -> UiNode {
        let selected = self.selected_session.as_deref() == Some(session.session_id.as_str());
        let attached = self.attached_session_id() == Some(session.session_id.as_str());
        let state = if session.pending {
            "pending spawn"
        } else if session.crashed() {
            // The pane names the lost worker; the row stays short enough for
            // the navigator.
            "crashed"
        } else if attached && session.is_attachable() {
            "attached"
        } else {
            session.lifecycle.as_str()
        };
        let mut label = format!("{} · {state}", session.session_id);
        if let Some(session_type_id) = &session.session_type_id {
            label.push_str(&format!(" · type={session_type_id}"));
        }
        if let Some(source) = &session.session_type_source {
            label.push_str(&format!(" · source={source}"));
        }
        if let Some(role) = &session.role {
            label.push_str(&format!(" · role={role}"));
        }
        if let Some(interaction) = &session.interaction {
            label.push_str(&format!(" · interaction={interaction}"));
        }
        if !session.traits.is_empty() {
            label.push_str(&format!(" · traits={}", session.traits.join(",")));
        }
        if let Some(lifecycle) = &session.session_type_lifecycle {
            label.push_str(&format!(" · type_lifecycle={lifecycle}"));
        }
        if let Some(reason) = session
            .failure_reason
            .as_ref()
            .filter(|_| !session.crashed())
        {
            label.push_str(&format!(" · {reason}"));
        }
        let mut item = node(
            UiNodeKind::ListItem,
            &format!("tui-session-{}", session.session_id),
            json!({
                "selected": selected,
                "value": session.session_id,
                "activation": {
                    "id": "botster.tui.attach",
                    "payload": { "session_id": session.session_id }
                }
            }),
        );
        item.slots.insert(
            "title".to_string(),
            vec![child(node(
                UiNodeKind::Text,
                &format!("tui-session-{}-title", session.session_id),
                json!({ "text": label }),
            ))],
        );
        item
    }

    pub(super) fn focused_session_panel(&self) -> UiNode {
        let mut body = node(
            UiNodeKind::Stack,
            "workspace-focused-session",
            json!({ "direction": "vertical" }),
        );
        if let Some(error) = &self.error {
            body.children.push(child(node(
                UiNodeKind::Text,
                "workspace-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        body.children.push(child(self.terminal_panel()));
        body
    }

    pub(super) fn selected_session_row(&self) -> Option<&SessionRow> {
        let selected = self.selected_session.as_deref()?;
        self.sessions
            .iter()
            .find(|session| session.session_id == selected)
    }

    pub(super) fn confirmation_surface(&self) -> UiNode {
        let confirmation = self
            .confirmation
            .as_ref()
            .expect("confirmation surface requires pending action");
        let (verb, session_id) = match confirmation {
            DestructiveAction::Shutdown(session_id) => ("Shut down", session_id),
            DestructiveAction::Remove(session_id) => ("Remove", session_id),
        };
        let mut actions = node(UiNodeKind::Inline, "workspace-confirm-actions", json!({}));
        actions.children = vec![
            child(workspace_button(
                "workspace-confirm-cancel",
                "Cancel",
                "botster.tui.confirm.cancel",
                json!({}),
                "never",
                None,
            )),
            child(workspace_button(
                "workspace-confirm-accept",
                verb,
                "botster.tui.confirm.accept",
                json!({}),
                "never",
                Some("danger"),
            )),
        ];
        let mut body = node(
            UiNodeKind::Stack,
            "workspace-confirm-body",
            json!({ "direction": "vertical" }),
        );
        body.children = vec![
            child(node(
                UiNodeKind::Text,
                "workspace-confirm-message",
                json!({ "text": format!("{verb} session {session_id}? This action cannot be undone from this workspace.") }),
            )),
            child(actions),
        ];
        let mut dialog = node(
            UiNodeKind::Dialog,
            "workspace-confirmation",
            json!({ "title": format!("Confirm {}", verb.to_lowercase()), "presentation": "auto" }),
        );
        dialog.slots.insert("body".to_string(), vec![child(body)]);
        dialog
    }

    pub(super) fn unsafe_paste_consent_surface(&self) -> UiNode {
        let stage = match self.pending_unsafe_paste.as_ref() {
            Some(PendingUnsafePaste::AwaitingConsent { stage, .. }) => *stage,
            _ => panic!("unsafe paste consent surface requires pending consent"),
        };
        let mut actions = node(
            UiNodeKind::Inline,
            "workspace-unsafe-paste-actions",
            json!({}),
        );
        actions.children.push(child(workspace_button(
            "workspace-unsafe-paste-primary",
            if stage == UnsafePasteConsentStage::Review {
                "Review paste"
            } else {
                "Cancel"
            },
            if stage == UnsafePasteConsentStage::Review {
                "botster.tui.unsafe_paste.review"
            } else {
                "botster.tui.unsafe_paste.cancel"
            },
            json!({}),
            "never",
            None,
        )));
        if stage == UnsafePasteConsentStage::Armed {
            actions.children.push(child(workspace_button(
                "workspace-unsafe-paste-confirm",
                "Paste anyway",
                "botster.tui.unsafe_paste.confirm",
                json!({}),
                "never",
                Some("danger"),
            )));
        }
        let mut body = node(
            UiNodeKind::Stack,
            "workspace-unsafe-paste-body",
            json!({ "direction": "vertical" }),
        );
        body.children = vec![
            child(node(
                UiNodeKind::Text,
                "workspace-unsafe-paste-warning",
                json!({ "text": "This paste contains multiple lines or terminal control characters. It can run commands or change terminal state." }),
            )),
            child(actions),
        ];
        let mut dialog = node(
            UiNodeKind::Dialog,
            "workspace-unsafe-paste-consent",
            json!({ "title": "Unsafe paste blocked", "presentation": "auto" }),
        );
        dialog.slots.insert("body".to_string(), vec![child(body)]);
        dialog
    }

    /// One line and one Resolve action per Hub quarantine.
    pub(super) fn quarantine_nodes(&self) -> Vec<UiNode> {
        let mut nodes = Vec::new();
        for (index, quarantine) in self.quarantines.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-quarantine-{index}"),
                json!({ "text": format!("quarantine: {}", quarantine_text(quarantine)) }),
            ));
            let target = quarantine_target(quarantine);
            nodes.push(button(
                &format!("tui-quarantine-{index}-resolve"),
                "Resolve",
                "botster.tui.quarantine.resolve",
                serde_json::to_value(&target).unwrap_or(Value::Null),
            ));
        }
        nodes
    }

    /// Informational band while the Hub holds quarantines.
    pub(super) fn quarantine_band(&self) -> Option<UiNode> {
        let count = self.quarantines.len();
        if count == 0 {
            return None;
        }
        let noun = if count == 1 {
            "quarantine"
        } else {
            "quarantines"
        };
        Some(node(
            UiNodeKind::Text,
            "workspace-quarantine-notice",
            json!({ "text": format!("{count} {noun} awaiting resolution (System details)") }),
        ))
    }

    pub(super) fn system_details_panel(&self) -> UiNode {
        let mut panel = node(
            UiNodeKind::Panel,
            "tui-status-panel",
            json!({ "title": "System details" }),
        );
        let mut children = vec![
            child(node(
                UiNodeKind::Text,
                "tui-status",
                json!({ "text": self.status }),
            )),
            child(node(
                UiNodeKind::Text,
                "tui-hub-software",
                json!({ "text": self.hub_software_text() }),
            )),
            child(node(
                UiNodeKind::Text,
                "tui-compatibility",
                json!({ "text": self.compatibility_text() }),
            )),
            child(node(
                UiNodeKind::Text,
                "tui-package-storage-context",
                json!({
                    "text": format!(
                        "package storage context: {}",
                        if self.package_storage_context_configured {
                            "configured"
                        } else {
                            "not supplied"
                        }
                    )
                }),
            )),
            child(button(
                "tui-refresh",
                "Refresh",
                "botster.tui.refresh",
                json!({}),
            )),
            child(button(
                "tui-connect",
                "Reconnect",
                "botster.tui.connect",
                json!({}),
            )),
        ];
        children.extend(self.session_types_section_nodes().into_iter().map(child));
        if let Some(error) = &self.connection_error {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-connection-error",
                json!({ "text": format!("connection: {error}") }),
            )));
        }
        children.extend(self.quarantine_nodes().into_iter().map(child));
        if let Some(counters) = hub_counters_text(&self.hub_counters) {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-hub-counters",
                json!({ "text": counters }),
            )));
        }
        children.push(child(node(
            UiNodeKind::Text,
            "tui-package-summary",
            json!({ "text": self.package_summary_text() }),
        )));
        children.extend(self.package_navigation_nodes().into_iter().map(child));
        children.extend(self.app_nodes().into_iter().map(child));
        if self.packages.is_empty() {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-package-empty",
                json!({ "text": "packages: none reported" }),
            )));
        } else {
            for (index, package) in self.packages.iter().enumerate() {
                children.push(child(node(
                    UiNodeKind::Text,
                    &format!("tui-package-{index}"),
                    json!({ "text": format!("package: {}", package_text(package)) }),
                )));
                children.extend(package_surface_nodes(package, index).into_iter().map(child));
                children.extend(
                    package_availability_nodes(package, index)
                        .into_iter()
                        .map(child),
                );
                children.extend(package_action_nodes(package, index).into_iter().map(child));
                if let Some(logs) = self.plugin_logs.get(&package.package_name) {
                    children.extend(plugin_log_nodes(logs, index).into_iter().map(child));
                }
                for (entrypoint_index, entrypoint) in
                    package.runnable_entrypoints.iter().enumerate()
                {
                    children.push(child(node(
                        UiNodeKind::Text,
                        &format!("tui-package-{index}-entrypoint-{entrypoint_index}"),
                        json!({
                            "text": format!(
                                "entrypoint: {} {}",
                                package.package_name,
                                entrypoint_text(entrypoint)
                            )
                        }),
                    )));
                    children.extend(
                        entrypoint_action_nodes(package, index, entrypoint, entrypoint_index)
                            .into_iter()
                            .map(child),
                    );
                }
                children.extend(
                    self.package_configuration_nodes(package, index)
                        .into_iter()
                        .map(child),
                );
            }
        }
        if !self.available_packages.is_empty() {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-marketplace-summary",
                json!({ "text": format!("marketplace: {} available", self.available_packages.len()) }),
            )));
            for (index, available_package) in self.available_packages.iter().enumerate() {
                children.push(child(node(
                    UiNodeKind::Text,
                    &format!("tui-available-package-{index}"),
                    json!({ "text": format!("available package: {}", available_package_text(available_package)) }),
                )));
            }
        }
        if let Some(install_plan) = &self.install_plan {
            children.extend(
                install_plan_nodes(install_plan)
                    .into_iter()
                    .enumerate()
                    .map(|(index, mut node)| {
                        node.id = Some(UiNodeId(format!("tui-install-plan-{index}")).into());
                        child(node)
                    }),
            );
        }
        if let Some(update_status) = &self.update_status {
            children.extend(
                update_status_nodes(update_status)
                    .into_iter()
                    .enumerate()
                    .map(|(index, mut node)| {
                        node.id = Some(UiNodeId(format!("tui-update-status-{index}")).into());
                        child(node)
                    }),
            );
        }
        if let Some(decision) = &self.package_decision {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-package-decision",
                json!({
                    "text": format!(
                        "package decision: package={} action={} state={} classification={}",
                        decision.package_name,
                        decision.action,
                        decision.state,
                        decision.classification
                    )
                }),
            )));
        }
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            children.push(child(node(
                UiNodeKind::Text,
                &format!("tui-diagnostic-{index}"),
                json!({ "text": format!("diagnostic: {}", diagnostic_text(diagnostic)) }),
            )));
        }
        if let Some(feedback) = &self.action_feedback {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-action-feedback",
                json!({ "text": format!("action: {feedback}") }),
            )));
        }
        if let Some(error) = &self.error {
            children.push(child(node(
                UiNodeKind::Text,
                "tui-error",
                json!({ "text": format!("error: {error}") }),
            )));
        }
        children.push(child(node(
            UiNodeKind::Text,
            "tui-hints",
            json!({ "text": "hints: Tab focus | up/down select | Enter/Space activate | terminal focus forwards keys" }),
        )));
        let mut scroll = node(
            UiNodeKind::ScrollArea,
            "workspace-system-details-scroll",
            json!({}),
        );
        scroll.children = children;
        panel.slots.insert("body".to_string(), vec![child(scroll)]);
        panel
    }

    pub(super) fn package_summary_text(&self) -> String {
        format!(
            "packages: {} installed; {} enabled",
            self.package_count, self.enabled_package_count
        )
    }

    pub(super) fn app_nodes(&self) -> Vec<UiNode> {
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-app-summary",
            json!({ "text": format!("apps: {} installed", self.apps.len()) }),
        )];
        if self.apps.is_empty() {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-app-empty",
                json!({ "text": "apps: none reported" }),
            ));
            return nodes;
        }

        for (app_index, app) in self.apps.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-app-{app_index}"),
                json!({ "text": format!("app: {}", app_text(app)) }),
            ));
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-app-{app_index}-launch-target"),
                json!({ "text": format!("launch target: {}", app_launch_target_text(app)) }),
            ));
            for (reason_index, reason) in app.blocked_reasons.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-app-{app_index}-blocked-{reason_index}"),
                    json!({ "text": format!("app blocked: {reason}") }),
                ));
            }
            for (diagnostic_index, diagnostic) in app.diagnostics.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-app-{app_index}-diagnostic-{diagnostic_index}"),
                    json!({ "text": format!("app diagnostic: {}", package_diagnostic_text(diagnostic)) }),
                ));
            }
            nodes.extend(action_state_nodes(
                &app.actions,
                "app action",
                &format!("tui-app-{app_index}"),
            ));
            if let Some(route) = &app.route {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-app-{app_index}-route"),
                    json!({ "text": format!("app route: {}", route_text(route)) }),
                ));
            }
        }
        nodes
    }

    pub(super) fn package_navigation_nodes(&self) -> Vec<UiNode> {
        if self.package_navigation.is_empty() {
            return Vec::new();
        }

        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-package-navigation-summary",
            json!({ "text": format!("navigation: {} admitted entries", self.package_navigation.len()) }),
        )];

        for (index, entry) in self.package_navigation.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-package-navigation-{index}"),
                json!({ "text": format!("navigation entry: {}", navigation_entry_text(entry)) }),
            ));
            for (diagnostic_index, diagnostic) in entry.diagnostics.iter().enumerate() {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-package-navigation-{index}-diagnostic-{diagnostic_index}"),
                    json!({ "text": format!("navigation diagnostic: {}", package_diagnostic_text(diagnostic)) }),
                ));
            }
            match navigation_open_payload_for_entry(entry) {
                Some(payload) if entry.enabled && !entry.blocked => {
                    nodes.push(button(
                        &format!("tui-package-navigation-{index}-open"),
                        "Open",
                        "botster.tui.navigation.open",
                        payload,
                    ));
                }
                Some(_) => nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-package-navigation-{index}-blocked"),
                    json!({ "text": format!("navigation blocked: {}", navigation_blocked_text(entry)) }),
                )),
                None => nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-package-navigation-{index}-unsupported"),
                    json!({ "text": format!("navigation unsupported: {}", navigation_unsupported_text(entry)) }),
                )),
            }
        }
        nodes
    }

    pub(super) fn package_configuration_nodes(
        &self,
        package: &DaemonPackage,
        index: usize,
    ) -> Vec<UiNode> {
        let fields = package_configuration_fields(package);
        if fields.is_empty() && package.configuration.schema.is_none() {
            return Vec::new();
        }

        let mut nodes = vec![node(
            UiNodeKind::Text,
            &format!("tui-package-{index}-configuration-summary"),
            json!({
                "text": format!(
                    "configuration: schema={} values={} missing={} diagnostics={}",
                    if package.configuration.schema.is_some() { "yes" } else { "no" },
                    package.configuration.effective_values.len(),
                    package.configuration.missing_required.len(),
                    package.configuration.diagnostics.len()
                )
            }),
        )];

        for missing in &package.configuration.missing_required {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-package-{index}-configuration-missing-{missing}"),
                json!({ "text": format!("configuration missing: {missing}") }),
            ));
        }

        for (diagnostic_index, diagnostic) in package.configuration.diagnostics.iter().enumerate() {
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-package-{index}-configuration-diagnostic-{diagnostic_index}"),
                json!({
                    "text": format!(
                        "configuration diagnostic: {}",
                        package_configuration_diagnostic_text(diagnostic)
                    )
                }),
            ));
        }

        for field in fields {
            nodes.push(self.package_configuration_field_node(package, index, &field));
        }

        if !nodes.is_empty() {
            nodes.push(button(
                &format!("tui-package-{index}-configuration-submit"),
                "Update configuration",
                "botster.tui.package_config.submit",
                json!({ "package_name": package.package_name }),
            ));
        }

        nodes
    }

    pub(super) fn package_configuration_field_node(
        &self,
        package: &DaemonPackage,
        index: usize,
        field: &PackageConfigurationField,
    ) -> UiNode {
        let field_name = package_config_field_name(&package.package_name, &field.key);
        let draft = self.drafts.get(&field_name);
        let effective = package.configuration.effective_values.get(&field.key);
        let error = package_configuration_field_error(package, &field.key);
        let mut props = json!({
            "name": field_name,
            "label": package_configuration_field_label(field),
        });
        if let Some(error) = error {
            props["error"] = Value::String(error);
        }

        match field.field_type.as_str() {
            "boolean" => {
                props["checked"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::Bool(configuration_value_bool(effective)));
                node(
                    UiNodeKind::Checkbox,
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "select" => {
                props["selected"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                let mut select = node(
                    UiNodeKind::Select,
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                );
                select.slots.insert(
                    "options".to_string(),
                    field
                        .options
                        .iter()
                        .enumerate()
                        .map(|(option_index, option)| {
                            child(node(
                                UiNodeKind::SelectOption,
                                &format!(
                                    "tui-package-{index}-configuration-{}-option-{option_index}",
                                    field.key
                                ),
                                json!({ "value": option.value, "label": option.label }),
                            ))
                        })
                        .collect(),
                );
                select
            }
            "multiline_text" => {
                props["value"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                node(
                    UiNodeKind::Textarea,
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "secret" => {
                props["checked"] = draft.cloned().unwrap_or(Value::Bool(false));
                let state = configuration_secret_state(effective);
                props["label"] = Value::String(format!(
                    "{} secret ({state}; Space marks write-only update)",
                    field.label
                ));
                node(
                    UiNodeKind::Checkbox,
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            "string" | "path" | "url" => {
                props["value"] = draft
                    .cloned()
                    .unwrap_or_else(|| Value::String(configuration_value_text(effective)));
                node(
                    UiNodeKind::TextInput,
                    &format!("tui-package-{index}-configuration-{}", field.key),
                    props,
                )
            }
            other => node(
                UiNodeKind::Text,
                &format!("tui-package-{index}-configuration-{}", field.key),
                json!({
                    "text": format!(
                        "{}: unsupported configuration type {}",
                        package_configuration_field_label(field),
                        other
                    )
                }),
            ),
        }
    }

    /// Renders authoritative Hub identity from `DaemonStatus.software` alone.
    ///
    /// An absent `build_revision` is omitted rather than filled with a
    /// placeholder, and a Hub that has not reported status reads as unknown —
    /// the same convention [`TuiApp::compatibility_text`] uses for
    /// `schema_version`. No value here is ever derived from a package row.
    pub(super) fn hub_software_text(&self) -> String {
        match &self.software {
            Some(software) => {
                let mut text = format!(
                    "hub software: {} {} ({})",
                    software.product_name, software.version, software.product_id
                );
                if let Some(build_revision) = &software.build_revision {
                    text.push_str(&format!("; build {build_revision}"));
                }
                text
            }
            None => "hub software: unknown".to_string(),
        }
    }

    pub(super) fn compatibility_text(&self) -> String {
        match &self.compatibility {
            Some(compatibility) => format!(
                "compatibility: protocol {} version {}; features {}; conformance {}; daemon schema {}",
                compatibility.protocol,
                compatibility.protocol_version,
                compatibility.features.join(","),
                compatibility.conformance_fixture_revision,
                self.schema_version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
            None => format!(
                "compatibility: expected protocol {PROTOCOL}; daemon schema {}; descriptor unavailable",
                self.schema_version
                    .map(|version| version.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            ),
        }
    }

    pub(super) fn session_types_section_nodes(&self) -> Vec<UiNode> {
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-session-types-heading",
            json!({ "text": "Session types" }),
        )];
        if let Some(error) = &self.session_type_subscription_error {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-session-types-subscription-error",
                json!({ "text": format!("session type subscription: {error}") }),
            ));
        }
        nodes.push(button(
            "tui-session-type-create",
            "Add session type",
            "botster.tui.session_type.create",
            json!({}),
        ));
        if let Some(form) = &self.session_type_form {
            nodes.extend(self.session_type_form_nodes(form));
        }
        let mut by_source: BTreeMap<String, Vec<&DaemonSessionType>> = BTreeMap::new();
        for entity in self.session_type_entities.ordered() {
            by_source
                .entry(entity.source.clone())
                .or_default()
                .push(entity);
        }
        if by_source.is_empty() {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-session-types-empty",
                json!({ "text": "session types: none reported" }),
            ));
        } else {
            for (source, rows) in by_source {
                nodes.push(node(
                    UiNodeKind::Text,
                    &format!("tui-session-type-source-{source}"),
                    json!({ "text": format!("source: {source}") }),
                ));
                for entity in rows {
                    nodes.extend(self.session_type_row_nodes(entity));
                }
            }
        }
        if let Some(selected_id) = &self.selected_session_type_id
            && let Some(entity) = self.session_type_entities.entities.get(selected_id)
        {
            nodes.push(self.session_type_detail_node(entity));
        }
        nodes
    }

    pub(super) fn session_type_row_nodes(&self, entity: &DaemonSessionType) -> Vec<UiNode> {
        let selected =
            self.selected_session_type_id.as_deref() == Some(entity.session_type_id.as_str());
        let availability = if entity.available {
            "available"
        } else {
            "unavailable"
        };
        let editable = if entity.editable {
            "editable"
        } else {
            "read-only"
        };
        let mut label = format!(
            "{} · {} · {} · {} · {availability} · {editable}",
            entity.label, entity.role, entity.interaction, entity.lifecycle
        );
        if !entity.traits.is_empty() {
            label.push_str(&format!(" · traits={}", entity.traits.join(",")));
        }
        if !entity.diagnostics.is_empty() {
            label.push_str(&format!(" · {}", entity.diagnostics.join("; ")));
        }
        let mut nodes = vec![
            node(
                UiNodeKind::Text,
                &format!("tui-session-type-{}-label", entity.session_type_id),
                json!({ "text": label }),
            ),
            button(
                &format!("tui-session-type-{}", entity.session_type_id),
                "Select",
                "botster.tui.session_type.select",
                json!({ "session_type_id": entity.session_type_id, "selected": selected }),
            ),
        ];
        if entity.editable {
            nodes.push(button(
                &format!("tui-session-type-{}-edit", entity.session_type_id),
                "Edit",
                "botster.tui.session_type.edit",
                json!({ "session_type_id": entity.session_type_id }),
            ));
            nodes.push(button(
                &format!("tui-session-type-{}-delete", entity.session_type_id),
                "Delete",
                "botster.tui.session_type.delete",
                json!({ "session_type_id": entity.session_type_id }),
            ));
        }
        nodes
    }

    pub(super) fn session_type_detail_node(&self, entity: &DaemonSessionType) -> UiNode {
        let mut detail = node(
            UiNodeKind::Stack,
            "tui-session-type-detail",
            json!({ "direction": "vertical" }),
        );
        let override_chain = entity
            .overridden_sources
            .iter()
            .map(|source| format!("{}:{}", source.kind, source.name))
            .collect::<Vec<_>>()
            .join(", ");
        let lines = [
            format!("session_type_id: {}", entity.session_type_id),
            format!("id: {}", entity.id),
            format!("source: {} ({})", entity.source, entity.source_name),
            format!(
                "execution: {}",
                match &entity.execution {
                    DaemonSessionTypeExecution::RelativeExecutable => "relative_executable",
                    DaemonSessionTypeExecution::ShellCommand => "shell_command",
                }
            ),
            format!("command: {} {:?}", entity.command, entity.args),
            format!(
                "working_directory_policy: {}",
                entity.working_directory_policy
            ),
            format!(
                "allowed_environment_overrides: {}",
                entity.allowed_environment_overrides.join(", ")
            ),
            format!("context_keys: {}", entity.context_keys.join(", ")),
            format!("target_id: {}", entity.target_id),
            format!("override_chain: {override_chain}"),
            format!("role: {}", entity.role),
            format!("interaction: {}", entity.interaction),
            format!("traits: {}", entity.traits.join(", ")),
            format!("lifecycle: {}", entity.lifecycle),
        ];
        detail.children = lines
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                child(node(
                    UiNodeKind::Text,
                    &format!("tui-session-type-detail-{index}"),
                    json!({ "text": text }),
                ))
            })
            .collect();
        detail
    }

    pub(super) fn session_type_form_nodes(&self, form: &SessionTypeFormDraft) -> Vec<UiNode> {
        let title = match form.mode {
            SessionTypeFormMode::Create => "Create session type",
            SessionTypeFormMode::Edit => "Edit session type",
        };
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-session-type-form-title",
            json!({ "text": title }),
        )];
        if let Some(error) = &form.error {
            nodes.push(node(
                UiNodeKind::Text,
                "tui-session-type-form-error",
                json!({ "text": format!("form error: {error}") }),
            ));
        }
        let fields = [
            ("session_type_source", "source", form.source.as_str()),
            (
                "session_type_source_target_id",
                "source target id",
                form.source_target_id.as_str(),
            ),
            ("session_type_id", "id", form.id.as_str()),
            ("session_type_label", "label", form.label.as_str()),
            (
                "session_type_description",
                "description",
                form.description.as_str(),
            ),
            ("session_type_role", "role", form.role.as_str()),
            (
                "session_type_interaction",
                "interaction",
                form.interaction.as_str(),
            ),
            ("session_type_traits", "traits", form.traits.as_str()),
            (
                "session_type_lifecycle",
                "lifecycle",
                form.lifecycle.as_str(),
            ),
            ("session_type_command", "command", form.command.as_str()),
            ("session_type_args", "args", form.args.as_str()),
            (
                "session_type_working_directory_policy",
                "working directory policy",
                form.working_directory_policy.as_str(),
            ),
            (
                "session_type_working_directory_path",
                "working directory path",
                form.working_directory_path.as_str(),
            ),
            (
                "session_type_environment",
                "environment",
                form.environment.as_str(),
            ),
            (
                "session_type_allowed_environment_overrides",
                "allowed environment overrides",
                form.allowed_environment_overrides.as_str(),
            ),
            (
                "session_type_context_keys",
                "context keys",
                form.context_keys.as_str(),
            ),
        ];
        for (name, label, value) in fields {
            let displayed = self
                .drafts
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or(value);
            // Render label+value as text so System details always shows the draft,
            // plus a TextInput for keyboard editing.
            nodes.push(node(
                UiNodeKind::Text,
                &format!("tui-session-type-field-text-{name}"),
                json!({ "text": format!("{label}: {displayed}") }),
            ));
            nodes.push(node(
                UiNodeKind::TextInput,
                &format!("tui-session-type-field-{name}"),
                json!({
                    "name": name,
                    "label": label,
                    "value": displayed
                }),
            ));
        }
        let execution = self
            .drafts
            .get("session_type_execution")
            .and_then(Value::as_str)
            .unwrap_or(&form.execution);
        nodes.push(node(
            UiNodeKind::Text,
            "tui-session-type-field-text-session_type_execution",
            json!({ "text": format!("execution: {execution}") }),
        ));
        let mut execution_select = node(
            UiNodeKind::Select,
            "tui-session-type-field-session_type_execution",
            json!({
                "name": "session_type_execution",
                "label": "execution",
                "selected": execution
            }),
        );
        execution_select.slots.insert(
            "options".to_string(),
            [
                ("relative_executable", "Relative executable"),
                ("shell_command", "Shell command"),
            ]
            .into_iter()
            .enumerate()
            .map(|(index, (value, label))| {
                child(node(
                    UiNodeKind::SelectOption,
                    &format!("tui-session-type-execution-option-{index}"),
                    json!({ "value": value, "label": label }),
                ))
            })
            .collect(),
        );
        nodes.push(execution_select);
        nodes.push(button(
            "tui-session-type-form-cancel",
            "Cancel",
            "botster.tui.session_type.form.cancel",
            json!({}),
        ));
        nodes.push(button(
            "tui-session-type-form-submit",
            "Save",
            "botster.tui.session_type.form.submit",
            json!({}),
        ));
        nodes
    }

    pub(super) fn target_first_spawn_nodes(&self, flow: &TargetFirstSpawnFlow) -> Vec<UiNode> {
        let mut nodes = vec![node(
            UiNodeKind::Text,
            "tui-target-first-spawn-title",
            json!({ "text": "Target-first spawn" }),
        )];
        match &flow.step {
            TargetFirstSpawnStep::PickTarget => {
                let options = self.launch_target_options();
                let help = if !options.is_empty() {
                    "Select a launch target first".to_string()
                } else if let Some(failure) = &self.spawn_targets_failure {
                    format!("Launch targets failed to load: {failure}")
                } else if self.spawn_targets_loaded {
                    "No launch targets available".to_string()
                } else {
                    "Loading launch targets…".to_string()
                };
                nodes.push(node(
                    UiNodeKind::Text,
                    "tui-target-first-spawn-help",
                    json!({ "text": help }),
                ));
                for target in options {
                    nodes.push(button(
                        &format!("tui-spawn-target-{}", target.target_id),
                        &format!("{} ({})", target.label, target.target_id),
                        "botster.tui.spawn.pick_target",
                        json!({ "target_id": target.target_id }),
                    ));
                }
            }
            TargetFirstSpawnStep::PickSessionType {
                target_id,
                target_label,
                session_types,
            } => {
                nodes.push(node(
                    UiNodeKind::Text,
                    "tui-target-first-spawn-target",
                    json!({ "text": format!("Target: {target_label} ({target_id})") }),
                ));
                // Membership comes only from Hub list-for-target rows stored on
                // the flow — never from entity.target_id equality filtering.
                if session_types.is_empty() {
                    nodes.push(node(
                        UiNodeKind::Text,
                        "tui-target-first-spawn-empty",
                        json!({ "text": "No session types for this target" }),
                    ));
                } else {
                    for session_type in session_types {
                        let label = if session_type.available {
                            format!("{} · {}", session_type.label, session_type.session_type_id)
                        } else {
                            format!(
                                "{} · {} · unavailable · {}",
                                session_type.label,
                                session_type.session_type_id,
                                session_type.diagnostics.join("; ")
                            )
                        };
                        if session_type.available {
                            nodes.push(button(
                                &format!("tui-spawn-session-type-{}", session_type.session_type_id),
                                &label,
                                "botster.tui.spawn.pick_session_type",
                                json!({ "session_type_id": session_type.session_type_id }),
                            ));
                        } else {
                            nodes.push(node(
                                UiNodeKind::Text,
                                &format!("tui-spawn-session-type-{}", session_type.session_type_id),
                                json!({ "text": label }),
                            ));
                        }
                    }
                }
            }
            TargetFirstSpawnStep::Prompt {
                target_label,
                session_type_id,
                prompt,
                ..
            } => {
                nodes.push(node(
                    UiNodeKind::Text,
                    "tui-target-first-spawn-prompt-meta",
                    json!({
                        "text": format!("Target {target_label} · type {session_type_id}")
                    }),
                ));
                let displayed_prompt = self
                    .drafts
                    .get("spawn_prompt")
                    .and_then(Value::as_str)
                    .unwrap_or(prompt);
                nodes.push(node(
                    UiNodeKind::TextInput,
                    "tui-spawn-prompt",
                    json!({
                        "name": "spawn_prompt",
                        "label": "prompt",
                        "value": displayed_prompt
                    }),
                ));
                nodes.push(button(
                    "tui-spawn-submit",
                    "Start session",
                    "botster.tui.spawn.submit",
                    json!({}),
                ));
            }
        }
        nodes.push(button(
            "tui-spawn-cancel",
            "Cancel spawn",
            "botster.tui.spawn.cancel",
            json!({}),
        ));
        nodes
    }

    pub(super) fn target_first_spawn_dialog(&self) -> UiNode {
        let flow = self
            .target_first_spawn
            .as_ref()
            .expect("target-first spawn dialog requires active flow");
        let mut body = node(
            UiNodeKind::Stack,
            "tui-target-first-spawn-body",
            json!({ "direction": "vertical" }),
        );
        body.children = self
            .target_first_spawn_nodes(flow)
            .into_iter()
            .map(child)
            .collect();
        let mut dialog = node(
            UiNodeKind::Dialog,
            "tui-target-first-spawn",
            json!({ "title": "Target-first spawn", "presentation": "auto" }),
        );
        dialog.slots.insert("body".to_string(), vec![child(body)]);
        dialog
    }

    pub(super) fn terminal_panel(&self) -> UiNode {
        let mut terminal = node(
            UiNodeKind::TerminalView,
            "tui-terminal",
            json!({
                "title": self.terminal_title(),
                "session_id": self.attached_session_id().map(str::to_string)
                    .or_else(|| self.attach_hydration.as_ref().map(|hydration| hydration.session_id.clone()))
                    .unwrap_or_else(|| "not attached".to_string())
            }),
        );
        terminal.children = vec![child(node(
            UiNodeKind::Text,
            "tui-terminal-output",
            json!({ "text": self.terminal_content() }),
        ))];
        terminal
    }

    pub(super) fn terminal_title(&self) -> String {
        match (
            self.attached_session_id(),
            self.attach_hydration.as_ref(),
            &self.selected_session,
        ) {
            (None, Some(hydration), _) => {
                format!("Terminal · {} · attaching", hydration.session_id)
            }
            (Some(attached), _, _) => format!("Terminal · {attached}"),
            (None, None, Some(selected))
                if self.selected_session_row().is_some_and(SessionRow::crashed) =>
            {
                format!("Terminal · {selected} · crashed")
            }
            (None, None, Some(selected)) => match self.detaches.get(selected) {
                Some((_, DetachState::Pending)) => format!("Terminal · {selected} · detaching"),
                Some((_, DetachState::Failed(_))) => {
                    format!("Terminal · {selected} · detach failed")
                }
                Some((_, DetachState::Confirmed)) | None => {
                    format!("Terminal · {selected} · detached")
                }
            },
            (None, None, None) => "Terminal".to_string(),
        }
    }

    pub(super) fn terminal_content(&self) -> String {
        // When a Ghostty projection is installed, styled paint is authoritative.
        // Kit Text child is chrome placeholder only — not ReadScreen authority.
        if self.ghostty_projection.is_some() || self.ghostty_viewport_cache.is_some() {
            if self.attached.is_some()
                || self
                    .attach_hydration
                    .as_ref()
                    .is_some_and(|hydration| hydration.snapshot_ready)
            {
                return String::new();
            }
            return "Detached · Ghostty projection retained for scrollback.".to_string();
        }
        if self.attached.is_some() {
            return "Waiting for terminal projection.".to_string();
        }
        match self.selected_session_row() {
            Some(session) if session.pending => {
                "This session is pending; attachment is unavailable.".to_string()
            }
            Some(session) if session.is_attachable() => {
                "Activate this session to open its terminal.".to_string()
            }
            Some(session) if session.crashed() => {
                "This session crashed: its worker was lost. Remove it, or Spawn a new session."
                    .to_string()
            }
            Some(session) => format!(
                "This session is {}; attachment is unavailable{}.",
                session.lifecycle,
                session
                    .failure_reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            ),
            None if self.connection_error.is_some() => {
                "Hub unavailable. Reconnect from System details.".to_string()
            }
            None => "Choose a session, or Spawn to create one.".to_string(),
        }
    }
}
