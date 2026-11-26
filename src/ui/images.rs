use crate::events::{AppEvent, EntityId};
use secrecy::{ExposeSecret, SecretString};
use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::collections::HashMap;
use std::io::Cursor;
use std::time::Duration;
use url::Url;

/// Largest album-art response accepted from Home Assistant or a third party.
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
/// Largest width or height accepted for an album-art image.
pub const MAX_IMAGE_DIMENSION: u32 = 4096;
/// Largest pixel count accepted for an album-art image.
pub const MAX_IMAGE_PIXELS: u64 = 8_000_000;
/// Default memory budget for decoded album art.
pub const DEFAULT_CACHE_BYTES: usize = 8 * 1024 * 1024;
pub const IMAGE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, thiserror::Error)]
pub enum ImageFetchError {
    #[error("image request failed: {0}")]
    Transport(String),
    #[error("image request returned HTTP {0}")]
    Http(u16),
    #[error("image exceeds the {MAX_IMAGE_BYTES} byte limit")]
    TooLarge,
    #[error("image exceeds the {MAX_IMAGE_PIXELS} pixel limit")]
    TooManyPixels,
    #[error("image could not be decoded: {0}")]
    Decode(String),
}

/// Scheme, host and port must match for a request to carry the Home Assistant token.
pub fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

/// Loggable form of an image URL: scheme, host and path only. The query string
/// is never included because it can carry a signed access token.
pub fn log_target(url: &Url) -> String {
    let mut target = format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default());
    if let Some(port) = url.port() {
        target.push(':');
        target.push_str(&port.to_string());
    }
    target.push_str(url.path());
    target
}

/// Follows redirects only while they stay on the request's original origin, so
/// the Home Assistant token is never forwarded to another host.
pub fn redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > 5 {
            return attempt.stop();
        }
        match attempt.previous().first() {
            Some(origin) if same_origin(origin, attempt.url()) => attempt.follow(),
            _ => attempt.stop(),
        }
    })
}

pub fn build_client(allow_invalid_certs: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .redirect(redirect_policy())
        .timeout(IMAGE_TIMEOUT);
    if allow_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder
        .build()
        .expect("HTTP client builder with fixed redirect policy")
}

pub async fn fetch_album_art(
    client: &reqwest::Client,
    ha_base: &Url,
    token: Option<&str>,
    image_url: &Url,
) -> Result<SharedPixelBuffer<Rgba8Pixel>, ImageFetchError> {
    if !matches!(image_url.scheme(), "http" | "https") {
        return Err(ImageFetchError::Transport("unsupported URL scheme".into()));
    }

    let target = log_target(image_url);
    tracing::debug!("fetching album art from {target}");

    let mut request = client.get(image_url.clone());
    if let Some(token) = token.filter(|_| same_origin(image_url, ha_base)) {
        request = request.bearer_auth(token);
    }

    let mut response = request
        .send()
        .await
        .map_err(|e| ImageFetchError::Transport(e.without_url().to_string()))?;
    if !response.status().is_success() {
        return Err(ImageFetchError::Http(response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|len| len > MAX_IMAGE_BYTES as u64)
    {
        return Err(ImageFetchError::TooLarge);
    }

    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| ImageFetchError::Transport(e.without_url().to_string()))?
    {
        if body.len() + chunk.len() > MAX_IMAGE_BYTES {
            return Err(ImageFetchError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }

    tokio::task::spawn_blocking(move || decode_image(&body))
        .await
        .map_err(|e| ImageFetchError::Decode(e.to_string()))?
}

/// Decodes and size-checks an image. Runs off the UI thread in the fetch path.
pub fn decode_image(bytes: &[u8]) -> Result<SharedPixelBuffer<Rgba8Pixel>, ImageFetchError> {
    // Read the header first so oversized images are rejected before allocation.
    let (width, height) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageFetchError::Decode(e.to_string()))?
        .into_dimensions()
        .map_err(|e| ImageFetchError::Decode(e.to_string()))?;
    if width == 0 || height == 0 {
        return Err(ImageFetchError::Decode("invalid image dimensions".into()));
    }
    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(ImageFetchError::Decode(format!(
            "image dimensions {width}x{height} exceed the {MAX_IMAGE_DIMENSION} pixel limit"
        )));
    }
    if u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS {
        return Err(ImageFetchError::TooManyPixels);
    }

    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageFetchError::Decode(e.to_string()))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_PIXELS * 4);
    reader.limits(limits);

    let image = reader
        .decode()
        .map_err(|e| ImageFetchError::Decode(e.to_string()))?
        .to_rgba8();
    let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
    buffer.make_mut_bytes().copy_from_slice(image.as_raw());
    Ok(buffer)
}

