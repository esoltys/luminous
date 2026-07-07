use anyhow::{anyhow, Context, Result};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag};
use lofty::config::WriteOptions;
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
            tagged_file.primary_tag_mut().ok_or_else(|| anyhow!("could not create a tag frame"))?
        }
    };

    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album.to_string());
    tag.set_genre(genre.to_string());

    tag.insert_text(lofty::tag::ItemKey::AlbumArtist, album_artist.to_string());
    tag.insert_text(lofty::tag::ItemKey::Composer, composer.to_string());

    if let Some(t) = track {
        tag.set_track(t);
    } else {
        tag.remove_key(&lofty::tag::ItemKey::TrackNumber);
    }

    if let Some(d) = disc {
        tag.set_disk(d);
    } else {
        tag.remove_key(&lofty::tag::ItemKey::DiscNumber);
    }

    if let Some(y) = year {
        tag.set_year(y);
    } else {
        tag.remove_key(&lofty::tag::ItemKey::Year);
    }

    tagged_file.save_to_path(path, WriteOptions::default()).context("failed to write tags back to file")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// AcoustID Fingerprinting Engine
// ---------------------------------------------------------------------------

fn get_fpcalc_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("FPCALC_PATH") {
        if !env_path.trim().is_empty() {
            return PathBuf::from(env_path);
        }
    }
    PathBuf::from("fpcalc")
}

pub fn generate_fingerprint(path: &Path) -> Result<(String, u32)> {
    let fpcalc_bin = get_fpcalc_path();

    let output = Command::new(fpcalc_bin)
        .arg("-json")
        .arg(path)
        .output()
        .context("failed to execute fpcalc binary. Is libchromaprint-tools installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("fpcalc failed: {}", stderr));
    }

    let res: FpCalcOutput = serde_json::from_slice(&output.stdout)
        .context("failed to parse fpcalc JSON output")?;
    Ok((res.fingerprint, res.duration.round() as u32))
}

pub async fn lookup_acoustid(fingerprint: &str, duration_sec: u32) -> Result<SuggestedTags> {
    let client_key = std::env::var("ACOUSTID_API_KEY").unwrap_or_else(|_| "8Xt5vjYtOS".to_string());
    let url = format!(
        "https://api.acoustid.org/v2/lookup?client={}&meta=recordings+releasegroups+releases&duration={}&fingerprint={}",
        client_key, duration_sec, fingerprint
    );

    let client = Client::builder()
        .timeout(Duration::from_secs(8))
        .user_agent("LuminousMusicPlayer/0.1.0")
        .build()?;

    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        return Err(anyhow!("AcoustID API request failed: {}", response.status()));
    }

    let resp: AcoustIdResponse = response.json().await?;
    if resp.status != "ok" {
        return Err(anyhow!("AcoustID service status error"));
    }

    let results = resp.results.unwrap_or_default();
    let best_result = results
        .iter()
        .filter(|r| r.recordings.is_some() && !r.recordings.as_ref().unwrap().is_empty())
        .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));

    if let Some(r) = best_result {
        if let Some(recordings) = &r.recordings {
            if let Some(rec) = recordings.first() {
                let title = rec.title.clone();
                let artist = rec.artists.as_ref()
                    .and_then(|artists| artists.first().map(|a| a.name.clone()));
                let album = rec.release_groups.as_ref()
                    .and_then(|rgs| rgs.first().and_then(|rg| rg.title.clone()))
                    .or_else(|| rec.releases.as_ref().and_then(|rels| rels.first().and_then(|rel| rel.title.clone())));

                let year = rec.releases.as_ref().and_then(|rels| {
                    rels.iter().find_map(|rel| {
                        rel.date.as_ref().and_then(|d| match d {
                            AcoustIdDate::Year(y) => Some(*y),
                            AcoustIdDate::Full(s) => s.split('-').next().and_then(|part| part.parse::<u32>().ok()),
                        })
                    })
                });

                return Ok(SuggestedTags { title, artist, album, year });
            }
        }
    }

    Err(anyhow!("no matching audio recordings found on AcoustID"))
}
