use slint::Window;

/// Enable fullscreen on the given window (LinuxKMS/X11/Wayland).
pub fn set_fullscreen(window: &Window) {
    window.set_fullscreen(true);
}
