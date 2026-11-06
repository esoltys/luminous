//! Tag clean-up: finding and removing metadata cruft (#1939).
//!
//! Cruft is metadata that describes how a file was made or where it was
//! downloaded, not the recording: promotional URLs in comments, encoder
//! banners, iTunes' `iTunNORM` volume blob, and an ID3v1 tag duplicating an
//! ID3v2 one. [`find_cruft`] lists it; [`strip_cruft`] removes a chosen subset.
//! Both run the same classification over the file's native tags, so what the
//! preview shows is exactly what the apply step can remove.
//!
//! Everything else is left byte-for-byte alone. Tags are edited through each
//! format's own tag type rather than lofty's generic `Tag`, whose save rebuilds
//! an ID3v2 tag from the generic items and drops frames it has no mapping for
//! (`PRIV`, `GEOB`, unrecognised `TXXX`). `iTunSMPB` in particular is never
//! touched: it carries the encoder delay and padding gapless AAC playback needs.

use crate::audio::save_with_retry;
use anyhow::{Context, Result};
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, FileType};
use lofty::id3::v1::Id3v1Tag;
use lofty::id3::v2::{Frame, Id3v2Tag};
use lofty::iff::wav::RiffInfoList;
use lofty::mp4::{AtomData, AtomIdent, Ilst};
use lofty::ogg::tag::VorbisComments;
use lofty::ogg::OggPictureStorage;
use lofty::probe::Probe;
use lofty::tag::{TagExt, TagType};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// What kind of cruft a [`CruftField`] is. Format-agnostic: an encoder banner
/// is `Encoder` whether it lives in ID3v2 `TSSE`, Vorbis `ENCODER`, MP4 `©too`
/// or RIFF `ISFT`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CruftKind {
    /// A comment that is mostly a URL, or a URL alongside a download/promo phrase.
    PromoComment,
    /// The encoding software and its settings (e.g. "LAME 3.99r").
    Encoder,
    /// Who encoded the file, usually a rip group's signature.
    EncodedBy,
    /// iTunes' `iTunNORM` Sound Check blob, superseded by measured loudness.
    ItunesNormalization,
    /// An ID3v1 tag in a file that also has an ID3v2 tag. The value is a
    /// short summary of what the ID3v1 tag holds.
    Id3v1Duplicate,
}

/// One removable field: its kind and its current value. Passing it back to
/// [`strip_cruft`] removes every field in the file that still has this kind
/// and value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CruftField {
    pub kind: CruftKind,
    pub value: String,
}

/// Lists the cruft in `path`'s tags, without writing anything. A clean file,
/// or one in a format this module doesn't edit (APE, WavPack, Musepack, ...),
/// yields an empty list. Errors only when the file can't be opened or parsed.
pub fn find_cruft(path: &Path) -> Result<Vec<CruftField>> {
    Ok(scan(path, None)?.found)
}

/// Removes the listed fields from `path` and returns how many were removed.
/// The file is re-read and matched by kind and value, so a file that changed
/// since [`find_cruft`] only loses fields that still match. An empty
/// selection, or one that no longer matches anything, writes nothing.
pub fn strip_cruft(path: &Path, fields: &[CruftField]) -> Result<usize> {
    if fields.is_empty() {
        return Ok(0);
    }
    Ok(scan(path, Some(fields))?.removed)
}

