//! End to end: real files through the real encoders. Opus runs everywhere;
//! AAC and MP3 run where this computer can encode them (Media Foundation on
//! Windows, ffmpeg elsewhere).

use image::codecs::jpeg::JpegEncoder;
use lofty::config::{ParseOptions, WriteOptions};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::picture::{Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{Accessor, Tag, TagType};
use luminous_transcode::{
    ConvertMode, ConvertSettings, Decision, DeviceProfile, Error, TargetFormat, Transcoder,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

const SECONDS: usize = 2;

/// A 44.1 kHz stereo WAV with a title and `art_px` square cover art.
fn library_file(dir: &Path, art_px: u32) -> PathBuf {
    let path = dir.join("Valediction.wav");
    let frames = 44_100 * SECONDS;
    let tone: Vec<i16> = (0..frames)
        .map(|i| ((i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin() * 8000.0) as i16)
        .collect();
    luminous_audio::write_test_pcm16_wav(&path, 44_100, &[tone.clone(), tone]);

    let image = image::RgbImage::from_fn(art_px, art_px, |x, y| image::Rgb([x as u8, y as u8, 90]));
    let mut jpeg = Vec::new();
    JpegEncoder::new(&mut jpeg).encode_image(&image).unwrap();

    let mut file = Probe::open(&path).unwrap().read().unwrap();
    let mut tag = Tag::new(TagType::Id3v2);
    tag.set_title("Valediction".into());
    tag.set_artist("Insomnium".into());
    tag.push_picture(
        Picture::unchecked(jpeg)
            .pic_type(PictureType::CoverFront)
            .build(),
    );
    file.insert_tag(tag);
    file.save_to_path(&path, WriteOptions::default()).unwrap();
    path
}

fn profile(mode: ConvertMode, format: TargetFormat, max_art_px: Option<u32>) -> DeviceProfile {
    DeviceProfile {
        supported_formats: vec!["mp3".into(), "wav".into()],
        convert: ConvertSettings {
            mode,
            format,
            bitrate_kbps: 192,
            max_art_px,
        },
        ..DeviceProfile::default()
    }
}

/// Title, largest art side, and decoded (rate, frames) of `path`.
fn inspect(path: &Path) -> (Option<String>, u32, u32, usize) {
    let file = Probe::open(path)
        .unwrap()
        .options(ParseOptions::new().read_cover_art(true))
        .guess_file_type()
        .unwrap()
        .read()
        .unwrap();
    let tag = file.primary_tag().or_else(|| file.first_tag()).unwrap();
    let art = tag
        .pictures()
        .iter()
        .map(|p| {
            let image = image::load_from_memory(p.data()).unwrap();
            image.width().max(image.height())
        })
        .max()
        .unwrap_or(0);
    let title = tag.title().map(|t| t.to_string());

    let decoder = luminous_audio::OfflineDecoder::open_file(path).unwrap();
    let rate = decoder.sample_rate();
    let mut frames = 0;
    decoder.decode_blocks(1, |planes| frames += planes[0].len());
    (title, art, rate, frames)
}

fn convert_and_check(format: TargetFormat) {
    let dir = tempfile::tempdir().unwrap();
    let source = library_file(dir.path(), 1200);
    let transcoder = Transcoder::new(dir.path().join("cache"), 1 << 30);
    let profile = profile(ConvertMode::All, format, Some(500));

    let estimate = transcoder.estimated_size(&source, &profile).unwrap();
    let prepared = transcoder
        .materialize(&source, &profile, &AtomicBool::new(false))
        .unwrap();
    let (resolved, _) = transcoder.capabilities().resolve(format, 192);
    assert!(matches!(
        prepared.decision,
        Decision::Convert { format: f, .. } if f == resolved
    ));
    assert_eq!(prepared.extension, resolved.extension());
    assert!(prepared.path.starts_with(dir.path().join("cache")));

    let (title, art, rate, frames) = inspect(&prepared.path);
    assert_eq!(title.as_deref(), Some("Valediction"));
    assert_eq!(art, 500);
    let seconds = frames as f64 / f64::from(rate);
    assert!(
        (seconds - SECONDS as f64).abs() < 0.1,
        "{format:?} decoded to {seconds} s"
    );
    // The estimate is in the right range before encoding, exact after.
    let size = prepared.size as f64;
    assert!((estimate as f64) > size * 0.5 && (estimate as f64) < size * 2.0);
    assert_eq!(
        transcoder.estimated_size(&source, &profile).unwrap(),
        prepared.size
    );

    // Unchanged source and settings: the same cache entry.
    let again = transcoder
        .materialize(&source, &profile, &AtomicBool::new(false))
        .unwrap();
    assert_eq!(again, prepared);
}

#[test]
fn converts_to_opus() {
    convert_and_check(TargetFormat::Opus);
}

#[test]
fn converts_to_aac_and_mp3_where_available() {
    let caps = Transcoder::new(PathBuf::new(), 0).capabilities().clone();
    for format in [TargetFormat::Aac, TargetFormat::Mp3] {
        if caps.formats.iter().any(|c| c.format == format) {
            convert_and_check(format);
        } else {
            eprintln!("{format:?} encoding isn't available here; skipped");
        }
    }
}

#[test]
fn playable_files_are_copied_or_only_have_their_art_shrunk() {
    let dir = tempfile::tempdir().unwrap();
    let source = library_file(dir.path(), 1200);
    let transcoder = Transcoder::new(dir.path().join("cache"), 1 << 30);
    let stop = AtomicBool::new(false);

    let copied = transcoder
        .materialize(
            &source,
            &profile(ConvertMode::Unsupported, TargetFormat::Opus, None),
            &stop,
        )
        .unwrap();
    assert_eq!(copied.decision, Decision::Copy);
    assert_eq!(copied.path, source);
    assert_eq!(copied.extension, "wav");

    let shrunk = transcoder
        .materialize(
            &source,
            &profile(ConvertMode::Unsupported, TargetFormat::Opus, Some(300)),
            &stop,
        )
        .unwrap();
    assert_eq!(shrunk.decision, Decision::ShrinkArt { max_px: 300 });
    assert_eq!(shrunk.extension, "wav");
    let (title, art, rate, frames) = inspect(&shrunk.path);
    assert_eq!(title.as_deref(), Some("Valediction"));
    assert_eq!(art, 300);
    assert_eq!((rate, frames), (44_100, 44_100 * SECONDS));
    assert!(shrunk.size < std::fs::metadata(&source).unwrap().len());
}

#[test]
fn a_cancelled_conversion_caches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let source = library_file(dir.path(), 100);
    let cache = dir.path().join("cache");
    let transcoder = Transcoder::new(cache.clone(), 1 << 30);
    let err = transcoder
        .materialize(
            &source,
            &profile(ConvertMode::All, TargetFormat::Opus, None),
            &AtomicBool::new(true),
        )
        .unwrap_err();
    assert_eq!(err, Error::Cancelled);
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
}

#[test]
fn a_changed_source_is_converted_again() {
    let dir = tempfile::tempdir().unwrap();
    let source = library_file(dir.path(), 100);
    let transcoder = Transcoder::new(dir.path().join("cache"), 1 << 30);
    let profile = profile(ConvertMode::All, TargetFormat::Opus, None);
    let stop = AtomicBool::new(false);
    let first = transcoder.materialize(&source, &profile, &stop).unwrap();

    let mut file = Probe::open(&source).unwrap().read().unwrap();
    file.first_tag_mut()
        .unwrap()
        .set_title("Valediction (Live)".into());
    file.save_to_path(&source, WriteOptions::default()).unwrap();

    let second = transcoder.materialize(&source, &profile, &stop).unwrap();
    assert_ne!(second.path, first.path);
    assert_eq!(
        inspect(&second.path).0.as_deref(),
        Some("Valediction (Live)")
    );
}

#[test]
fn mono_sources_convert_to_every_available_format() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mono.wav");
    luminous_audio::write_test_pcm16_wav(&source, 44_100, &[vec![1000; 44_100]]);
    let transcoder = Transcoder::new(dir.path().join("cache"), 1 << 30);
    let formats: Vec<_> = transcoder
        .capabilities()
        .formats
        .iter()
        .map(|c| c.format)
        .collect();
    for format in formats {
        let prepared = transcoder
            .materialize(
                &source,
                &profile(ConvertMode::All, format, None),
                &AtomicBool::new(false),
            )
            .unwrap_or_else(|e| panic!("{format:?}: {e}"));
        assert!(matches!(
            prepared.decision,
            Decision::Convert { channels: 1, .. }
        ));
        let decoder = luminous_audio::OfflineDecoder::open_file(&prepared.path).unwrap();
        let rate = decoder.sample_rate();
        let mut frames = 0;
        decoder.decode_blocks(1, |planes| frames += planes[0].len());
        let seconds = frames as f64 / f64::from(rate);
        assert!(
            (seconds - 1.0).abs() < 0.1,
            "{format:?} decoded to {seconds} s"
        );
    }
}
