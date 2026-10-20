//! ListenBrainz and CritiqueBrainz HTTP wire-protocol clients (#83, #1386, #1767).
//!
//! Handles token validation, listen submission, feedback, and user ratings retrieval.
//! SQLite caching and offline queuing remain in `src-tauri`.

use reqwest::header::AUTHORIZATION;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const LISTENBRAINZ_API_BASE: &str = "https://api.listenbrainz.org/1";
pub const CRITIQUEBRAINZ_API_BASE: &str = "https://critiquebrainz.org/ws/1";
pub const SUBMISSION_CLIENT_NAME: &str = "Luminous";

/// Information required to submit a listen or now-playing entry.
#[derive(Debug, Clone, Default)]
pub struct ListenTrackInfo {
    pub artist: String,
    pub track: String,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub track_number: Option<i32>,
    pub recording_mbid: Option<String>,
    pub release_mbid: Option<String>,
    pub artist_mbids: Option<String>,
    pub release_group_mbid: Option<String>,
    pub track_mbid: Option<String>,
    pub listened_at: Option<i64>,
}

/// A published review rating from CritiqueBrainz.
#[derive(Debug, Clone, PartialEq)]
pub struct CritiqueRatingItem {
    pub entity_type: String,
    pub entity_mbid: String,
    pub stars: f32,
}

/// One-line description of a failed ListenBrainz response.
pub fn describe_error_response(status: reqwest::StatusCode, body: &str) -> String {
    #[derive(Deserialize)]
    struct ApiError {
        error: String,
    }
    match serde_json::from_str::<ApiError>(body) {
        Ok(api) => format!("HTTP {}: {}", status.as_u16(), api.error),
        Err(_) => format!("HTTP {status}"),
    }
}

// ---------------------------------------------------------------------------
// Private wire-format models
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct FeedbackPage {
    feedback: Vec<FeedbackItem>,
    total_count: usize,
}

#[derive(Deserialize)]
struct FeedbackItem {
    recording_mbid: Option<String>,
    score: i32,
}

#[derive(Deserialize)]
struct CritiquePage {
    count: usize,
    reviews: Vec<CritiqueReview>,
}

#[derive(Deserialize)]
struct CritiqueReview {
    entity_id: String,
    entity_type: String,
    rating: Option<u8>,
    #[serde(default)]
    is_draft: bool,
    #[serde(default)]
    is_hidden: bool,
}

#[derive(Deserialize)]
struct ValidateTokenResponse {
    valid: bool,
    user_name: Option<String>,
    message: Option<String>,
}

#[derive(Serialize)]
struct ListenBrainzSubmitRequest<'a> {
    listen_type: &'a str,
    payload: Vec<ListenBrainzPayload<'a>>,
}

#[derive(Serialize)]
struct ListenBrainzPayload<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    listened_at: Option<i64>,
    track_metadata: ListenBrainzTrackMetadata<'a>,
}

#[derive(Serialize)]
struct ListenBrainzTrackMetadata<'a> {
    artist_name: &'a str,
    track_name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_name: Option<&'a str>,
    additional_info: ListenBrainzAdditionalInfo<'a>,
}

#[derive(Serialize)]
struct ListenBrainzAdditionalInfo<'a> {
    submission_client: &'a str,
    submission_client_version: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tracknumber: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    recording_mbid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_mbid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artist_mbids: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_group_mbid: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    track_mbid: Option<&'a str>,
}

