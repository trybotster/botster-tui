use super::*;

impl TuiApp {
    /// Reserve the next operation id for the live route.
    pub(super) fn next_input_operation_id(&mut self) -> Option<u64> {
        match self.input_window.next_operation_id() {
            Ok(id) => Some(id),
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }

    /// Encode one typed command, admit it into the window, and write what the
    /// window releases. Returns whether the window admitted the command.
    pub(super) fn send_command(
        &mut self,
        operation_id: u64,
        command: TerminalInputCommand,
    ) -> bool {
        let frame = match encode_terminal_input(&command) {
            Ok(frame) => frame,
            Err(error) => {
                self.error = Some(error.to_string());
                return false;
            }
        };
        #[cfg(test)]
        self.observed_terminal_inputs.push(command);
        self.send_operation_frames(operation_id, false, vec![frame.into_bytes()])
    }

    /// Admit one operation of encoded frames into the window and write what
    /// the window releases, in order.
    ///
    /// Returns whether the window admitted the operation. Admission means the
    /// frames were written now or queued behind in-flight operations; it does
    /// not confirm transport delivery or the Hub's INPUT_RESULT.
    pub(super) fn send_operation_frames(
        &mut self,
        operation_id: u64,
        paste: bool,
        frames: Vec<Vec<u8>>,
    ) -> bool {
        match self.input_window.admit(operation_id, paste, frames) {
            Ok(ready) => {
                self.send_encoded_frames(ready);
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    pub(super) fn send_key(&mut self, key: KeyEvent) {
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let Some(command) = terminal_input::key_command(key, operation_id) else {
            return;
        };
        self.send_command(operation_id, command);
    }

    pub(super) fn send_focus(&mut self, focused: bool) {
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let command = terminal_input::focus_command(focused, operation_id);
        self.send_command(operation_id, command);
    }

    /// Returns whether the input window admitted the RESIZE (written now or
    /// queued). A refusal leaves the caller's local size unchanged, so the
    /// next draw retries.
    pub(super) fn send_resize(&mut self, size: TerminalScreenSize) -> bool {
        if self.attached.is_none() {
            return false;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            return false;
        };
        let command = terminal_input::resize_command(size.rows, size.cols, operation_id);
        self.send_command(operation_id, command)
    }

    pub(super) fn send_paste(&mut self, data: Vec<u8>) {
        if self.input_window.has_paste() {
            self.error = Some("terminal paste unavailable: another paste is in flight".to_string());
            return;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            return;
        };
        let Some((_, route)) = self.current_owner_pair() else {
            self.error = Some("terminal stream unavailable: no attached route".to_string());
            return;
        };
        let Some(generation) = self.route_generation else {
            self.error = Some("terminal stream unavailable: route generation unknown".to_string());
            return;
        };
        let frames = match encode_paste(operation_id, false, &data) {
            Ok(frames) => frames,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        #[cfg(test)]
        self.observed_terminal_inputs.extend(
            frames
                .iter()
                .filter_map(|frame| decode_terminal_input(frame).ok()),
        );
        let frames = frames
            .into_iter()
            .map(botster_terminal_protocol_client::TerminalInputFrame::into_bytes)
            .collect();
        match self
            .input_window
            .admit_retaining(operation_id, true, frames, data.len())
        {
            Ok(ready) => {
                self.pending_unsafe_paste = Some(PendingUnsafePaste::AwaitingResult {
                    operation_id,
                    route,
                    generation,
                    payload: data,
                });
                self.send_encoded_frames(ready);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    pub(super) fn invalidate_unsafe_paste(&mut self) {
        if let Some(pending) = self.pending_unsafe_paste.take() {
            self.input_window.release_retained(pending.payload_len());
        }
    }

    pub(super) fn expire_unsafe_paste_consent(&mut self, now: Instant) {
        let expired = matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent { deadline, .. }) if *deadline <= now
        );
        if expired {
            self.invalidate_unsafe_paste();
        }
    }

    pub(super) fn confirm_unsafe_paste(&mut self) {
        let ready = matches!(
            self.pending_unsafe_paste.as_ref(),
            Some(PendingUnsafePaste::AwaitingConsent {
                stage: UnsafePasteConsentStage::Armed,
                ..
            })
        );
        if !ready {
            return;
        }
        let Some(PendingUnsafePaste::AwaitingConsent {
            route,
            generation,
            payload,
            deadline,
            ..
        }) = self.pending_unsafe_paste.take()
        else {
            return;
        };
        let payload_bytes = payload.len();
        if deadline <= Instant::now()
            || self.route_generation != Some(generation)
            || !self.attachment_matches_route(&route)
        {
            self.input_window.release_retained(payload_bytes);
            return;
        }
        let Some(operation_id) = self.next_input_operation_id() else {
            self.input_window.release_retained(payload_bytes);
            return;
        };
        let frames = match encode_paste(operation_id, true, &payload) {
            Ok(frames) => frames,
            Err(error) => {
                self.input_window.release_retained(payload_bytes);
                self.error = Some(error.to_string());
                return;
            }
        };
        #[cfg(test)]
        self.observed_terminal_inputs.extend(
            frames
                .iter()
                .filter_map(|frame| decode_terminal_input(frame).ok()),
        );
        let frames = frames
            .into_iter()
            .map(botster_terminal_protocol_client::TerminalInputFrame::into_bytes)
            .collect();
        match self.input_window.admit(operation_id, true, frames) {
            Ok(ready) => {
                self.input_window.release_retained(payload_bytes);
                self.send_encoded_frames(ready);
            }
            Err(error) => {
                self.input_window.release_retained(payload_bytes);
                self.error = Some(error.to_string());
            }
        }
    }

    pub(super) fn send_mouse(&mut self, mouse: MouseEvent, inner: Rect) -> bool {
        let Some(operation_id) = self.next_input_operation_id() else {
            return true;
        };
        let Some(command) = terminal_input::mouse_command(mouse, inner, operation_id) else {
            return false;
        };
        self.send_command(operation_id, command);
        true
    }

    /// Queue input for a route that is still attaching, bounded by bytes.
    pub(super) fn queue_pending_input(&mut self, input: PendingTerminalInput) {
        let Some(hydration) = self.attach_hydration.as_mut() else {
            return;
        };
        let bytes = input.retained_bytes();
        if hydration.pending_input_bytes.saturating_add(bytes) > MAX_PENDING_HYDRATION_INPUT_BYTES {
            self.error = Some(format!(
                "terminal input unavailable: {} bytes queued at the {MAX_PENDING_HYDRATION_INPUT_BYTES} byte attach bound",
                hydration.pending_input_bytes
            ));
            return;
        }
        hydration.pending_input_bytes += bytes;
        hydration.pending_input.push(input);
        self.error = None;
    }
}
