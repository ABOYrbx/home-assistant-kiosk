use anyhow::Result;
use ha_kiosk::{AppController, Config, MainWindow, Theme, kiosk};
use slint::{ComponentHandle, Global};
use std::env;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut config_path: Option<String> = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--config" {
            config_path = args.next();
        }
    }

    let ui = MainWindow::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;

    let config = config_path.and_then(|path| match Config::load(std::path::Path::new(&path)) {
        Ok(config) => Some(config),
        Err(error) => {
            tracing::error!("config error: {error}");
            None
        }
    });

    if let Some(config) = &config {
        Theme::get(&ui).set_dark_mode(config.display.theme != "light");
        if config.display.fullscreen {
            kiosk::set_fullscreen(ui.window());
        }
    }

    // The shell runs without a network session until the Wave 1 event loop is
    // wired to the controller. Keeping the receiver alive makes the command
    // sender usable for that integration.
    let (command_tx, _command_rx) = tokio::sync::mpsc::channel(8);
    let controller = AppController::new(ui, command_tx);
    controller
        .window()
        .run()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