/// Walks every native tag in `path`, classifying each item. With a
/// `selection`, items matching it are removed and each changed tag is saved.
fn scan<'a>(path: &Path, selection: Option<&'a [CruftField]>) -> Result<Scan<'a>> {
    let probe = Probe::open(path)
        .context("cannot open file")?
        .guess_file_type()
        .context("cannot read file header")?;
    let file_type = probe.file_type();
    let reader = probe.into_inner();
    // A preview never saves, so it can skip pictures; a strip must read them,
    // since saving a tag writes back only the pictures it holds.
    let options = ParseOptions::new()
        .read_properties(false)
        .read_cover_art(selection.is_some());
    let mut scan = Scan {
        selection,
        found: Vec::new(),
        removed: 0,
    };

    match file_type {
        Some(FileType::Mpeg) => {
            let mut file: lofty::mpeg::MpegFile = read(reader, options)?;
            let has_id3v2 = file.id3v2().is_some();
            if let Some(tag) = file.id3v2_mut() {
                scan.id3v2(path, tag)?;
            }
            if has_id3v2 {
                if let Some(tag) = file.id3v1() {
                    scan.id3v1(path, tag)?;
                }
            }
        }
        Some(FileType::Flac) => {
            let mut file: lofty::flac::FlacFile = read(reader, options)?;
            if let Some(tag) = file.id3v2_mut() {
                scan.id3v2(path, tag)?;
            }
            // FlacFile keeps picture blocks outside its VorbisComments, but
            // saving the tag rewrites every picture block from the tag's own
            // list, so hand the pictures to the tag first.
            let mut pictures = Vec::new();
            while !file.pictures().is_empty() {
                pictures.push(file.remove_picture(0));
            }
            if let Some(tag) = file.vorbis_comments_mut() {
                for (picture, info) in pictures {
                    tag.insert_picture(picture, Some(info))?;
                }
                scan.vorbis(path, tag)?;
            }
        }
        Some(FileType::Vorbis) => {
            let mut file: lofty::ogg::VorbisFile = read(reader, options)?;
            scan.vorbis(path, file.vorbis_comments_mut())?;
        }
        Some(FileType::Opus) => {
            let mut file: lofty::ogg::OpusFile = read(reader, options)?;
            scan.vorbis(path, file.vorbis_comments_mut())?;
        }
        Some(FileType::Speex) => {
            let mut file: lofty::ogg::SpeexFile = read(reader, options)?;
            scan.vorbis(path, file.vorbis_comments_mut())?;
        }
        Some(FileType::Mp4) => {
            let mut file: lofty::mp4::Mp4File = read(reader, options)?;
            if let Some(tag) = file.ilst_mut() {
                scan.ilst(path, tag)?;
            }
        }
        Some(FileType::Wav) => {
            let mut file: lofty::iff::wav::WavFile = read(reader, options)?;
            if let Some(tag) = file.id3v2_mut() {
                scan.id3v2(path, tag)?;
            }
            if let Some(tag) = file.riff_info_mut() {
                scan.riff_info(path, tag)?;
            }
        }
        Some(FileType::Aiff) => {
            let mut file: lofty::iff::aiff::AiffFile = read(reader, options)?;
            if let Some(tag) = file.id3v2_mut() {
                scan.id3v2(path, tag)?;
            }
        }
        _ => {}
    }

    Ok(scan)
}

/// Parses the file from `reader`, consuming it so no handle on the file is
/// left open while a tag is saved.
fn read<T: AudioFile>(mut reader: BufReader<File>, options: ParseOptions) -> Result<T> {
    T::read_from(&mut reader, options).context("cannot read tags")
}

struct Scan<'a> {
    selection: Option<&'a [CruftField]>,
    found: Vec<CruftField>,
    removed: usize,
}

