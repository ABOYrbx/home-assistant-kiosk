use crate::config::DisplayConfig;
use slint::Window;

/// Apply the display settings that belong to the window backend.
///
/// Fullscreen is requested through [`Window::set_fullscreen`]. Rotation is a
/// deployment/backend concern (`SLINT_KMS_ROTATION`), so it is not applied here.
pub fn apply_display(window: &Window, display: &DisplayConfig) {
    window.set_fullscreen(display.fullscreen);
}
