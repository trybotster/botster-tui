use botster_core::RequestId;
use botster_core::ui::UiSurfaceId;

pub const DOGFOOD_SURFACE_ID: &str = "botster-tui.dogfood";

pub use botster_tui_kit::{
    ActionRequestContext, HitMap, InputDispatch, InputRouter, render_node, tui_capabilities,
};

pub fn action_request_context() -> ActionRequestContext {
    ActionRequestContext::new(
        UiSurfaceId(DOGFOOD_SURFACE_ID.to_string()),
        |node_id, _kind| RequestId(format!("req-{node_id}")),
    )
}

#[cfg(test)]
pub fn render_to_lines(
    root: &botster_core::ui::UiNode,
    width: u16,
    height: u16,
) -> (Vec<String>, HitMap) {
    botster_tui_kit::render_to_lines(root, width, height).expect("test backend should draw fixture")
}
