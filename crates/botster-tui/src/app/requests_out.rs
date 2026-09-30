use super::*;

impl TuiApp {
    pub(super) fn refresh_read_models(&mut self) {
        self.refresh_status();
        self.refresh_apps();
        self.refresh_package_navigation();
        self.refresh_packages();
        self.refresh_spawn_targets();
    }

    /// Submit one request whose response only updates read models.
    pub(super) fn submit_apply(&mut self, request: DaemonRequest) {
        self.submit(request, PendingReply::Apply, REQUEST_DEADLINE);
    }

    /// Submit one request with a continuation. Returns the request id.
    ///
    /// Completion, expiry, or loss arrives as `AppWake::Completed` and is
    /// routed through `complete_request`. Nothing waits here.
    pub(super) fn submit(
        &mut self,
        request: DaemonRequest,
        reply: PendingReply,
        deadline: Duration,
    ) -> u64 {
        #[cfg(test)]
        self.record_request(&request);
        // timer: deadline — host-control request expiry; expiry completes the request with DeadlineExpired
        let request_id = self.hub_io.submit(&request, Instant::now() + deadline);
        self.pending_requests.insert(request_id, reply);
        request_id
    }

    pub(super) fn complete_request(
        &mut self,
        request_id: u64,
        result: Result<DaemonResponse, DaemonRequestError>,
    ) {
        let Some(reply) = self.pending_requests.remove(&request_id) else {
            return;
        };
        match result {
            Ok(response) => self.apply_completion(reply, response),
            Err(error) => self.apply_request_failure(reply, error),
        }
    }

    pub(super) fn apply_completion(&mut self, reply: PendingReply, response: DaemonResponse) {
        match reply {
            PendingReply::Apply | PendingReply::Unsubscribe => {
                self.apply_response(response);
            }
            PendingReply::ResolveQuarantine { target } => {
                let resolved = response.error.is_none()
                    && matches!(response.kind, DaemonResponseKind::QuarantineResolved);
                if resolved && matches!(target, DaemonQuarantineTarget::Package { .. }) {
                    self.packages = response.packages.clone();
                    self.sync_notice_subscriptions();
                }
                self.apply_response(response);
            }
            PendingReply::Detach { session_id, route } => {
                // Success is a correlated Events response with no operator
                // error; anything else is a failed detach, never a release.
                let state = match &response.error {
                    None if response.kind == DaemonResponseKind::Events => DetachState::Confirmed,
                    Some(error) => {
                        DetachState::Failed(format!("{} (code={})", error.message, error.code))
                    }
                    None => DetachState::Failed(format!("unexpected {:?} response", response.kind)),
                };
                self.finish_detach(&session_id, &route, state);
                self.apply_response(response);
            }
            PendingReply::SpawnTargets => {
                self.spawn_targets_failure = response
                    .error
                    .as_ref()
                    .map(|error| format!("{} (code={})", error.message, error.code));
                self.apply_response(response);
            }
            PendingReply::ListForTarget {
                target_id,
                target_label,
            } => self.apply_list_for_target(&target_id, &target_label, response),
            PendingReply::Spawn { session_id } => {
                let failed = response.error.is_some();
                self.apply_response(response);
                if failed {
                    self.pending_sessions.remove(&session_id);
                    self.rebuild_session_rows();
                } else if self.pending_sessions.contains_key(&session_id) {
                    self.action_feedback = Some(format!(
                        "spawn accepted: {session_id}; waiting for authoritative session"
                    ));
                }
            }
            PendingReply::ShowSessionTypeDefinition { session_type_id } => {
                self.apply_show_session_type_definition(&session_type_id, response);
            }
            PendingReply::SessionTypeForm => {
                let failed = response.error.clone();
                self.apply_response(response);
                match failed {
                    Some(error) => {
                        if let Some(form) = self.session_type_form.as_mut() {
                            form.error = Some(format!("{}: {}", error.code, error.message));
                        }
                    }
                    None => self.session_type_form = None,
                }
            }
            PendingReply::Attach { session_id, route } => {
                let failed = response.error.clone();
                let attached = response.terminal_attach.clone();
                self.apply_response(response);
                if !self.hydration_matches_route(&route) {
                    return;
                }
                if let Some(error) = failed {
                    self.fail_attach_campaign(
                        &session_id,
                        &route,
                        &format!("attach rejected: {}", error.message),
                        false,
                    );
                    return;
                }
                // The Attach response is the only source of the attachment
                // generation.
                match attached {
                    Some(attach) if attach.subscription_id == route => {
                        self.route_generation = Some(attach.generation);
                    }
                    _ => self.fail_attach_campaign(
                        &session_id,
                        &route,
                        "attach response omitted the terminal attachment",
                        true,
                    ),
                }
            }
            PendingReply::SubscribeEvents {
                key,
                subscription_id,
            } => self.complete_notice_subscription(&key, &subscription_id, response),
            PendingReply::SubscribeEntities {
                family,
                subscription_id,
            } => self.complete_entity_subscription(&family, &subscription_id, response),
        }
    }

