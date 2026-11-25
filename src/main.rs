use anyhow::{Context, Result};
use ha_kiosk::cache::FileCache;
use ha_kiosk::state::{AppState, CacheStore};
use ha_kiosk::ui::images::{AlbumArtFetcher, album_art_loop, build_client};
use ha_kiosk::{AppController, Config, MainWindow, kiosk};
use slint::ComponentHandle;
use std::env;
use std::path::PathBuf;
use std::sync::mpsc;
use tokio::sync::mpsc as tokio_mpsc;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config_path = parse_args(env::args().skip(1))?;
    let config = config_path
        .as_ref()
        .map(|path| {
            Config::load(path)
                .with_context(|| format!("failed to load configuration {}", path.display()))
        })
        .transpose()?;

    if let Some(config) = &config {
        let plan = kiosk::plan(&config.display)?;
        if let Some(rotation) = plan.rotation {
            export_rotation(rotation);
        }
    }

    let cached_state = config.as_ref().and_then(|config| {
        let cache = FileCache::new(
            config.cache.path.clone(),
            3,
            config.cache_fingerprint(),
            config.cache.max_bytes,
        );
        match cache.load() {
            Ok(state) => state.map(AppState::restore_from_cache),
            Err(error) => {
                tracing::warn!("could not restore cache: {error}");
                None
            }
        }
    });

    let ui = MainWindow::new()
        .map_err(|error| anyhow::anyhow!("failed to start the display backend: {error}"))?;

    let Some(config) = config else {
        ui.run()
            .map_err(|error| anyhow::anyhow!("UI error: {error}"))?;
        return Ok(());
    };

    kiosk::apply_display(ui.window(), &config.display);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("could not start the Tokio runtime")?;
    let (commands_tx, commands_rx) = tokio_mpsc::channel(64);
    let (events_tx, events_rx) = tokio_mpsc::channel(256);
    let (ha_tx, mut ha_rx) = tokio_mpsc::channel(256);
    let (art_tx, art_rx) = tokio_mpsc::channel(8);

    let ha_config = config.ha.clone();
    let entity_ids = config.subscribed_entity_ids();
    runtime.spawn(async move {
        ha_kiosk::ha::run(ha_config, entity_ids, ha_tx, commands_rx).await;
    });
    let app_events = events_tx.clone();
    runtime.spawn(async move {
        while let Some(event) = ha_rx.recv().await {
            if app_events
                .send(ha_kiosk::AppEvent::Ha(event))
                .await
                .is_err()
            {
                break;
            }
        }
    });

    let art_client = build_client(config.ha.allow_invalid_certs);
    let art_fetcher = AlbumArtFetcher::new(
        art_client,
        config.ha.url.clone(),
        Some(config.ha.token.clone()),
        8 * 1024 * 1024,
    );
    runtime.spawn(album_art_loop(art_fetcher, art_rx, events_tx));

    let mut controller = AppController::with_state(
        ui,
        config.clone(),
        commands_tx,
        cached_state.unwrap_or_default(),
    );
    controller.set_album_art_sender(art_tx);
    controller.start_event_pump(events_rx);

    let cache = FileCache::new(
        config.cache.path.clone(),
        3,
        config.cache_fingerprint(),
        config.cache.max_bytes,
    );
    let (cache_tx, cache_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("state-cache".into())
        .spawn(move || {
            while let Ok(state) = cache_rx.recv() {
                if let Err(error) = cache.save(&state) {
                    tracing::warn!("could not save cache: {error}");
                }
            }
        })
        .expect("spawn cache worker");
    controller.start_cache_pump(cache_tx, config.cache.write_debounce_seconds.max(2));

    controller
        .window()
        .run()
        .map_err(|error| anyhow::anyhow!("UI error: {error}"))?;
    runtime.shutdown_background();
    Ok(())
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

/// Pass the configured rotation to the LinuxKMS backend before it starts.
fn export_rotation(rotation: u16) {
    // SAFETY: this runs before Slint and Tokio start any other thread.
    unsafe {
        env::set_var(kiosk::KMS_ROTATION_ENV, rotation.to_string());
    }
}
