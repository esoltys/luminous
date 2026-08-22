use luminous_lib::{audio::AudioEngine, db::Database, models::PlayState, player::Player};
use std::sync::Arc;
use tokio::sync::Mutex;

fn setup_test_db() -> (Database, std::path::PathBuf) {
    let temp_dir = std::env::temp_dir().join(format!(
        "luminous_resume_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = Database::new(temp_dir.clone()).unwrap();
    (db, temp_dir)
}

#[tokio::test]
async fn test_playback_resume_on_startup() {
    let (db, temp_dir) = setup_test_db();
    let db_arc = Arc::new(db);

    {
        let conn = db_arc.pool.get().unwrap();
        conn.execute(
            "INSERT INTO songs (id, path, title, artist, album, length_nanosec) VALUES (101, '/test/song.flac', 'Restored Track', 'Restored Artist', 'Restored Album', 240000000000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_song_id', '101')",
            [],
        )
        .unwrap();
        conn.execute("INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_position_nanosec', '60000000000')", []).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_playlist_id', '0')",
            [],
        )
        .unwrap();
        conn.execute("INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_item_uuid', 'restored-uuid-999')", []).unwrap();
    }

    let audio = Arc::new(Mutex::new(AudioEngine::new()));
    let player = Player::new(db_arc.clone(), audio.clone());

    let state = player.get_state().await;
    assert_eq!(state.state, PlayState::Paused);
    assert_eq!(state.position_nanosec, 60_000_000_000);
    assert!(state.current_song.is_some());
    let song = state.current_song.unwrap();
    assert_eq!(song.id, 101);
    assert_eq!(song.title.as_deref(), Some("Restored Track"));
    assert_eq!(
        state.playlist_item_uuid.as_deref(),
        Some("restored-uuid-999")
    );
    // By default, loudness normalization is disabled in fresh DB
    assert_eq!(state.loudness_source, luminous_lib::models::LoudnessGainSource::Disabled);
    assert_eq!(state.loudness_gain_db, None);
    let engine_loudness = f32::from_bits(audio.lock().await.loudness_gain.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(engine_loudness, 1.0);

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[tokio::test]
async fn test_playback_resume_with_volume_and_analyzed_loudness() {
    let (db, temp_dir) = setup_test_db();
    let db_arc = Arc::new(db);

    {
        let conn = db_arc.pool.get().unwrap();
        conn.execute(
            "UPDATE loudness_settings SET enabled = 1, target_lufs = -14.0 WHERE id = 1",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO songs (id, path, title, artist, album, length_nanosec, ebur128_integrated_loudness_lufs)
             VALUES (202, '/test/analyzed.flac', 'Analyzed Track', 'Artist', 'Album', 180000000000, -8.0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('volume', '0.5')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_song_id', '202')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_position_nanosec', '30000000000')",
            [],
        ).unwrap();
    }

    let audio = Arc::new(Mutex::new(AudioEngine::new()));
    let player = Player::new(db_arc.clone(), audio.clone());

    let state = player.get_state().await;
    assert_eq!(state.state, PlayState::Paused);
    assert_eq!(state.volume, 0.5);
    assert_eq!(audio.lock().await.current_volume(), 0.5);

    // Target -14.0 LUFS with track at -8.0 LUFS -> -6.0 dB gain
    assert_eq!(state.loudness_source, luminous_lib::models::LoudnessGainSource::Analyzed);
    assert_eq!(state.loudness_gain_db, Some(-6.0));
    let engine_loudness = f32::from_bits(audio.lock().await.loudness_gain.load(std::sync::atomic::Ordering::Relaxed));
    assert!((engine_loudness - 0.5011872).abs() < 1e-4);

    let _ = std::fs::remove_dir_all(temp_dir);
}

#[tokio::test]
async fn test_playback_resume_with_fallback_loudness() {
    let (db, temp_dir) = setup_test_db();
    let db_arc = Arc::new(db);

    {
        let conn = db_arc.pool.get().unwrap();
        conn.execute(
            "UPDATE loudness_settings SET enabled = 1, fallback_gain_db = -6.0 WHERE id = 1",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO songs (id, path, title, artist, album, length_nanosec)
             VALUES (303, '/test/unanalyzed.flac', 'Unanalyzed Track', 'Artist', 'Album', 180000000000)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO app_state (key, value) VALUES ('last_song_id', '303')",
            [],
        ).unwrap();
    }

    let audio = Arc::new(Mutex::new(AudioEngine::new()));
    let player = Player::new(db_arc.clone(), audio.clone());

    let state = player.get_state().await;
    assert_eq!(state.state, PlayState::Paused);
    assert_eq!(state.loudness_source, luminous_lib::models::LoudnessGainSource::Fallback);
    assert_eq!(state.loudness_gain_db, Some(-6.0));
    let engine_loudness = f32::from_bits(audio.lock().await.loudness_gain.load(std::sync::atomic::Ordering::Relaxed));
    assert!((engine_loudness - 0.5011872).abs() < 1e-4);

    let _ = std::fs::remove_dir_all(temp_dir);
}
