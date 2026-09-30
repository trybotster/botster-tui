use super::*;

impl TuiApp {
    /// Application state without a terminal input thread. The caller starts
    /// the connection with `connect`.
    #[cfg(test)]
    pub(super) fn new(endpoint: Option<DaemonEndpoint>) -> Self {
        Self::new_with_connection(endpoint, None)
    }

    #[cfg(test)]
    pub(super) fn new_with_connection(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
    ) -> Self {
        Self::new_with_runtime_context(endpoint, connection_error, false, HubIo::new())
    }

    pub(super) fn new_with_runtime_context(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
        package_storage_context_configured: bool,
        hub_io: HubIo,
    ) -> Self {
        Self::new_with_runtime_context_and_requirement(
            endpoint,
            connection_error,
            package_storage_context_configured,
            tui_compatibility_requirement(),
            hub_io,
        )
    }

    pub(super) fn new_with_runtime_context_and_requirement(
        endpoint: Option<DaemonEndpoint>,
        connection_error: Option<String>,
        package_storage_context_configured: bool,
        host_requirement: DaemonCompatibilityRequirement,
        hub_io: HubIo,
    ) -> Self {
        Self {
            endpoint,
            host_requirement,
            hub_io,
            connected_generation: None,
            reconnect_at: None,
            reconnect_failures: 0,
            pending_requests: BTreeMap::new(),
            status: "disconnected".to_string(),
            connection_error,
            error: None,
            action_feedback: None,
            compatibility: None,
            software: None,
            diagnostics: Vec::new(),
            package_count: 0,
            enabled_package_count: 0,
            quarantines: Vec::new(),
            hub_counters: DaemonObservabilityCounters::default(),
            plugin_logs: BTreeMap::new(),
            apps: Vec::new(),
            package_navigation: Vec::new(),
            packages: Vec::new(),
            available_packages: Vec::new(),
            install_plan: None,
            update_status: None,
            package_decision: None,
            plugin_surface: None,
            plugin_presentation: renderer::PresentationState::default(),
            plugin_action_result: None,
            pending_plugin_request: None,
            session_entities: SessionEntityState::default(),
            pending_sessions: BTreeMap::new(),
            session_type_entities: SessionTypeEntityState::default(),
            session_type_subscription_error: None,
            entity_options: EntityOptionsStore::default(),
            entity_options_subscriptions: BTreeSet::new(),
            entity_options_retry: BTreeMap::new(),
            entity_options_invalid_fields: BTreeSet::new(),
            notice_subscriptions: BTreeMap::new(),
            notice_subscription_by_id: BTreeMap::new(),
            notice_parked: BTreeMap::new(),
            notice_overflow_dropped: 0,
            transient_notice: None,
            spawn_targets: Vec::new(),
            spawn_targets_loaded: false,
            spawn_targets_failure: None,
            selected_session_type_id: None,
            session_type_form: None,
            target_first_spawn: None,
            sessions: Vec::new(),
            selected_session: None,
            attached: None,
            schema_version: None,
            subscription_id: format!("btui-sub-{}", short_suffix()),
            next_terminal_subscription_sequence: 1,
            route_generation: None,
            route_epoch: None,
            ghostty_projection: None,
            ghostty_projection_session_id: None,
            ghostty_viewport_cache: None,
            projection_dirty: false,
            attach_hydration: None,
            attach_recovery_used: false,
            recovery_notice: None,
            retired_subscription_ids: BTreeSet::new(),
            terminal_close_evidence: None,
            detaches: BTreeMap::new(),
            terminal_modes: None,
            input_window: InputWindow::new(),
            pending_unsafe_paste: None,
            terminal_viewport_size: TerminalScreenSize::new(
                DEFAULT_TERMINAL_ROWS,
                DEFAULT_TERMINAL_COLS,
            ),
            drafts: BTreeMap::new(),
            system_details_visible: false,
            package_storage_context_configured,
            confirmation: None,
            #[cfg(test)]
            workspace_test_mode: false,
            #[cfg(test)]
            observed_requests: Vec::new(),
            #[cfg(test)]
            observed_terminal_inputs: Vec::new(),
            #[cfg(test)]
            applied_live_payloads: Vec::new(),
            #[cfg(test)]
            entity_options_forced_subscribe_error: None,
            #[cfg(test)]
            entity_options_subscribe_attempts: BTreeMap::new(),
        }
    }

