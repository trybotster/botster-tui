use super::*;

impl TuiApp {
    /// Subscribe to the built-in session family on the current connection.
    pub(super) fn start_session_subscription(&mut self) {
        let subscription_id = format!("btui-sessions-{}", short_suffix());
        self.session_entities
            .begin_generation(subscription_id.clone());
        self.rebuild_session_rows();
        self.submit(
            DaemonRequest::SubscribeEntities {
                entity_type: "session".to_string(),
                subscription_id: subscription_id.clone(),
            },
            PendingReply::SubscribeEntities {
                family: "session".to_string(),
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    /// Drop the session generation and unsubscribe when connected.
    pub(super) fn invalidate_session_generation(&mut self) {
        if let Some(subscription_id) = self.session_entities.subscription_id.take()
            && self.is_connected()
        {
            self.submit(
                DaemonRequest::UnsubscribeEntities { subscription_id },
                PendingReply::Unsubscribe,
                REQUEST_DEADLINE,
            );
        }
        self.session_entities = SessionEntityState::default();
        self.rebuild_session_rows();
    }

    pub(super) fn start_session_type_subscription(&mut self) {
        let subscription_id = format!("btui-session-types-{}", short_suffix());
        self.session_type_entities
            .begin_generation(subscription_id.clone());
        self.session_type_subscription_error = None;
        self.submit(
            DaemonRequest::SubscribeEntities {
                entity_type: "session_type".to_string(),
                subscription_id: subscription_id.clone(),
            },
            PendingReply::SubscribeEntities {
                family: "session_type".to_string(),
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    pub(super) fn invalidate_session_type_generation(&mut self) {
        if let Some(subscription_id) = self.session_type_entities.subscription_id.take()
            && self.is_connected()
        {
            self.submit(
                DaemonRequest::UnsubscribeEntities { subscription_id },
                PendingReply::Unsubscribe,
                REQUEST_DEADLINE,
            );
        }
        self.session_type_entities = SessionTypeEntityState::default();
        self.session_type_subscription_error = None;
    }

    /// Route one entity frame from the connection to its family state.
    pub(super) fn apply_entity_frame(&mut self, frame: DaemonEntityFrame) {
        let entity_type = entity_frame_type(&frame).to_string();
        match entity_type.as_str() {
            "session" => match self.session_entities.apply(frame) {
                Ok(true) => {
                    self.rebuild_session_rows();
                    // Session family feeds entity-options when demanded.
                    self.reconcile_entity_option_drafts();
                }
                Ok(false) => {}
                Err(error) => {
                    self.error = Some(format!("session sync: {error}"));
                    self.invalidate_session_generation();
                    if self.is_connected() {
                        self.start_session_subscription();
                    }
                }
            },
            "session_type" => match self.session_type_entities.apply(frame) {
                Ok(SessionTypeFrameOutcome::Applied { replaced }) => {
                    if replaced {
                        self.session_type_subscription_error = None;
                    }
                    if self
                        .selected_session_type_id
                        .as_ref()
                        .is_some_and(|id| !self.session_type_entities.entities.contains_key(id))
                    {
                        self.selected_session_type_id = None;
                    }
                }
                Ok(SessionTypeFrameOutcome::Ignored) => {}
                Ok(SessionTypeFrameOutcome::HubError(error)) => {
                    self.session_type_subscription_error = Some(error);
                }
                Err(error) => {
                    self.session_type_subscription_error = Some(error.clone());
                    self.error = Some(format!("session type sync: {error}"));
                    self.invalidate_session_type_generation();
                    self.session_type_subscription_error = Some(error);
                    if self.is_connected() {
                        self.start_session_type_subscription();
                    }
                }
            },
            family => self.apply_entity_options_frame(family, frame),
        }
    }

    pub(super) fn complete_entity_subscription(
        &mut self,
        family: &str,
        subscription_id: &str,
        response: DaemonResponse,
    ) {
        self.record_diagnostics(response.diagnostics);
        if response.kind == DaemonResponseKind::EntitySubscribed && response.error.is_none() {
            if !is_process_wide_entity_family(family) {
                self.reset_entity_options_backoff(family);
            }
            return;
        }
        let detail = response
            .error
            .as_ref()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| format!("{:?}", response.kind));
        if let Some(error) = response.error {
            self.record_diagnostics(error.diagnostics);
        }
        self.fail_entity_subscription(
            family,
            subscription_id,
            format!("entity subscription was not accepted: {detail}"),
        );
    }

    pub(super) fn fail_entity_subscription(
        &mut self,
        family: &str,
        subscription_id: &str,
        message: String,
    ) {
        match family {
            "session" => {
                if self.session_entities.subscription_id.as_deref() == Some(subscription_id) {
                    self.session_entities = SessionEntityState::default();
                    self.rebuild_session_rows();
                    self.error = Some(format!("session subscription failed: {message}"));
                }
            }
            "session_type" => {
                if self.session_type_entities.subscription_id.as_deref() == Some(subscription_id) {
                    self.session_type_entities = SessionTypeEntityState::default();
                    self.session_type_subscription_error = Some(message.clone());
                    self.error = Some(format!("session type subscription failed: {message}"));
                }
            }
            other => {
                let matches = self
                    .entity_options
                    .family(other)
                    .and_then(|state| state.subscription_id.as_deref())
                    == Some(subscription_id);
                if matches {
                    self.entity_options_subscriptions.remove(other);
                    self.entity_options.drop_family(other);
                    self.note_entity_options_admission_failure(other, message);
                }
            }
        }
    }

    pub(super) fn drop_entity_options_subscriptions(&mut self) {
        self.drop_entity_options_families(None);
    }

    pub(super) fn drop_entity_options_families(&mut self, keep: Option<BTreeSet<String>>) {
        let keep = keep.unwrap_or_default();
        let stale: Vec<String> = self
            .entity_options_subscriptions
            .iter()
            .filter(|family| !keep.contains(*family))
            .cloned()
            .collect();
        for family in stale {
            self.stop_entity_options_subscription(&family);
        }
        if keep.is_empty() {
            self.entity_options = EntityOptionsStore::default();
            self.entity_options_retry.clear();
        } else {
            self.entity_options.retain_families(&keep);
        }
    }

    /// Collect options_source families from the active plugin surface and ensure
    /// SubscribeEntities for non-process-wide families. Process-wide families
    /// (session, session_type) are served from the existing stores.
    pub(super) fn sync_entity_options_subscriptions(&mut self) {
        let owned = self.demanded_entity_option_families_now();

        let stale: Vec<String> = self
            .entity_options_subscriptions
            .iter()
            .filter(|family| !owned.contains(*family))
            .cloned()
            .collect();
        for family in stale {
            self.stop_entity_options_subscription(&family);
        }
        self.entity_options.retain_families(&owned);

        if !self.is_connected() {
            self.reconcile_entity_option_drafts();
            return;
        }

        for family in owned {
            if self.entity_options_subscriptions.contains(&family) {
                continue;
            }
            if !self.entity_options_retry_ready(&family) {
                continue;
            }
            self.start_entity_options_subscription(&family);
        }
        self.reconcile_entity_option_drafts();
    }

    /// Unsubscribe one entity-options family and forget its generation.
    pub(super) fn stop_entity_options_subscription(&mut self, family: &str) {
        self.entity_options_subscriptions.remove(family);
        let subscription_id = self
            .entity_options
            .family(family)
            .and_then(|state| state.subscription_id.clone());
        if let Some(subscription_id) = subscription_id
            && self.is_connected()
        {
            self.submit(
                DaemonRequest::UnsubscribeEntities { subscription_id },
                PendingReply::Unsubscribe,
                REQUEST_DEADLINE,
            );
        }
        self.entity_options.drop_family(family);
        self.entity_options_retry.remove(family);
    }

    pub(super) fn start_entity_options_subscription(&mut self, entity_type: &str) {
        #[cfg(test)]
        {
            *self
                .entity_options_subscribe_attempts
                .entry(entity_type.to_string())
                .or_insert(0) += 1;
            if let Some(message) = self.entity_options_forced_subscribe_error {
                self.note_entity_options_admission_failure(entity_type, message.to_string());
                return;
            }
        }
        let subscription_id = format!("btui-entity-options-{entity_type}-{}", short_suffix());
        self.entity_options
            .begin_generation(entity_type, subscription_id.clone());
        self.entity_options_subscriptions
            .insert(entity_type.to_string());
        self.submit(
            DaemonRequest::SubscribeEntities {
                entity_type: entity_type.to_string(),
                subscription_id: subscription_id.clone(),
            },
            PendingReply::SubscribeEntities {
                family: entity_type.to_string(),
                subscription_id,
            },
            REQUEST_DEADLINE,
        );
    }

    /// Re-open demanded entity-options families whose backoff expired.
    pub(super) fn heal_entity_options_subscriptions(&mut self) {
        let demanded: Vec<String> = self
            .demanded_entity_option_families_now()
            .into_iter()
            .filter(|family| !self.entity_options_subscriptions.contains(family))
            .collect();
        for family in demanded {
            if !self.entity_options_retry_ready(&family) {
                continue;
            }
            self.start_entity_options_subscription(&family);
        }
    }

    pub(super) fn note_entity_options_admission_failure(&mut self, family: &str, error: String) {
        let previous = self.entity_options_retry.get(family).cloned();
        let consecutive_failures = previous
            .as_ref()
            .map(|state| state.consecutive_failures.saturating_add(1))
            .unwrap_or(1);
        let delay = entity_options_backoff_delay(consecutive_failures);
        self.entity_options_retry.insert(
            family.to_string(),
            EntityOptionsRetryState {
                consecutive_failures,
                // timer: backoff — entity-options subscribe admission failure; capped exponential retry delay
                next_attempt_at: Instant::now() + delay,
            },
        );
        if previous.is_none() {
            self.error = Some(format!(
                "entity options subscription failed for {family}: {error}"
            ));
        }
    }

    /// Apply one entity-options frame. A sync error drops the generation and
    /// opens a fresh SubscribeEntities when the family is still demanded.
    pub(super) fn apply_entity_options_frame(&mut self, family: &str, frame: DaemonEntityFrame) {
        match self.entity_options.apply_daemon_frame(frame) {
            Ok(true) => self.reconcile_entity_option_drafts(),
            Ok(false) => {}
            Err(error) => {
                self.error = Some(format!("entity options sync: {error}"));
                self.stop_entity_options_subscription(family);
                if self.is_connected()
                    && self.family_still_demanded(family)
                    && self.entity_options_retry_ready(family)
                {
                    self.start_entity_options_subscription(family);
                }
            }
        }
    }

    pub(super) fn rebuild_session_rows(&mut self) {
        self.pending_sessions
            .retain(|session_id, _| !self.session_entities.entities.contains_key(session_id));
        self.sessions = self
            .session_entities
            .entity_order
            .iter()
            .filter_map(|session_id| self.session_entities.entities.get(session_id))
            .map(SessionRow::from_entity)
            .chain(self.pending_sessions.values().cloned())
            .collect();
        if self.selected_session.as_ref().is_none_or(|selected| {
            !self
                .sessions
                .iter()
                .any(|session| session.session_id == *selected)
        }) {
            self.set_selected_session(
                self.sessions
                    .first()
                    .map(|session| session.session_id.clone()),
            );
        }
    }

    /// Build the multi-family store for shared projection, injecting process-wide
    /// session / session_type maps when those families are demanded.
    pub(super) fn entity_options_projection_store(&self) -> EntityFamilyStore {
        let mut process_wide = EntityFamilyStore::new();
        if !self.session_entities.entities.is_empty() || self.session_entities.has_snapshot {
            let mut session_records = BTreeMap::new();
            for (id, entity) in &self.session_entities.entities {
                if let Ok(Value::Object(fields)) = serde_json::to_value(entity) {
                    session_records.insert(id.clone(), fields);
                }
            }
            process_wide.insert("session".to_string(), session_records);
        }
        if !self.session_type_entities.entities.is_empty()
            || self.session_type_entities.has_snapshot
        {
            let mut type_records = BTreeMap::new();
            for (id, entity) in &self.session_type_entities.entities {
                if let Ok(Value::Object(fields)) = serde_json::to_value(entity) {
                    type_records.insert(id.clone(), fields);
                }
            }
            process_wide.insert("session_type".to_string(), type_records);
        }
        self.entity_options
            .projection_store_with_process_wide(&process_wide)
    }

    pub(super) fn demanded_entity_option_families_now(&self) -> BTreeSet<String> {
        let mut owned = BTreeSet::new();
        if let Some(surface) = self.plugin_surface.as_ref() {
            owned.extend(
                demanded_entity_option_families(&surface.ui_tree_snapshot.body)
                    .into_iter()
                    .filter(|family| !is_process_wide_entity_family(family)),
            );
        }
        owned
    }

    pub(super) fn entity_options_retry_ready(&self, family: &str) -> bool {
        self.entity_options_retry
            .get(family)
            .is_none_or(|state| Instant::now() >= state.next_attempt_at)
    }

    pub(super) fn reset_entity_options_backoff(&mut self, family: &str) {
        self.entity_options_retry.remove(family);
    }

    pub(super) fn family_still_demanded(&self, family: &str) -> bool {
        self.demanded_entity_option_families_now().contains(family)
    }

    /// Clear drafts whose selected values disappeared or became excluded.
    pub(super) fn reconcile_entity_option_drafts(&mut self) {
        let Some(surface) = self.plugin_surface.as_ref() else {
            return;
        };
        let store = self.entity_options_projection_store();
        let mut invalid = BTreeSet::new();
        collect_invalid_entity_option_fields(
            &surface.ui_tree_snapshot.body,
            &store,
            &self.drafts,
            &mut invalid,
        );
        for field in &invalid {
            self.drafts.remove(field);
            self.entity_options_invalid_fields.insert(field.clone());
        }
    }

    pub(super) fn apply_session_type_form_values(&mut self, values: &UiFormValues) {
        let Some(form) = self.session_type_form.as_mut() else {
            return;
        };
        let set = |key: &str, target: &mut String| {
            if let Some(value) = values.0.get(key).and_then(Value::as_str) {
                *target = value.to_string();
            }
        };
        set("session_type_source", &mut form.source);
        set("session_type_source_target_id", &mut form.source_target_id);
        set("session_type_id", &mut form.id);
        set("session_type_label", &mut form.label);
        set("session_type_description", &mut form.description);
        set("session_type_icon", &mut form.icon);
        set("session_type_role", &mut form.role);
        set("session_type_interaction", &mut form.interaction);
        set("session_type_traits", &mut form.traits);
        set("session_type_lifecycle", &mut form.lifecycle);
        set("session_type_execution", &mut form.execution);
        set("session_type_command", &mut form.command);
        set("session_type_args", &mut form.args);
        set(
            "session_type_working_directory_policy",
            &mut form.working_directory_policy,
        );
        set(
            "session_type_working_directory_path",
            &mut form.working_directory_path,
        );
        set("session_type_environment", &mut form.environment);
        set(
            "session_type_allowed_environment_overrides",
            &mut form.allowed_environment_overrides,
        );
        set("session_type_context_keys", &mut form.context_keys);
    }
}