    pub(super) fn apply_request_failure(&mut self, reply: PendingReply, error: DaemonRequestError) {
        let message = error.to_string();
        match reply {
            PendingReply::Apply | PendingReply::ResolveQuarantine { .. } => {
                self.error = Some(format!("request failed: {message}"));
            }
            PendingReply::SpawnTargets => self.spawn_targets_failure = Some(message),
            PendingReply::Unsubscribe => {}
            PendingReply::Detach { session_id, route } => {
                self.finish_detach(&session_id, &route, DetachState::Failed(message));
            }
            PendingReply::ListForTarget { target_label, .. } => {
                self.error = Some(format!("session types failed to load: {message}"));
                self.action_feedback = Some(format!(
                    "session types for {target_label} failed to load; pick another target or cancel"
                ));
            }
            PendingReply::Spawn { session_id } => {
                self.pending_sessions.remove(&session_id);
                self.rebuild_session_rows();
                self.error = Some(format!("spawn failed: {message}"));
            }
            PendingReply::ShowSessionTypeDefinition { session_type_id } => {
                self.error = Some(format!(
                    "show_session_type_definition failed for {session_type_id}: {message}"
                ));
            }
            PendingReply::SessionTypeForm => {
                if let Some(form) = self.session_type_form.as_mut() {
                    form.error = Some(message);
                }
            }
            PendingReply::Attach { session_id, route } => {
                if self.hydration_matches_route(&route) {
                    // The Hub may have attached after the deadline; retire the route
                    // with a bounded Detach so a late attachment cannot leak.
                    self.fail_attach_campaign(
                        &session_id,
                        &route,
                        &format!("attach request failed: {message}"),
                        true,
                    );
                }
            }
            PendingReply::SubscribeEvents {
                subscription_id, ..
            } => self.reject_event_subscription_candidate(
                &subscription_id,
                format!("event subscription failed: {message}"),
            ),
            PendingReply::SubscribeEntities {
                family,
                subscription_id,
            } => self.fail_entity_subscription(&family, &subscription_id, message),
        }
    }

    pub(super) fn refresh_spawn_targets(&mut self) {
        self.submit(
            DaemonRequest::ListSpawnTargets,
            PendingReply::SpawnTargets,
            REQUEST_DEADLINE,
        );
    }

    pub(super) fn refresh_status(&mut self) {
        self.submit_apply(DaemonRequest::Status);
    }

    pub(super) fn refresh_apps(&mut self) {
        self.submit_apply(DaemonRequest::ListApps);
    }

    pub(super) fn refresh_package_navigation(&mut self) {
        self.submit_apply(DaemonRequest::ListPackageNavigation);
    }

    pub(super) fn refresh_packages(&mut self) {
        self.submit_apply(DaemonRequest::ListPackages);
    }
}
