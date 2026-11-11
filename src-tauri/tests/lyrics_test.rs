use luminous_lib::db::Database;
use luminous_lib::lyrics::{get_lyrics_for_song, LyricsManager};
use std::path::PathBuf;
use std::sync::Arc;

const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio");

#[tokio::test]
async fn test_cached_lyrics_returned_immediately_without_network() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 1i64;
    let expected_lyrics = "[00:12.00] Look at the stars\n[00:18.00] Look how they shine for you";

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, lyrics, source, filetype, unavailable)
         VALUES (?1, 'Yellow', 'Coldplay', ?2, 1, 1, 0)",
        rusqlite::params![song_id, expected_lyrics],
    )
    .unwrap();

    let lyrics_manager = LyricsManager::new();
    let start = std::time::Instant::now();
    let result = get_lyrics_for_song(&db, &lyrics_manager, song_id, false).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "Network call was made unexpectedly"
    );
    let payload = result.expect("get_lyrics_for_song failed");
    assert_eq!(payload.text, expected_lyrics);
}

#[tokio::test]
async fn test_sidecar_lrc_lyrics_returned_without_network() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 2i64;

    let audio_file = temp_dir.path().join("1-02 Big Guns.flac");
    std::fs::write(&audio_file, b"dummy flac content").expect("failed to write audio file");

    let sidecar_file = temp_dir
        .path()
        .join("Dorothy - Gifts From the Holy Ghost - 02 Big Guns.lrc");
    let expected_lyrics = "[00:07.61] Not gonna play the fool, big boss (when you kiss me)\n[00:11.40] Not gonna be the sad wife, babe (like the '50s)";
    std::fs::write(&sidecar_file, expected_lyrics.as_bytes()).expect("failed to write sidecar lrc");

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, path, source, filetype, unavailable, track)
         VALUES (?1, 'Big Guns', 'Dorothy', ?2, 1, 1, 0, 2)",
        rusqlite::params![song_id, audio_file.to_str().unwrap()],
    )
    .unwrap();

    let lyrics_manager = LyricsManager::new();
    let start = std::time::Instant::now();
    let result = get_lyrics_for_song(&db, &lyrics_manager, song_id, false).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "Network call was made unexpectedly"
    );
    let payload = result.expect("get_lyrics_for_song failed");
    assert_eq!(payload.text, expected_lyrics);
}

#[tokio::test]
async fn test_sidecar_srt_lyrics_returned_without_network() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 2i64;

    let audio_file = temp_dir.path().join("1-02 Big Guns.flac");
    std::fs::write(&audio_file, b"dummy flac content").expect("failed to write audio file");

    let sidecar_file = temp_dir.path().join("1-02 Big Guns.srt");
    let srt = "1\r\n00:00:07,610 --> 00:00:11,000\r\n<i>Not gonna play the fool,</i>\r\nbig boss\r\n\r\n2\r\n00:00:11,400 --> 00:00:14,000\r\nNot gonna be the sad wife\r\n";
    std::fs::write(&sidecar_file, srt.as_bytes()).expect("failed to write sidecar srt");

    let expected_lyrics =
        "[00:07.61] Not gonna play the fool, big boss\n[00:11.40] Not gonna be the sad wife";

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, path, source, filetype, unavailable, track)
         VALUES (?1, 'Big Guns', 'Dorothy', ?2, 1, 1, 0, 2)",
        rusqlite::params![song_id, audio_file.to_str().unwrap()],
    )
    .unwrap();

    let lyrics_manager = LyricsManager::new();
    let start = std::time::Instant::now();
    let result = get_lyrics_for_song(&db, &lyrics_manager, song_id, false).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "Network call was made unexpectedly"
    );
    let payload = result.expect("get_lyrics_for_song failed");
    assert_eq!(payload.text, expected_lyrics);
}

