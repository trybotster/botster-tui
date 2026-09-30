use super::*;

impl TuiApp {
    pub(super) fn set_drafts(&mut self, drafts: BTreeMap<String, Value>) {
        self.drafts = drafts;
        // A fresh router draft for a previously invalidated field clears the banner.
        self.entity_options_invalid_fields
            .retain(|field| !self.drafts.contains_key(field));
        self.reconcile_entity_option_drafts();
    }

    pub(super) fn set_selected_session(&mut self, session_id: Option<String>) {
        if self.selected_session == session_id {
            return;
        }
        self.selected_session = session_id;
        self.sync_notice_subscriptions();
    }

    pub(super) fn sync_focused_session(&mut self, selected_row: Option<&Value>) {
        let Some(session_id) = selected_row.and_then(Value::as_str) else {
            return;
        };
        if self
            .sessions
            .iter()
            .any(|candidate| candidate.session_id == session_id)
        {
            self.set_selected_session(Some(session_id.to_string()));
        }
    }

    pub(super) fn handle_dispatch(&mut self, dispatch: InputDispatch) {
        match dispatch {
            InputDispatch::Action(request) => {
                if self.plugin_surface.is_some() {
                    self.handle_plugin_action(request);
                } else {
                    self.handle_action(request.action_id.0, request.values, request.payload);
                }
            }
            InputDispatch::Scroll { node_id, lines } => {
                // Map kit scroll deltas on the terminal node to Ghostty ScrollOp.
                // Non-terminal scroll areas are kit-owned presentation scroll.
                if is_terminal_node(Some(node_id.as_str())) && lines != 0 {
                    self.scroll_projection(ScrollOp::Delta(i32::from(lines)));
                }
            }
            // Kit classic key bytes never reach the PTY: the TUI intercepts
            // terminal-focused keys before the router and sends typed KEY frames.
            InputDispatch::TerminalForward { .. }
            | InputDispatch::HostKey(_)
            | InputDispatch::Hover { .. }
            | InputDispatch::Focus { .. }
            | InputDispatch::Ignored => {}
        }
    }

    /// Terminal-focused keys become typed KEY frames. While a route is still
    /// attaching the key is queued in order and released on the live path.
    pub(super) fn handle_focused_terminal_key(
        &mut self,
        key: KeyEvent,
        focused_node_id: Option<&str>,
    ) -> bool {
        if !is_terminal_node(focused_node_id) {
            return false;
        }
        if self.attach_hydration.is_some() {
            self.queue_pending_input(PendingTerminalInput::Key(key));
            return true;
        }
        if self.attached.is_none() {
            return false;
        }
        self.send_key(key);
        true
    }

    pub(super) fn handle_focused_terminal_paste(
        &mut self,
        text: &str,
        focused_node_id: Option<&str>,
    ) -> bool {
        // Every new paste event invalidates a prior retry, even when a modal
        // currently owns focus. The current event is never replayed.
        self.invalidate_unsafe_paste();
        if !is_terminal_node(focused_node_id) {
            return false;
        }
        if text.is_empty() {
            return true;
        }
        let data = text.as_bytes().to_vec();
        if self.attach_hydration.is_some() {
            if self.input_window.has_paste()
                || self.attach_hydration.as_ref().is_some_and(|hydration| {
                    hydration
                        .pending_input
                        .iter()
                        .any(|input| matches!(input, PendingTerminalInput::Paste(_)))
                })
            {
                self.error =
                    Some("terminal paste unavailable: another paste is in flight".to_string());
                return true;
            }
            self.queue_pending_input(PendingTerminalInput::Paste(data));
            return true;
        }
        if self.attached.is_none() {
            self.error = Some(
                "terminal stream unavailable: attach a session before sending terminal input"
                    .to_string(),
            );
            return true;
        }
        self.send_paste(data);
        true
    }

    /// Mouse events over the live terminal become MOUSE frames when the nested
    /// application enabled mouse tracking. Other pointer events stay with the kit.
    pub(super) fn handle_focused_terminal_mouse(
        &mut self,
        mouse: MouseEvent,
        focused_node_id: Option<&str>,
        hit_map: &HitMap,
    ) -> bool {
        if !is_terminal_node(focused_node_id) || self.attached.is_none() {
            return false;
        }
        if !terminal_input::mouse_tracking_enabled(self.current_mode_bits()) {
            return false;
        }
        let Some(outer) = tui_terminal_region(hit_map) else {
            return false;
        };
        // Occluded points (open menus, modals) belong to the kit router.
        if !hit_map
            .lookup(mouse.column, mouse.row)
            .is_some_and(|region| is_terminal_node(Some(region.node_id.as_str())))
        {
            return false;
        }
        let inner = botster_tui_kit::terminal_inner_rect(outer);
        self.send_mouse(mouse, inner)
    }

