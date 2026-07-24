use botster_core_ui::RequestId;
use botster_core_ui::ui::UiSurfaceId;

pub const WORKSPACE_SURFACE_ID: &str = "botster-tui.workspace";

pub use botster_tui_kit::{
    ActionRequestContext, HitMap, InputDispatch, InputRouter, RenderState, render_node_with_state,
    tui_capabilities,
};

pub fn action_request_context() -> ActionRequestContext {
    ActionRequestContext::new(
        UiSurfaceId(WORKSPACE_SURFACE_ID.to_string()),
        |node_id, _kind| RequestId(format!("req-{node_id}")),
    )
}

#[cfg(test)]
pub fn render_to_lines(
    root: &botster_core_ui::ui::UiNode,
    width: u16,
    height: u16,
) -> (Vec<String>, HitMap) {
    botster_tui_kit::render_to_lines(root, width, height).expect("test backend should draw fixture")
}

#[cfg(test)]
pub fn render_to_lines_with_state(
    root: &botster_core_ui::ui::UiNode,
    width: u16,
    height: u16,
    state: &RenderState,
) -> (Vec<String>, HitMap) {
    botster_tui_kit::render_to_lines_with_state(root, width, height, state)
        .expect("state-aware test backend should draw fixture")
}
