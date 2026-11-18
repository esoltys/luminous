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
    /// The file couldn't be reached (missing, permission denied, drive unavailable),
    /// so nothing is known about its contents.
    Unverified,
}

impl IntegrityVerdict {
    /// True if the track was successfully verified and confirmed free of corruption.
    pub fn is_valid(&self) -> bool {
        matches!(self, IntegrityVerdict::Valid)
    }

    /// True if the track was read and found corrupt.
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
/// Only a file that was read and failed to probe or decode is `Corrupt`. A file that
/// can't be reached (missing, permission denied, drive asleep or disconnected) is
/// `Unverified`, since failing to read it says nothing about its contents (#2162).
pub fn verify_audio_file(path: &Path) -> IntegrityVerdict {
    // 1. Reach the file: metadata and an open both work before anything is judged.
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return IntegrityVerdict::Unverified,
    };
    if std::fs::File::open(path).is_err() {
        return IntegrityVerdict::Unverified;
    }

    if meta.len() == 0 {
        return IntegrityVerdict::Corrupt {
            reason: "Empty audio file (0 bytes)".to_string(),
        };
    }

    // 2. Open the decoder with verification enabled, then 3. verify stream
    // packets and frame decodability.
    let failure = match OfflineDecoder::open_file_verified(path) {
        Ok(decoder) => match decoder.verify_integrity() {
            Ok(()) => return IntegrityVerdict::Valid,
            Err(reason) => reason,
        },
        Err(e) => format!("Container or format probe failed: {e}"),
    };

    // The decoder reports a read error the same way as bad data. If the file
    // is no longer reachable, the drive went away mid-read: not corruption.
    if std::fs::metadata(path).is_err() {
        return IntegrityVerdict::Unverified;
    }
    IntegrityVerdict::Corrupt { reason: failure }
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
    fn verify_missing_file_is_unverified_not_corrupt() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let verdict = verify_audio_file(&dir.path().join("gone.flac"));
        assert_eq!(verdict, IntegrityVerdict::Unverified);
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
