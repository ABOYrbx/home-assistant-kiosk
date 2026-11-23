use slint::Window;

/// Enable fullscreen on the given window (LinuxKMS/X11/Wayland).
pub fn set_fullscreen(window: &Window) {
    window.set_fullscreen(true);
}

/// Name of the configured display backend, if any.
pub fn backend_from_env() -> Option<String> {
    std::env::var("SLINT_BACKEND").ok()
}

/// Detect whether a LinuxKMS backend is selected.
pub fn is_linuxkms() -> bool {
    std::env::var("SLINT_BACKEND")
        .map(|v| v.contains("linuxkms"))
        .unwrap_or(false)
}

/// Human-readable platform label for diagnostics.
pub fn platform() -> &'static str {
    if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
}