impl ListenTrackInfo {
    fn to_payload<'a>(&'a self, listened_at: Option<i64>) -> ListenBrainzPayload<'a> {
        let artist_mbids_vec: Option<Vec<&str>> = self.artist_mbids.as_deref().map(|s| {
            s.split(';')
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .collect()
        });

        ListenBrainzPayload {
            listened_at,
            track_metadata: ListenBrainzTrackMetadata {
                artist_name: &self.artist,
                track_name: &self.track,
                release_name: self.album.as_deref(),
                additional_info: ListenBrainzAdditionalInfo {
                    submission_client: SUBMISSION_CLIENT_NAME,
                    submission_client_version: env!("CARGO_PKG_VERSION"),
                    duration_ms: self.duration_ms,
                    tracknumber: self.track_number,
                    recording_mbid: self.recording_mbid.as_deref(),
                    release_mbid: self.release_mbid.as_deref(),
                    artist_mbids: artist_mbids_vec,
                    release_group_mbid: self.release_group_mbid.as_deref(),
                    track_mbid: self.track_mbid.as_deref(),
                },
            },
        }
    }
}

#[derive(Serialize)]
struct FeedbackRequest {
    recording_mbid: String,
    score: i32,
}

// ---------------------------------------------------------------------------
// Clients
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct ListenBrainzClient {
    client: Client,
}

