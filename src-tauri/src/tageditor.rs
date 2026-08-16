//! Tag editing and AcoustID-based tag suggestion.
//!
//! Two independent capabilities: writing user-edited tag fields back to a
//! file on disk (`write_tags`), and suggesting tags for an unidentified file
//! by fingerprinting it and querying the AcoustID lookup service
//! (`generate_fingerprint` + `lookup_acoustid`, chained by the command layer
//! in `commands/tageditor.rs`). Fingerprinting shells out to the external
//! `fpcalc` (Chromaprint) binary rather than reimplementing the algorithm.

use anyhow::{anyhow, Context, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Deserialize)]
struct FpCalcOutput {
    duration: f64,
    fingerprint: String,
}

/// Tag fields proposed by an AcoustID match, for the caller to review and
/// selectively apply — a `None` field means AcoustID didn't return a value
/// for it, not that the value should be cleared.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SuggestedTags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<u32>,
}

#[derive(Deserialize)]
struct AcoustIdArtist {
    name: String,
}

#[derive(Deserialize)]
struct AcoustIdReleaseGroup {
    title: Option<String>,
}

#[derive(Deserialize)]
struct AcoustIdRelease {
    title: Option<String>,
    date: Option<AcoustIdDate>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum AcoustIdDate {
    Year(u32),
    Full(String),
}

#[derive(Deserialize)]
struct AcoustIdRecording {
    title: Option<String>,
    artists: Option<Vec<AcoustIdArtist>>,
    #[serde(rename = "releasegroups")]
    release_groups: Option<Vec<AcoustIdReleaseGroup>>,
    releases: Option<Vec<AcoustIdRelease>>,
}

#[derive(Deserialize)]
struct AcoustIdResult {
    score: f64,
    recordings: Option<Vec<AcoustIdRecording>>,
}

#[derive(Deserialize)]
struct AcoustIdResponse {
    status: String,
    results: Option<Vec<AcoustIdResult>>,
}

// ---------------------------------------------------------------------------
// Tag Editor File Writer
// ---------------------------------------------------------------------------

/// Write the given tag fields to `path`'s primary tag, creating one if the
/// file has none yet. Every field is written unconditionally — `Option`
/// fields (`track`, `disc`, `year`, `bpm`) are cleared from the file when
/// `None` rather than left untouched, so callers must pass the full desired
/// state, not a partial patch. Preserves any existing embedded cover art,
/// which lofty's tag-mutation calls would otherwise silently drop (#106).
/// Retries the on-disk save up to 5 times with a short delay, since it can
/// transiently fail if another process (e.g. an antivirus scanner) has the
/// file open.
#[allow(clippy::too_many_arguments)]
pub fn write_tags(
    path: &Path,
    title: &str,
    artist: &str,
    album: &str,
    album_artist: &str,
    composer: &str,
    genre: &str,
    track: Option<u32>,
    disc: Option<u32>,
    year: Option<u32>,
    grouping: &str,
    bpm: Option<f32>,
    initial_key: &str,
    compilation: bool,
) -> Result<()> {
    let mut tagged_file = Probe::open(path)
        .context("failed to open audio file for tag writing")?
        .read()
        .context("failed to parse audio tags")?;

    let tag = match tagged_file.primary_tag_mut() {
        Some(t) => t,
        None => {
            let tag_type = tagged_file.primary_tag_type();
            tagged_file.insert_tag(Tag::new(tag_type));
            tagged_file
                .primary_tag_mut()
                .ok_or_else(|| anyhow!("could not create a tag frame"))?
        }
    };

    // Guard against embedded cover art being dropped by the text-field edits
    // below (#106) — nothing here is meant to touch pictures, so restore
    // them if they come back empty after editing.
    let original_pictures = tag.pictures().to_vec();

    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album.to_string());
    tag.set_genre(genre.to_string());

