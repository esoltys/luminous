//! Audio file integrity verification & corrupt track detection (#1938, epic #1441).
//!
//! Evaluates whether an audio track has valid structural and container integrity,
//! decodable audio frames, and (for FLAC) valid stream checksums matching the
//! embedded 128-bit STREAMINFO MD5.
//!
//! Must never depend on Tauri, SQLite, reqwest, or Tokio (#1765, epic #1762).

use luminous_audio::OfflineDecoder;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Verdict from structural and stream verification of an audio track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityVerdict {
    /// Track decoded cleanly and verified against embedded stream checksums.
    Valid,
    /// Audio stream or container is corrupted, truncated, or failed checksum verification.
    Corrupt { reason: String },
    /// File cannot be verified (e.g. unsupported container or unreadable stream).
    Unverified,
}

impl IntegrityVerdict {
    /// True if the track was successfully verified and confirmed free of corruption.
    pub fn is_valid(&self) -> bool {
        matches!(self, IntegrityVerdict::Valid)
    }

    /// True if the track is corrupt or unreadable.
    pub fn is_corrupt(&self) -> bool {
        matches!(self, IntegrityVerdict::Corrupt { .. })
    }

    /// Optional failure detail if corrupt.
    pub fn error_reason(&self) -> Option<&str> {
        match self {
            IntegrityVerdict::Corrupt { reason } => Some(reason.as_str()),
            _ => None,
        }
    }
}

/// Verifies container structure, frame decodability, and embedded checksums for an audio file.
///
/// Encapsulates format probing, stream packet decoding, and MD5 streaminfo verification.
pub fn verify_audio_file(path: &Path) -> IntegrityVerdict {
    // 1. Inspect physical file existence and non-zero size
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) => {
            return IntegrityVerdict::Corrupt {
                reason: format!("File inaccessible or unreadable: {e}"),
            };
        }
    };

    if meta.len() == 0 {
        return IntegrityVerdict::Corrupt {
            reason: "Empty audio file (0 bytes)".to_string(),
        };
    }

    // 2. Open decoder with verification enabled
    let decoder = match OfflineDecoder::open_file_verified(path) {
        Ok(d) => d,
        Err(e) => {
            return IntegrityVerdict::Corrupt {
                reason: format!("Container or format probe failed: {e}"),
            };
        }
    };

    // 3. Verify stream packets and frame decodability
    match decoder.verify_integrity() {
        Ok(()) => IntegrityVerdict::Valid,
        Err(reason) => IntegrityVerdict::Corrupt { reason },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn verify_healthy_flac() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio/song_gamma.flac");
        let verdict = verify_audio_file(&path);
        assert_eq!(verdict, IntegrityVerdict::Valid);
    }

    #[test]
    fn verify_healthy_mp3() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio/song_alpha.mp3");
        let verdict = verify_audio_file(&path);
        assert_eq!(verdict, IntegrityVerdict::Valid);
    }

    #[test]
    fn verify_healthy_wav() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio/song_beta.wav");
        let verdict = verify_audio_file(&path);
        assert_eq!(verdict, IntegrityVerdict::Valid);
    }

    #[test]
    fn verify_healthy_ogg() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio/song_delta.ogg");
        let verdict = verify_audio_file(&path);
        assert_eq!(verdict, IntegrityVerdict::Valid);
    }

    #[test]
    fn verify_zero_byte_file() {
        let temp = NamedTempFile::new().expect("create temp");
        let verdict = verify_audio_file(temp.path());
        assert!(verdict.is_corrupt());
        assert_eq!(verdict.error_reason(), Some("Empty audio file (0 bytes)"));
    }

    #[test]
    fn verify_truncated_flac() {
        let orig_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio/song_gamma.flac");
        let bytes = std::fs::read(&orig_path).expect("read flac fixture");

        // Keep first 1/4 of file bytes to simulate truncated download
        let mut truncated = NamedTempFile::new().expect("create temp file");
        truncated
            .write_all(&bytes[..bytes.len() / 4])
            .expect("write truncated flac");

        let verdict = verify_audio_file(truncated.path());
        assert!(verdict.is_corrupt());
        let reason = verdict.error_reason().expect("has error reason");
        assert!(
            reason.contains("Truncated") || reason.contains("Corrupt") || reason.contains("EOF"),
            "unexpected reason: {reason}"
        );
    }

    #[test]
    fn verify_corrupted_garbage_file() {
        let mut temp = NamedTempFile::new().expect("create temp");
        temp.write_all(b"RIFF\x00\x00\x00\x00NOT_VALID_AUDIO_HEADER_DATA_123456789")
            .expect("write garbage");

        let verdict = verify_audio_file(temp.path());
        assert!(verdict.is_corrupt());
    }
}
