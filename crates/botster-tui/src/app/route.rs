use super::*;

impl TuiApp {
    /// One routed scheme 2 frame from the connection.
    ///
    /// Identity rules (root ruling on attachment identity versus resync state):
    ///
    /// - Frames for retired or foreign routes are dropped.
    /// - `generation` is the fixed attachment generation from the trusted
    ///   Attach response and never changes; frames that arrive before the
    ///   response wait (bounded) and frames with another generation are dropped.
    /// - `stream_epoch` fences snapshot and live continuity. It is 0 after
    ///   ATTACH_STATE attached. A ROUTE_RESYNC is accepted only when its
    ///   `from_epoch` equals the accepted epoch and its envelope epoch equals
    ///   `to_epoch`; other RESYNC frames are stale and dropped. Data frames
    ///   with another epoch are dropped. No numeric comparison is used.
    /// - A ROUTE_RESYNC must also change the epoch (`to_epoch != from_epoch`).
    /// - INPUT_RESULT is correlated by operation id within the attachment on
    ///   any epoch, so an accepted operation is never left unresolved by a
    ///   resync; unknown or already completed ids are reported, not tracked.
    pub(super) fn apply_routed_terminal_frame(&mut self, routed: RoutedTerminalFrame) {
        let route = routed.route.as_str().to_string();
        if self.retired_subscription_ids.contains(&route) {
            return;
        }
        if !self.hydration_matches_route(&route) && !self.attached_matches_route(&route) {
            return;
        }
        let event = match decode_terminal_event(&routed.frame) {
            Ok(event) => event,
            Err(error) => {
                self.recover_from_decode_or_phase_gap(&format!(
                    "terminal event decode failed: {error}"
                ));
                return;
            }
        };
        // The attachment generation comes only from the trusted Attach
        // response. Frames that arrive first wait, bounded, and replay once the
        // response lands; a frame is never allowed to set the reservation.
        let Some(generation) = self.route_generation else {
            self.park_pre_attach_frame(routed);
            return;
        };
        if routed.generation != generation {
            return;
        }
        let epoch_ok = match &event {
            TerminalEvent::AttachState(AttachStateCode::Attached) => routed.stream_epoch == 0,
            TerminalEvent::AttachState(_) => self
                .route_epoch
                .is_none_or(|accepted| accepted == routed.stream_epoch),
            TerminalEvent::RouteResync(transition) => {
                transition.to_epoch != transition.from_epoch
                    && self.route_epoch == Some(transition.from_epoch)
                    && routed.stream_epoch == transition.to_epoch
            }
            TerminalEvent::InputResult(_) => true,
            _ => self.route_epoch == Some(routed.stream_epoch),
        };
        if !epoch_ok {
            return;
        }
        match &event {
            TerminalEvent::AttachState(AttachStateCode::Attached) => self.route_epoch = Some(0),
            TerminalEvent::RouteResync(transition) => {
                self.route_epoch = Some(transition.to_epoch);
            }
            _ => {}
        }
        self.apply_terminal_event(&route, event);
    }

