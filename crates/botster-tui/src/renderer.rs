#[cfg(test)]
use botster_ui_contract::UiNode;
use botster_ui_contract::{UiActionRequestId, UiSurfaceId};

pub const WORKSPACE_SURFACE_ID: &str = "botster-tui.workspace";

pub use botster_tui_kit::{
    ActionRequestContext, HitMap, InputDispatch, InputRouter, PresentationState, RenderState,
    apply_action_result, render_node_with_presentation_state, tui_capabilities, viewport_for_area,
};

pub fn action_request_context() -> ActionRequestContext {
    action_request_context_for(WORKSPACE_SURFACE_ID)
}

pub fn action_request_context_for(surface_id: &str) -> ActionRequestContext {
    ActionRequestContext::new(UiSurfaceId(surface_id.to_string()), |node_id, _kind| {
        UiActionRequestId(format!("req-{node_id}-{}", crate::app::short_suffix()))
    })
}

#[cfg(test)]
pub fn render_to_lines(root: &UiNode, width: u16, height: u16) -> (Vec<String>, HitMap) {
    botster_tui_kit::render_to_lines(root, width, height).expect("test backend should draw fixture")
}

#[cfg(test)]
pub fn render_to_lines_with_presentation_state(
    root: &UiNode,
    width: u16,
    height: u16,
    state: &RenderState,
    presentation: &PresentationState,
) -> (Vec<String>, HitMap) {
    botster_tui_kit::render_to_lines_with_presentation_state(
        root,
        width,
        height,
        state,
        presentation,
    )
    .expect("presentation-aware test backend should draw fixture")
}