    /// Host focus changes are forwarded as FOCUS frames on the live route.
    pub(super) fn handle_host_focus(&mut self, focused: bool) {
        if self.attach_hydration.is_some() {
            self.queue_pending_input(PendingTerminalInput::Focus(focused));
            return;
        }
        if self.attached.is_some() {
            self.send_focus(focused);
        }
    }

    pub(super) fn handle_plugin_action(&mut self, request: UiActionRequest) {
        let Some(surface) = self.plugin_surface.as_ref() else {
            return;
        };
        if request.surface_id.0 != surface.surface_id {
            self.error = Some(format!(
                "plugin action surface mismatch: active={} request={}",
                surface.surface_id, request.surface_id.0
            ));
            return;
        }

        let package_name = surface.package_name.clone();
        self.error = None;
        self.action_feedback = Some(format!(
            "plugin action requested: {package_name}/{}",
            request.action_id.0
        ));
        self.pending_plugin_request = Some(request.clone());
        self.submit_apply(DaemonRequest::PluginSurfaceAction {
            package_name,
            request,
        });
    }

    pub(super) fn active_plugin_surface_id(&self) -> Option<&str> {
        self.plugin_surface
            .as_ref()
            .map(|surface| surface.surface_id.as_str())
    }

    pub(super) fn clear_active_plugin_surface(&mut self) -> bool {
        if self.plugin_surface.is_none() {
            return false;
        }
        self.reset_active_plugin_surface();
        self.system_details_visible = true;
        self.action_feedback = Some("returned to System".to_string());
        true
    }

