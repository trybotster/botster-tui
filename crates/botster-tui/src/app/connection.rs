use super::*;

impl TuiApp {
    /// Start a connection attempt on the I/O owner. Hello completes as
    /// `AppWake::Connected` or `AppWake::Disconnected`.
    pub(super) fn connect(&mut self) {
        self.reconnect_at = None;
        let Some(endpoint) = self.endpoint.clone() else {
            self.status = "Hub connection not configured".to_string();
            if self.connection_error.is_none() {
                self.connection_error = Some("BOTSTER_HUB_CONNECTION is required".to_string());
            }
            return;
        };
        self.connected_generation = None;
        self.status = "connecting".to_string();
        self.hub_io.connect(
            endpoint,
            self.host_requirement.clone(),
            tui_terminal_compatibility_requirement(),
        );
    }

    /// Schedule the next reconnect with capped exponential backoff.
    pub(super) fn schedule_reconnect(&mut self) {
        if self.endpoint.is_none() {
            return;
        }
        self.reconnect_failures = self.reconnect_failures.saturating_add(1);
        // timer: backoff — failed Hub connect or lost link; capped exponential reconnect delay
        self.reconnect_at = Some(Instant::now() + reconnect_backoff_delay(self.reconnect_failures));
    }

    /// Operator-requested reconnect: detach, drop connection state, connect now.
    pub(super) fn force_reconnect(&mut self) {
        self.detach_owner_if_writable();
        self.drop_connection_state();
        self.reconnect_failures = 0;
        self.connect();
    }

    /// Forget every connection-scoped state.
    pub(super) fn drop_connection_state(&mut self) {
        if !self.hub_io.disconnect(DETACH_ON_DISCONNECT_BOUND) {
            self.error =
                Some("the previous Hub link did not close within the detach bound".to_string());
        }
        self.connected_generation = None;
        self.hub_offers_restart = false;
        self.restarting_sessions.clear();
        self.pending_requests.clear();
        self.reset_active_plugin_surface();
        self.invalidate_session_generation();
        self.invalidate_session_type_generation();
        self.drop_entity_options_subscriptions();
        self.clear_event_subscription_state();
        self.clear_route_state();
        self.attach_recovery_used = false;
        self.recovery_notice = None;
        // Quarantines and counters describe the Hub of the dropped
        // connection; the next Status fills them again.
        self.quarantines.clear();
        self.hub_counters = DaemonObservabilityCounters::default();
        self.plugin_logs.clear();
        self.terminal_close_evidence = None;
        // Detach answers and spawn targets are per connection.
        self.detaches.clear();
        self.spawn_targets.clear();
        self.spawn_targets_loaded = false;
        self.spawn_targets_failure = None;
    }

    /// Forget the current route: attachment, hydration, projection, modes, input window.
    pub(super) fn clear_route_state(&mut self) {
        self.attached = None;
        self.drop_attach_hydration();
        self.route_generation = None;
        self.route_epoch = None;
        self.terminal_modes = None;
        self.resolve_unknown_input_operations("route closed");
        self.clear_ghostty_projection();
    }

    /// Resolve every in-flight input operation as unknown when its route
    /// closes. Their INPUT_RESULT frames can no longer arrive; the user sees
    /// one explicit line instead of a silently dropped result.
    pub(super) fn resolve_unknown_input_operations(&mut self, reason: &str) {
        let unresolved = self.input_window.in_flight_len();
        self.invalidate_unsafe_paste();
        self.input_window = InputWindow::new();
        if unresolved > 0 {
            self.action_feedback = Some(format!(
                "{unresolved} terminal input operation(s) unresolved: {reason}"
            ));
        }
    }

    pub(super) fn apply_connected(&mut self, generation: u64, ack: DaemonHelloAck) {
        if generation != self.hub_io.generation() {
            return;
        }
        if let Err(error) = admit_terminal_hello(&ack) {
            self.hub_io.disconnect_now();
            self.apply_link_failure(error);
            return;
        }
        self.connected_generation = Some(generation);
        self.hub_offers_restart = host_offers_restart(&ack.compatibility);
        self.reconnect_failures = 0;
        self.reconnect_at = None;
        self.status = "connected".to_string();
        self.connection_error = None;
        self.record_diagnostics(ack.diagnostics);
        self.refresh_read_models();
        self.start_session_subscription();
        self.start_session_type_subscription();
        self.sync_notice_subscriptions();
        self.sync_entity_options_subscriptions();
    }

    pub(super) fn apply_disconnected(&mut self, generation: u64, error: DaemonTransportError) {
        if generation != self.hub_io.generation() {
            return;
        }
        self.apply_link_failure(error);
    }

    /// The connection ended. Reset connection-scoped state and schedule a reconnect.
    pub(super) fn apply_link_failure(&mut self, error: DaemonTransportError) {
        self.drop_connection_state();
        match error {
            DaemonTransportError::Protocol(message) => {
                self.status = "compatibility mismatch".to_string();
                self.connection_error = Some(format!(
                    "expected daemon protocol {PROTOCOL}; daemon protocol error: {message}"
                ));
                self.record_diagnostic(DaemonDiagnostic::compatibility_mismatch(message));
            }
            DaemonTransportError::ProtocolViolation(code) => {
                self.status = "protocol violation; reconnecting".to_string();
                let message = format!("hub protocol violation: {}", code.as_str());
                self.connection_error = Some(message.clone());
                self.record_diagnostic(DaemonDiagnostic::disconnected(message));
            }
            DaemonTransportError::Compatibility(error) => {
                self.status = "compatibility mismatch".to_string();
                self.connection_error = Some(error.diagnostic.clone());
                self.record_diagnostics(error.diagnostics);
            }
            DaemonTransportError::NotRunning => {
                self.status = "hub unavailable; reconnecting".to_string();
                self.connection_error = Some(DaemonTransportError::NotRunning.to_string());
            }
            DaemonTransportError::ClientDisconnected => {
                self.status = "disconnected; reconnecting".to_string();
                let message = DaemonTransportError::ClientDisconnected.to_string();
                self.connection_error = Some(message.clone());
                self.record_diagnostic(DaemonDiagnostic::disconnected(message));
            }
            DaemonTransportError::ClosedByHub(reason) => {
                self.status = "closed by hub; reconnecting".to_string();
                let message = format!("hub closed the connection: {reason:?}");
                self.connection_error = Some(message.clone());
                self.record_diagnostic(DaemonDiagnostic::disconnected(message));
            }
            other => {
                self.status = "reconnecting".to_string();
                self.connection_error = Some(other.to_string());
            }
        }
        self.schedule_reconnect();
    }
}
