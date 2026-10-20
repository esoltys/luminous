//! MusicBrainz OAuth and Web Service wire-protocol client (#1767).
//!
//! Handles PKCE generation, OAuth authorization URL construction, code exchange,
//! token refresh, user profile retrieval, collections lookup, and token revocation.
//! Loopback listener, SQLite token storage, and Tauri events remain in `src-tauri`.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, USER_AGENT};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const DEFAULT_MUSICBRAINZ_CLIENT_ID: &str = "e68tnaXh59MOsC7KyGZkAEJ_5b6A8E0JJzRcPYZYsXw";
pub const DEFAULT_MUSICBRAINZ_CLIENT_SECRET: &str = "r66Debc7m_ZPFwNJ3tNRwmweXgw5DHTVetJJFLU-PrI";
pub const LOOPBACK_PORT: u16 = 12083;
pub const LOOPBACK_REDIRECT_URI: &str = "http://localhost:12083/oauth/callback";
pub const OOB_REDIRECT_URI: &str = "urn:ietf:wg:oauth:2.0:oob";

pub const MB_AUTH_URL: &str = "https://musicbrainz.org/oauth2/authorize";
pub const MB_TOKEN_URL: &str = "https://musicbrainz.org/oauth2/token";
pub const MB_REVOKE_URL: &str = "https://musicbrainz.org/oauth2/revoke";
pub const MB_USERINFO_URL: &str = "https://musicbrainz.org/oauth2/userinfo";
pub const MB_COLLECTION_URL: &str = "https://musicbrainz.org/ws/2/collection";

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MusicBrainzAuthState {
    pub is_logged_in: bool,
    pub username: Option<String>,
    pub email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct MusicBrainzUserStats {
    pub username: String,
    pub collections_count: u32,
    pub releases_count: u32,
    pub cached_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub access_token: String,
    pub token_type: Option<String>,
    pub expires_in: Option<i64>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub sub: Option<String>,
    pub email: Option<String>,
}

// ---------------------------------------------------------------------------
// Private wire-format models
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
struct TokenResponse {
    access_token: String,
    token_type: Option<String>,
    expires_in: Option<i64>,
    refresh_token: Option<String>,
}

#[derive(Deserialize, Debug)]
struct UserInfoResponse {
    sub: Option<String>,
    email: Option<String>,
}

#[derive(Deserialize, Debug)]
struct CollectionItem {
    #[serde(rename = "count")]
    count: Option<u32>,
}

#[derive(Deserialize, Debug)]
struct CollectionsResponse {
    #[serde(rename = "collection-count")]
    collection_count: Option<u32>,
    collections: Option<Vec<CollectionItem>>,
}

// ---------------------------------------------------------------------------
// PKCE Helpers
// ---------------------------------------------------------------------------

pub fn generate_pkce_verifier() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn generate_pkce_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

pub fn generate_state() -> String {
    let bytes: [u8; 16] = rand::rng().random();
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn build_authorization_url(
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    code_challenge: &str,
) -> Result<String, String> {
    let mut url = reqwest::Url::parse(MB_AUTH_URL).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("scope", "profile email tag rating collection")
        .append_pair("state", state)
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256");
    Ok(url.to_string())
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct MusicBrainzClient {
    client: Client,
}

impl Default for MusicBrainzClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicBrainzClient {
    pub fn new() -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION"))),
        );

        let client = Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        Self { client }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    async fn request_token(&self, form: &[(&str, String)]) -> Result<TokenData, String> {
        let resp = self
            .client
            .post(MB_TOKEN_URL)
            .form(form)
            .send()
            .await
            .map_err(|e| format!("Token request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Token request failed ({status}): {body}"));
        }

        let token_resp: TokenResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {e}"))?;

        Ok(TokenData {
            access_token: token_resp.access_token,
            token_type: token_resp.token_type,
            expires_in: token_resp.expires_in,
            refresh_token: token_resp.refresh_token,
        })
    }

    /// Exchange authorization code for access and refresh tokens.
    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Result<TokenData, String> {
        let mut form = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("client_id", client_id.to_string()),
            ("redirect_uri", redirect_uri.to_string()),
        ];

        if let Some(sec) = client_secret {
            form.push(("client_secret", sec.to_string()));
        }

        if let Some(verifier) = code_verifier {
            if !verifier.is_empty() {
                form.push(("code_verifier", verifier.to_string()));
            }
        }

        self.request_token(&form).await
    }

    /// Refresh access token using a refresh token.
    pub async fn refresh_access_token(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        refresh_token: &str,
    ) -> Result<TokenData, String> {
        let mut form = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.to_string()),
            ("client_id", client_id.to_string()),
        ];

        if let Some(sec) = client_secret {
            form.push(("client_secret", sec.to_string()));
        }

        self.request_token(&form).await
    }

    /// Fetch logged in user's profile info.
    pub async fn fetch_user_info(&self, access_token: &str) -> Result<UserInfo, String> {
        let resp = self
            .client
            .get(MB_USERINFO_URL)
            .header(AUTHORIZATION, format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| format!("Userinfo request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Userinfo failed ({status}): {body}"));
        }

        let info: UserInfoResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse userinfo: {e}"))?;

        Ok(UserInfo {
            sub: info.sub,
            email: info.email,
        })
    }

    /// Fetch collections statistics for `username`.
    /// Returns `(collections_count, releases_count)`.
    pub async fn fetch_collections(
        &self,
        username: &str,
        access_token: &str,
    ) -> Result<(u32, u32), String> {
        let mut url = reqwest::Url::parse(MB_COLLECTION_URL).map_err(|e| e.to_string())?;
        url.query_pairs_mut()
            .append_pair("editor", username)
            .append_pair("fmt", "json");

        let resp = self
            .client
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {access_token}"))
            .send()
            .await
            .map_err(|e| format!("Collections request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Collections query failed ({status}): {body}"));
        }

        let collections_data: CollectionsResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse collections JSON: {e}"))?;

        let collections_count = collections_data.collection_count.unwrap_or_else(|| {
            collections_data
                .collections
                .as_ref()
                .map(|c| c.len() as u32)
                .unwrap_or(0)
        });

        let releases_count = collections_data
            .collections
            .as_ref()
            .map(|items| items.iter().filter_map(|i| i.count).sum())
            .unwrap_or(0);

        Ok((collections_count, releases_count))
    }

    /// Revoke access or refresh token (best-effort).
    pub async fn revoke_token(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        token: &str,
    ) -> Result<(), String> {
        let mut form = vec![
            ("token", token.to_string()),
            ("client_id", client_id.to_string()),
        ];
        if let Some(sec) = client_secret {
            form.push(("client_secret", sec.to_string()));
        }

        let resp = self
            .client
            .post(MB_REVOKE_URL)
            .form(&form)
            .send()
            .await
            .map_err(|e| format!("Revocation request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Revocation failed ({status}): {body}"));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pkce_generation() {
        let verifier = generate_pkce_verifier();
        assert!(!verifier.is_empty());
        assert_eq!(verifier.len(), 43); // 32 bytes URL-safe unpadded base64 is 43 chars

        let challenge = generate_pkce_challenge(&verifier);
        assert!(!challenge.is_empty());
        assert_eq!(challenge.len(), 43);

        let challenge2 = generate_pkce_challenge(&verifier);
        assert_eq!(challenge, challenge2);

        let verifier2 = generate_pkce_verifier();
        assert_ne!(verifier, verifier2);
        assert_ne!(challenge, generate_pkce_challenge(&verifier2));
    }

    #[test]
    fn test_state_generation() {
        let state1 = generate_state();
        let state2 = generate_state();
        assert_eq!(state1.len(), 22); // 16 bytes URL-safe unpadded base64 is 22 chars
        assert_ne!(state1, state2);
    }

    #[test]
    fn test_build_authorization_url() {
        let url = build_authorization_url(
            "client_123",
            "http://localhost:12083/oauth/callback",
            "state_abc",
            "challenge_xyz",
        )
        .unwrap();

        assert!(url.starts_with("https://musicbrainz.org/oauth2/authorize?"));
        assert!(url.contains("client_id=client_123"));
        assert!(url.contains("state=state_abc"));
        assert!(url.contains("code_challenge=challenge_xyz"));
        assert!(url.contains("code_challenge_method=S256"));
    }

    #[test]
    fn test_auth_state_default() {
        let state = MusicBrainzAuthState::default();
        assert!(!state.is_logged_in);
        assert!(state.username.is_none());
        assert!(state.email.is_none());
    }

    #[test]
    fn test_user_stats_serialization() {
        let stats = MusicBrainzUserStats {
            username: "testuser".to_string(),
            collections_count: 5,
            releases_count: 42,
            cached_at: 123456789,
        };
        let json = serde_json::to_string(&stats).expect("serialize stats");
        let deserialized: MusicBrainzUserStats =
            serde_json::from_str(&json).expect("deserialize stats");
        assert_eq!(deserialized, stats);
    }

    #[test]
    fn test_deserialize_collections_response() {
        let json = r#"{"collection-count": 2, "collections": [{"count": 10}, {"count": 25}]}"#;
        let res: CollectionsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(res.collection_count, Some(2));
        let count_sum: u32 = res
            .collections
            .unwrap()
            .iter()
            .filter_map(|i| i.count)
            .sum();
        assert_eq!(count_sum, 35);
    }
}
