//! Allowlisted, credential-free JSON fetcher for extensions.
//!
//! The host checks the policy before it hands a URL to a [`HttpFetcher`]. The
//! production fetcher is GET-only, follows no redirects, and caps the response
//! body. Plain HTTP is rejected unless development mode explicitly allows it
//! for a loopback address.

use std::io::Read;
use std::net::IpAddr;
use std::time::Duration;

use serde_json::Value;
use thiserror::Error;
use url::{Host, Url};

/// One exact host, optionally with an explicit port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowedHost {
    host: String,
    port: Option<u16>,
}

/// Parse a manifest `http_hosts` entry.
///
/// Wildcards, schemes, paths, user information, IPv6 literals, and empty
/// labels are rejected. The host is stored lowercase.
pub fn parse_allowed_host(entry: &str) -> Result<AllowedHost, HttpError> {
    let entry = entry.trim();
    if entry.is_empty()
        || entry.len() > 255
        || entry.contains("://")
        || entry.contains('/')
        || entry.contains('@')
        || entry.contains('*')
        || entry.contains('?')
        || entry.contains('#')
        || entry.contains('[')
        || entry.contains(']')
    {
        return Err(HttpError::InvalidHostEntry(entry.to_string()));
    }

    let (host, port) = match entry.rsplit_once(':') {
        Some((host, port)) => {
            let port = port
                .parse::<u16>()
                .ok()
                .filter(|port| *port != 0)
                .ok_or_else(|| HttpError::InvalidHostEntry(entry.to_string()))?;
            (host, Some(port))
        }
        None => (entry, None),
    };

    if host.is_empty() || host.contains(':') {
        return Err(HttpError::InvalidHostEntry(entry.to_string()));
    }

    let host = match Host::parse(host) {
        Ok(Host::Domain(domain)) => domain.to_ascii_lowercase(),
        Ok(Host::Ipv4(address)) => address.to_string(),
        Ok(Host::Ipv6(_)) | Err(_) => {
            return Err(HttpError::InvalidHostEntry(entry.to_string()));
        }
    };

    Ok(AllowedHost { host, port })
}

/// The enforced HTTP policy for one extension.
#[derive(Debug, Clone, Default)]
pub struct HttpPolicy {
    allowed: Vec<AllowedHost>,
    dev_mode: bool,
}

impl HttpPolicy {
    /// Build a policy from the manifest allowlist.
    pub fn new(hosts: &[String], dev_mode: bool) -> Result<Self, HttpError> {
        let allowed = hosts
            .iter()
            .map(|host| parse_allowed_host(host))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { allowed, dev_mode })
    }

    /// Check a URL against the policy.
    ///
    /// The returned URL is the parsed value that the fetcher may use. The
    /// method rejects a scheme other than HTTPS, credentials in the URL, and a
    /// host or port outside the manifest allowlist.
    pub fn check(&self, raw: &str) -> Result<Url, HttpError> {
        let url = Url::parse(raw).map_err(HttpError::InvalidUrl)?;
        let redacted_url = redacted(&url);

        match url.scheme() {
            "https" => {}
            "http" if self.dev_mode && is_loopback(&url) => {}
            _ => return Err(HttpError::InsecureScheme { url: redacted_url }),
        }

        if !url.username().is_empty() || url.password().is_some() {
            return Err(HttpError::CredentialsNotAllowed { url: redacted_url });
        }

        let host = url
            .host_str()
            .ok_or(HttpError::InvalidUrl(url::ParseError::EmptyHost))?
            .to_ascii_lowercase();
        let actual_port = url.port_or_known_default();
        let allowed = self.allowed.iter().any(|entry| {
            if entry.host != host {
                return false;
            }
            match entry.port {
                Some(port) => actual_port == Some(port),
                None => actual_port == default_port(url.scheme()),
            }
        });

        if allowed {
            Ok(url)
        } else {
            Err(HttpError::HostNotAllowed { host })
        }
    }
}