    /// Whether a Hello-complete connection is installed.
    pub(super) fn is_connected(&self) -> bool {
        self.connected_generation.is_some() && self.hub_io.is_connected()
    }

    /// Session id of the live attached route.
    pub(super) fn attached_session_id(&self) -> Option<&str> {
        self.attached
            .as_ref()
            .map(|attached| attached.session_id.as_str())
    }

    /// Earliest absolute deadline the loop must wake for.
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        let mut deadline = self.hub_io.earliest_deadline();
        let mut consider = |candidate: Option<Instant>| {
            if let Some(candidate) = candidate {
                deadline = Some(deadline.map_or(candidate, |current| current.min(candidate)));
            }
        };
        consider(self.reconnect_at);
        consider(self.transient_notice.as_ref().map(|notice| notice.deadline));
        consider(
            self.pending_unsafe_paste
                .as_ref()
                .and_then(|pending| match pending {
                    PendingUnsafePaste::AwaitingConsent { deadline, .. } => Some(*deadline),
                    PendingUnsafePaste::AwaitingResult { .. } => None,
                }),
        );
        consider(
            self.entity_options_retry
                .values()
                .map(|state| state.next_attempt_at)
                .min(),
        );
        deadline
    }

    /// Block for the next wake or the earliest deadline.
    pub(super) fn next_wake(&mut self) -> AppWake {
        let until = self.next_deadline();
        self.hub_io.next_wake(until)
    }

    /// Take one wake without blocking.
    pub(super) fn try_next_wake(&mut self) -> Option<AppWake> {
        self.hub_io.try_next_wake()
    }

    /// Apply one non-input wake.
    pub(super) fn apply_wake(&mut self, wake: AppWake) {
        match wake {
            AppWake::Input(_) | AppWake::Shutdown => {}
            AppWake::Terminal(routed) => self.apply_routed_terminal_frame(routed),
            AppWake::Completed { request_id, result } => {
                self.complete_request(request_id, result.map(|response| *response))
            }
            AppWake::Event(event) => self.apply_mux_event(event),
            AppWake::Entity(frame) => self.apply_entity_frame(frame),
            AppWake::Connected { generation, ack } => self.apply_connected(generation, *ack),
            AppWake::Disconnected { generation, error } => {
                self.apply_disconnected(generation, error);
            }
            AppWake::RouteFault {
                route,
                generation,
                reason,
            } => self.apply_route_fault(route, generation, &reason),
            AppWake::Deadline => self.apply_deadlines(),
        }
    }

    /// Run every absolute-deadline action that is due.
    pub(super) fn apply_deadlines(&mut self) {
        let now = Instant::now();
        self.expire_transient_notice();
        self.expire_unsafe_paste_consent(now);
        if self.reconnect_at.is_some_and(|at| at <= now) {
            self.reconnect_at = None;
            self.connect();
        }
        if self.is_connected() {
            self.heal_entity_options_subscriptions();
        }
    }

    /// Project the viewport once when the projection changed since last paint.
    pub(super) fn prepare_paint(&mut self) {
        self.expire_transient_notice();
        self.expire_unsafe_paste_consent(Instant::now());
        if self.projection_dirty {
            self.refresh_ghostty_viewport_cache();
        }
    }

    /// Stop the I/O owner within the shutdown bound. Returns whether every
    /// I/O thread confirmed its stop.
    pub(super) fn shutdown(self) -> bool {
        let mut app = self;
        app.detach_owner_if_writable();
        app.hub_io.shutdown(SHUTDOWN_BOUND)
    }
}
