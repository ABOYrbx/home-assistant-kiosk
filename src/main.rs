#![deny(unsafe_code)]
//! Binary entry point.
//!
//! Wave 0 starts and runs the minimal Slint window. Command-line parsing, the
//! runtime, and the application controller arrive in later waves.

use anyhow::{Context, Result};
use slint::ComponentHandle;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let window = ha_kiosk::ui::AppWindow::new().context("failed to create the kiosk window")?;
    window.run().context("failed to run the kiosk window")?;

    Ok(())
}
