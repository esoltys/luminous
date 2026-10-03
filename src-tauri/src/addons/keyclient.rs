//! Client for the add-on key-release Worker (#1416).
//!
//! Flow: `POST /v1/ticket` returns an Entra service ticket; the Windows Store
//! layer (#1414) trades it for a Store ID key; `POST /v1/key` then returns the
//! add-on key that decrypts the bundle. The Worker is the only place
//! entitlement is decided; this module only speaks its wire contract.
//!
//! Neither the Store ID key nor the add-on key is ever logged, and neither
//! appears in an error's `Display` or `Debug` output.

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Where the key-release Worker is deployed.
pub const DEFAULT_KEY_BASE_URL: &str = "https://luminous-keys.esoltys.dev";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, PartialEq, Eq)]
pub enum KeyError {
    /// 403: the account does not own the add-on. Do not retry.
    NotEntitled,
    /// 404: the Worker has no key for this add-on id.
    UnknownAddin,
    /// 429: honour `retry_after_secs` before trying again.
    RateLimited { retry_after_secs: Option<u64> },
    /// 5xx or an unreachable Worker: retryable, and not evidence of non-ownership.
    Upstream,
    /// 400/405/413 or any other unexpected status: a bug on our side.
    BadRequest,
    /// The Worker answered 200 with something unusable.
    BadResponse,
}

impl KeyError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, KeyError::RateLimited { .. } | KeyError::Upstream)
    }
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyError::NotEntitled => write!(f, "this Microsoft account does not own the add-on"),
            KeyError::UnknownAddin => write!(f, "the add-on is not available from the key service"),
            KeyError::RateLimited { .. } => write!(f, "too many requests; try again later"),
            KeyError::Upstream => write!(f, "the key service is unavailable; try again later"),
            KeyError::BadRequest => write!(f, "the key service rejected the request"),
            KeyError::BadResponse => write!(f, "the key service sent an invalid response"),
        }
    }
}

impl std::error::Error for KeyError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TicketResponse {
    service_ticket: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyRequest<'a> {
    store_id_key: &'a str,
    addin_id: &'a str,
}

#[derive(Deserialize)]
struct KeyResponse {
    key: String,
}

/// Map a non-200 status to its error. Pure, so the contract is testable offline.
fn classify(status: u16, retry_after: Option<&str>) -> KeyError {
    match status {
        403 => KeyError::NotEntitled,
        404 => KeyError::UnknownAddin,
        429 => KeyError::RateLimited {
            retry_after_secs: retry_after.and_then(|v| v.trim().parse().ok()),
        },
        500..=599 => KeyError::Upstream,
        _ => KeyError::BadRequest,
    }
}

fn parse_key(hex: &str) -> Result<[u8; 32], KeyError> {
    let hex = hex.trim();
    if hex.len() != 64 || !hex.is_ascii() {
        return Err(KeyError::BadResponse);
    }
    let mut key = [0u8; 32];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| KeyError::BadResponse)?;
    }
    Ok(key)
}

fn client() -> Result<reqwest::Client, KeyError> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| KeyError::BadRequest)
}

async fn post<B: Serialize>(
    base_url: &str,
    path: &str,
    body: Option<&B>,
) -> Result<reqwest::Response, KeyError> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let mut request = client()?.post(url);
    if let Some(body) = body {
        request = request.json(body);
    }
    // A transport failure says nothing about ownership, so it is retryable.
    let response = request.send().await.map_err(|_| KeyError::Upstream)?;
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok());
    Err(classify(status.as_u16(), retry_after))
}

/// Get the Entra service ticket the Store needs to mint a Store ID key.
pub async fn fetch_service_ticket(base_url: &str) -> Result<String, KeyError> {
    let response = post::<()>(base_url, "/v1/ticket", None).await?;
    let ticket: TicketResponse = response.json().await.map_err(|_| KeyError::BadResponse)?;
    if ticket.service_ticket.is_empty() {
        return Err(KeyError::BadResponse);
    }
    Ok(ticket.service_ticket)
}