impl Scan<'_> {
    /// Records `field` (if any) and returns whether its item should be kept.
    fn keep(&mut self, field: Option<CruftField>) -> bool {
        let Some(field) = field else {
            return true;
        };
        let remove = self.selection.is_some_and(|s| s.contains(&field));
        if remove {
            self.removed += 1;
        }
        self.found.push(field);
        !remove
    }

    /// Runs `visit` over a tag and saves it when that removed anything.
    fn edit<T: TagExt>(
        &mut self,
        path: &Path,
        tag: &mut T,
        visit: impl FnOnce(&mut Self, &mut T),
    ) -> Result<()> {
        let before = self.removed;
        visit(self, tag);
        if self.removed > before {
            save_with_retry(path, "remove tag cruft", || {
                tag.save_to_path(path, WriteOptions::default())
            })?;
        }
        Ok(())
    }

    fn id3v2(&mut self, path: &Path, tag: &mut Id3v2Tag) -> Result<()> {
        self.edit(path, tag, |scan, tag| {
            tag.retain(|frame| scan.keep(classify_id3v2(frame)));
        })
    }

    fn vorbis(&mut self, path: &Path, tag: &mut VorbisComments) -> Result<()> {
        self.edit(path, tag, |scan, tag| {
            // No `retain` on VorbisComments: take every item and put back the
            // ones to keep, which preserves their order.
            let items: Vec<(String, String)> = tag.take_items().collect();
            for (key, value) in items {
                if scan.keep(classify_vorbis(&key, &value)) {
                    tag.push(key, value);
                }
            }
        })
    }

    fn ilst(&mut self, path: &Path, tag: &mut Ilst) -> Result<()> {
        self.edit(path, tag, |scan, tag| {
            tag.retain(|atom| {
                let text = atom.data().find_map(|data| match data {
                    AtomData::UTF8(s) | AtomData::UTF16(s) => Some(s.as_str()),
                    _ => None,
                });
                scan.keep(text.and_then(|text| classify_mp4(atom.ident(), text)))
            });
        })
    }

    fn riff_info(&mut self, path: &Path, tag: &mut RiffInfoList) -> Result<()> {
        self.edit(path, tag, |scan, tag| {
            let items: Vec<(String, String)> = (&*tag).into_iter().cloned().collect();
            for (key, value) in items {
                if !scan.keep(classify_riff(&key, &value)) {
                    tag.remove(&key);
                }
            }
        })
    }

    /// The whole ID3v1 tag is one field: it only ever duplicates (and
    /// truncates) the ID3v2 tag the caller has already checked for.
    fn id3v1(&mut self, path: &Path, tag: &Id3v1Tag) -> Result<()> {
        let summary = [tag.artist.as_deref(), tag.title.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" – ");
        let field = CruftField {
            kind: CruftKind::Id3v1Duplicate,
            value: if summary.is_empty() {
                "ID3v1".to_string()
            } else {
                summary
            },
        };
        if !self.keep(Some(field)) {
            save_with_retry(path, "remove ID3v1 tag", || {
                remove_tag(path, TagType::Id3v1)
            })?;
        }
        Ok(())
    }
}

/// Removes `tag_type` from `path`. `TagType::remove_from_path` opens the file
/// read-only and so can never write, hence the explicit read-write handle.
fn remove_tag(
    path: &Path,
    tag_type: TagType,
) -> std::result::Result<(), lofty::error::FileEncodingError> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    tag_type.remove_from(&mut file, WriteOptions::default())
}

fn field(kind: CruftKind, value: &str) -> Option<CruftField> {
    Some(CruftField {
        kind,
        value: value.to_string(),
    })
}

/// A comment is cruft when it's promotional; a comment with an `iTun*`
/// description is iTunes data, of which only `iTunNORM` is cruft.
fn classify_comment(description: &str, text: &str) -> Option<CruftField> {
    if description.eq_ignore_ascii_case("iTunNORM") {
        return field(CruftKind::ItunesNormalization, text);
    }
    if description.len() >= 4 && description[..4].eq_ignore_ascii_case("iTun") {
        return None;
    }
    is_promotional(text)
        .then(|| field(CruftKind::PromoComment, text))
        .flatten()
}

fn classify_id3v2(frame: &Frame<'_>) -> Option<CruftField> {
    match frame {
        Frame::Comment(comment) => classify_comment(&comment.description, &comment.content),
        Frame::Text(text) => match frame.id_str() {
            "TSSE" => field(CruftKind::Encoder, &text.value),
            "TENC" => field(CruftKind::EncodedBy, &text.value),
            _ => None,
        },
        _ => None,
    }
}