    /// Retain one frame until the Attach response fixes the generation.
    ///
    /// The frame is charged against the connection's aggregate pending budget,
    /// not a separate per-route buffer. At the bound only this route fails.
    pub(super) fn park_pre_attach_frame(&mut self, routed: RoutedTerminalFrame) {
        let Some(hydration) = self.attach_hydration.as_ref() else {
            return;
        };
        let bytes = routed.frame.len();
        if !self.hub_io.try_retain(bytes) {
            let session_id = hydration.session_id.clone();
            let route = hydration.route.clone();
            self.recover_current_subscription(
                &session_id,
                &route,
                "frames before the attach response exceeded the pending budget",
                "frames before the attach response exceeded the pending budget",
            );
            return;
        }
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.pending_frame_bytes += bytes;
            hydration.pending_frames.push_back(routed);
        }
    }

    /// Take the parked frames out of the campaign and release their budget.
    pub(super) fn take_parked_frames(&mut self) -> VecDeque<RoutedTerminalFrame> {
        let Some(hydration) = self.attach_hydration.as_mut() else {
            return VecDeque::new();
        };
        let parked = std::mem::take(&mut hydration.pending_frames);
        hydration.pending_frame_bytes = 0;
        for routed in &parked {
            self.hub_io.release_retained(routed.frame.len());
        }
        parked
    }

    /// Replay frames parked before the Attach response, in arrival order.
    /// Output only: queued user input is never replayed here.
    pub(super) fn replay_pre_attach_frames(&mut self) {
        for routed in self.take_parked_frames() {
            if self.attach_hydration.is_none() && self.attached.is_none() {
                return;
            }
            self.apply_routed_terminal_frame(routed);
        }
    }

    /// Drop the attach campaign and release every frame it retained.
    pub(super) fn drop_attach_hydration(&mut self) {
        let _ = self.take_parked_frames();
        self.attach_hydration = None;
    }

    /// The route restarts from a fresh SNAPSHOT_READY. The decoder state is
    /// reset, never continued. The attachment itself survives: Core keeps the
    /// route attached across a resync and Hub never gates input, so the
    /// hydration is marked `resync` and the input window keeps its slots.
    /// Input captured so far stays queued and is never replayed.
    pub(super) fn begin_route_resync(&mut self) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            return;
        };
        self.invalidate_unsafe_paste();
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_ghostty_projection();
        let was_attached = self.attached.take().is_some();
        let _ = self.take_parked_frames();
        let mut hydration = AttachHydration::new(&session_id, &route);
        if let Some(previous) = self.attach_hydration.take() {
            hydration.attached_seen = previous.attached_seen;
            hydration.resync = previous.resync;
            hydration.pending_input = previous.pending_input;
            hydration.pending_input_bytes = previous.pending_input_bytes;
            hydration.pending_resize = previous.pending_resize;
            // A resync during a recovery's hydration is still that recovery.
            hydration.recovery_cause = previous.recovery_cause;
        }
        hydration.attached_seen |= was_attached;
        hydration.resync |= was_attached;
        self.attach_hydration = Some(hydration);
        self.action_feedback = Some(format!("terminal route resync: {session_id}"));
    }

    /// Whether `route` names the current valid attachment for input purposes:
    /// the live attached route, or a resync hydration of a route that was
    /// live, with the trusted attachment generation known. Initial hydration
    /// before the Attach response and retired or replaced routes never match.
    pub(super) fn attachment_matches_route(&self, route: &str) -> bool {
        if self.route_generation.is_none() {
            return false;
        }
        self.attached_matches_route(route)
            || self
                .attach_hydration
                .as_ref()
                .is_some_and(|hydration| hydration.resync && hydration.route == route)
    }

    pub(super) fn send_encoded_frames(&mut self, frames: Vec<Vec<u8>>) {
        if frames.is_empty() {
            return;
        }
        let Some((_, route)) = self.current_owner_pair() else {
            self.error = Some("terminal stream unavailable: no attached route".to_string());
            return;
        };
        if !self.attachment_matches_route(&route) {
            self.error = Some("terminal stream unavailable: no attached route".to_string());
            return;
        }
        let Some(generation) = self.route_generation else {
            self.error = Some("terminal stream unavailable: route generation unknown".to_string());
            return;
        };
        for frame in frames {
            if !self.hub_io.send_terminal(&route, generation, &frame) {
                self.error = Some("terminal stream unavailable: not connected".to_string());
                return;
            }
        }
        self.error = None;
    }

    /// INPUT_RESULT is correlated by operation id within the current
    /// attachment, including a resync hydration of that attachment. Results
    /// for a lost or replaced attachment never reach the window.
    pub(super) fn apply_terminal_input_result(&mut self, route: &str, result: InputResultBody) {
        if !self.attachment_matches_route(route) {
            return;
        }
        // `result.mode_bits` is the mode set at the time of that operation;
        // current stream state comes only from epoch-valid MODES frames.
        let (completed, released) = self.input_window.complete(result.operation_id);
        if completed.is_none() {
            self.error = Some(format!(
                "terminal input result {} has no pending operation",
                result.operation_id
            ));
        }
        self.send_encoded_frames(released);
        let retry_matches = matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingResult {
                operation_id,
                route: pending_route,
                generation,
                ..
            }) if *operation_id == result.operation_id
                && pending_route == route
                && self.route_generation == Some(*generation)
        );
        if retry_matches {
            let may_retry = completed.as_ref().is_some_and(|operation| operation.paste)
                && result.outcome == InputOutcome::RejectedUnsafePaste
                && result.accepted_payload_bytes == Some(0)
                && result.written_pty_bytes == Some(0);
            if may_retry {
                let Some(PendingUnsafePaste::AwaitingResult {
                    route,
                    generation,
                    payload,
                    ..
                }) = self.pending_unsafe_paste.take()
                else {
                    unreachable!("matching unsafe paste state changed")
                };
                self.pending_unsafe_paste = Some(PendingUnsafePaste::AwaitingConsent {
                    route,
                    generation,
                    payload,
                    // timer: deadline — unsafe-paste consent expires; expiry cancels the retry
                    deadline: Instant::now() + UNSAFE_PASTE_CONSENT_TIMEOUT,
                    stage: UnsafePasteConsentStage::Review,
                });
            } else {
                self.invalidate_unsafe_paste();
            }
        }
        match result.outcome {
            InputOutcome::Written => {
                if completed.is_some() {
                    self.error = None;
                }
            }
            _ => self.error = Some(input_outcome_message(&result)),
        }
    }

    pub(super) fn apply_terminal_event(&mut self, route: &str, event: TerminalEvent) {
        let Some((session_id, _)) = self.current_owner_pair() else {
            return;
        };
        match event {
            TerminalEvent::Output(frame) => {
                let bytes = frame.body();
                if let Some(hydration) = self.attach_hydration.as_mut() {
                    if hydration
                        .buffered_live_output
                        .len()
                        .saturating_add(bytes.len())
                        > MAX_HYDRATION_OUTPUT_BYTES
                    {
                        self.recover_current_subscription(
                            &session_id,
                            route,
                            "live output exceeded the attach buffer bound",
                            "live output exceeded the attach buffer bound",
                        );
                        return;
                    }
                    hydration.buffered_live_output.extend_from_slice(bytes);
                } else {
                    self.apply_live_terminal_output(bytes);
                }
            }
            TerminalEvent::SnapshotReady(frame) => {
                self.apply_snapshot_ready(&session_id, frame.body());
            }
            TerminalEvent::SnapshotHistory(frame) => {
                self.apply_snapshot_history(&session_id, frame.body());
            }
            TerminalEvent::SnapshotFinish => self.apply_snapshot_finish(&session_id),
            TerminalEvent::ProcessExit(exit) => {
                self.apply_process_exit(session_id, route.to_string(), exit.code);
            }
            TerminalEvent::Modes(modes) => {
                self.terminal_modes = Some(TerminalModeState {
                    route: route.to_string(),
                    modes,
                });
            }
            TerminalEvent::AttachState(state) => {
                self.apply_attach_state_kind(session_id, route.to_string(), state);
            }
            TerminalEvent::InputResult(result) => self.apply_terminal_input_result(route, result),
            TerminalEvent::HistoryUnavailable(reason) => {
                self.apply_history_unavailable(&session_id, reason);
            }
            TerminalEvent::RouteResync(_) => self.begin_route_resync(),
        }
    }

    pub(super) fn handle_terminal_subscription_closed(
        &mut self,
        session_id: String,
        subscription_id: String,
        generation: u64,
        reason: String,
    ) {
        if self.retired_subscription_ids.contains(&subscription_id) {
            return;
        }
        if !self.hydration_matches_route(&subscription_id)
            && !self.attached_matches_route(&subscription_id)
        {
            return;
        }
        self.terminal_close_evidence = Some((generation, reason.clone()));
        self.action_feedback = Some(format!(
            "terminal subscription closed generation={generation} reason={reason}: {session_id}"
        ));
        if reason == TERMINAL_SUBSCRIPTION_CLOSED_WORKER_LOST {
            // The worker is gone: a re-attach can only fail. End the route
            // and report the crash instead of recovering.
            self.end_route_after_worker_lost(&session_id, &subscription_id);
            return;
        }
        self.recover_current_subscription(
            &session_id,
            &subscription_id,
            &format!("terminal subscription closed ({reason})"),
            &reason,
        );
    }

    /// The client queue shed frames for a route or a frame failed to decode.
    /// Byte continuity is gone: detach and re-attach that route only.
    pub(super) fn apply_route_fault(&mut self, route: RouteId, generation: u64, reason: &str) {
        let route = route.as_str().to_string();
        if self.retired_subscription_ids.contains(&route) {
            return;
        }
        if !self.hydration_matches_route(&route) && !self.attached_matches_route(&route) {
            return;
        }
        if self
            .route_generation
            .is_some_and(|current| generation < current)
        {
            return;
        }
        let Some((session_id, _)) = self.current_owner_pair() else {
            return;
        };
        self.recover_current_subscription(&session_id, &route, reason, reason);
    }

    /// The session's worker died: retire the route with a bounded Detach and
    /// leave the session to its failed entity row.
    pub(super) fn end_route_after_worker_lost(&mut self, session_id: &str, route: &str) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_route_state();
        self.retire_subscription(route);
        if self.is_connected() {
            self.send_bounded_detach(session_id.to_string(), route.to_string());
        }
        self.recovery_notice = None;
        self.error = Some(format!(
            "session {session_id} crashed: its worker was lost (worker_lost)"
        ));
    }

    pub(super) fn recover_from_decode_or_phase_gap(&mut self, reason: &str) {
        let Some((session_id, route)) = self.current_owner_pair() else {
            self.error = Some(reason.to_string());
            return;
        };
        self.recover_current_subscription(&session_id, &route, reason, reason);
    }

    /// Retire the current route with a bounded Detach and re-attach with a
    /// fresh route. A campaign has one recovery at a time: a failure during
    /// the recovery's own hydration fails closed, and a completed recovery
    /// restores it, so each later independent failure recovers once.
    ///
    /// `reason` is the error line while the recovery runs; `cause` is the
    /// short cause the recovery notice names once it completes.
    pub(super) fn recover_current_subscription(
        &mut self,
        session_id: &str,
        route: &str,
        reason: &str,
        cause: &str,
    ) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.clear_route_state();
        self.retire_subscription(route);
        if self.is_connected() {
            self.send_bounded_detach(session_id.to_string(), route.to_string());
        }
        if self.attach_recovery_used {
            self.error = Some(format!(
                "terminal attach failed closed after recovery: {reason}"
            ));
            return;
        }
        self.attach_recovery_used = true;
        self.error = Some(format!("terminal attach recovering: {reason}"));
        let replacement = self.mint_subscription_id();
        self.begin_attach_hydration(session_id, &replacement);
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.recovery_cause = Some(cause.to_string());
        }
        if self.is_connected() {
            self.submit(
                DaemonRequest::Attach {
                    session_id: session_id.to_string(),
                    subscription_id: replacement.clone(),
                },
                PendingReply::Attach {
                    session_id: session_id.to_string(),
                    route: replacement,
                },
                REQUEST_DEADLINE,
            );
        }
    }

    pub(super) fn apply_process_exit(
        &mut self,
        session_id: String,
        route: String,
        code: Option<i32>,
    ) {
        let hydration_matches = self.hydration_matches_route(&route);
        if !hydration_matches && !self.attached_matches_route(&route) {
            return;
        }
        self.status = format!("process exited {}", code.unwrap_or_default());
        self.retire_subscription(&route);
        self.attached = None;
        self.terminal_modes = None;
        self.clear_ghostty_projection();
        if hydration_matches {
            self.drop_attach_hydration();
        }
        let _ = session_id;
    }

    pub(super) fn apply_attach_state_kind(
        &mut self,
        session_id: String,
        route: String,
        state: AttachStateCode,
    ) {
        let hydration_matches = self.hydration_matches_route(&route);
        let attached_matches = self.attached_matches_route(&route);
        if !hydration_matches && !attached_matches {
            return;
        }
        self.action_feedback = Some(format!("attach {state:?}: {session_id}"));
        match state {
            AttachStateCode::Attached if hydration_matches => {
                if let Some(hydration) = self.attach_hydration.as_mut() {
                    hydration.attached_seen = true;
                }
                self.maybe_open_attach_live_path(&session_id);
            }
            AttachStateCode::Detached => {
                if let Some(projection) = self.ghostty_projection.as_mut() {
                    projection.abort_ghostsnp_history();
                }
                self.retire_subscription(&route);
                self.attached = None;
                self.drop_attach_hydration();
                self.terminal_modes = None;
                self.clear_ghostty_projection();
            }
            AttachStateCode::Failed if hydration_matches => {
                // Capture failed before READY; Hub tears the route down. One
                // fresh attach with a new route is the allowed recovery, with no
                // input replay.
                self.recover_current_subscription(
                    &session_id,
                    &route,
                    "attach failed before READY",
                    "attach failed before READY",
                );
            }
            AttachStateCode::Attaching | AttachStateCode::Attached | AttachStateCode::Failed => {}
        }
    }

    /// HISTORY_UNAVAILABLE on the stream: the remaining history pages are
    /// replaced and SNAPSHOT_FINISH still follows.
    ///
    /// Core guarantees SNAPSHOT_READY precedes HISTORY_UNAVAILABLE on a live
    /// attach, so the live screen is already authoritative and only retained
    /// history is missing. Before READY the frame is a phase gap; a capture
    /// failure before READY arrives as ATTACH_STATE failed instead.
    pub(super) fn apply_history_unavailable(
        &mut self,
        session_id: &str,
        reason: HistoryUnavailableReason,
    ) {
        let ready = self
            .attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.snapshot_ready);
        if self.attach_hydration.is_some() && !ready {
            self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP phase gap (closed): HISTORY_UNAVAILABLE ({reason:?}) before SNAPSHOT_READY"
            ));
            return;
        }
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        self.action_feedback = Some(format!(
            "terminal history unavailable ({reason:?}): {session_id}"
        ));
    }

    pub(super) fn hydration_matches_route(&self, route: &str) -> bool {
        self.attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.route == route)
    }

    pub(super) fn attached_matches_route(&self, route: &str) -> bool {
        self.attached
            .as_ref()
            .is_some_and(|attached| attached.route == route)
    }

    pub(super) fn apply_snapshot_ready(&mut self, session_id: &str, bytes: &[u8]) {
        if self
            .attach_hydration
            .as_ref()
            .is_none_or(|hydration| hydration.snapshot_ready)
        {
            self.recover_from_decode_or_phase_gap(
                "GHOSTSNP phase gap (closed): unexpected SNAPSHOT_READY",
            );
            return;
        }
        self.ensure_ghostty_projection(session_id);
        let Some(projection) = self.ghostty_projection.as_mut() else {
            return;
        };
        match projection.install_ghostsnp_ready(bytes) {
            Ok(GhosttySnapshotDecodeProgress::Ready) => {
                if let Some(hydration) = self.attach_hydration.as_mut() {
                    hydration.snapshot_ready = true;
                }
                self.ghostty_projection_session_id = Some(session_id.to_string());
                self.terminal_viewport_size = projection.dimensions();
                self.projection_dirty = true;
            }
            Ok(progress) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental sequence failed (closed): unexpected {progress:?}"
            )),
            Err(error) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental apply failed (closed): {error}"
            )),
        }
    }

    pub(super) fn apply_snapshot_history(&mut self, session_id: &str, bytes: &[u8]) {
        let ready = self
            .attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.snapshot_ready && !hydration.snapshot_finished);
        if !ready {
            self.recover_from_decode_or_phase_gap(
                "GHOSTSNP phase gap (closed): unexpected SNAPSHOT_HISTORY",
            );
            return;
        }
        let _ = session_id;
        let Some(projection) = self.ghostty_projection.as_mut() else {
            return;
        };
        match projection.apply_ghostsnp_history(bytes) {
            // One SNAPSHOT_HISTORY frame carries one page; the GHOSTSNP finish
            // record is the last page. Paint the new retained history at the
            // next paint. The live path opens on the SNAPSHOT_FINISH frame.
            Ok(GhosttySnapshotDecodeProgress::History | GhosttySnapshotDecodeProgress::Finish) => {
                self.projection_dirty = true;
            }
            Ok(progress) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental sequence failed (closed): unexpected {progress:?}"
            )),
            Err(error) => self.recover_from_decode_or_phase_gap(&format!(
                "GHOSTSNP incremental apply failed (closed): {error}"
            )),
        }
    }

    pub(super) fn apply_snapshot_finish(&mut self, session_id: &str) {
        let ready = self
            .attach_hydration
            .as_ref()
            .is_some_and(|hydration| hydration.snapshot_ready && !hydration.snapshot_finished);
        if !ready {
            self.recover_from_decode_or_phase_gap(
                "GHOSTSNP phase gap (closed): unexpected SNAPSHOT_FINISH",
            );
            return;
        }
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.abort_ghostsnp_history();
        }
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.snapshot_finished = true;
        }
        self.projection_dirty = true;
        self.maybe_open_attach_live_path(session_id);
    }

    pub(super) fn apply_live_terminal_output(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        #[cfg(test)]
        self.applied_live_payloads.push(data.to_vec());
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.apply_terminal_output(data);
            self.projection_dirty = true;
        }
    }

    pub(super) fn maybe_open_attach_live_path(&mut self, session_id: &str) {
        let Some(hydration) = self.attach_hydration.as_ref() else {
            return;
        };
        if hydration.session_id != session_id
            || !hydration.snapshot_finished
            || !hydration.attached_seen
        {
            return;
        }
        self.open_attach_live_path(session_id);
    }

    /// Open the post-barrier live path and release queued client operations.
    pub(super) fn open_attach_live_path(&mut self, session_id: &str) {
        let Some(hydration) = self.attach_hydration.take() else {
            return;
        };
        if hydration.session_id != session_id
            || hydration.route != self.subscription_id
            || !hydration.snapshot_finished
            || !hydration.attached_seen
        {
            self.attach_hydration = Some(hydration);
            return;
        }
        self.attached = Some(AttachedRoute {
            session_id: session_id.to_string(),
            route: hydration.route.clone(),
        });
        if let Some(cause) = hydration.recovery_cause.clone() {
            self.attach_recovery_used = false;
            let count = self
                .recovery_notice
                .as_ref()
                .map_or(0, |notice| notice.count)
                + 1;
            self.recovery_notice = Some(RecoveryNotice { count, cause });
        }
        if !hydration.buffered_live_output.is_empty() {
            self.apply_live_terminal_output(&hydration.buffered_live_output);
        }
        let size = hydration
            .pending_resize
            .unwrap_or(self.terminal_viewport_size);
        if self.send_resize(size) {
            self.apply_local_resize(size);
        }
        for input in hydration.pending_input {
            match input {
                PendingTerminalInput::Key(key) => self.send_key(key),
                PendingTerminalInput::Focus(focused) => self.send_focus(focused),
                PendingTerminalInput::Paste(data) => self.send_paste(data),
            }
        }
    }

    /// Keep the Hub PTY size equal to the terminal pane in the frame just drawn.
    ///
    /// Attach, outer resize, and layout changes all converge here whether or
    /// not the pane has focus. An unchanged size sends nothing. A RESIZE the
    /// input window refuses (for example QueueFull) leaves the local size
    /// unchanged, so the next draw retries; draws follow wakes such as the
    /// INPUT_RESULT that frees window capacity. There is no timer.
    pub(super) fn sync_terminal_pane_size(&mut self, hit_map: &HitMap) {
        let Some(outer) = tui_terminal_region(hit_map) else {
            return;
        };
        let inner = botster_tui_kit::terminal_inner_rect(outer);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let size = TerminalScreenSize::new(inner.height, inner.width);
        if let Some(hydration) = self.attach_hydration.as_mut() {
            hydration.pending_resize = Some(size);
            return;
        }
        if self.attached.is_none() || self.terminal_viewport_size == size {
            return;
        }
        if self.send_resize(size) {
            self.apply_local_resize(size);
        }
    }

    pub(super) fn apply_local_resize(&mut self, size: TerminalScreenSize) {
        self.terminal_viewport_size = size;
        if let Some(projection) = self.ghostty_projection.as_mut()
            && let Err(error) = projection.resize(size)
        {
            self.error = Some(format!("terminal resize failed: {error}"));
        }
        self.projection_dirty = true;
    }

    pub(super) fn scroll_projection(&mut self, op: ScrollOp) {
        if let Some(projection) = self.ghostty_projection.as_mut() {
            projection.scroll(op);
            self.projection_dirty = true;
        }
    }

    /// Project the viewport once. Called from `prepare_paint` when dirty and
    /// from tests that inspect the cache directly.
    pub(super) fn refresh_ghostty_viewport_cache(&mut self) {
        self.projection_dirty = false;
        let Some(projection) = self.ghostty_projection.as_mut() else {
            self.ghostty_viewport_cache = None;
            return;
        };
        self.ghostty_viewport_cache = projection.project_viewport().ok();
    }

    /// MODES bits for the live route, or zero.
    pub(super) fn current_mode_bits(&self) -> u32 {
        match (self.terminal_modes.as_ref(), self.attached.as_ref()) {
            (Some(state), Some(attached)) if state.route == attached.route => state.modes.mode_bits,
            _ => 0,
        }
    }

    pub(super) fn apply_terminal_mouse_mode(&self, hit_map: &mut HitMap) {
        hit_map.set_terminal_mouse_mode(
            "tui-terminal",
            terminal_input::kit_mouse_bits(self.current_mode_bits()),
        );
    }
}
