use anyhow::Result;
use ha_kiosk::{AppController, Config, MainWindow, kiosk};
use slint::ComponentHandle;
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

    if let Some(path) = config_path {
        match Config::load(std::path::Path::new(&path)) {
            Ok(cfg) => {
                kiosk::apply_display(ui.window(), &cfg.display);
                let (tx, _rx) = tokio::sync::mpsc::channel(1);
                let controller = AppController::new(ui, cfg, tx);
                return controller
                    .window()
                    .run()
                    .map_err(|e| anyhow::anyhow!(e.to_string()));
            }
            Err(e) => {
                tracing::error!("config error: {e}");
            }
        }
    }

    ui.run().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