fn classify_vorbis(key: &str, value: &str) -> Option<CruftField> {
    match key.to_ascii_uppercase().as_str() {
        "COMMENT" | "DESCRIPTION" => classify_comment("", value),
        "ENCODER" => field(CruftKind::Encoder, value),
        "ENCODED-BY" | "ENCODEDBY" | "ENCODED_BY" => field(CruftKind::EncodedBy, value),
        "ITUNNORM" => field(CruftKind::ItunesNormalization, value),
        _ => None,
    }
}

fn classify_mp4(ident: &AtomIdent<'_>, text: &str) -> Option<CruftField> {
    match ident {
        AtomIdent::Fourcc([0xA9, b'c', b'm', b't']) => classify_comment("", text),
        AtomIdent::Fourcc([0xA9, b't', b'o', b'o']) => field(CruftKind::Encoder, text),
        AtomIdent::Fourcc([0xA9, b'e', b'n', b'c']) => field(CruftKind::EncodedBy, text),
        AtomIdent::Freeform { mean, name } if mean == "com.apple.iTunes" => {
            classify_comment(name, text)
        }
        _ => None,
    }
}

fn classify_riff(key: &str, value: &str) -> Option<CruftField> {
    match key {
        "ICMT" => classify_comment("", value),
        "ISFT" => field(CruftKind::Encoder, value),
        "ITCH" => field(CruftKind::EncodedBy, value),
        _ => None,
    }
}

/// Phrases that mark a comment containing a URL as an advert rather than a note.
const PROMO_PHRASES: &[&str] = &[
    "downloaded from",
    "downloaded at",
    "download from",
    "download at",
    "free download",
    "ripped by",
    "uploaded by",
    "brought to you by",
    "more music",
    "get it at",
    "visit",
    "promo",
];

/// Top-level domains common enough in download-site banners to recognise a
/// bare domain ("freemp3s.ru") without a scheme or `www.`.
const TLDS: &[&str] = &[
    "com", "net", "org", "info", "biz", "ru", "ua", "io", "me", "co", "to", "cc", "tv", "fm", "us",
    "uk", "de", "fr", "es", "it", "pl", "nl", "eu", "ws", "xyz", "club", "online", "site", "top",
];

/// Sites where a link in a comment is the artist's own page, written by the
/// artist's store (Bandcamp adds "Visit https://artist.bandcamp.com" to every
/// download), not by a download site advertising itself.
const ARTIST_SITES: &[&str] = &["bandcamp.com"];

/// A comment is promotional when it contains a URL and either the URL is
/// most of it (at most three other words) or it uses a download/promo phrase.
/// A personal note that merely mentions a link is not, and neither is a
/// comment whose only links are to [`ARTIST_SITES`].
pub(crate) fn is_promotional(text: &str) -> bool {
    let mut other_sites = 0;
    let mut words = 0;
    for token in text.split_whitespace() {
        match url_host(token) {
            Some(host) if is_artist_site(&host) => {}
            Some(_) => other_sites += 1,
            None if token.chars().any(char::is_alphanumeric) => words += 1,
            None => {}
        }
    }
    if other_sites == 0 {
        return false;
    }
    let lower = text.to_lowercase();
    words <= 3 || PROMO_PHRASES.iter().any(|phrase| lower.contains(phrase))
}

fn is_artist_site(host: &str) -> bool {
    ARTIST_SITES
        .iter()
        .any(|site| host == *site || host.ends_with(&format!(".{site}")))
}

