use super::*;

impl TuiApp {
    pub(super) fn attach_selected_or_first(&mut self) {
        let Some(session_id) = self.selected_attachable_session_id() else {
            return;
        };
        self.error = None;
        self.set_selected_session(Some(session_id.clone()));
        self.action_feedback = Some(format!("attach requested: {session_id}"));
        self.detach_owner_if_writable();
        self.reset_attach_campaign();
        let route = self.mint_subscription_id();
        self.begin_attach_hydration(&session_id, &route);
        self.submit(
            DaemonRequest::Attach {
                session_id: session_id.clone(),
                subscription_id: route.clone(),
            },
            PendingReply::Attach { session_id, route },
            REQUEST_DEADLINE,
        );
    }

    pub(super) fn begin_attach_hydration(&mut self, session_id: &str, route: &str) {
        // Every Attach owns a unique route and one incremental decoder.
        self.subscription_id = route.to_string();
        self.attached = None;
        self.route_generation = None;
        self.route_epoch = None;
        self.resolve_unknown_input_operations("route replaced");
        self.terminal_modes = None;
        self.clear_ghostty_projection();
        self.drop_attach_hydration();
        self.attach_hydration = Some(AttachHydration::new(session_id, route));
    }

    /// The Attach request itself failed. Close the campaign without a retry.
    pub(super) fn fail_attach_campaign(
        &mut self,
        session_id: &str,
        route: &str,
        reason: &str,
        detach: bool,
    ) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_route_state();
        self.retire_subscription(route);
        if detach && self.is_connected() {
            self.send_bounded_detach(session_id.to_string(), route.to_string());
        }
        self.error = Some(format!("attach failed (closed): {reason}: {session_id}"));
    }

    pub(super) fn detach_attached(&mut self) {
        let cancelling_hydration = self.attach_hydration.is_some();
        let Some((session_id, route)) = self.current_owner_pair() else {
            self.error = Some("no attached terminal stream to detach".to_string());
            return;
        };
        self.error = None;
        self.recovery_notice = None;
        self.action_feedback = Some(format!("detach requested: {session_id}"));
        if cancelling_hydration && let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        // A detached projection stays readable for scrollback; hydration state does not.
        if cancelling_hydration {
            self.clear_ghostty_projection();
        }
        self.retire_subscription(&route);
        self.drop_attach_hydration();
        self.attached = None;
        self.terminal_modes = None;
        self.send_bounded_detach(session_id, route);
    }

    pub(super) fn send_bounded_detach(&mut self, session_id: String, route: String) {
        self.detaches
            .insert(session_id.clone(), (route.clone(), DetachState::Pending));
        self.submit(
            DaemonRequest::Detach {
                session_id: session_id.clone(),
                subscription_id: route.clone(),
            },
            PendingReply::Detach { session_id, route },
            DETACH_ON_DISCONNECT_BOUND,
        );
    }

    /// Record the Hub's answer to one Detach. A stale answer for a route the
    /// session no longer owns changes nothing.
    pub(super) fn finish_detach(&mut self, session_id: &str, route: &str, state: DetachState) {
        let Some((current, slot)) = self.detaches.get_mut(session_id) else {
            return;
        };
        if current != route {
            return;
        }
        if let DetachState::Failed(reason) = &state {
            self.error = Some(format!("detach of {session_id} failed: {reason}"));
        }
        *slot = state;
    }

    /// Retire the current route and send a bounded Detach when connected.
    pub(super) fn detach_owner_if_writable(&mut self) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            return;
        };
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.retire_subscription(&route);
        if !self.is_connected() {
            return;
        }
        self.send_bounded_detach(session_id, route);
    }

    /// Retired routes ignore late frames and close events. The input window
    /// and adopted generation belong to the route and are dropped with it.
    pub(super) fn retire_subscription(&mut self, route: &str) {
        self.retired_subscription_ids.insert(route.to_string());
        self.hub_io.forget_route(route);
        self.resolve_unknown_input_operations("route retired");
        self.route_generation = None;
        self.route_epoch = None;
    }

    pub(super) fn current_owner_pair(&self) -> Option<(String, String)> {
        self.attach_hydration
            .as_ref()
            .map(|hydration| (hydration.session_id.clone(), hydration.route.clone()))
            .or_else(|| {
                self.attached
                    .as_ref()
                    .map(|attached| (attached.session_id.clone(), attached.route.clone()))
            })
    }

    pub(super) fn reset_attach_campaign(&mut self) {
        self.attach_recovery_used = false;
        self.recovery_notice = None;
        self.retired_subscription_ids.clear();
        self.terminal_close_evidence = None;
    }

    pub(super) fn mint_subscription_id(&mut self) -> String {
        let sequence = self.next_terminal_subscription_sequence;
        self.next_terminal_subscription_sequence = sequence.saturating_add(1);
        format!("btui-sub-{}-{sequence}", short_suffix())
    }

    pub(super) fn selected_attachable_session_id(&mut self) -> Option<String> {
        let Some(session_id) = self.selected_session.clone().or_else(|| {
            self.sessions
                .first()
                .map(|session| session.session_id.clone())
        }) else {
            self.error = Some("no session available to attach".to_string());
            return None;
        };
        self.set_selected_session(Some(session_id.clone()));

        let Some(session) = self
            .sessions
            .iter()
            .find(|candidate| candidate.session_id == session_id)
        else {
            self.error = Some(format!("{session_id} is not listed - cannot attach"));
            return None;
        };

        if session.is_attachable() {
            return Some(session_id);
        }

        self.error = Some(format!(
            "{} {} - cannot attach",
            session.session_id, session.lifecycle
        ));
        None
    }

    pub(super) fn submit_package_configuration(
        &mut self,
        package_name: &str,
        values: Option<&UiFormValues>,
    ) {
        let Some(values) = values else {
            self.error = Some("configuration form values were not submitted".to_string());
            return;
        };
        let Some(package) = self
            .packages
            .iter()
            .find(|package| package.package_name == package_name)
        else {
            self.error = Some(format!("package not found: {package_name}"));
            return;
        };

        let mut updates = BTreeMap::new();
        for field in package_configuration_fields(package) {
            let field_name = package_config_field_name(package_name, &field.key);
            let Some(draft) = values.0.get(&field_name) else {
                continue;
            };
            if let Some(value) = package_configuration_submit_value(&field, draft) {
                updates.insert(field.key, value);
            }
        }

        if updates.is_empty() {
            self.error = Some(format!("no configuration changes for {package_name}"));
            return;
        }

        self.error = None;
        self.action_feedback = Some(format!("configuration update requested: {package_name}"));
        self.submit_apply(DaemonRequest::SetPackageConfiguration {
            package_name: package_name.to_string(),
            values: updates,
        });
    }
}