struct CacheEntry {
    image: SharedPixelBuffer<Rgba8Pixel>,
    bytes: usize,
    used: u64,
}

/// Least-recently-used album-art cache bounded by the decoded image size.
pub struct AlbumArtCache {
    max_bytes: usize,
    current_bytes: usize,
    tick: u64,
    entries: HashMap<String, CacheEntry>,
}

impl AlbumArtCache {
    pub fn new(max_bytes: usize) -> Self {
        Self {
            max_bytes: max_bytes.max(1),
            current_bytes: 0,
            tick: 0,
            entries: HashMap::new(),
        }
    }

    pub fn get(&mut self, url: &str) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
        self.tick += 1;
        let tick = self.tick;
        let entry = self.entries.get_mut(url)?;
        entry.used = tick;
        Some(entry.image.clone())
    }

    /// Returns false when the image alone exceeds the cache budget.
    pub fn insert(&mut self, url: String, image: SharedPixelBuffer<Rgba8Pixel>) -> bool {
        let bytes = image.width() as usize * image.height() as usize * 4;
        if bytes > self.max_bytes {
            return false;
        }
        if let Some(old) = self.entries.remove(&url) {
            self.current_bytes -= old.bytes;
        }
        while self.current_bytes + bytes > self.max_bytes {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.current_bytes -= removed.bytes;
            }
        }
        self.tick += 1;
        self.entries.insert(
            url,
            CacheEntry {
                image,
                bytes,
                used: self.tick,
            },
        );
        self.current_bytes += bytes;
        true
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.current_bytes
    }
}

pub struct AlbumArtFetcher {
    client: reqwest::Client,
    ha_base: Url,
    token: Option<SecretString>,
    cache: AlbumArtCache,
}

impl AlbumArtFetcher {
    pub fn new(
        client: reqwest::Client,
        ha_base: Url,
        token: Option<SecretString>,
        max_cache_bytes: usize,
    ) -> Self {
        Self {
            client,
            ha_base,
            token,
            cache: AlbumArtCache::new(max_cache_bytes),
        }
    }

    pub async fn load(
        &mut self,
        url: &Url,
    ) -> Result<SharedPixelBuffer<Rgba8Pixel>, ImageFetchError> {
        if let Some(image) = self.cache.get(url.as_str()) {
            return Ok(image);
        }
        let token = self.token.as_ref().map(|token| token.expose_secret());
        let image = fetch_album_art(&self.client, &self.ha_base, token, url).await?;
        self.cache.insert(url.as_str().to_string(), image.clone());
        Ok(image)
    }

    pub fn cache(&self) -> &AlbumArtCache {
        &self.cache
    }
}

#[derive(Debug, Clone)]
pub struct AlbumArtRequest {
    pub entity_id: EntityId,
    pub url: Url,
}

/// Background loop: resolves album art requests into `AppEvent`s. The previous
/// image is kept by the UI state when a fetch fails.
pub async fn album_art_loop(
    mut fetcher: AlbumArtFetcher,
    mut requests: tokio::sync::mpsc::Receiver<AlbumArtRequest>,
    events: tokio::sync::mpsc::Sender<AppEvent>,
) {
    while let Some(request) = requests.recv().await {
        let event = match fetcher.load(&request.url).await {
            Ok(image) => AppEvent::AlbumArt {
                entity_id: request.entity_id,
                url: request.url.to_string(),
                image,
            },
            Err(error) => {
                tracing::warn!(
                    "album art fetch failed for {}: {error}",
                    log_target(&request.url)
                );
                AppEvent::ImageFailed {
                    entity_id: request.entity_id,
                    url: request.url.to_string(),
                    reason: error.to_string(),
                }
            }
        };
        if events.send(event).await.is_err() {
            break;
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlbumArt {
    pub entity_id: EntityId,
    pub url: String,
    pub image: SharedPixelBuffer<Rgba8Pixel>,
}

/// Keeps the last successfully decoded album art. A failed fetch never clears it.
#[derive(Default)]
pub struct AlbumArtState {
    current: Option<AlbumArt>,
    error: Option<String>,
}

impl AlbumArtState {
    pub fn apply_loaded(
        &mut self,
        entity_id: EntityId,
        url: String,
        image: SharedPixelBuffer<Rgba8Pixel>,
    ) {
        self.current = Some(AlbumArt {
            entity_id,
            url,
            image,
        });
        self.error = None;
    }

    pub fn apply_failed(&mut self, reason: String) {
        self.error = Some(reason);
    }

    pub fn current(&self) -> Option<&AlbumArt> {
        self.current.as_ref()
    }

    pub fn current_url(&self) -> Option<&str> {
        self.current.as_ref().map(|art| art.url.as_str())
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn clear(&mut self) {
        self.current = None;
        self.error = None;
    }
}
