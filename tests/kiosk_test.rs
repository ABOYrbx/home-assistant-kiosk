use ha_kiosk::kiosk::{self, BackendProbe, DisplayBackend, KioskError};

fn linux_kiosk_build() -> BackendProbe<'static> {
    BackendProbe {
        slint_backend: None,
        wayland_display: None,
        x11_display: None,
        has_drm_device: false,
        linuxkms_compiled: true,
        windowed_compiled: false,
        target_linux: true,
    }
}

fn desktop_build() -> BackendProbe<'static> {
    BackendProbe {
        linuxkms_compiled: false,
        windowed_compiled: true,
        ..linux_kiosk_build()
    }
}

#[test]
fn explicit_linuxkms_backend_selects_linuxkms() {
    let probe = BackendProbe {
        slint_backend: Some(kiosk::LINUXKMS_BACKEND),
        x11_display: Some(":0"),
        has_drm_device: true,
        ..linux_kiosk_build()
    };
    assert_eq!(kiosk::classify_backend(probe), Ok(DisplayBackend::LinuxKms));
}

#[test]
fn explicit_linuxkms_backend_requires_the_kiosk_feature() {
    let probe = BackendProbe {
        slint_backend: Some(kiosk::LINUXKMS_BACKEND),
        ..desktop_build()
    };
    assert_eq!(
        kiosk::classify_backend(probe),
        Err(KioskError::BackendNotCompiled(
            kiosk::LINUXKMS_BACKEND.to_string()
        ))
    );
}

#[test]
fn x11_display_selects_the_windowed_backend() {
    let probe = BackendProbe {
        x11_display: Some(":0"),
        ..desktop_build()
    };
    assert_eq!(kiosk::classify_backend(probe), Ok(DisplayBackend::Windowed));
}

#[test]
fn wayland_display_selects_the_windowed_backend() {
    let probe = BackendProbe {
        wayland_display: Some("wayland-0"),
        ..desktop_build()
    };
    assert_eq!(kiosk::classify_backend(probe), Ok(DisplayBackend::Windowed));
}

#[test]
fn kiosk_build_ignores_x11_variables_without_a_windowed_backend() {
    let probe = BackendProbe {
        x11_display: Some(":0"),
        has_drm_device: true,
        ..linux_kiosk_build()
    };
    assert_eq!(kiosk::classify_backend(probe), Ok(DisplayBackend::LinuxKms));
}

#[test]
fn drm_device_selects_linuxkms_without_display_variables() {
    let probe = BackendProbe {
        has_drm_device: true,
        ..linux_kiosk_build()
    };
    assert_eq!(kiosk::classify_backend(probe), Ok(DisplayBackend::LinuxKms));
}

#[test]
fn missing_display_reports_an_actionable_error() {
    let error = kiosk::classify_backend(linux_kiosk_build()).unwrap_err();
    let message = error.to_string();
    assert!(
        message.contains("SLINT_BACKEND=linuxkms-software"),
        "{message}"
    );
    assert!(message.contains("seatd"), "{message}");
    assert!(message.contains("docs/kiosk-display.md"), "{message}");
}

#[test]
fn non_linux_target_uses_the_windowed_backend() {
    let probe = BackendProbe {
        target_linux: false,
        ..desktop_build()
    };
    assert_eq!(kiosk::classify_backend(probe), Ok(DisplayBackend::Windowed));
}

#[test]
fn quarter_turn_rotations_are_valid() {
    for rotation in [0, 90, 180, 270] {
        assert_eq!(kiosk::validate_rotation(rotation), Ok(rotation));
    }
}

#[test]
fn other_rotations_are_rejected() {
    for rotation in [1, 45, 89, 91, 360] {
        assert_eq!(
            kiosk::validate_rotation(rotation),
            Err(KioskError::UnsupportedRotation(rotation))
        );
    }
}

#[test]
fn zero_config_rotation_leaves_the_environment_unset() {
    assert_eq!(kiosk::resolve_rotation(0, None), Ok(None));
}

#[test]
fn config_rotation_is_exported() {
    assert_eq!(kiosk::resolve_rotation(90, None), Ok(Some(90)));
}

#[test]
fn environment_rotation_wins_over_config() {
    assert_eq!(kiosk::resolve_rotation(90, Some("270")), Ok(Some(270)));
}

#[test]
fn blank_environment_rotation_falls_back_to_config() {
    assert_eq!(kiosk::resolve_rotation(180, Some("  ")), Ok(Some(180)));
}

#[test]
fn invalid_environment_rotation_is_rejected() {
    for raw in ["45", "left", "90deg"] {
        assert_eq!(
            kiosk::resolve_rotation(0, Some(raw)),
            Err(KioskError::InvalidRotationEnv {
                variable: kiosk::KMS_ROTATION_ENV,
                value: raw.to_string(),
            })
        );
    }
}

#[test]
fn platform_label_is_never_empty() {
    assert!(!kiosk::platform().is_empty());
}