/// The lower-cased host of `token` when it looks like a URL or bare domain.
fn url_host(token: &str) -> Option<String> {
    let token = token
        .trim_matches(|c: char| "()[]<>{}\"'.,;:!?".contains(c))
        .to_ascii_lowercase();
    let rest = token
        .strip_prefix("http://")
        .or_else(|| token.strip_prefix("https://"));
    let host = rest.unwrap_or(&token).split('/').next().unwrap_or_default();
    if rest.is_some() || host.starts_with("www.") {
        return Some(host.to_string());
    }
    if host.contains('@') {
        return None;
    }
    let (name, tld) = host.rsplit_once('.')?;
    (name.chars().filter(char::is_ascii_alphanumeric).count() >= 2 && TLDS.contains(&tld))
        .then(|| host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::read_tags;
    use lofty::id3::v2::{CommentFrame, FrameId, TextInformationFrame};
    use lofty::mp4::Atom;
    use lofty::tag::Accessor;
    use lofty::TextEncoding;
    use std::borrow::Cow;
    use std::io::Cursor;
    use std::path::PathBuf;

    fn fixture_copy(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("temp dir");
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../src-tauri/tests/fixtures/audio")
            .join(name);
        let dest = dir.path().join(name);
        std::fs::copy(&src, &dest).expect("copy fixture");
        (dir, dest)
    }

    fn kinds(fields: &[CruftField]) -> Vec<CruftKind> {
        let mut kinds: Vec<_> = fields.iter().map(|f| f.kind).collect();
        kinds.sort_by_key(|k| format!("{k:?}"));
        kinds
    }

    fn comment(description: &str, content: &str) -> Frame<'static> {
        Frame::Comment(CommentFrame::new(
            TextEncoding::UTF8,
            *b"eng",
            description.to_string(),
            content.to_string(),
        ))
    }

    fn text_frame(id: &'static str, value: &str) -> Frame<'static> {
        Frame::Text(TextInformationFrame::new(
            FrameId::Valid(Cow::Borrowed(id)),
            TextEncoding::UTF8,
            value.to_string(),
        ))
    }

    #[test]
    fn promotional_comments_are_recognised() {
        for text in [
            "www.freemp3s.ru",
            "Downloaded from https://example.com/free",
            "Visit FreeMusicSite.com for more free music!!!",
            "(http://ripgroup.net)",
            "Ripped by XYZ for musicsite.org and all its users, enjoy the album",
            // A download site riding along with a Bandcamp link is still flagged.
            "Visit https://artist.bandcamp.com or grab it free at freemp3s.ru",
        ] {
            assert!(is_promotional(text), "{text}");
        }
    }

    #[test]
    fn personal_notes_are_kept() {
        for text in [
            "",
            "Bought at the merch table after the show",
            "Recorded live at the Fillmore, 1971. Setlist on archive.org with photos",
            "Vol.II remaster, e.g. the 2009 one",
            "Mail me at someone@example.com about the bootleg",
            "Visit http://andyjamesuk.bandcamp.com",
            "Visit https://dancewiththedead.bandcamp.com",
            "bandcamp.com/album/xyz",
        ] {
            assert!(!is_promotional(text), "{text}");
        }
    }

    #[test]
    fn mp3_cruft_is_found_and_stripped_leaving_everything_else() {
        let (_dir, path) = fixture_copy("song_alpha.mp3");
        let mut tag = Id3v2Tag::new();
        tag.set_title("Keep Me".to_string());
        tag.insert(comment("", "Downloaded from www.freemp3s.ru"));
        tag.insert(comment("iTunNORM", " 00000A0B 00000C0D"));
        tag.insert(comment("iTunSMPB", " 00000000 00000210 000007B4"));
        tag.insert(text_frame("TSSE", "LAME 3.99r"));
        tag.insert(text_frame("TENC", "RipGroup"));
        tag.insert_user_text("REPLAYGAIN_TRACK_GAIN".to_string(), "-6.5 dB".to_string());
        tag.insert_picture(
            lofty::picture::Picture::unchecked(vec![0xFF, 0xD8, 0xFF, 0xE0])
                .pic_type(lofty::picture::PictureType::CoverFront)
                .mime_type(lofty::picture::MimeType::Jpeg)
                .build(),
        );
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        let mut v1 = Id3v1Tag::new();
        v1.title = Some("Keep Me".to_string());
        v1.save_to_path(&path, WriteOptions::default()).unwrap();

        let found = find_cruft(&path).unwrap();
        assert_eq!(
            kinds(&found),
            vec![
                CruftKind::EncodedBy,
                CruftKind::Encoder,
                CruftKind::Id3v1Duplicate,
                CruftKind::ItunesNormalization,
                CruftKind::PromoComment,
            ]
        );

        assert_eq!(strip_cruft(&path, &found).unwrap(), 5);
        assert!(find_cruft(&path).unwrap().is_empty());

        let mut reader = Cursor::new(std::fs::read(&path).unwrap());
        let file = lofty::mpeg::MpegFile::read_from(&mut reader, ParseOptions::new()).unwrap();
        assert!(file.id3v1().is_none());
        let tag = file.id3v2().unwrap();
        assert_eq!(tag.title().as_deref(), Some("Keep Me"));
        assert!(tag.comments().next().is_none());
        assert!(tag
            .iter()
            .any(|f| matches!(f, Frame::Comment(c) if c.description == "iTunSMPB")));
        assert_eq!(tag.get_user_text("REPLAYGAIN_TRACK_GAIN"), Some("-6.5 dB"));
        assert!(
            tag.iter().any(|f| matches!(f, Frame::Picture(_))),
            "cover art survives, though previews skip reading it"
        );
    }

    #[test]
    fn unticked_fields_survive_and_stale_values_are_not_removed() {
        let (_dir, path) = fixture_copy("song_alpha.mp3");
        let mut tag = Id3v2Tag::new();
        tag.insert(text_frame("TSSE", "LAME 3.99r"));
        tag.insert(text_frame("TENC", "RipGroup"));
        tag.save_to_path(&path, WriteOptions::default()).unwrap();

        let selection = [
            CruftField {
                kind: CruftKind::Encoder,
                value: "LAME 3.99r".to_string(),
            },
            CruftField {
                kind: CruftKind::EncodedBy,
                value: "Someone Else".to_string(),
            },
        ];
        assert_eq!(strip_cruft(&path, &selection).unwrap(), 1);
        assert_eq!(
            find_cruft(&path).unwrap(),
            vec![CruftField {
                kind: CruftKind::EncodedBy,
                value: "RipGroup".to_string(),
            }]
        );
    }

    #[test]
    fn id3v1_alone_is_not_cruft() {
        let (_dir, path) = fixture_copy("song_alpha.mp3");
        remove_tag(&path, TagType::Id3v2).unwrap();
        let mut v1 = Id3v1Tag::new();
        v1.title = Some("Only Tag".to_string());
        v1.save_to_path(&path, WriteOptions::default()).unwrap();
        assert!(find_cruft(&path).unwrap().is_empty());
    }

    #[test]
    fn flac_and_ogg_vorbis_comments_are_cleaned() {
        for name in ["song_gamma.flac", "song_delta.ogg"] {
            let (_dir, path) = fixture_copy(name);
            let mut tag = VorbisComments::new();
            tag.push("TITLE".to_string(), "Keep Me".to_string());
            tag.push("COMMENT".to_string(), "http://example.com".to_string());
            tag.push("ENCODER".to_string(), "Lavf58.29.100".to_string());
            tag.push("LYRICS".to_string(), "la la la".to_string());
            tag.save_to_path(&path, WriteOptions::default()).unwrap();

            let found = find_cruft(&path).unwrap();
            assert_eq!(
                kinds(&found),
                vec![CruftKind::Encoder, CruftKind::PromoComment],
                "{name}"
            );
            assert_eq!(strip_cruft(&path, &found).unwrap(), 2, "{name}");

            let meta = read_tags(&path).unwrap();
            assert_eq!(meta.title.as_deref(), Some("Keep Me"), "{name}");
            assert_eq!(meta.comment, None, "{name}");
            assert!(find_cruft(&path).unwrap().is_empty(), "{name}");
        }
    }

    #[test]
    fn flac_cover_art_survives_a_strip() {
        let (_dir, path) = fixture_copy("song_gamma.flac");
        let mut tag = VorbisComments::new();
        tag.push("ENCODER".to_string(), "Lavf58.29.100".to_string());
        tag.insert_picture(
            lofty::picture::Picture::unchecked(vec![0xFF, 0xD8, 0xFF, 0xE0])
                .pic_type(lofty::picture::PictureType::CoverFront)
                .mime_type(lofty::picture::MimeType::Jpeg)
                .build(),
            Some(lofty::picture::PictureInformation::default()),
        )
        .unwrap();
        tag.save_to_path(&path, WriteOptions::default()).unwrap();

        let found = find_cruft(&path).unwrap();
        assert_eq!(strip_cruft(&path, &found).unwrap(), 1);

        let mut reader = Cursor::new(std::fs::read(&path).unwrap());
        let file = lofty::flac::FlacFile::read_from(&mut reader, ParseOptions::new()).unwrap();
        assert_eq!(file.pictures().len(), 1);
    }

    #[test]
    fn m4a_atoms_are_cleaned_and_itunsmpb_kept() {
        let (_dir, path) = fixture_copy("song_epsilon.m4a");
        let mut tag = Ilst::new();
        let freeform = |name: &'static str, value: &str| {
            Atom::new(
                AtomIdent::Freeform {
                    mean: Cow::Borrowed("com.apple.iTunes"),
                    name: Cow::Borrowed(name),
                },
                AtomData::UTF8(value.to_string()),
            )
        };
        tag.set_title("Keep Me".to_string());
        tag.insert(Atom::new(
            AtomIdent::Fourcc(*b"\xa9too"),
            AtomData::UTF8("iTunes 12.9".to_string()),
        ));
        tag.insert(freeform("iTunNORM", " 00000A0B"));
        tag.insert(freeform("iTunSMPB", " 00000000 00000840"));
        tag.save_to_path(&path, WriteOptions::default()).unwrap();

        let found = find_cruft(&path).unwrap();
        assert_eq!(
            kinds(&found),
            vec![CruftKind::Encoder, CruftKind::ItunesNormalization]
        );
        assert_eq!(strip_cruft(&path, &found).unwrap(), 2);

        let mut reader = Cursor::new(std::fs::read(&path).unwrap());
        let file = lofty::mp4::Mp4File::read_from(&mut reader, ParseOptions::new()).unwrap();
        let ilst = file.ilst().unwrap();
        assert_eq!(ilst.title().as_deref(), Some("Keep Me"));
        assert!(ilst
            .get(&AtomIdent::Freeform {
                mean: Cow::Borrowed("com.apple.iTunes"),
                name: Cow::Borrowed("iTunSMPB"),
            })
            .is_some());
    }

    #[test]
    fn clean_file_is_empty_and_not_rewritten() {
        let (_dir, path) = fixture_copy("song_gamma.flac");
        let mut tag = VorbisComments::new();
        tag.push("TITLE".to_string(), "Clean".to_string());
        tag.push(
            "COMMENT".to_string(),
            "Bought at the merch table".to_string(),
        );
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(find_cruft(&path).unwrap().is_empty());
        let stale = [CruftField {
            kind: CruftKind::Encoder,
            value: "nothing".to_string(),
        }];
        assert_eq!(strip_cruft(&path, &stale).unwrap(), 0);
        assert_eq!(strip_cruft(&path, &[]).unwrap(), 0);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn read_tags_ignores_itunes_comment_blobs() {
        let (_dir, path) = fixture_copy("song_alpha.mp3");
        let mut tag = Id3v2Tag::new();
        tag.insert(comment("iTunNORM", " 00000A0B"));
        tag.insert(comment("", "A real note"));
        tag.save_to_path(&path, WriteOptions::default()).unwrap();
        assert_eq!(
            read_tags(&path).unwrap().comment.as_deref(),
            Some("A real note")
        );
    }
}
