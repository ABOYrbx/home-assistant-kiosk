use crate::config::DisplayConfig;
use slint::Window;
use thiserror::Error;

/// Slint backend name for the direct-to-DRM software renderer.
pub const LINUXKMS_BACKEND: &str = "linuxkms-software";
/// Environment variable Slint reads for the display backend.
pub const BACKEND_ENV: &str = "SLINT_BACKEND";
/// Environment variable Slint reads for the LinuxKMS rotation.
pub const KMS_ROTATION_ENV: &str = "SLINT_KMS_ROTATION";
/// Environment variable the X11 and Wayland backends read for the display.
pub const X11_DISPLAY_ENV: &str = "DISPLAY";
/// Environment variable the Wayland backend reads for the display.
pub const WAYLAND_DISPLAY_ENV: &str = "WAYLAND_DISPLAY";

/// The display path the application is about to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayBackend {
    /// Direct-to-DRM through LinuxKMS. No X server or compositor is involved.
    LinuxKms,
    /// A windowed backend: X11 or Wayland through winit.
    Windowed,
}

impl DisplayBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            DisplayBackend::LinuxKms => "linuxkms",
            DisplayBackend::Windowed => "windowed",
        }
    }

    /// Human-readable description for the startup log.
    pub fn describe(self) -> &'static str {
        match self {
            DisplayBackend::LinuxKms => "LinuxKMS direct-to-DRM software renderer",
            DisplayBackend::Windowed => "windowed X11/Wayland backend",
        }
    }
}

/// Errors that must be visible on the console instead of a blank screen.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum KioskError {
    #[error("display.rotation must be 0, 90, 180, or 270, not {0}")]
    UnsupportedRotation(u16),
    #[error("{variable}={value:?} is invalid; use a rotation of 0, 90, 180, or 270")]
    InvalidRotationEnv {
        variable: &'static str,
        value: String,
    },
    #[error(
        "SLINT_BACKEND={0:?} is not compiled into this binary; rebuild with \
         --no-default-features --features kiosk for LinuxKMS or with --features desktop \
         for X11 and Wayland"
    )]
    BackendNotCompiled(String),
    #[error(
        "no display backend is available: set SLINT_BACKEND=linuxkms-software for a \
         LinuxKMS kiosk (seatd, DRM and input access), or start the desktop build under \
         X11 or Wayland. See docs/kiosk-display.md and `journalctl -u ha-kiosk`"
    )]
    NoDisplayBackend,
}

/// Inputs for [`classify_backend`], split out so the decision is testable
/// without touching the process environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackendProbe<'a> {
    pub slint_backend: Option<&'a str>,
    pub wayland_display: Option<&'a str>,
    pub x11_display: Option<&'a str>,
    pub has_drm_device: bool,
    pub linuxkms_compiled: bool,
    pub windowed_compiled: bool,
    pub target_linux: bool,
}

/// The display configuration resolved for this process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayPlan {
    pub backend: DisplayBackend,
    pub rotation: Option<u16>,
    pub fullscreen: bool,
}

/// Enable fullscreen on the given window (LinuxKMS/X11/Wayland).
pub fn set_fullscreen(window: &Window) {
    window.set_fullscreen(true);
}

/// Name of the configured display backend, if any.
pub fn backend_from_env() -> Option<String> {
    env_nonempty(BACKEND_ENV)
}

/// Detect whether a LinuxKMS backend is selected.
pub fn is_linuxkms() -> bool {
    backend_from_env().is_some_and(|value| value.contains("linuxkms"))
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

/// Validate a quarter-turn rotation.
pub fn validate_rotation(rotation: u16) -> Result<u16, KioskError> {
    match rotation {
        0 | 90 | 180 | 270 => Ok(rotation),
        other => Err(KioskError::UnsupportedRotation(other)),
    }
}

/// Resolve the rotation that should reach Slint.
///
/// An explicit `SLINT_KMS_ROTATION` wins over `display.rotation`, and a zero
/// rotation leaves the variable unset so Slint keeps its default.
pub fn resolve_rotation(
    configured: u16,
    env_value: Option<&str>,
) -> Result<Option<u16>, KioskError> {
    if let Some(raw) = env_value.filter(|value| !value.trim().is_empty()) {
        return parse_rotation_env(raw).map(Some);
    }
    let rotation = validate_rotation(configured)?;
    Ok((rotation != 0).then_some(rotation))
}

/// Choose the display backend from the probe inputs.
///
/// An explicit `SLINT_BACKEND` always wins. Without it, a LinuxKMS build prefers
/// the DRM path even when X11 variables are present, and a desktop build needs
/// an X11 or Wayland display.
pub fn classify_backend(probe: BackendProbe<'_>) -> Result<DisplayBackend, KioskError> {
    if let Some(name) = probe.slint_backend.filter(|value| !value.is_empty()) {
        if name.contains("linuxkms") {
            return if probe.linuxkms_compiled {
                Ok(DisplayBackend::LinuxKms)
            } else {
                Err(KioskError::BackendNotCompiled(name.to_string()))
            };
        }
        return if probe.windowed_compiled {
            Ok(DisplayBackend::Windowed)
        } else {
            Err(KioskError::BackendNotCompiled(name.to_string()))
        };
    }

    if !probe.target_linux {
        return if probe.windowed_compiled {
            Ok(DisplayBackend::Windowed)
        } else {
            Err(KioskError::BackendNotCompiled("winit".to_string()))
        };
    }

    let windowed_display = probe.wayland_display.is_some_and(|value| !value.is_empty())
        || probe.x11_display.is_some_and(|value| !value.is_empty());
    if windowed_display {
        if probe.windowed_compiled {
            return Ok(DisplayBackend::Windowed);
        }
        if probe.linuxkms_compiled {
            return Ok(DisplayBackend::LinuxKms);
        }
        return Err(KioskError::NoDisplayBackend);
    }

    if probe.has_drm_device && probe.linuxkms_compiled {
        return Ok(DisplayBackend::LinuxKms);
    }

    Err(KioskError::NoDisplayBackend)
}

/// Resolve the display plan from the environment and `[display]` configuration.
pub fn plan(display: &DisplayConfig) -> Result<DisplayPlan, KioskError> {
    let slint_backend = backend_from_env();
    let wayland_display = env_nonempty(WAYLAND_DISPLAY_ENV);
    let x11_display = env_nonempty(X11_DISPLAY_ENV);
    let probe = BackendProbe {
        slint_backend: slint_backend.as_deref(),
        wayland_display: wayland_display.as_deref(),
        x11_display: x11_display.as_deref(),
        has_drm_device: has_drm_device(),
        linuxkms_compiled: cfg!(feature = "kiosk"),
        windowed_compiled: cfg!(feature = "desktop"),
        target_linux: cfg!(target_os = "linux"),
    };
    let backend = classify_backend(probe)?;
    let env_rotation = env_nonempty(KMS_ROTATION_ENV);
    let rotation = resolve_rotation(display.rotation, env_rotation.as_deref())?;
    Ok(DisplayPlan {
        backend,
        rotation,
        fullscreen: display.fullscreen,
    })
}

fn parse_rotation_env(raw: &str) -> Result<u16, KioskError> {
    let invalid = || KioskError::InvalidRotationEnv {
        variable: KMS_ROTATION_ENV,
        value: raw.to_string(),
    };
    let rotation: u16 = raw.trim().parse().map_err(|_| invalid())?;
    validate_rotation(rotation).map_err(|_| invalid())
}

fn has_drm_device() -> bool {
    let Ok(entries) = std::fs::read_dir("/dev/dri") else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with("card"))
    })
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}
