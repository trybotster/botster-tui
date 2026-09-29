use super::*;

impl TuiApp {
    pub(super) fn clear_ghostty_projection(&mut self) {
        self.ghostty_projection = None;
        self.ghostty_projection_session_id = None;
        self.ghostty_viewport_cache = None;
    }

    pub(super) fn ensure_ghostty_projection(&mut self, session_id: &str) {
        if self.ghostty_projection.is_some()
            && self.ghostty_projection_session_id.as_deref() == Some(session_id)
        {
            return;
        }
        match GhosttyClientProjection::with_config(
            self.terminal_viewport_size,
            GhosttyAdapterConfig::with_max_scrollback_bytes(GHOSTTY_SCROLLBACK_BYTES),
        ) {
            Ok(projection) => {
                self.ghostty_projection = Some(projection);
                self.ghostty_projection_session_id = Some(session_id.to_string());
                self.refresh_ghostty_viewport_cache();
            }
            Err(error) => {
                self.error = Some(format!("ghostty projection unavailable: {error}"));
                self.clear_ghostty_projection();
            }
        }
    }

    pub(super) fn paint_ghostty_projection(&self, frame: &mut Frame<'_>, hit_map: &HitMap) {
        let Some(viewport) = self.ghostty_viewport_cache.as_ref() else {
            return;
        };
        crate::projection_paint::paint_projection_on_hit_map(frame, hit_map, viewport);
    }
}
