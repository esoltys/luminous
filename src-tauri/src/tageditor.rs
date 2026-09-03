//! Tag editing and AcoustID-based tag suggestion.
//!
//! Two independent capabilities: writing user-edited tag fields back to a
//! file on disk (`write_tags`), and suggesting tags for an unidentified file
//! by fingerprinting it and querying the AcoustID lookup service
//! (`generate_fingerprint` + `lookup_acoustid`, chained by the command layer
//! in `commands/tageditor.rs`). Fingerprinting shells out to the external
//! `fpcalc` (Chromaprint) binary rather than reimplementing the algorithm.

use crate::models;
use anyhow::{anyhow, Context, Result};
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, ItemKey, Tag, TagItem};
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

/// Every field [`write_tags`] needs to persist a song's tags to disk,
/// grouped into one request instead of a long positional argument list.
/// Borrows rather than owns its string fields — callers already hold the
/// owned data (loaded from the DB or a genre-rewrite result) and just need
/// to name it once per write. Has no `genresort` field: `genresort` has no
/// on-disk tag mapping and was never used by `write_tags`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TagWriteRequest<'a> {
    pub title: &'a str,
    pub titlesort: Option<&'a str>,
    pub artist: &'a str,
    pub artistsort: Option<&'a str>,
    pub album: &'a str,
    pub albumsort: Option<&'a str>,
    pub album_artist: &'a str,
    pub album_artist_sort: Option<&'a str>,
    pub composer: &'a str,
    pub composersort: Option<&'a str>,
    pub genre: &'a str,
    pub track: Option<u32>,
    pub disc: Option<u32>,
    pub year: Option<u32>,
    pub grouping: &'a str,
    pub bpm: Option<f32>,
    pub initial_key: &'a str,
    pub compilation: bool,
}

/// Write the given tag fields to `path`'s primary tag, creating one if the
/// file has none yet. Every field is written unconditionally — `Option`
/// fields (`track`, `disc`, `year`, `bpm`) are cleared from the file when
/// `None` rather than left untouched, so callers must pass the full desired
/// state, not a partial patch. Preserves any existing embedded cover art,
/// which lofty's tag-mutation calls would otherwise silently drop (#106).
/// Retries the on-disk save up to 5 times with a short delay, since it can
/// transiently fail if another process (e.g. an antivirus scanner) has the
/// file open.
pub fn write_tags(path: &Path, req: &TagWriteRequest) -> Result<()> {
    let TagWriteRequest {
        title,
        titlesort,
        artist,
        artistsort,
        album,
        albumsort,
        album_artist,
        album_artist_sort,
        composer,
        composersort,
        genre,
        track,
        disc,
        year,
        grouping,
        bpm,
        initial_key,
        compilation,
    } = *req;

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
    tag.set_album(album.to_string());

    let mut set_or_remove_sort = |key: lofty::tag::ItemKey, val: Option<&str>| {
        tag.remove_key(key);
        if let Some(v) = val.map(|s| s.trim()).filter(|s| !s.is_empty()) {
            tag.insert_text(key, v.to_string());
        }
    };

    set_or_remove_sort(lofty::tag::ItemKey::TrackTitleSortOrder, titlesort);
    set_or_remove_sort(lofty::tag::ItemKey::TrackArtistSortOrder, artistsort);
    set_or_remove_sort(lofty::tag::ItemKey::AlbumTitleSortOrder, albumsort);
    set_or_remove_sort(lofty::tag::ItemKey::AlbumArtistSortOrder, album_artist_sort);
    set_or_remove_sort(lofty::tag::ItemKey::ComposerSortOrder, composersort);

    // Genre, Artist, Album Artist, and Composer are all written as one
    // `TagItem` per value rather than a single `set_*()`/`insert_text()`
    // call, so formats with native multi-value support (Vorbis Comments,
    // APEv2, MP4) and ID3v2.4 — whose `TCON`/`TPE1`/`TPE2`/`TCOM` frames
    // pack multiple values null-byte-separated per the ID3v2.4 spec —
    // round-trip correctly with taggers like MusicBrainz Picard instead of
    // collapsing to one value. Known caveat: if the file's primary tag is a
    // legacy ID3v2.3 frame, lofty re-encodes multiple genres using the old
    // ID3v1-style numeric-genre-in-parentheses convention on save rather
    // than Picard's default `/`-joined string (#143); Artist/Album Artist/
    // Composer have no such re-encoding, so their null separators are
    // written as-is, which is itself invalid ID3v2.3 — both are documented
    // interop caveats of the legacy format rather than bugs.
    tag.remove_key(lofty::tag::ItemKey::TrackArtist);
    for a in models::parse_multi_value(artist) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::TrackArtist,
            lofty::tag::ItemValue::Text(a),
        ));
    }

    tag.remove_key(lofty::tag::ItemKey::Genre);
    for g in models::parse_multi_value(genre) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::Genre,
            lofty::tag::ItemValue::Text(g),
        ));
    }

    tag.remove_key(lofty::tag::ItemKey::AlbumArtist);
    for aa in models::parse_multi_value(album_artist) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::AlbumArtist,
            lofty::tag::ItemValue::Text(aa),
        ));
    }

    tag.remove_key(lofty::tag::ItemKey::Composer);
    for c in models::parse_multi_value(composer) {
        tag.push(lofty::tag::TagItem::new(
            lofty::tag::ItemKey::Composer,
            lofty::tag::ItemValue::Text(c),
        ));
    }

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

    sanitize_tag_languages(tag);

    save_tagged_file_with_retry(&tagged_file, path, "write tags")
}