/// Trade a Store ID key for the add-on's decryption key.
pub async fn release_key(
    base_url: &str,
    store_id_key: &str,
    addin_id: &str,
) -> Result<[u8; 32], KeyError> {
    let body = KeyRequest {
        store_id_key,
        addin_id,
    };
    let response = post(base_url, "/v1/key", Some(&body)).await?;
    let key: KeyResponse = response.json().await.map_err(|_| KeyError::BadResponse)?;
    parse_key(&key.key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Serve one canned HTTP response and hand back the raw request it received.
    async fn serve_once(response: &'static str) -> (String, tokio::task::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = vec![0u8; 8192];
            let n = socket.read(&mut buf).await.unwrap();
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&buf[..n]).into_owned()
        });
        (base, handle)
    }

    const KEY_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

    #[test]
    fn classifies_documented_statuses() {
        assert_eq!(classify(403, None), KeyError::NotEntitled);
        assert_eq!(classify(404, None), KeyError::UnknownAddin);
        assert_eq!(classify(502, None), KeyError::Upstream);
        for status in [400, 405, 413, 418] {
            assert_eq!(classify(status, None), KeyError::BadRequest, "{status}");
        }
        assert_eq!(
            classify(429, Some("17")),
            KeyError::RateLimited {
                retry_after_secs: Some(17)
            }
        );
        assert_eq!(
            classify(429, Some("soon")),
            KeyError::RateLimited {
                retry_after_secs: None
            }
        );
    }

    #[test]
    fn only_transient_errors_retry() {
        assert!(KeyError::Upstream.is_retryable());
        assert!(KeyError::RateLimited {
            retry_after_secs: None
        }
        .is_retryable());
        assert!(!KeyError::NotEntitled.is_retryable());
        assert!(!KeyError::UnknownAddin.is_retryable());
    }

    #[test]
    fn parses_only_64_hex_chars() {
        let key = parse_key(KEY_HEX).unwrap();
        assert_eq!(key[0], 0);
        assert_eq!(key[31], 0x1f);
        assert_eq!(parse_key(&KEY_HEX.to_uppercase()).unwrap(), key);
        for bad in [
            "",
            "abcd",
            &KEY_HEX[..62],
            &format!("{KEY_HEX}00"),
            &"zz".repeat(32),
        ] {
            assert_eq!(parse_key(bad), Err(KeyError::BadResponse), "{bad}");
        }
    }

    #[tokio::test]
    async fn fetches_ticket() {
        let (base, handle) = serve_once(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 28\r\nconnection: close\r\n\r\n{\"serviceTicket\":\"ticket-1\"}",
        )
        .await;
        assert_eq!(fetch_service_ticket(&base).await.unwrap(), "ticket-1");
        assert!(handle.await.unwrap().starts_with("POST /v1/ticket "));
    }

    #[tokio::test]
    async fn releases_key_and_sends_camel_case_body() {
        let (base, handle) = serve_once(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 74\r\nconnection: close\r\n\r\n{\"key\":\"000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f\"}",
        )
        .await;
        let key = release_key(&base, "sik", "mothman").await.unwrap();
        assert_eq!(key[31], 0x1f);
        let request = handle.await.unwrap();
        assert!(request.starts_with("POST /v1/key "));
        assert!(request.contains(r#""storeIdKey":"sik""#));
        assert!(request.contains(r#""addinId":"mothman""#));
    }

    #[tokio::test]
    async fn maps_not_entitled_and_rate_limit_from_the_wire() {
        let (base, _) =
            serve_once("HTTP/1.1 403 Forbidden\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                .await;
        assert_eq!(
            release_key(&base, "k", "mothman").await,
            Err(KeyError::NotEntitled)
        );

        let (base, _) = serve_once(
            "HTTP/1.1 429 Too Many Requests\r\nretry-after: 30\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
        )
        .await;
        assert_eq!(
            release_key(&base, "k", "mothman").await,
            Err(KeyError::RateLimited {
                retry_after_secs: Some(30)
            })
        );
    }

    #[tokio::test]
    async fn unreachable_worker_is_retryable_not_unowned() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        assert_eq!(
            release_key(&base, "k", "mothman").await,
            Err(KeyError::Upstream)
        );
    }

    #[tokio::test]
    async fn malformed_key_response_is_rejected() {
        let (base, _) = serve_once(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 14\r\nconnection: close\r\n\r\n{\"key\":\"beef\"}",
        )
        .await;
        assert_eq!(
            release_key(&base, "k", "mothman").await,
            Err(KeyError::BadResponse)
        );
    }
}
