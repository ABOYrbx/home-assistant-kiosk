use ha_kiosk::ui::images::*;
use secrecy::SecretString;
use std::io::{Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug)]
struct Request {
    path: String,
    authorization: Option<String>,
}

struct TestServer {
    port: u16,
    requests: Arc<Mutex<Vec<Request>>>,
}

struct Response {
    status: &'static str,
    headers: Vec<(&'static str, String)>,
    body: Vec<u8>,
    content_length: Option<usize>,
}

impl Response {
    fn into_bytes(self) -> Vec<u8> {
        let mut head = format!("HTTP/1.1 {}\r\n", self.status);
        let content_length = self.content_length.unwrap_or(self.body.len());
        head.push_str(&format!("Content-Length: {content_length}\r\n"));
        head.push_str("Connection: close\r\n");
        for (name, value) in self.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        let mut bytes = head.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}

fn ok(content_type: &'static str, body: Vec<u8>) -> Response {
    Response {
        status: "200 OK",
        headers: vec![("Content-Type", content_type.to_string())],
        body,
        content_length: None,
    }
}

fn redirect(location: String) -> Response {
    Response {
        status: "302 Found",
        headers: vec![("Location", location)],
        body: Vec::new(),
        content_length: None,
    }
}

fn start_server(handler: impl Fn(u16, &Request) -> Response + Send + Sync + 'static) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let port = listener.local_addr().unwrap().port();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut stream = stream;
            let Some(request) = read_request(&mut stream) else {
                continue;
            };
            seen.lock().unwrap().push(request.clone());
            let response = handler(port, &request).into_bytes();
            let _ = stream.write_all(&response);
            let _ = stream.flush();
        }
    });
    TestServer { port, requests }
}

fn read_request(stream: &mut TcpStream) -> Option<Request> {
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let text = String::from_utf8_lossy(&buffer);
    let mut lines = text.lines();
    let path = lines.next()?.split_whitespace().nth(1)?.to_string();
    let mut authorization = None;
    for line in lines {
        if line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("authorization")
        {
            authorization = Some(value.trim().to_string());
        }
    }
    Some(Request {
        path,
        authorization,
    })
}

fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 40, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encode png");
    bytes
}

fn base_url(port: u16) -> Url {
    Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap()
}

#[tokio::test]
async fn sends_the_token_only_to_the_home_assistant_origin() {
    let art = png(2, 2);
    let ha_art = art.clone();
    let ha = start_server(move |_, request| match request.path.as_str() {
        "/art.png" => ok("image/png", ha_art.clone()),
        _ => ok("text/plain", b"missing".to_vec()),
    });
    let other = start_server(move |_, _| ok("image/png", art.clone()));

    let client = build_client(false);
    let ha_base = base_url(ha.port);

    let image = fetch_album_art(
        &client,
        &ha_base,
        Some("test-token"),
        &Url::parse(&format!("http://127.0.0.1:{}/art.png", ha.port)).unwrap(),
    )
    .await
    .expect("home assistant art");
    assert_eq!((image.width(), image.height()), (2, 2));

    fetch_album_art(
        &client,
        &ha_base,
        Some("test-token"),
        &Url::parse(&format!("http://127.0.0.1:{}/art.png", other.port)).unwrap(),
    )
    .await
    .expect("third-party art");

    assert_eq!(
        ha.requests.lock().unwrap()[0].authorization.as_deref(),
        Some("Bearer test-token")
    );
    assert_eq!(other.requests.lock().unwrap()[0].authorization, None);
}

#[tokio::test]
async fn follows_a_same_origin_redirect_with_the_token() {
    let art = png(2, 2);
    let server = start_server(move |port, request| match request.path.as_str() {
        "/redirect" => redirect(format!("http://127.0.0.1:{port}/art.png")),
        _ => ok("image/png", art.clone()),
    });

    let client = build_client(false);
    fetch_album_art(
        &client,
        &base_url(server.port),
        Some("test-token"),
        &Url::parse(&format!("http://127.0.0.1:{}/redirect", server.port)).unwrap(),
    )
    .await
    .expect("same-origin redirect");

    let requests = server.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].path, "/redirect");
    assert_eq!(requests[1].path, "/art.png");
    assert_eq!(
        requests[1].authorization.as_deref(),
        Some("Bearer test-token")
    );
}

#[tokio::test]
async fn does_not_forward_the_token_across_an_origin_redirect() {
    let other = start_server(|_, _| ok("image/png", png(2, 2)));
    let other_port = other.port;
    let ha = start_server(move |_, _| redirect(format!("http://127.0.0.1:{other_port}/art.png")));

    let client = build_client(false);
    let result = fetch_album_art(
        &client,
        &base_url(ha.port),
        Some("test-token"),
        &Url::parse(&format!("http://127.0.0.1:{}/cross", ha.port)).unwrap(),
    )
    .await;

    assert!(matches!(result, Err(ImageFetchError::Http(302))));
    assert!(
        other.requests.lock().unwrap().is_empty(),
        "the redirect target must never be contacted with credentials"
    );
}