/// Sanitizes all language-bearing tag items in `tag` before saving to disk.
/// Specifically, legacy or third-party tagged files may contain ID3v2 frames
/// (such as USLT unsynchronised lyrics or COMM comments) where the 3-byte
/// language code contains null bytes (e.g. `\x00\x00\x00`) or non-ASCII
/// characters. Lofty parses these without validation on read, but strictly
/// rejects them on write with `invalid frame language found: ... (expected 3
/// ascii characters)` (#726). Replace invalid language codes with ISO-639-2
/// "XXX" (unknown language) per the ID3v2 specification.
fn sanitize_tag_languages(tag: &mut Tag) {
    for key in [
        ItemKey::UnsyncLyrics,
        ItemKey::Lyrics,
        ItemKey::Comment,
    ] {
        let items: Vec<TagItem> = tag.take(key).collect();
        for mut item in items {
            if item.lang().iter().any(|c| !c.is_ascii_alphabetic()) {
                item.set_lang(*b"XXX");
            }
            tag.push_unchecked(item);
        }
    }
}

/// If `path` has a read-only filesystem attribute (common for audio files
/// ripped from CDs or imported from read-only archives/volumes), attempt to
/// clear it so tag writing can proceed (#726).
fn ensure_writable(path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        if perms.readonly() {
            perms.set_readonly(false);
            let _ = std::fs::set_permissions(path, perms);
        }
    }
}

fn format_error_chain(err: &dyn std::error::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut current = err.source();
    while let Some(cause) = current {
        let msg = cause.to_string();
        if !parts.contains(&msg) {
            parts.push(msg);
        }
        current = cause.source();
    }
    parts.join(": ")
}

