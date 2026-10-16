//! The app's edge of the playback engine (`luminous_audio`). The engine knows
//! nothing about songs, the database, remote sources or Tauri; this module
//! translates between its small types and the app's models: a `Song` becomes
//! a `Track`, `EngineState` becomes `PlayState`, and the engine's output
//! facts are combined with song metadata into the Audio Pipeline panel's
//! `AudioPipelineInfo`. Remote sources are opened by `media_source`.

use crate::models::{
    AudioPipelineInfo, FileType, LoudnessGainSource, PlayState, QualityTier, Song,
};
use luminous_audio::{EngineState, OutputInfo, Track};
use luminous_equalizer::{EqMode, Equalizer};
use std::path::Path;

pub use luminous_audio::{AudioEngine, AudioEvent};

/// Runs a synchronous `AudioEngine` operation while `audio`'s async mutex is
/// held, via `block_in_place` rather than directly — some `AudioEngine`
/// callers (e.g. equalizer preset persistence) do rusqlite work, and running
/// that straight on the tokio worker would both stall the runtime and block
/// every other task waiting on the same mutex for the duration (#1097, #1102).
/// Mirrors `playlist::with_playlists`. Note: this locks `AppState.audio`
/// (`tokio::sync::Mutex<AudioEngine>`), distinct from the `parking_lot::Mutex`
/// fields inside `AudioEngine` used on the allocation-free audio callback path.
pub async fn with_audio<F, R>(audio: &tokio::sync::Mutex<AudioEngine>, f: F) -> R
where
    F: FnOnce(&mut AudioEngine) -> R,
{
    let mut a = audio.lock().await;
    tokio::task::block_in_place(move || f(&mut a))
}

/// The engine's view of a song. The location prefers the library path, then
/// a stream URL, then the source URL; when it's a URL with no extension, the
/// library path's extension still hints the format. A song with none of the
/// three gets an empty location, which the engine reports as a playback
/// `Error` event like any other track that fails to open.
pub fn track_for(song: &Song) -> Track {
    let location = song
        .path
        .as_deref()
        .or(song.stream_url.as_deref())
        .or(song.url.as_deref())
        .unwrap_or_default()
        .to_owned();
    let extension_hint = song
        .path
        .as_deref()
        .and_then(|p| Path::new(p).extension())
        .and_then(|e| e.to_str())
        .map(str::to_owned);
    Track {
        id: song.id,
        location,
        extension_hint,
        end_ns: (song.end_nanosec > 0).then_some(song.end_nanosec as u64),
        length_ns: song.length_nanosec.map(|ns| ns.max(0) as u64),
    }
}

impl From<EngineState> for PlayState {
    fn from(state: EngineState) -> Self {
        match state {
            EngineState::Stopped => PlayState::Stopped,
            EngineState::Playing => PlayState::Playing,
            EngineState::Paused => PlayState::Paused,
        }
    }
}

/// Pure function mapping codec + bitrate + sample rate + bit depth to a `QualityTier` (#1041).
/// - LQ — lossy codec below 256 kbps
/// - SQ — lossy codec at 256 kbps or higher
/// - HQ — lossless codec (standard resolution: <= 48 kHz and <= 16-bit)
/// - Hi-Res — lossless codec with sample rate above 48 kHz or bit depth above 16-bit
pub fn classify_quality_tier(
    filetype: FileType,
    bitrate_kbps: Option<i32>,
    sample_rate: Option<u32>,
    bit_depth: Option<i32>,
) -> QualityTier {
    if filetype.is_lossless() {
        let is_hires = sample_rate.is_some_and(|r| r > 48_000) || bit_depth.is_some_and(|d| d > 16);
        if is_hires {
            QualityTier::HiRes
        } else {
            QualityTier::Hq
        }
    } else {
        let kbps = bitrate_kbps.unwrap_or(0);
        if kbps >= 256 {
            QualityTier::Sq
        } else {
            QualityTier::Lq
        }
    }
}

/// Snapshots the audio pipeline for the playing song (#1041), or `None`
/// when nothing is loaded.
pub fn pipeline_info(
    audio: &AudioEngine,
    song: Option<&Song>,
    loudness_source: LoudnessGainSource,
    loudness_gain_db: Option<f32>,
) -> Option<AudioPipelineInfo> {
    let song = song?;
    let output = audio.output_info();
    Some(audio.with_equalizer(|eq| {
        assemble_pipeline_info(song, loudness_source, loudness_gain_db, eq, output)
    }))
}