#[tokio::test]
async fn test_sidecar_vtt_lyrics_returned_without_network() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 2i64;

    let audio_file = temp_dir.path().join("1-02 Big Guns.flac");
    std::fs::write(&audio_file, b"dummy flac content").expect("failed to write audio file");

    let sidecar_file = temp_dir
        .path()
        .join("Dorothy - Gifts From the Holy Ghost - 02 Big Guns.vtt");
    let vtt = "WEBVTT\n\nNOTE exported by a karaoke tool\n\nline-1\n00:07.610 --> 00:11.000 align:start\n<v Dorothy>Not gonna play the fool, big boss</v>\n\n00:11.400 --> 00:14.000\nNot gonna be the sad wife\n";
    std::fs::write(&sidecar_file, vtt.as_bytes()).expect("failed to write sidecar vtt");

    let expected_lyrics =
        "[00:07.61] Not gonna play the fool, big boss\n[00:11.40] Not gonna be the sad wife";

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, path, source, filetype, unavailable, track)
         VALUES (?1, 'Big Guns', 'Dorothy', ?2, 1, 1, 0, 2)",
        rusqlite::params![song_id, audio_file.to_str().unwrap()],
    )
    .unwrap();

    let lyrics_manager = LyricsManager::new();
    let start = std::time::Instant::now();
    let result = get_lyrics_for_song(&db, &lyrics_manager, song_id, false).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "Network call was made unexpectedly"
    );
    let payload = result.expect("get_lyrics_for_song failed");
    assert_eq!(payload.text, expected_lyrics);
}

#[tokio::test]
async fn test_embedded_synced_lyrics_returned_without_network() {
    use lofty::file::AudioFile;

    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 3i64;

    let audio_file = temp_dir.path().join("song_gamma.flac");
    std::fs::copy(
        PathBuf::from(FIXTURES_DIR).join("song_gamma.flac"),
        &audio_file,
    )
    .expect("failed to copy audio fixture");

    let mut file = std::fs::File::open(&audio_file).unwrap();
    let mut flac_file =
        lofty::flac::FlacFile::read_from(&mut file, lofty::config::ParseOptions::default())
            .unwrap();
    drop(file);

    let mut vorbis = lofty::ogg::tag::VorbisComments::default();
    let expected_lyrics = "[00:05.00] Embedded synced line\n[00:10.00] Second line";
    vorbis.insert(String::from("SYNCEDLYRICS"), expected_lyrics.to_string());
    flac_file.set_vorbis_comments(vorbis);
    flac_file
        .save_to_path(&audio_file, lofty::config::WriteOptions::default())
        .unwrap();

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, path, source, filetype, unavailable, lyrics)
         VALUES (?1, 'Song Gamma', 'Artist', ?2, 1, 1, 0, NULL)",
        rusqlite::params![song_id, audio_file.to_str().unwrap()],
    )
    .unwrap();

    let lyrics_manager = LyricsManager::new();
    let start = std::time::Instant::now();
    let result = get_lyrics_for_song(&db, &lyrics_manager, song_id, false).await;
    let elapsed = start.elapsed();

    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "Network call was made unexpectedly"
    );
    let payload = result.expect("get_lyrics_for_song failed");
    assert_eq!(payload.text, expected_lyrics);
}

#[tokio::test]
async fn test_blocked_lyrics_provider_is_skipped() {
    use luminous_lib::lyrics::{LyricsClient, LyricsEndpoints, LyricsSource};
    use luminous_lib::metadata_blocklist::MetadataBlocklist;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 100i64;

    let server = MockServer::start().await;
    // LRCLIB server mock
    Mock::given(method("GET"))
        .and(path("/api/get"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "syncedLyrics": "[00:01.00] Incorrect LRCLIB lyrics"
        })))
        .mount(&server)
        .await;

    // NetEase server mock
    Mock::given(method("POST"))
        .and(path("/api/cloudsearch/pc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "result": {
                "songs": [{ "id": 9999, "name": "Song Title", "dt": 180000 }]
            }
        })))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/song/lyric"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "lrc": { "lyric": "[00:01.00] Correct NetEase lyrics" }
        })))
        .mount(&server)
        .await;

    let lyrics_manager = LyricsClient::with_endpoints(LyricsEndpoints {
        lrclib_base_url: server.uri(),
        netease_base_url: server.uri(),
        lyrics_ovh_base_url: server.uri(),
    });

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT INTO songs (id, title, artist, source, filetype, unavailable, length_nanosec)
         VALUES (?1, 'Song Title', 'Band', 1, 1, 0, 180000000000)",
        rusqlite::params![song_id],
    )
    .unwrap();

    // Block lrclib for this song
    MetadataBlocklist::reject(&conn, "song", &song_id.to_string(), "lyrics", "lrclib", "*")
        .unwrap();
    drop(conn);

    let result = get_lyrics_for_song(&db, &lyrics_manager, song_id, true)
        .await
        .unwrap();
    assert_eq!(result.source, LyricsSource::Netease);
    assert_eq!(result.text, "[00:01.00] Correct NetEase lyrics");
}