#[tokio::test]
async fn rejects_an_image_over_the_byte_limit() {
    let server = start_server(|_, _| Response {
        status: "200 OK",
        headers: vec![("Content-Type", "image/png".to_string())],
        body: Vec::new(),
        content_length: Some(MAX_IMAGE_BYTES + 1),
    });
    let client = build_client(false);
    let url = Url::parse(&format!("http://127.0.0.1:{}/huge.png", server.port)).unwrap();

    // A content length above the limit is rejected before the body is read.
    let result = fetch_album_art(&client, &base_url(server.port), None, &url).await;
    assert!(matches!(result, Err(ImageFetchError::TooLarge)));
}

#[tokio::test]
async fn rejects_a_non_image_response() {
    let server = start_server(|_, _| ok("text/plain", b"not an image".to_vec()));
    let client = build_client(false);
    let result = fetch_album_art(
        &client,
        &base_url(server.port),
        None,
        &Url::parse(&format!("http://127.0.0.1:{}/art.png", server.port)).unwrap(),
    )
    .await;

    assert!(matches!(result, Err(ImageFetchError::Decode(_))));
}

#[test]
fn decode_image_enforces_dimension_limits() {
    let small = decode_image(&png(2, 3)).expect("small image");
    assert_eq!((small.width(), small.height()), (2, 3));

    let oversized = decode_image(&png(5000, 8));
    assert!(oversized.is_err());

    let too_many_pixels = decode_image(&png(4096, 2048));
    assert!(matches!(
        too_many_pixels,
        Err(ImageFetchError::TooManyPixels)
    ));
}

#[test]
fn a_failed_fetch_keeps_the_previous_art() {
    let mut state = AlbumArtState::default();
    state.apply_loaded(
        "media_player.example".into(),
        "http://homeassistant.local/api/art.png".into(),
        decode_image(&png(2, 2)).unwrap(),
    );
    assert!(state.current().is_some());
    assert!(state.error().is_none());

    state.apply_failed("image request returned HTTP 404".into());

    let current = state.current().expect("previous art is kept");
    assert_eq!(current.entity_id, "media_player.example");
    assert_eq!(current.image.width(), 2);
    assert_eq!(state.error(), Some("image request returned HTTP 404"));
}

#[test]
fn cache_evicts_the_least_recently_used_image_and_respects_its_budget() {
    let first = decode_image(&png(2, 2)).unwrap();
    let bytes = first.width() as usize * first.height() as usize * 4;
    let mut cache = AlbumArtCache::new(bytes * 2);

    assert!(cache.insert("a".into(), first.clone()));
    assert!(cache.insert("b".into(), first.clone()));
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.bytes(), bytes * 2);

    // Refresh "a" so "b" becomes the least recently used entry.
    assert!(cache.get("a").is_some());
    assert!(cache.insert("c".into(), first.clone()));
    assert!(cache.get("b").is_none());
    assert!(cache.get("a").is_some());
    assert!(cache.get("c").is_some());
    assert!(cache.bytes() <= bytes * 2);

    let big = decode_image(&png(8, 8)).unwrap();
    assert!(!cache.insert("big".into(), big));
}

#[tokio::test]
async fn fetcher_caches_by_url() {
    let server = start_server(|_, _| ok("image/png", png(2, 2)));
    let base = base_url(server.port);
    let mut fetcher = AlbumArtFetcher::new(
        build_client(false),
        base.clone(),
        Some(SecretString::from("test-token".to_string())),
        DEFAULT_CACHE_BYTES,
    );
    let url = Url::parse(&format!("http://127.0.0.1:{}/art.png", server.port)).unwrap();

    fetcher.load(&url).await.expect("first load");
    fetcher.load(&url).await.expect("cached load");

    assert_eq!(server.requests.lock().unwrap().len(), 1);
    assert_eq!(fetcher.cache().len(), 1);
}

#[tokio::test]
async fn rejects_non_http_image_urls() {
    let client = build_client(false);
    let result = fetch_album_art(
        &client,
        &Url::parse("http://127.0.0.1:1/").unwrap(),
        None,
        &Url::parse("file:///etc/passwd").unwrap(),
    )
    .await;
    assert!(matches!(result, Err(ImageFetchError::Transport(_))));
}

#[test]
fn log_target_never_includes_the_query_string() {
    let url =
        Url::parse("https://homeassistant.local:8123/api/media_player_proxy/x?token=secret-value")
            .unwrap();
    let target = log_target(&url);

    assert_eq!(
        target,
        "https://homeassistant.local:8123/api/media_player_proxy/x"
    );
    assert!(!target.contains("secret-value"));
    assert!(!target.contains('?'));
}

#[test]
fn same_origin_compares_scheme_host_and_port() {
    let a = Url::parse("http://homeassistant.local:8123/x").unwrap();
    assert!(same_origin(
        &a,
        &Url::parse("http://homeassistant.local:8123/y").unwrap()
    ));
    assert!(!same_origin(
        &a,
        &Url::parse("https://homeassistant.local:8123/y").unwrap()
    ));
    assert!(!same_origin(
        &a,
        &Url::parse("http://homeassistant.local:8124/y").unwrap()
    ));
    assert!(!same_origin(
        &a,
        &Url::parse("http://other.local:8123/y").unwrap()
    ));
}
