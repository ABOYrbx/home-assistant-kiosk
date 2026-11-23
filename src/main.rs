use anyhow::Result;
use ha_kiosk::ui::images::{self, AlbumArtFetcher};
use ha_kiosk::{AppController, Config, MainWindow, kiosk};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::mpsc;

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
                if cfg.display.fullscreen {
                    kiosk::set_fullscreen(ui.window());
                }

                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

                let (art_tx, art_rx) = mpsc::channel(8);
                let (events_tx, mut events_rx) = mpsc::channel(64);
                let fetcher = AlbumArtFetcher::new(
                    images::build_client(cfg.ha.allow_invalid_certs),
                    cfg.ha.url.clone(),
                    Some(cfg.ha.token.clone()),
                    images::DEFAULT_CACHE_BYTES,
                );
                runtime.spawn(images::album_art_loop(fetcher, art_rx, events_tx));

                let (tx, _rx) = mpsc::channel(1);
                let controller = Rc::new(RefCell::new(AppController::new(ui, cfg, tx)));
                controller.borrow_mut().set_album_art_sender(art_tx);

                let drain = slint::Timer::default();
                let weak_controller = Rc::downgrade(&controller);
                drain.start(
                    slint::TimerMode::Repeated,
                    Duration::from_millis(100),
                    move || {
                        if let Some(controller) = weak_controller.upgrade() {
                            while let Ok(event) = events_rx.try_recv() {
                                controller.borrow_mut().apply(event);
                            }
                        }
                    },
                );

                let window = controller.borrow().window().clone_strong();
                let result = window.run();
                drop(drain);
                drop(runtime);
                return result.map_err(|e| anyhow::anyhow!(e.to_string()));
            }
            Err(e) => {
                tracing::error!("config error: {e}");
            }
        }
    }

    ui.run().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(())
}