    tag.insert_text(lofty::tag::ItemKey::AlbumArtist, album_artist.to_string());
    tag.insert_text(lofty::tag::ItemKey::Composer, composer.to_string());
    tag.insert_text(lofty::tag::ItemKey::ContentGroup, grouping.to_string());
    tag.insert_text(lofty::tag::ItemKey::InitialKey, initial_key.to_string());
    // TCMP/cpil/COMPILATION "part of a compilation" flag — written as text
    // "1"/"0" the same way lofty reads it back (see read_tags).
    tag.insert_text(
        lofty::tag::ItemKey::FlagCompilation,
        if compilation { "1" } else { "0" }.to_string(),
    );

    if let Some(b) = bpm {
        // ID3v2 (TBPM) and MP4 (tmpo) map from IntegerBpm; Vorbis/APE's freeform
        // "BPM" field maps from Bpm — write both so every format picks up its own.
        let bpm_str = if b.fract() == 0.0 {
            (b as i64).to_string()
        } else {
            b.to_string()
        };
        tag.insert_text(lofty::tag::ItemKey::IntegerBpm, bpm_str.clone());
        tag.insert_text(lofty::tag::ItemKey::Bpm, bpm_str);
    } else {
        tag.remove_key(lofty::tag::ItemKey::IntegerBpm);
        tag.remove_key(lofty::tag::ItemKey::Bpm);
    }

    if let Some(t) = track {
        tag.set_track(t);
    } else {
        tag.remove_key(lofty::tag::ItemKey::TrackNumber);
    }

    if let Some(d) = disc {
        tag.set_disk(d);
    } else {
        tag.remove_key(lofty::tag::ItemKey::DiscNumber);
    }

