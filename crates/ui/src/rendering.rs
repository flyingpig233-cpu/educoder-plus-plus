//! Native child windows can invalidate pixels without changing Slint properties.
use crate::AppWindow;
use slint::{
    ComponentHandle,
    private_unstable_api::re_exports::{LogicalRect, WindowInner},
};

/// `request_redraw` alone only schedules a frame: the software renderer may still
/// consider the covered pixels clean and skip presenting them. Explicitly damage
/// the whole surface after hiding a native WebView, once its HWND no longer clips
/// the parent DC. Keep this version-specific adapter isolated and pin Slint.
pub(crate) fn repaint_all(app: &AppWindow) {
    let size = app.window().size().to_logical(app.window().scale_factor());
    let mut rect = LogicalRect::zero();
    rect.size.width = size.width;
    rect.size.height = size.height;
    WindowInner::from_pub(app.window())
        .window_adapter()
        .renderer()
        .mark_dirty_region(rect.into());
    app.window().request_redraw();
}
