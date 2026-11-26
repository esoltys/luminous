//! The subset of the HLS playlist format (RFC 8216) a live radio station
//! needs: master playlists with variants and audio renditions, and media
//! playlists with a sliding live window (#2012).

use reqwest::Url;

/// A parsed `.m3u8` playlist: either a master playlist listing variants, or
/// a media playlist listing segments.
#[derive(Debug, Clone, PartialEq)]
pub enum Playlist {
    Master(MasterPlaylist),
    Media(MediaPlaylist),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MasterPlaylist {
    pub variants: Vec<Variant>,
    /// `EXT-X-MEDIA:TYPE=AUDIO` renditions that carry their own `URI`.
    pub audio_renditions: Vec<AudioRendition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub uri: Url,
    pub bandwidth: u64,
    /// Lowercased `CODECS` entries, empty when the attribute is missing.
    pub codecs: Vec<String>,
    pub audio_group: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioRendition {
    pub group_id: String,
    pub uri: Url,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaPlaylist {
    pub target_duration_secs: u64,
    pub media_sequence: u64,
    pub segments: Vec<Segment>,
    /// `EXT-X-ENDLIST`: the playlist will not grow (a recording, not live).
    pub end_list: bool,
    /// An `EXT-X-KEY` with a method other than `NONE`.
    pub encrypted: bool,
    /// An `EXT-X-MAP` init section URI (used by fragmented MP4 / CMAF segments).
    pub init_map: Option<Url>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub sequence: u64,
    pub uri: Url,
}

/// True when `text` is an HLS playlist rather than a plain `.m3u` list of
/// stream URLs: it must start with `#EXTM3U` and use at least one `#EXT-X-` tag.
pub fn looks_like_hls(text: &str) -> bool {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    trimmed.starts_with("#EXTM3U") && trimmed.contains("#EXT-X-")
}

/// Parses `text`, resolving every URI against `base` (the playlist's own URL
/// after redirects).
pub fn parse(text: &str, base: &Url) -> Result<Playlist, String> {
    if !looks_like_hls(text) {
        return Err("not an HLS playlist".to_string());
    }

    let mut master = MasterPlaylist::default();
    let mut media = MediaPlaylist::default();
    let mut is_master = false;
    let mut pending_variant: Option<(u64, Vec<String>, Option<String>)> = None;
    let mut pending_segment = false;
    let mut next_sequence: Option<u64> = None;

    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-STREAM-INF:") {
            is_master = true;
            let attrs = parse_attributes(rest);
            let bandwidth = attr(&attrs, "BANDWIDTH")
                .and_then(|b| b.parse().ok())
                .unwrap_or(0);
            let codecs = attr(&attrs, "CODECS")
                .map(|c| {
                    c.split(',')
                        .map(|t| t.trim().to_ascii_lowercase())
                        .filter(|t| !t.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let group = attr(&attrs, "AUDIO").map(str::to_string);
            pending_variant = Some((bandwidth, codecs, group));
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA:") {
            let attrs = parse_attributes(rest);
            if attr(&attrs, "TYPE") == Some("AUDIO") {
                if let (Some(group_id), Some(uri)) = (attr(&attrs, "GROUP-ID"), attr(&attrs, "URI"))
                {
                    if let Ok(uri) = base.join(uri) {
                        master.audio_renditions.push(AudioRendition {
                            group_id: group_id.to_string(),
                            uri,
                            is_default: attr(&attrs, "DEFAULT") == Some("YES"),
                        });
                    }
                }
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-TARGETDURATION:") {
            media.target_duration_secs = rest.trim().parse().unwrap_or(0);
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            media.media_sequence = rest.trim().parse().unwrap_or(0);
        } else if line.starts_with("#EXT-X-ENDLIST") {
            media.end_list = true;
        } else if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            let attrs = parse_attributes(rest);
            if attr(&attrs, "METHOD").is_some_and(|m| m != "NONE") {
                media.encrypted = true;
            }
        } else if let Some(rest) = line.strip_prefix("#EXT-X-MAP:") {
            let attrs = parse_attributes(rest);
            if let Some(uri) = attr(&attrs, "URI").and_then(|u| base.join(u).ok()) {
                media.init_map = Some(uri);
            }
        } else if line.starts_with("#EXTINF") {
            pending_segment = true;
        } else if line.starts_with('#') {
            // Other tags (EXT-X-DISCONTINUITY, PROGRAM-DATE-TIME, ...) need no
            // handling: ADTS and MPEG audio frames resync on their own.
        } else if let Some((bandwidth, codecs, audio_group)) = pending_variant.take() {
            if let Ok(uri) = base.join(line) {
                master.variants.push(Variant {
                    uri,
                    bandwidth,
                    codecs,
                    audio_group,
                });
            }
        } else if pending_segment {
            pending_segment = false;
            let sequence = *next_sequence.get_or_insert(media.media_sequence);
            next_sequence = Some(sequence + 1);
            if let Ok(uri) = base.join(line) {
                media.segments.push(Segment { sequence, uri });
            }
        }
    }

    if is_master {
        Ok(Playlist::Master(master))
    } else {
        Ok(Playlist::Media(media))
    }
}

/// Picks the URI of the media playlist to play from `master`.
///
/// Prefers, in order: audio-only variants (or a variant's separate audio
/// rendition), variants that don't declare codecs, then variants that mux
/// video, whose video is dropped. Variants with a codec the engine can't
/// decode (AC-3, E-AC-3, ...) are skipped. Among equals, `prefer_lc` ranks
/// AAC-LC above HE-AAC (for platforms that decode only HE-AAC's core), then
/// the highest bandwidth wins, except for video variants, where the lowest
/// does, since their extra bits are all video.
pub fn choose_variant(master: &MasterPlaylist, prefer_lc: bool) -> Option<Url> {
    let mut best: Option<((u8, u8, i128), Url)> = None;
    for variant in &master.variants {
        let mut has_video = false;
        let mut has_he = false;
        let mut has_unsupported = false;
        for codec in &variant.codecs {
            if is_video_codec(codec) {
                has_video = true;
            } else if is_he_aac_codec(codec) {
                has_he = true;
            } else if !codec.starts_with("mp4a") {
                has_unsupported = true;
            }
        }
        if has_unsupported {
            continue;
        }

        let rendition = variant.audio_group.as_deref().and_then(|group| {
            let mut in_group = master
                .audio_renditions
                .iter()
                .filter(|r| r.group_id == group);
            let first = in_group.clone().next();
            in_group.find(|r| r.is_default).or(first)
        });

        let (class, uri) = match rendition {
            Some(r) => (0u8, r.uri.clone()),
            None if variant.codecs.is_empty() => (1, variant.uri.clone()),
            None if has_video => (2, variant.uri.clone()),
            None => (0, variant.uri.clone()),
        };
        let lc_rank = u8::from(prefer_lc && has_he);
        let bandwidth = i128::from(variant.bandwidth);
        let bandwidth_rank = if class == 2 { bandwidth } else { -bandwidth };
        let key = (class, lc_rank, bandwidth_rank);
        if best.as_ref().is_none_or(|(k, _)| key < *k) {
            best = Some((key, uri));
        }
    }
    best.map(|(_, uri)| uri)
}

fn is_video_codec(codec: &str) -> bool {
    [
        "avc1", "avc3", "hvc1", "hev1", "av01", "vp09", "vp8", "vp9", "dvh1", "dvhe",
    ]
    .iter()
    .any(|p| codec.starts_with(p))
}

fn is_he_aac_codec(codec: &str) -> bool {
    codec == "mp4a.40.5" || codec == "mp4a.40.29"
}

/// Parses an HLS attribute list (`KEY=value,KEY="quoted, value"`).
fn parse_attributes(list: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = list.trim();
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else { break };
        let key = rest[..eq].trim().to_ascii_uppercase();
        rest = &rest[eq + 1..];
        let value;
        if let Some(quoted) = rest.strip_prefix('"') {
            let end = quoted.find('"').unwrap_or(quoted.len());
            value = quoted[..end].to_string();
            rest = quoted.get(end + 1..).unwrap_or("");
            rest = rest.trim_start_matches(',');
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            value = rest[..end].trim().to_string();
            rest = rest.get(end + 1..).unwrap_or("");
        }
        out.push((key, value));
        rest = rest.trim_start();
    }
    out
}

fn attr<'a>(attrs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Url {
        Url::parse("https://radio.example/live/station/master.m3u8?token=abc").unwrap()
    }

    #[test]
    fn looks_like_hls_rejects_plain_m3u() {
        assert!(looks_like_hls("#EXTM3U\n#EXT-X-VERSION:3\n"));
        assert!(looks_like_hls("\u{feff}#EXTM3U\n#EXT-X-TARGETDURATION:6\n"));
        assert!(!looks_like_hls(
            "#EXTM3U\n#EXTINF:-1,Station\nhttp://a/stream\n"
        ));
        assert!(!looks_like_hls("http://a/stream.mp3\n"));
    }

    #[test]
    fn parses_media_playlist_with_sequence_and_relative_uris() {
        let text = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:10\n#EXT-X-MEDIA-SEQUENCE:4711\n\
                    #EXTINF:10.0,\nseg4711.ts\n#EXT-X-DISCONTINUITY\n#EXTINF:10.0,\n/abs/seg4712.ts\n\
                    #EXTINF:9.98,title=\"x\"\nhttps://cdn.example/seg4713.aac\n";
        let Playlist::Media(media) = parse(text, &base()).unwrap() else {
            panic!("expected a media playlist");
        };
        assert_eq!(media.target_duration_secs, 10);
        assert_eq!(media.media_sequence, 4711);
        assert!(!media.end_list && !media.encrypted && media.init_map.is_none());
        let seqs: Vec<u64> = media.segments.iter().map(|s| s.sequence).collect();
        assert_eq!(seqs, vec![4711, 4712, 4713]);
        assert_eq!(
            media.segments[0].uri.as_str(),
            "https://radio.example/live/station/seg4711.ts"
        );
        assert_eq!(
            media.segments[1].uri.as_str(),
            "https://radio.example/abs/seg4712.ts"
        );
        assert_eq!(
            media.segments[2].uri.as_str(),
            "https://cdn.example/seg4713.aac"
        );
    }

    #[test]
    fn flags_encryption_init_maps_and_endlist() {
        let text = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=AES-128,URI=\"k\"\n\
                    #EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:6,\na.m4s\n#EXT-X-ENDLIST\n";
        let Playlist::Media(media) = parse(text, &base()).unwrap() else {
            panic!("expected a media playlist");
        };
        assert!(media.encrypted && media.init_map.is_some() && media.end_list);

        let clear = "#EXTM3U\n#EXT-X-TARGETDURATION:6\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:6,\na.ts\n";
        let Playlist::Media(media) = parse(clear, &base()).unwrap() else {
            panic!("expected a media playlist");
        };
        assert!(!media.encrypted);
    }

    #[test]
    fn parses_master_with_quoted_codecs_and_audio_renditions() {
        let text = "#EXTM3U\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"English\",DEFAULT=NO,URI=\"audio/en.m3u8\"\n\
            #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"aud\",NAME=\"Main\",DEFAULT=YES,URI=\"audio/main.m3u8\"\n\
            #EXT-X-STREAM-INF:BANDWIDTH=96000,CODECS=\"mp4a.40.2\"\nlow.m3u8\n\
            #EXT-X-STREAM-INF:BANDWIDTH=2000000,CODECS=\"avc1.42c00d,mp4a.40.2\",AUDIO=\"aud\"\nvideo.m3u8\n";
        let Playlist::Master(master) = parse(text, &base()).unwrap() else {
            panic!("expected a master playlist");
        };
        assert_eq!(master.variants.len(), 2);
        assert_eq!(master.variants[1].codecs, vec!["avc1.42c00d", "mp4a.40.2"]);
        assert_eq!(master.variants[1].audio_group.as_deref(), Some("aud"));
        assert_eq!(master.audio_renditions.len(), 2);
        assert!(master.audio_renditions[1].is_default);
    }

    fn variant(uri: &str, bandwidth: u64, codecs: &[&str], group: Option<&str>) -> Variant {
        Variant {
            uri: base().join(uri).unwrap(),
            bandwidth,
            codecs: codecs.iter().map(|c| c.to_string()).collect(),
            audio_group: group.map(str::to_string),
        }
    }

    fn chosen(master: &MasterPlaylist, prefer_lc: bool) -> String {
        let uri = choose_variant(master, prefer_lc).expect("a variant");
        uri.path().rsplit('/').next().unwrap().to_string()
    }

    #[test]
    fn choose_variant_prefers_highest_audio_only_and_skips_unsupported_codecs() {
        let master = MasterPlaylist {
            variants: vec![
                variant("video.m3u8", 3_000_000, &["avc1.64001f", "mp4a.40.2"], None),
                variant("a48.m3u8", 48_000, &["mp4a.40.5"], None),
                variant("a320.m3u8", 320_000, &["mp4a.40.2"], None),
                variant("ec3.m3u8", 640_000, &["ec-3"], None),
            ],
            audio_renditions: Vec::new(),
        };
        assert_eq!(chosen(&master, false), "a320.m3u8");
    }

    #[test]
    fn choose_variant_prefers_lc_over_he_aac_only_when_asked() {
        let master = MasterPlaylist {
            variants: vec![
                variant("he128.m3u8", 128_000, &["mp4a.40.5"], None),
                variant("lc96.m3u8", 96_000, &["mp4a.40.2"], None),
            ],
            audio_renditions: Vec::new(),
        };
        assert_eq!(chosen(&master, false), "he128.m3u8");
        assert_eq!(chosen(&master, true), "lc96.m3u8");
    }

    #[test]
    fn choose_variant_follows_default_audio_rendition_of_a_video_variant() {
        let master = MasterPlaylist {
            variants: vec![variant(
                "video.m3u8",
                2_000_000,
                &["avc1.42c00d", "mp4a.40.2"],
                Some("aud"),
            )],
            audio_renditions: vec![
                AudioRendition {
                    group_id: "aud".into(),
                    uri: base().join("other.m3u8").unwrap(),
                    is_default: false,
                },
                AudioRendition {
                    group_id: "aud".into(),
                    uri: base().join("main.m3u8").unwrap(),
                    is_default: true,
                },
            ],
        };
        assert_eq!(chosen(&master, false), "main.m3u8");
    }

    #[test]
    fn choose_variant_takes_lowest_bandwidth_when_only_video_variants_exist() {
        let master = MasterPlaylist {
            variants: vec![
                variant("hi.m3u8", 4_000_000, &["avc1.64001f", "mp4a.40.2"], None),
                variant("lo.m3u8", 800_000, &["avc1.42c00d", "mp4a.40.2"], None),
            ],
            audio_renditions: Vec::new(),
        };
        assert_eq!(chosen(&master, false), "lo.m3u8");
    }

    #[test]
    fn choose_variant_returns_none_when_nothing_is_decodable() {
        let master = MasterPlaylist {
            variants: vec![variant("ac3.m3u8", 384_000, &["ac-3"], None)],
            audio_renditions: Vec::new(),
        };
        assert_eq!(choose_variant(&master, false), None);
    }
}