    /// Keys the TUI handles before terminal forwarding: Esc for dialogs and
    /// plugin content. PageUp/PageDown and Ctrl+Home/End reach a focused
    /// session; the Shift variants scroll as reserved host keys.
    pub(super) fn handle_tui_owned_key(&mut self, key: KeyEvent) -> bool {
        if key.code != KeyCode::Esc || key.modifiers != KeyModifiers::NONE {
            return false;
        }
        if matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { .. })
        ) {
            self.invalidate_unsafe_paste();
            return true;
        }
        if self.confirmation.is_some() {
            self.confirmation = None;
            return true;
        }
        self.clear_active_plugin_surface()
    }

    pub(super) fn reset_active_plugin_surface(&mut self) {
        self.plugin_surface = None;
        self.plugin_presentation = renderer::PresentationState::default();
        self.plugin_action_result = None;
        self.pending_plugin_request = None;
        self.drop_entity_options_subscriptions();
        self.entity_options_invalid_fields.clear();
        if self.is_connected() {
            self.sync_entity_options_subscriptions();
        }
    }

    pub(super) fn apply_plugin_action_result(&mut self, result: UiActionResult) {
        let Some(request) = self.pending_plugin_request.as_ref() else {
            self.error = Some(format!(
                "ignored plugin action result without an in-flight request: {}",
                result.request_id.0
            ));
            return;
        };
        let Some(surface) = self.plugin_surface.as_mut() else {
            self.error = Some("ignored plugin action result without an active owner".to_string());
            return;
        };
        let identity_matches = result.request_id == request.request_id
            && result.surface_id == request.surface_id
            && result.action_id == request.action_id
            && result.node_id == request.node_id
            && result.surface_id.0 == surface.surface_id;
        if !identity_matches {
            self.error = Some(format!(
                "ignored mismatched plugin action result: request={} result={}",
                request.request_id.0, result.request_id.0
            ));
            return;
        }

        match renderer::apply_action_result(&mut self.plugin_presentation, &result) {
            Ok(transition) => {
                let body_replaced = transition.replacement.is_some();
                if let Some(replacement) = transition.replacement {
                    // The accepted owner replacement is canonical, including
                    // confirmation trees that drop entity-option producers.
                    surface.ui_tree_snapshot.body = replacement;
                }
                self.pending_plugin_request = None;
                self.action_feedback = Some(plugin_action_result_text(&result));
                self.plugin_action_result = Some(result);
                // Replacement can add/remove options_source families — resync demand.
                if body_replaced {
                    self.sync_entity_options_subscriptions();
                }
            }
            Err(error) => {
                self.error = Some(format!("invalid plugin action result: {error}"));
            }
        }
    }

    pub(super) fn handle_action(
        &mut self,
        action_id: String,
        values: Option<UiFormValues>,
        payload: Option<Value>,
    ) {
        if let Some(values) = values.as_ref() {
            self.apply_session_type_form_values(values);
            self.apply_spawn_flow_values(values);
        }

        match action_id.as_str() {
            "botster.tui.connect" => self.force_reconnect(),
            "botster.tui.spawn" => self.begin_target_first_spawn(),
            "botster.tui.attach" => {
                if let Some(session_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_id"))
                    .and_then(Value::as_str)
                {
                    self.set_selected_session(Some(session_id.to_string()));
                }
                self.attach_selected_or_first();
            }
            "botster.tui.detach" => self.detach_attached(),
            "botster.tui.refresh" => self.refresh_read_models(),
            "botster.tui.system.toggle" => {
                self.system_details_visible = !self.system_details_visible;
            }
            "botster.tui.session.shutdown" => {
                if let Some(session_id) =
                    session_id_from_payload(&payload).or_else(|| self.selected_session.clone())
                {
                    self.confirmation = Some(DestructiveAction::Shutdown(session_id));
                }
            }
            "botster.tui.session.remove" => {
                if let Some(session_id) =
                    session_id_from_payload(&payload).or_else(|| self.selected_session.clone())
                {
                    self.confirmation = Some(DestructiveAction::Remove(session_id));
                }
            }
            "botster.tui.session.restart" => {
                if let Some(session_id) =
                    session_id_from_payload(&payload).or_else(|| self.selected_session.clone())
                {
                    self.restart_session(&session_id);
                }
            }
            "botster.tui.confirm.cancel" => {
                self.confirmation = None;
            }
            "botster.tui.confirm.accept" => {
                if let Some(confirmation) = self.confirmation.take() {
                    match confirmation {
                        DestructiveAction::Shutdown(session_id) => {
                            self.action_feedback =
                                Some(format!("shutdown requested: {session_id}"));
                            self.submit_apply(DaemonRequest::ShutdownSession { session_id });
                        }
                        DestructiveAction::Remove(session_id) => {
                            self.action_feedback = Some(format!("remove requested: {session_id}"));
                            self.submit_apply(DaemonRequest::RemoveSession { session_id });
                        }
                    }
                }
            }
            "botster.tui.unsafe_paste.review" => {
                if let Some(PendingUnsafePaste::AwaitingConsent { stage, .. }) =
                    self.pending_unsafe_paste.as_mut()
                {
                    *stage = UnsafePasteConsentStage::Armed;
                }
            }
            "botster.tui.unsafe_paste.cancel" => self.invalidate_unsafe_paste(),
            "botster.tui.unsafe_paste.confirm" => self.confirm_unsafe_paste(),
            "botster.tui.navigation.open" => {
                if let Some((package_name, surface_id, route_id)) =
                    navigation_open_payload(&payload)
                {
                    self.open_package_navigation(package_name, surface_id, route_id);
                }
            }
            "botster.tui.package_config.submit" => {
                if let Some(package_name) = payload
                    .as_ref()
                    .and_then(|value| value.get("package_name"))
                    .and_then(Value::as_str)
                {
                    self.submit_package_configuration(package_name, values.as_ref());
                }
            }
            "botster.tui.package.show" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("show requested: {package_name}"));
                    self.submit_apply(DaemonRequest::ShowPackage { package_name });
                }
            }
            "botster.tui.quarantine.resolve" => {
                match payload.map(serde_json::from_value::<DaemonQuarantineTarget>) {
                    Some(Ok(target)) => {
                        self.action_feedback = Some(format!(
                            "resolve requested: {}",
                            quarantine_target_text(&target)
                        ));
                        self.submit(
                            DaemonRequest::ResolveQuarantine {
                                target: target.clone(),
                            },
                            PendingReply::ResolveQuarantine { target },
                            REQUEST_DEADLINE,
                        );
                    }
                    _ => self.error = Some("resolve: invalid quarantine target".to_string()),
                }
            }
            "botster.tui.package.logs" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("logs requested: {package_name}"));
                    self.submit_apply(DaemonRequest::ReadPluginLogs {
                        package_name,
                        after_seq: 0,
                    });
                }
            }
            "botster.tui.package.enable" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("enable requested: {package_name}"));
                    self.submit_apply(DaemonRequest::EnablePackage { package_name });
                }
            }
            "botster.tui.package.disable" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("disable requested: {package_name}"));
                    self.submit_apply(DaemonRequest::DisablePackage { package_name });
                }
            }
            "botster.tui.package.remove" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("remove requested: {package_name}"));
                    self.submit_apply(DaemonRequest::RemovePackage { package_name });
                }
            }
            "botster.tui.package.update_status" => {
                if let Some(package_name) = package_name_from_payload(&payload) {
                    self.action_feedback = Some(format!("update status requested: {package_name}"));
                    self.submit_apply(DaemonRequest::CheckPackageUpdate { package_name });
                }
            }
            "botster.tui.package.update_preview" => {
                if let Some((package_name, pin)) = package_name_and_pin_from_payload(&payload) {
                    self.action_feedback =
                        Some(format!("update preview requested: {package_name}"));
                    self.submit_apply(DaemonRequest::PreviewPackageUpdate { package_name, pin });
                }
            }
            "botster.tui.package.update_apply" => {
                if let Some((package_name, pin)) = package_name_and_pin_from_payload(&payload) {
                    self.action_feedback = Some(format!("update apply requested: {package_name}"));
                    self.submit_apply(DaemonRequest::ApplyPackageUpdate { package_name, pin });
                }
            }
            "botster.tui.entrypoint.start" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint start requested: {package_name}/{entrypoint_id}"
                    ));
                    self.submit_apply(DaemonRequest::StartPackageEntrypoint {
                        package_name,
                        entrypoint_id,
                        environment_overrides: BTreeMap::new(),
                    });
                }
            }
            "botster.tui.entrypoint.stop" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint stop requested: {package_name}/{entrypoint_id}"
                    ));
                    self.submit_apply(DaemonRequest::StopPackageEntrypoint {
                        package_name,
                        entrypoint_id,
                    });
                }
            }
            "botster.tui.entrypoint.restart" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint restart requested: {package_name}/{entrypoint_id}"
                    ));
                    self.submit_apply(DaemonRequest::RestartPackageEntrypoint {
                        package_name,
                        entrypoint_id,
                    });
                }
            }
            "botster.tui.entrypoint.status" => {
                if let Some((package_name, entrypoint_id)) =
                    package_entrypoint_from_payload(&payload)
                {
                    self.action_feedback = Some(format!(
                        "entrypoint status requested: {package_name}/{entrypoint_id}"
                    ));
                    self.submit_apply(DaemonRequest::PackageEntrypointStatus {
                        package_name,
                        entrypoint_id,
                    });
                }
            }
            // The input router already focuses the terminal. Attachment is an
            "botster.tui.session_type.select" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.selected_session_type_id = Some(session_type_id.to_string());
                }
            }
            "botster.tui.session_type.create" => {
                self.session_type_form = Some(SessionTypeFormDraft::create_default());
            }
            "botster.tui.session_type.edit" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.open_session_type_edit(session_type_id);
                }
            }
            "botster.tui.session_type.delete" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.delete_session_type(session_type_id);
                }
            }
            "botster.tui.session_type.form.cancel" => {
                self.session_type_form = None;
            }
            "botster.tui.session_type.form.submit" => {
                self.submit_session_type_form();
            }
            "botster.tui.spawn.cancel" => {
                self.target_first_spawn = None;
            }
            "botster.tui.spawn.pick_target" => {
                if let Some(target_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("target_id"))
                    .and_then(Value::as_str)
                {
                    self.spawn_pick_target(target_id);
                }
            }
            "botster.tui.spawn.pick_session_type" => {
                if let Some(session_type_id) = payload
                    .as_ref()
                    .and_then(|value| value.get("session_type_id"))
                    .and_then(Value::as_str)
                {
                    self.spawn_pick_session_type(session_type_id);
                }
            }
            "botster.tui.spawn.submit" => {
                self.submit_target_first_spawn();
            }
            // explicit session activation and must not be a terminal side effect.
            "botster.terminal.focus" => {}
            _ => {}
        }
    }
}