fn assemble_pipeline_info(
    song: &Song,
    loudness_source: LoudnessGainSource,
    loudness_gain_db: Option<f32>,
    eq: &Equalizer,
    output: OutputInfo,
) -> AudioPipelineInfo {
    let sample_rate = song.samplerate.map(|r| r as u32);
    let input_codec = match song.filetype {
        FileType::Mp3 => "mp3",
        FileType::Flac | FileType::OggFlac => "flac",
        FileType::OggVorbis => "vorbis",
        FileType::OggOpus => "opus",
        FileType::OggSpeex => "speex",
        FileType::Aac => "aac",
        FileType::Alac => "alac",
        FileType::Aiff => "pcm_s16be",
        FileType::Wav => "pcm_s16le",
        FileType::WavPack => "wavpack",
        FileType::Mpc => "musepack",
        FileType::TrueAudio => "trueaudio",
        FileType::Ape => "monkeys_audio",
        FileType::Dsf | FileType::Dsdiff => "dsd",
        FileType::Asf => "wma",
        FileType::Stream => "stream",
        FileType::Unknown => "unknown",
    };
    // Every format decodes natively inside Symphonia except Opus (#1121),
    // which has no first-party Symphonia decoder and goes through the
    // engine's libopus adapter instead — label it distinctly so the panel
    // doesn't misattribute the decode to Symphonia itself.
    let decoder_name = if song.filetype == FileType::OggOpus {
        "libopus (via Symphonia)".to_string()
    } else {
        format!("Symphonia {} decoder", song.filetype.display_name())
    };
    let resample_rate = sample_rate.filter(|&r| r != output.sample_rate);
    let config = eq.config();
    let eq_mode = match config.mode {
        EqMode::Graphic10 => "10-band Graphic",
        EqMode::Parametric => "Parametric",
    };

    AudioPipelineInfo {
        quality_tier: classify_quality_tier(
            song.filetype,
            song.bitrate,
            sample_rate,
            song.bitdepth,
        ),
        input_source: song.source,
        input_format: song.filetype.display_name().to_string(),
        input_codec: input_codec.to_string(),
        input_bitrate_kbps: song.bitrate,
        input_sample_rate: sample_rate,
        input_bit_depth: song.bitdepth,
        input_channels: song.channels.map(|c| c as u16),
        input_path: song.path.clone().or_else(|| song.url.clone()),
        decoder_name,
        headroom: "32-bit float PCM".to_string(),
        resample_rate: resample_rate.map(|_| output.sample_rate),
        loudness_source,
        loudness_gain_db,
        eq_enabled: config.enabled,
        eq_mode: Some(eq_mode.to_string()),
        eq_preamp_db: Some(config.preamp),
        eq_active_bands_count: eq.active_band_count(),
        limiter: "-1.0 dBTP True Peak limiter".to_string(),
        output_sample_rate: output.sample_rate,
        output_channels: output.channels,
        output_format: "32-bit float PCM".to_string(),
        output_device_name: output.device_name,
        output_backend: output.backend_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_output() -> OutputInfo {
        OutputInfo {
            sample_rate: 48_000,
            channels: 2,
            device_name: "Test DAC".into(),
            backend_name: "CPAL".into(),
        }
    }

    #[test]
    fn track_for_prefers_the_library_path_and_keeps_its_extension_as_hint() {
        let song = Song {
            id: 3,
            path: Some("/music/a.flac".into()),
            url: Some("https://x/stream".into()),
            end_nanosec: 0,
            length_nanosec: Some(5_000),
            ..Song::default()
        };
        let track = track_for(&song);
        assert_eq!(track.id, 3);
        assert_eq!(track.location, "/music/a.flac");
        assert_eq!(track.extension_hint.as_deref(), Some("flac"));
        assert_eq!(track.end_ns, None);
        assert_eq!(track.length_ns, Some(5_000));
    }

    #[test]
    fn track_for_falls_back_to_stream_then_source_url() {
        let streamed = Song {
            stream_url: Some("https://s/1".into()),
            url: Some("https://u/1".into()),
            end_nanosec: 9,
            ..Song::default()
        };
        assert_eq!(track_for(&streamed).location, "https://s/1");
        assert_eq!(track_for(&streamed).end_ns, Some(9));
        assert_eq!(track_for(&Song::default()).location, "");
    }

    /// Pins the `AudioPipelineInfo` JSON the frontend's Audio Pipeline panel
    /// reads. The strings were captured from the assembly as it stood inside
    /// the engine before #1556 moved it here.
    #[test]
    fn pipeline_info_json_is_pinned() {
        let eq = Equalizer::new();
        let song = Song {
            filetype: FileType::Flac,
            bitrate: Some(900),
            samplerate: Some(44_100),
            bitdepth: Some(16),
            channels: Some(2),
            source: crate::models::SongSource::Collection,
            path: Some("/music/a.flac".into()),
            ..Song::default()
        };
        let info = assemble_pipeline_info(
            &song,
            LoudnessGainSource::Analyzed,
            Some(-3.5),
            &eq,
            test_output(),
        );
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"quality_tier":"hq","input_source":"collection","input_format":"FLAC","input_codec":"flac","input_bitrate_kbps":900,"input_sample_rate":44100,"input_bit_depth":16,"input_channels":2,"input_path":"/music/a.flac","decoder_name":"Symphonia FLAC decoder","headroom":"32-bit float PCM","resample_rate":48000,"loudness_source":"analyzed","loudness_gain_db":-3.5,"eq_enabled":false,"eq_mode":"10-band Graphic","eq_preamp_db":0.0,"eq_active_bands_count":0,"limiter":"-1.0 dBTP True Peak limiter","output_sample_rate":48000,"output_channels":2,"output_format":"32-bit float PCM","output_device_name":"Test DAC","output_backend":"CPAL"}"#
        );

        let opus = Song {
            filetype: FileType::OggOpus,
            bitrate: Some(128),
            samplerate: Some(48_000),
            url: Some("https://x/a.opus".into()),
            ..Song::default()
        };
        let info = assemble_pipeline_info(
            &opus,
            LoudnessGainSource::Disabled,
            None,
            &eq,
            test_output(),
        );
        assert_eq!(
            serde_json::to_string(&info).unwrap(),
            r#"{"quality_tier":"lq","input_source":"unknown","input_format":"Opus","input_codec":"opus","input_bitrate_kbps":128,"input_sample_rate":48000,"input_bit_depth":null,"input_channels":null,"input_path":"https://x/a.opus","decoder_name":"libopus (via Symphonia)","headroom":"32-bit float PCM","resample_rate":null,"loudness_source":"disabled","loudness_gain_db":null,"eq_enabled":false,"eq_mode":"10-band Graphic","eq_preamp_db":0.0,"eq_active_bands_count":0,"limiter":"-1.0 dBTP True Peak limiter","output_sample_rate":48000,"output_channels":2,"output_format":"32-bit float PCM","output_device_name":"Test DAC","output_backend":"CPAL"}"#
        );
    }

    #[test]
    fn pipeline_info_is_none_without_a_song() {
        let engine = AudioEngine::new();
        assert!(pipeline_info(&engine, None, LoudnessGainSource::Disabled, None).is_none());
    }

    #[test]
    fn test_classify_quality_tier() {
        // Lossless: Standard (<= 48kHz, <= 16-bit) -> HQ
        assert_eq!(
            classify_quality_tier(FileType::Flac, Some(900), Some(44100), Some(16)),
            QualityTier::Hq
        );
        assert_eq!(
            classify_quality_tier(FileType::Wav, None, Some(48000), Some(16)),
            QualityTier::Hq
        );
        assert_eq!(
            classify_quality_tier(FileType::Alac, Some(850), Some(44100), None),
            QualityTier::Hq
        );

        // Lossless: High-Res (> 48kHz or > 16-bit) -> HiRes
        assert_eq!(
            classify_quality_tier(FileType::Flac, Some(1500), Some(96000), Some(24)),
            QualityTier::HiRes
        );
        assert_eq!(
            classify_quality_tier(FileType::Flac, Some(1100), Some(44100), Some(24)),
            QualityTier::HiRes
        );
        assert_eq!(
            classify_quality_tier(FileType::Aiff, None, Some(88200), Some(16)),
            QualityTier::HiRes
        );
        assert_eq!(
            classify_quality_tier(FileType::Dsf, None, Some(2822400), Some(1)),
            QualityTier::HiRes
        );

        // Lossy: >= 256 kbps -> SQ
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(320), Some(44100), None),
            QualityTier::Sq
        );
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(256), Some(44100), None),
            QualityTier::Sq
        );
        assert_eq!(
            classify_quality_tier(FileType::Aac, Some(256), Some(48000), None),
            QualityTier::Sq
        );
        assert_eq!(
            classify_quality_tier(FileType::OggOpus, Some(320), Some(48000), None),
            QualityTier::Sq
        );

        // Lossy: < 256 kbps -> LQ
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(192), Some(44100), None),
            QualityTier::Lq
        );
        assert_eq!(
            classify_quality_tier(FileType::Mp3, Some(128), Some(44100), None),
            QualityTier::Lq
        );
        assert_eq!(
            classify_quality_tier(FileType::Aac, Some(96), Some(44100), None),
            QualityTier::Lq
        );
        assert_eq!(
            classify_quality_tier(FileType::Unknown, None, None, None),
            QualityTier::Lq
        );
    }
}