    if let Some(y) = year {
        // set_date() writes ItemKey::RecordingDate (ID3v2 TDRC, MP4, Vorbis
        // DATE) — writing ItemKey::Year directly has no ID3v2 mapping and
        // silently fails to produce a frame most players/taggers read (#428).
        tag.set_date(lofty::tag::items::Timestamp {
            year: y as u16,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
    } else {
        tag.remove_date();
        tag.remove_key(lofty::tag::ItemKey::Year);
    }

    if tag.pictures().is_empty() && !original_pictures.is_empty() {
        for picture in original_pictures {
            tag.push_picture(picture);
        }
    }

    let mut attempts = 0;
    loop {
        match tagged_file.save_to_path(path, WriteOptions::default()) {
            Ok(_) => break,
            Err(e) => {
                attempts += 1;
                if attempts >= 5 {
                    return Err(e)
                        .context("failed to write tags back to file after multiple attempts");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

/// Remove every embedded cover art picture from `path`'s primary tag,
/// leaving all other tag fields untouched (#386). A no-op if the file has no
/// tag, or its tag has no pictures.
pub fn clear_embedded_art(path: &Path) -> Result<()> {
    let mut tagged_file = Probe::open(path)
        .context("failed to open audio file for cover art clearing")?
        .read()
        .context("failed to parse audio tags")?;

    let tag = match tagged_file.primary_tag_mut() {
        Some(t) => t,
        None => return Ok(()),
    };

    if tag.pictures().is_empty() {
        return Ok(());
    }

    while !tag.pictures().is_empty() {
        tag.remove_picture(0);
    }

    let mut attempts = 0;
    loop {
        match tagged_file.save_to_path(path, WriteOptions::default()) {
            Ok(_) => break,
            Err(e) => {
                attempts += 1;
                if attempts >= 5 {
                    return Err(e)
                        .context("failed to write tags back to file after multiple attempts");
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// AcoustID Fingerprinting Engine
// ---------------------------------------------------------------------------

/// Resolves to the `FPCALC_PATH` env var when set (for dev/packaging
/// environments where `fpcalc` isn't on `PATH`), otherwise falls back to
/// letting the OS resolve the bare `fpcalc` name.
fn get_fpcalc_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("FPCALC_PATH") {
        if !env_path.trim().is_empty() {
            return PathBuf::from(env_path);
        }
    }
    PathBuf::from("fpcalc")
}

/// Run the external `fpcalc` (Chromaprint) binary on `path` and return its
/// `(fingerprint, duration_seconds)`. Fails if `fpcalc` isn't installed/on
/// `PATH`, isn't executable, or exits non-zero.
pub fn generate_fingerprint(path: &Path) -> Result<(String, u32)> {
    let fpcalc_bin = get_fpcalc_path();
    eprintln!(
        "[Luminous Backend] AcoustID: Running fpcalc binary '{:?}' on file '{:?}'",
        fpcalc_bin, path
    );

    let mut cmd = Command::new(&fpcalc_bin);
    cmd.arg("-json").arg(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let output = cmd
        .output()
        .context("failed to execute fpcalc binary. Is libchromaprint-tools installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("[Luminous Backend] AcoustID: fpcalc failed: {}", stderr);
        return Err(anyhow!("fpcalc failed: {}", stderr));
    }

    let res: FpCalcOutput =
        serde_json::from_slice(&output.stdout).context("failed to parse fpcalc JSON output")?;
    eprintln!(
        "[Luminous Backend] AcoustID: fpcalc completed successfully. Duration: {}s",
        res.duration.round() as u32
    );
    Ok((res.fingerprint, res.duration.round() as u32))
}

/// Look up a fingerprint (from `generate_fingerprint`) against the AcoustID
/// API and return the best-scoring match's tags. `api_key` takes precedence
/// over the `ACOUSTID_API_KEY` env var; if neither is set, fails with the
/// literal error text `"NO_API_KEY"` — the frontend (`TagEditor.svelte`)
/// pattern-matches on that string to show a "configure your API key"
/// prompt instead of a generic error, so it must not be reworded without
/// updating that check.
pub async fn lookup_acoustid(
    fingerprint: &str,
    duration_sec: u32,
    api_key: Option<String>,
) -> Result<SuggestedTags> {
    let client_key = match api_key.or_else(|| std::env::var("ACOUSTID_API_KEY").ok()) {
        Some(k) if !k.trim().is_empty() => k,
        _ => return Err(anyhow!("NO_API_KEY")),
    };

    let masked_key = if client_key.len() > 4 {
        format!(
            "{}***{}",
            &client_key[..2],
            &client_key[client_key.len() - 2..]
        )
    } else {
        "***".to_string()
    };

    eprintln!(
        "[Luminous Backend] AcoustID: Querying API lookup service via POST (client key: {}, duration: {}s)...",
        masked_key,
        duration_sec
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent(concat!("LuminousMusicPlayer/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let params = [
        ("client", client_key),
        ("meta", "recordings releasegroups releases".to_string()),
        ("duration", duration_sec.to_string()),
        ("fingerprint", fingerprint.to_string()),
    ];

    let response = client
        .post("https://api.acoustid.org/v2/lookup")
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!(
            "[Luminous Backend] AcoustID: API request failed with status: {}. Body: {}",
            status, body
        );
        return Err(anyhow!(
            "AcoustID API request failed: {}. Details: {}",
            status,
            body
        ));
    }

    let resp: AcoustIdResponse = response.json().await?;
    if resp.status != "ok" {
        eprintln!(
            "[Luminous Backend] AcoustID: Service returned non-ok status: {}",
            resp.status
        );
        return Err(anyhow!("AcoustID service status error"));
    }

    let results = resp.results.unwrap_or_default();
    let best_result = results
        .iter()
        .filter(|r| r.recordings.is_some() && !r.recordings.as_ref().unwrap().is_empty())
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    if let Some(r) = best_result {
        if let Some(recordings) = &r.recordings {
            if let Some(rec) = recordings.first() {
                let title = rec.title.clone();
                let artist = rec
                    .artists
                    .as_ref()
                    .and_then(|artists| artists.first().map(|a| a.name.clone()));
                let album = rec
                    .release_groups
                    .as_ref()
                    .and_then(|rgs| rgs.first().and_then(|rg| rg.title.clone()))
                    .or_else(|| {
                        rec.releases
                            .as_ref()
                            .and_then(|rels| rels.first().and_then(|rel| rel.title.clone()))
                    });

                let year = rec.releases.as_ref().and_then(|rels| {
                    rels.iter().find_map(|rel| {
                        rel.date.as_ref().and_then(|d| match d {
                            AcoustIdDate::Year(y) => Some(*y),
                            AcoustIdDate::Full(s) => s
                                .split('-')
                                .next()
                                .and_then(|part| part.parse::<u32>().ok()),
                        })
                    })
                });

                let suggested = SuggestedTags {
                    title,
                    artist,
                    album,
                    year,
                };
                eprintln!(
                    "[Luminous Backend] AcoustID: Successfully matched track. Suggestions: {:?}",
                    suggested
                );
                return Ok(suggested);
            }
        }
    }

    eprintln!("[Luminous Backend] AcoustID: No matching recording found in AcoustID database");
    Err(anyhow!("No matching audio recordings found on AcoustID"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lofty::picture::{MimeType, Picture, PictureType};

    /// Writes a minimal valid WAV file, so tests don't need a binary audio
    /// fixture checked into the repo.
    fn write_test_wav(path: &Path) {
        let sample_rate = 8_000u32;
        let channels = 1u16;
        let bits_per_sample = 16u16;
        let data = vec![0u8; (sample_rate / 10) as usize * 2]; // 100ms of silence

        let byte_rate = sample_rate * channels as u32 * (bits_per_sample / 8) as u32;
        let block_align = channels * (bits_per_sample / 8);

        let mut wav = Vec::with_capacity(44 + data.len());
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
        wav.extend_from_slice(&channels.to_le_bytes());
        wav.extend_from_slice(&sample_rate.to_le_bytes());
        wav.extend_from_slice(&byte_rate.to_le_bytes());
        wav.extend_from_slice(&block_align.to_le_bytes());
        wav.extend_from_slice(&bits_per_sample.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(data.len() as u32).to_le_bytes());
        wav.extend_from_slice(&data);

        std::fs::write(path, wav).expect("failed to write test wav fixture");
    }

    #[test]
    fn test_clear_embedded_art_removes_picture_and_keeps_other_tags() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        // Write a tag with a title and an embedded picture.
        let mut tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag_type = tagged_file.primary_tag_type();
        let mut tag = Tag::new(tag_type);
        tag.set_title("Original Title".to_string());
        let picture = Picture::unchecked(vec![0xFF, 0xD8, 0xFF, 0xE0])
            .pic_type(PictureType::CoverFront)
            .mime_type(MimeType::Jpeg)
            .build();
        tag.push_picture(picture);
        tagged_file.insert_tag(tag);
        tagged_file
            .save_to_path(&path, WriteOptions::default())
            .expect("failed to write fixture tag");

        // Sanity check: the picture round-tripped.
        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        assert_eq!(tagged_file.primary_tag().unwrap().pictures().len(), 1);
        drop(tagged_file);

        clear_embedded_art(&path).expect("clear_embedded_art should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert!(
            tag.pictures().is_empty(),
            "embedded picture should be removed"
        );
        assert_eq!(
            tag.title().as_deref(),
            Some("Original Title"),
            "clearing artwork must not touch other tag fields"
        );
    }

    #[test]
    fn test_clear_embedded_art_is_noop_without_a_tag() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("untagged.wav");
        write_test_wav(&path);

        clear_embedded_art(&path).expect("clearing an untagged file should not error");
    }

    #[tokio::test]
    #[ignore]
    async fn test_debug_lookup() {
        let db_path = "C:\\Users\\ericj\\AppData\\Roaming\\org.luminous.app\\luminous.db";
        println!("Checking database at {}", db_path);
        let conn = rusqlite::Connection::open(db_path).unwrap();
        let path_str: String = conn
            .query_row(
                "SELECT path FROM songs WHERE id = ?1",
                rusqlite::params![1336],
                |row| row.get(0),
            )
            .expect("Could not find song 1336 in database");
        println!("Found path: {}", path_str);

        let path = std::path::PathBuf::from(path_str);
        let (fingerprint, duration_sec) = generate_fingerprint(&path).unwrap();
        println!(
            "Generated fingerprint length: {}, duration: {}s",
            fingerprint.len(),
            duration_sec
        );

        let suggestions = lookup_acoustid(&fingerprint, duration_sec, None).await;
        match suggestions {
            Ok(s) => println!("Success! Suggestions: {:?}", s),
            Err(e) => println!("Error during AcoustID lookup: {:?}", e),
        }
    }
}
