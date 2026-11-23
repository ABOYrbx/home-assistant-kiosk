use slint::SharedPixelBuffer;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_BYTES: usize = 5 * 1024 * 1024; // 5 MB
const MAX_DIM: u32 = 4096;

#[derive(Clone)]
pub struct ImageCache {
    inner: Arc<Mutex<std::collections::HashMap<String, SharedPixelBuffer<slint::Rgba8Pixel>>>>,
    max_entries: usize,
}

impl ImageCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(std::collections::HashMap::new())),
            max_entries,
        }
    }

    pub async fn get(&self, url: &str) -> Option<SharedPixelBuffer<slint::Rgba8Pixel>> {
        self.inner.lock().await.get(url).cloned()
    }

    pub async fn insert(&self, url: String, buf: SharedPixelBuffer<slint::Rgba8Pixel>) {
        let mut g = self.inner.lock().await;
        if g.len() >= self.max_entries {
            // drop one arbitrary entry
            if let Some(k) = g.keys().next().cloned() {
                g.remove(&k);
            }
        }
        g.insert(url, buf);
    }
}

pub fn same_origin(a: &url::Url, b: &url::Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

pub async fn fetch_image(
    client: &reqwest::Client,
    ha_base: &url::Url,
    token: Option<&str>,
    image_url: &url::Url,
) -> Result<SharedPixelBuffer<slint::Rgba8Pixel>, String> {
    // origin check for token
    let send_token = same_origin(image_url, ha_base);

    let mut req = client.get(image_url.clone());
    if send_token && let Some(t) = token {
        req = req.bearer_auth(t);
    }

    let resp = req.send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("http {}", resp.status()));
    }
    // content-length check
    if let Some(cl) = resp.content_length()
        && cl as usize > MAX_BYTES
    {
        return Err("image too large".into());
    }
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("image too large".into());
    }

    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    if w == 0 || h == 0 || w > MAX_DIM || h > MAX_DIM {
        return Err("image dimensions invalid".into());
    }
    let mut buf = SharedPixelBuffer::<slint::Rgba8Pixel>::new(w, h);
    let dst = buf.make_mut_bytes();
    // slint SharedPixelBuffer bytes are RGBA8 packed
    dst.copy_from_slice(rgba.as_raw());
    Ok(buf)
}

pub fn build_client(allow_invalid_certs: bool) -> reqwest::Client {
    let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none());
    if allow_invalid_certs {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(test)]
mod tests {
    #[test]
    fn same_origin_fn() {
        let a = url::Url::parse("http://ha.local:8123").unwrap();
        let b = url::Url::parse("http://ha.local:8123/x.png").unwrap();
        let c = url::Url::parse("https://evil/x.png").unwrap();
        assert!(super::same_origin(&a, &b));
        assert!(!super::same_origin(&a, &c));
    }
}
