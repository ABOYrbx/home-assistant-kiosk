use anyhow::{Context, Result};
use ha_kiosk::{AppController, Config, MainWindow, kiosk};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path = parse_args(env::args().skip(1))?;

    let config = match config_path {
        Some(path) => Some(
            Config::load(&path)
                .with_context(|| format!("failed to load configuration {}", path.display()))?,
        ),
        None => None,
    };

    // The display plan must be resolved before the Slint backend starts: the
    // LinuxKMS backend reads SLINT_KMS_ROTATION during initialisation.
    if let Some(config) = &config {
        let plan = kiosk::plan(&config.display)?;
        tracing::info!(
            backend = plan.backend.as_str(),
            backend_description = plan.backend.describe(),
            rotation = ?plan.rotation,
            fullscreen = plan.fullscreen,
            "resolved display plan"
        );
        if let Some(rotation) = plan.rotation {
            export_rotation(rotation);
        }
    }

    let ui = MainWindow::new()
        .map_err(|error| anyhow::anyhow!("failed to start the display backend: {error}"))?;

    let Some(config) = config else {
        ui.run()
            .map_err(|error| anyhow::anyhow!("UI error: {error}"))?;
        return Ok(());
    };

    if config.display.fullscreen {
        kiosk::set_fullscreen(ui.window());
    }
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let controller = AppController::new(ui, config, tx);
    controller
        .window()
        .run()
        .map_err(|error| anyhow::anyhow!("UI error: {error}"))
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Option<PathBuf>> {
    let mut config_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                let value = args
                    .next()
                    .context("--config needs a path to a TOML file")?;
                config_path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => {
                println!(
                    "Usage: ha-kiosk [--config <path>]\n\n\
                     Without --config the UI starts with built-in defaults and no data."
                );
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument {other:?}; try --help"),
        }
    }
    Ok(config_path)
}

/// Pass the configured rotation to the LinuxKMS backend.
///
/// `std::env::set_var` is unsafe in edition 2024 because the environment is
/// shared with other threads. This runs before the Slint backend and the Tokio
/// runtime start, so no other thread can read the environment concurrently.
fn export_rotation(rotation: u16) {
    unsafe {
        env::set_var(kiosk::KMS_ROTATION_ENV, rotation.to_string());
    }
}