/// A JSON fetcher used after the policy accepted the URL.
///
/// The trait exists so tests can exercise the policy without network access;
/// the production implementation is [`ReqwestFetcher`]. Implementations must
/// honor the supplied timeout, which is bounded by the run's remaining
/// wall-clock budget.
pub trait HttpFetcher: Send + Sync {
    fn get_json(&self, url: &Url, timeout: Duration) -> Result<Value, HttpError>;
}

/// Blocking GET-only JSON fetcher backed by reqwest.
///
/// It follows no redirects, sends no credentials, and reads at most the
/// configured number of bytes. Construct and call it on a background thread;
/// it must not block the UI thread.
pub struct ReqwestFetcher {
    client: reqwest::blocking::Client,
    max_bytes: usize,
}

impl ReqwestFetcher {
    pub fn new(timeout: Duration, max_bytes: usize) -> Result<Self, HttpError> {
        let client = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(timeout)
            .user_agent(concat!(
                "ha-kiosk-extension-host/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|error| HttpError::Client(error.without_url().to_string()))?;
        Ok(Self { client, max_bytes })
    }
}

impl HttpFetcher for ReqwestFetcher {
    fn get_json(&self, url: &Url, timeout: Duration) -> Result<Value, HttpError> {
        let safe_url = redacted(url);
        let response = self
            .client
            .get(url.clone())
            .timeout(timeout)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    HttpError::Timeout {
                        url: safe_url.clone(),
                    }
                } else {
                    HttpError::RequestFailed {
                        url: safe_url.clone(),
                        reason: error.without_url().to_string(),
                    }
                }
            })?;

        let status = response.status();
        if status.is_redirection() {
            return Err(HttpError::RedirectNotAllowed { url: safe_url });
        }
        if !status.is_success() {
            return Err(HttpError::Status {
                status: status.as_u16(),
                url: safe_url,
            });
        }

        let mut reader = response.take(self.max_bytes.saturating_add(1) as u64);
        let mut body = Vec::new();
        reader
            .read_to_end(&mut body)
            .map_err(|error| HttpError::RequestFailed {
                url: safe_url.clone(),
                reason: error.to_string(),
            })?;
        if body.len() > self.max_bytes {
            return Err(HttpError::ResponseTooLarge {
                url: safe_url,
                max: self.max_bytes,
            });
        }

        serde_json::from_slice(&body).map_err(|_| HttpError::InvalidJson { url: safe_url })
    }
}

/// HTTP policy and transport errors.
#[derive(Debug, Error)]
pub enum HttpError {
    #[error("invalid URL: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("invalid HTTP host entry: {0}")]
    InvalidHostEntry(String),
    #[error("URL must use HTTPS: {url}")]
    InsecureScheme { url: String },
    #[error("URL credentials are not allowed: {url}")]
    CredentialsNotAllowed { url: String },
    #[error("host is not allowed: {host}")]
    HostNotAllowed { host: String },
    #[error("HTTP redirects are not allowed: {url}")]
    RedirectNotAllowed { url: String },
    #[error("HTTP request failed: {url}: {reason}")]
    RequestFailed { url: String, reason: String },
    #[error("HTTP response exceeded the {max} byte limit: {url}")]
    ResponseTooLarge { url: String, max: usize },
    #[error("HTTP request timed out: {url}")]
    Timeout { url: String },
    #[error("HTTP response was not valid JSON: {url}")]
    InvalidJson { url: String },
    #[error("HTTP request returned status {status}: {url}")]
    Status { status: u16, url: String },
    #[error("HTTP client could not be created: {0}")]
    Client(String),
}

/// Render a URL without credentials, query string, or fragment.
///
/// Query strings can carry signed tokens, so they must never reach a log line,
/// an error message, or the status page.
fn redacted(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();
    format!("{}://{host}{port}{}", url.scheme(), url.path())
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "https" => Some(443),
        "http" => Some(80),
        _ => None,
    }
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => IpAddr::V4(address).is_loopback(),
        Some(Host::Ipv6(address)) => IpAddr::V6(address).is_loopback(),
        None => false,
    }
}
