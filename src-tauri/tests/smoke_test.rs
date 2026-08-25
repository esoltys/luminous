//! End-to-end smoke test for the core pipeline: tag writing, library/db,
//! playlists, the equalizer, and real audio playback through an actual
//! output device.
//!
//! Not run by `cargo test` by default (`#[ignore]`) — it opens a real CPAL
//! stream, so it needs a working audio device and is meant to be run by hand
//! before cutting a release:
//!
//!   cargo test --test smoke_test -- --ignored --nocapture --test-threads=1

use luminous_lib::{
    audio::AudioEngine, db::Database, equalizer::Equalizer, models::PlayState, player::Player,
    playlist::PlaylistManager, tageditor,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

/// Writes a minimal 16-bit PCM WAV file containing a sine tone, so the test
/// doesn't need a binary audio fixture checked into the repo.
fn write_test_wav(path: &std::path::Path, seconds: f32) {
    let sample_rate = 44_100u32;
    let channels = 2u16;
    let bits_per_sample = 16u16;
    let num_frames = (sample_rate as f32 * seconds) as u32;

    let mut data = Vec::with_capacity((num_frames * channels as u32 * 2) as usize);
    for i in 0..num_frames {
        let t = i as f32 / sample_rate as f32;
        let sample = ((t * 440.0 * std::f32::consts::TAU).sin() * 0.2 * i16::MAX as f32) as i16;
        data.extend_from_slice(&sample.to_le_bytes());
        data.extend_from_slice(&sample.to_le_bytes());
    }

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

    std::fs::write(path, wav).expect("failed to write smoke-test wav fixture");
}

#[tokio::test]
#[ignore = "opens a real audio device; run manually before release: cargo test --test smoke_test -- --ignored --nocapture"]
async fn smoke_full_pipeline() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let song_path = temp_dir.path().join("smoke_test_tone.wav");
    // Long enough that the cumulative sleeps below (start + seek + pause +
    // resume) can't run past the end of the track and turn a real "the song
    // finished" Stopped transition into a flaky pause/resume assertion.
    write_test_wav(&song_path, 6.0);

    // --- Tag writing (real lofty write to a real file on disk) ---
    tageditor::write_tags(
        &song_path,
        "Smoke Test Tone",
        None,
        "Smoke Test Artist",
        None,
        "Smoke Test Album",
        None,
        "",
        None,
        "",
        None,
        "",
        None,
        None,
        None,
        None,
        "",
        None,
        "",
        false,
    )
    .expect("write_tags should succeed against a real audio file");
    println!("[ok] tag writing (lofty)");

    // --- Database + a library row pointing at the real file ---
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("db init"));
    {
        let conn = db.pool.get().expect("db conn");
        conn.execute(
            "INSERT INTO songs (id, path, title, artist, album, length_nanosec, source, filetype, unavailable)
             VALUES (1, ?1, 'Smoke Test Tone', 'Smoke Test Artist', 'Smoke Test Album', 2000000000, 1, 1, 0)",
            rusqlite::params![song_path.to_string_lossy()],
        )
        .expect("insert song");
    }
    println!("[ok] database + song row");

    // --- Playlist creation and membership ---
    let mut playlists = PlaylistManager::new(db.clone()).expect("playlist manager");
    let playlist = playlists
        .create_playlist("Smoke Test Playlist")
        .expect("create playlist");
    playlists
        .add_songs_to_playlist(playlist.id, &[1])
        .expect("add song to playlist");
    let tracks = playlists
        .get_playlist_tracks(playlist.id)
        .expect("get playlist tracks");
    assert_eq!(tracks.len(), 1, "playlist should contain the song we added");
    println!("[ok] playlist create/add/list");

    // --- Equalizer actually processes samples when enabled ---
    let mut eq = Equalizer::new();
    eq.update_format(44_100, 2);
    eq.enabled = true;
    eq.set_preamp(3.0);
    eq.recalculate();
    let mut samples = vec![0.3f32; 200];
    let original = samples.clone();
    eq.process_interleaved(&mut samples);
    assert!(
        samples
            .iter()
            .zip(original.iter())
            .any(|(a, b)| (a - b).abs() > 0.0001),
        "equalizer should audibly modify samples when enabled"
    );
    println!("[ok] equalizer processing");

    // --- Real playback through the audio engine (actual CPAL device) ---
    let audio = Arc::new(Mutex::new(AudioEngine::new()));
    let mut player = Player::new(db.clone(), audio.clone());

    player
        .play_playlist(tracks, 0, playlist.id, None)
        .await
        .expect("playback should start on real hardware");
    // Opening a real CPAL output device and starting the decode thread isn't
    // instant — give it a moment before asserting position has advanced.
    tokio::time::sleep(Duration::from_millis(1500)).await;

    let state = player.get_state().await;
    assert_eq!(
        state.state,
        PlayState::Playing,
        "player should report Playing after starting playback"
    );
    assert!(
        state.position_nanosec > 0,
        "playback position should be advancing"
    );
    println!("[ok] playback start ({}ns elapsed)", state.position_nanosec);

    player.set_volume(0.5).await.expect("set_volume");
    println!("[ok] volume control");

    player.seek_to(1_000_000_000).await.expect("seek_to");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let state = player.get_state().await;
    assert!(
        state.position_nanosec >= 1_000_000_000,
        "seek should move playback position forward"
    );
    println!("[ok] seek");

    // Pausing/resuming fades over the app's default 300ms, per
    // fade_pause_duration_ms in models::FadeSettings — wait it out before
    // asserting the post-fade state.
    player.pause().await.expect("pause");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let state = player.get_state().await;
    assert_eq!(state.state, PlayState::Paused);
    println!("[ok] pause");

    player.resume().await.expect("resume");
    tokio::time::sleep(Duration::from_millis(500)).await;
    let state = player.get_state().await;
    assert_eq!(state.state, PlayState::Playing);
    println!("[ok] resume");

    player.stop().await.expect("stop");
    println!("[ok] stop");

    println!(
        "\nSMOKE TEST PASSED — tags, library, playlists, equalizer, and real audio playback are all working."
    );
}