impl Default for ListenBrainzClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ListenBrainzClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// Validate a ListenBrainz user token by hitting `/1/validate-token`.
    pub async fn validate_token(&self, token: &str) -> Result<String, String> {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return Err("Token cannot be empty".into());
        }

        let url = format!("{LISTENBRAINZ_API_BASE}/validate-token?token={trimmed}");
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Network request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("ListenBrainz returned HTTP {}", resp.status()));
        }

        let body: ValidateTokenResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {e}"))?;

        if !body.valid {
            let msg = body.message.unwrap_or_else(|| "Invalid token".into());
            return Err(msg);
        }

        body.user_name
            .ok_or_else(|| "Validated, but no username returned".to_string())
    }

    /// Submit a now-playing notification to ListenBrainz.
    pub async fn submit_now_playing(
        &self,
        token: &str,
        track: &ListenTrackInfo,
    ) -> Result<(), String> {
        let request = ListenBrainzSubmitRequest {
            listen_type: "playing_now",
            payload: vec![track.to_payload(None)],
        };

        let resp = self
            .client
            .post(format!("{LISTENBRAINZ_API_BASE}/submit-listens"))
            .header(AUTHORIZATION, format!("Token {token}"))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("ListenBrainz now-playing failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(describe_error_response(status, &body));
        }

        Ok(())
    }

    /// Submit listened tracks to ListenBrainz.
    pub async fn submit_listens(
        &self,
        token: &str,
        listens: &[ListenTrackInfo],
    ) -> Result<(), String> {
        if listens.is_empty() {
            return Ok(());
        }

        let payloads: Vec<ListenBrainzPayload> = listens
            .iter()
            .map(|e| e.to_payload(e.listened_at))
            .collect();

        let listen_type = if listens.len() == 1 {
            "single"
        } else {
            "import"
        };

        let request = ListenBrainzSubmitRequest {
            listen_type,
            payload: payloads,
        };

        let resp = self
            .client
            .post(format!("{LISTENBRAINZ_API_BASE}/submit-listens"))
            .header(AUTHORIZATION, format!("Token {token}"))
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Network request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(describe_error_response(status, &body));
        }

        Ok(())
    }

    /// Submit love/hate feedback for a recording to ListenBrainz.
    pub async fn submit_feedback(
        &self,
        token: &str,
        recording_mbid: &str,
        score: i32,
    ) -> Result<(), String> {
        let req = FeedbackRequest {
            recording_mbid: recording_mbid.to_string(),
            score,
        };

        let resp = self
            .client
            .post(format!(
                "{LISTENBRAINZ_API_BASE}/feedback/recording-feedback"
            ))
            .header(AUTHORIZATION, format!("Token {token}"))
            .json(&req)
            .send()
            .await
            .map_err(|e| format!("Feedback submission failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(describe_error_response(status, &body));
        }

        Ok(())
    }

    /// Every love (1) and hate (-1) the user has recorded on ListenBrainz,
    /// keyed by recording MBID.
    pub async fn fetch_user_feedback(
        &self,
        username: &str,
        token: &str,
    ) -> Result<std::collections::HashMap<String, i32>, String> {
        const PAGE: usize = 100;
        let mut feedback = std::collections::HashMap::new();
        let mut offset = 0usize;
        loop {
            let resp = self
                .client
                .get(format!(
                    "{LISTENBRAINZ_API_BASE}/feedback/user/{}/get-feedback?count={PAGE}&offset={offset}",
                    percent_encoding::utf8_percent_encode(
                        username,
                        percent_encoding::NON_ALPHANUMERIC
                    )
                ))
                .header(AUTHORIZATION, format!("Token {token}"))
                .send()
                .await
                .map_err(|e| format!("ListenBrainz feedback request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!(
                    "ListenBrainz feedback returned HTTP {}",
                    resp.status()
                ));
            }
            let page: FeedbackPage = resp
                .json()
                .await
                .map_err(|e| format!("Unreadable ListenBrainz feedback: {e}"))?;
            let received = page.feedback.len();
            for item in page.feedback {
                if let Some(mbid) = item.recording_mbid.filter(|m| !m.trim().is_empty()) {
                    feedback.insert(mbid.trim().to_string(), item.score);
                }
            }
            offset += received;
            if received == 0 || offset >= page.total_count {
                return Ok(feedback);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[derive(Clone)]
pub struct CritiqueBrainzClient {
    client: Client,
}

impl Default for CritiqueBrainzClient {
    fn default() -> Self {
        Self::new()
    }
}

impl CritiqueBrainzClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }

    /// The user's published CritiqueBrainz reviews that carry a star rating,
    /// for recordings and release groups.
    pub async fn fetch_user_ratings(
        &self,
        reviewer_uuid: &str,
    ) -> Result<Vec<CritiqueRatingItem>, String> {
        const PAGE: usize = 50;
        let mut ratings = Vec::new();
        let mut offset = 0usize;
        loop {
            let resp = self
                .client
                .get(format!(
                    "{CRITIQUEBRAINZ_API_BASE}/review/?user_id={reviewer_uuid}&limit={PAGE}&offset={offset}"
                ))
                .send()
                .await
                .map_err(|e| format!("CritiqueBrainz request failed: {e}"))?;
            if !resp.status().is_success() {
                return Err(format!("CritiqueBrainz returned HTTP {}", resp.status()));
            }
            let page: CritiquePage = resp
                .json()
                .await
                .map_err(|e| format!("Unreadable CritiqueBrainz reviews: {e}"))?;
            let received = page.reviews.len();
            for review in page.reviews {
                if review.is_draft || review.is_hidden {
                    continue;
                }
                if let Some(stars) = review.rating {
                    ratings.push(CritiqueRatingItem {
                        entity_type: review.entity_type,
                        entity_mbid: review.entity_id,
                        stars: stars as f32,
                    });
                }
            }
            offset += received;
            if received == 0 || offset >= page.count {
                return Ok(ratings);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_info_default() {
        let t = ListenTrackInfo::default();
        assert_eq!(t.artist, "");
        assert_eq!(t.track, "");
        assert!(t.album.is_none());
        assert!(t.duration_ms.is_none());
    }

    #[test]
    fn test_critique_rating_item() {
        let item = CritiqueRatingItem {
            entity_type: "recording".to_string(),
            entity_mbid: "123".to_string(),
            stars: 4.5,
        };
        assert_eq!(item.stars, 4.5);
    }

    #[test]
    fn test_describe_error_response_parses_json() {
        let msg = describe_error_response(
            reqwest::StatusCode::BAD_REQUEST,
            r#"{"error": "Invalid token"}"#,
        );
        assert_eq!(msg, "HTTP 400: Invalid token");
    }

    #[test]
    fn test_describe_error_response_handles_non_json() {
        let msg = describe_error_response(reqwest::StatusCode::BAD_GATEWAY, "Bad Gateway");
        assert_eq!(msg, "HTTP 502 Bad Gateway");
    }
}