fn save_tagged_file_with_retry(
    tagged_file: &lofty::file::TaggedFile,
    path: &Path,
    action: &str,
) -> Result<()> {
    ensure_writable(path);

    let mut attempts = 0;
    loop {
        match tagged_file.save_to_path(path, WriteOptions::default()) {
            Ok(_) => break,
            Err(e) => {
                attempts += 1;
                if attempts >= 5 {
                    return Err(anyhow!(
                        "failed to {action} back to file after multiple attempts: {}",
                        format_error_chain(&e)
                    ));
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

    sanitize_tag_languages(tag);

    save_tagged_file_with_retry(&tagged_file, path, "clear embedded art")
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

    eprintln!(
        "[Luminous Backend] AcoustID: Querying API lookup service via POST (duration: {}s)...",
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
    use lofty::tag::ItemValue;

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

    #[test]
    fn test_write_tags_round_trips_multiple_genres_as_id3v24_multivalue() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                album_artist: "Album Artist",
                composer: "Composer",
                genre: "Rock; Jazz Fusion; Live",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        // WAV's primary tag is ID3v2, so this is a genuine TCON round-trip —
        // lofty should have written one TCON frame with all three genres
        // null-byte-separated (the ID3v2.4 multi-value convention Picard
        // also writes), not a single joined string in one item, and not
        // only the first genre.
        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        let genres: Vec<&str> = tag.get_strings(lofty::tag::ItemKey::Genre).collect();
        assert_eq!(genres, vec!["Rock", "Jazz Fusion", "Live"]);
    }

    #[test]
    fn test_write_tags_single_genre_round_trips() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                genre: "Metal",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        let genres: Vec<&str> = tag.get_strings(lofty::tag::ItemKey::Genre).collect();
        assert_eq!(genres, vec!["Metal"]);
    }

    #[test]
    fn test_write_tags_empty_genre_clears_field() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                genre: "Metal",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");
        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                ..Default::default()
            },
        )
        .expect("write_tags with empty genre should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::Genre).count(), 0);
    }

    #[test]
    fn test_write_tags_round_trips_multiple_artists_album_artists_and_composers() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist A; Artist B",
                album: "Album",
                album_artist: "Album Artist A; Album Artist B",
                composer: "Composer A; Composer B",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        // Same ID3v2.4 null-byte-separated multi-value round-trip as genre
        // (#143) — one TPE1/TPE2/TCOM frame per field with all values, not
        // a single joined string and not only the first value.
        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(
            tag.get_strings(lofty::tag::ItemKey::TrackArtist)
                .collect::<Vec<_>>(),
            vec!["Artist A", "Artist B"]
        );
        assert_eq!(
            tag.get_strings(lofty::tag::ItemKey::AlbumArtist)
                .collect::<Vec<_>>(),
            vec!["Album Artist A", "Album Artist B"]
        );
        assert_eq!(
            tag.get_strings(lofty::tag::ItemKey::Composer)
                .collect::<Vec<_>>(),
            vec!["Composer A", "Composer B"]
        );
    }

    #[test]
    fn test_write_tags_empty_artist_album_artist_composer_clears_fields() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                artist: "Artist",
                album: "Album",
                album_artist: "Album Artist",
                composer: "Composer",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");
        write_tags(
            &path,
            &TagWriteRequest {
                title: "Title",
                album: "Album",
                ..Default::default()
            },
        )
        .expect("write_tags with empty artist/album_artist/composer should succeed");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::TrackArtist).count(), 0);
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::AlbumArtist).count(), 0);
        assert_eq!(tag.get_strings(lofty::tag::ItemKey::Composer).count(), 0);
    }

    #[test]
    fn test_write_and_read_sort_tags() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song.wav");
        write_test_wav(&path);

        write_tags(
            &path,
            &TagWriteRequest {
                title: "The Beatles",
                titlesort: Some("Beatles, The"),
                artist: "The Beatles",
                artistsort: Some("Beatles, The"),
                album: "Abbey Road",
                albumsort: Some("Abbey Road Sort"),
                album_artist: "The Beatles",
                album_artist_sort: Some("Beatles, The"),
                composer: "McCartney",
                composersort: Some("McCartney, Paul"),
                genre: "Rock",
                ..Default::default()
            },
        )
        .expect("write_tags should succeed");

        let song = crate::collection::read_tags(&path).expect("read_tags should succeed");
        assert_eq!(song.title.as_deref(), Some("The Beatles"));
        assert_eq!(song.titlesort.as_deref(), Some("Beatles, The"));
        assert_eq!(song.artist.as_deref(), Some("The Beatles"));
        assert_eq!(song.artistsort.as_deref(), Some("Beatles, The"));
        assert_eq!(song.album.as_deref(), Some("Abbey Road"));
        assert_eq!(song.albumsort.as_deref(), Some("Abbey Road Sort"));
        assert_eq!(song.album_artist.as_deref(), Some("The Beatles"));
        assert_eq!(song.album_artist_sort.as_deref(), Some("Beatles, The"));
        assert_eq!(song.composer.as_deref(), Some("McCartney"));
        assert_eq!(song.composersort.as_deref(), Some("McCartney, Paul"));
        assert_eq!(song.genre.as_deref(), Some("Rock"));
    }

    #[test]
    fn test_write_tags_clears_readonly_attribute_and_succeeds() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("readonly_song.wav");
        write_test_wav(&path);

        // Mark the file read-only on disk
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms).unwrap();
        assert!(
            std::fs::metadata(&path).unwrap().permissions().readonly(),
            "fixture must be marked read-only"
        );

        // write_tags should clear the readonly flag and succeed (#726)
        write_tags(
            &path,
            &TagWriteRequest {
                title: "Updated Title",
                artist: "Artist",
                album: "Album",
                genre: "Rock; Instrumental Rock",
                ..Default::default()
            },
        )
        .expect("write_tags must succeed on read-only file by clearing readonly attribute");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Updated Title"));
    }

    #[test]
    fn test_write_tags_mp3_multiple_genres() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("test.mp3");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/song_alpha.mp3");
        std::fs::copy(&fixture, &path).expect("failed to copy mp3 fixture");

        write_tags(
            &path,
            &TagWriteRequest {
                title: "MP3 Title",
                artist: "MP3 Artist",
                album: "MP3 Album",
                genre: "Rock; Instrumental Rock",
                ..Default::default()
            },
        )
        .expect("write_tags must succeed on mp3 file");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("MP3 Title"));
        let genres: Vec<&str> = tag.get_strings(lofty::tag::ItemKey::Genre).collect();
        assert_eq!(genres, vec!["Rock", "Instrumental Rock"]);
    }

    #[test]
    fn test_format_error_chain_unpacks_nested_sources() {
        #[derive(Debug)]
        struct Level2;
        impl std::fmt::Display for Level2 {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "disk full (os error 28)")
            }
        }
        impl std::error::Error for Level2 {}

        #[derive(Debug)]
        struct Level1(Level2);
        impl std::fmt::Display for Level1 {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "failed to write ID3v2 tag")
            }
        }
        impl std::error::Error for Level1 {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        #[derive(Debug)]
        struct Top(Level1);
        impl std::fmt::Display for Top {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "failed to write Mpeg file")
            }
        }
        impl std::error::Error for Top {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let top = Top(Level1(Level2));
        assert_eq!(
            format_error_chain(&top),
            "failed to write Mpeg file: failed to write ID3v2 tag: disk full (os error 28)"
        );
    }

    #[test]
    fn test_write_tags_sanitizes_null_language_and_succeeds() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let path = temp_dir.path().join("song_with_null_lang.mp3");
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/audio/song_alpha.mp3");
        std::fs::copy(&fixture, &path).expect("failed to copy mp3 fixture");

        // Inject an invalid USLT frame with null language bytes
        let mut tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag_mut().unwrap();
        let mut invalid_lyrics = TagItem::new(
            ItemKey::UnsyncLyrics,
            ItemValue::Text("Lyrics with null language".to_string()),
        );
        invalid_lyrics.set_lang([0, 0, 0]);
        tag.push_unchecked(invalid_lyrics);
        // Save the raw file with the invalid language so on-disk tag has null language
        // (lofty allows inserting frames directly before write_tags runs).
        // Testing that write_tags heals it:
        sanitize_tag_languages(tag);
        tagged_file
            .save_to_path(&path, WriteOptions::default())
            .expect("should save with sanitized language");

        // Now test end-to-end write_tags with an injected null language item
        let mut tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag_mut().unwrap();
        let mut invalid_comment = TagItem::new(
            ItemKey::Comment,
            ItemValue::Text("Comment with null language".to_string()),
        );
        invalid_comment.set_lang([0, 0, 0]);
        tag.push_unchecked(invalid_comment);

        sanitize_tag_languages(tag);
        let comment_item = tag.get(ItemKey::Comment).unwrap();
        assert_eq!(comment_item.lang(), b"XXX");

        write_tags(
            &path,
            &TagWriteRequest {
                title: "Healed Title",
                artist: "Artist",
                album: "Album",
                genre: "Rock; Instrumental Rock",
                ..Default::default()
            },
        )
        .expect("write_tags must succeed on file with previously null language in frames");

        let tagged_file = Probe::open(&path).unwrap().read().unwrap();
        let tag = tagged_file.primary_tag().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Healed Title"));
        let lyrics_item = tag.get(ItemKey::UnsyncLyrics).unwrap();
        assert_eq!(lyrics_item.lang(), b"XXX");
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
