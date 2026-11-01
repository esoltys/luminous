use luminous_lib::{covermanager::CoverManager, db::Database};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Real synthetic audio files (see `scripts/generate_test_fixtures.sh`).
const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio");

#[test]
fn test_extract_embedded_cover_art_on_library_scan() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let cover_manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf());
    let song_id = 1i64;

    let song_path = temp_dir.path().join("song_alpha.mp3");
    std::fs::copy(
        PathBuf::from(FIXTURES_DIR).join("song_alpha.mp3"),
        &song_path,
    )
    .expect("failed to copy fixture");

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, path, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, ?2, 'Song Alpha', 'Artist One', 'Album Gold', 1, 0, 1, 1, 0)",
        rusqlite::params![song_id, song_path.to_string_lossy()],
    )
    .unwrap();

    let resolved_filename = cover_manager
        .extract_embedded_art(&song_path, "Artist One", "Album Gold")
        .expect("extract_embedded_art failed")
        .or_else(|| {
            cover_manager
                .scan_folder_art(&song_path)
                .map(|p| p.to_string_lossy().to_string())
        });

    assert!(resolved_filename.is_some());
    let filename = resolved_filename.unwrap();

    conn.execute(
        "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
        rusqlite::params![filename, song_id],
    )
    .unwrap();

    let (art_auto, art_emb): (String, bool) = conn
        .query_row(
            "SELECT art_automatic, art_embedded FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("song row missing");

    assert!(art_auto.starts_with("album-"));
    assert!(art_emb);

    let cached_path = temp_dir.path().join("covers").join(&art_auto);
    assert!(
        cached_path.exists(),
        "expected cached cover art file at {}",
        cached_path.display()
    );
}

#[test]
fn test_scan_folder_for_cover_art() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let cover_manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf());
    let song_id = 1i64;

    let song_path = temp_dir.path().join("song_gamma.flac");
    std::fs::copy(
        PathBuf::from(FIXTURES_DIR).join("song_gamma.flac"),
        &song_path,
    )
    .expect("failed to copy fixture");

    let cover_file = temp_dir.path().join("cover.jpg");
    std::fs::write(&cover_file, b"fake image bytes").unwrap();

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, path, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, ?2, 'Song Beta', 'Artist Two', 'Album Silver', 0, 1, 1, 1, 0)",
        rusqlite::params![song_id, song_path.to_string_lossy()],
    )
    .unwrap();

    let found = cover_manager
        .scan_folder_art(&song_path)
        .expect("scan_folder_art found nothing");
    assert!(found.to_string_lossy().contains("cover.jpg"));

    conn.execute(
        "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
        rusqlite::params![found.to_string_lossy().to_string(), song_id],
    )
    .unwrap();

    let art_auto: String = conn
        .query_row(
            "SELECT art_automatic FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .expect("art_automatic missing");
    assert!(art_auto.contains("cover.jpg"));
}

#[tokio::test]
async fn test_fetch_remote_cover_art_fallback() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
    let song_id = 1i64;

    let conn = db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, 'Yellow', 'Coldplay', 'Parachutes', 0, 0, 1, 1, 0)",
        rusqlite::params![song_id],
    )
    .unwrap();

    let mock_server = MockServer::start().await;
    let artwork_url_100 = format!("{}/img/100x100bb.jpg", mock_server.uri());

    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "artworkUrl100": artwork_url_100 }]
        })))
        .mount(&mock_server)
        .await;

    let mut cover_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0];
    cover_bytes.extend(std::iter::repeat_n(0u8, 64));
    Mock::given(method("GET"))
        .and(path("/img/600x600bb.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(cover_bytes.clone()))
        .expect(1)
        .mount(&mock_server)
        .await;

    let cover_manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf())
        .with_itunes_base_url(mock_server.uri());

    let result = cover_manager
        .fetch_remote_cover(song_id)
        .await
        .expect("fetch_remote_cover failed");
    assert!(
        result.is_some(),
        "fetch_remote_cover returned no cover art from the mock server"
    );

    mock_server.verify().await;

    let art_auto: String = conn
        .query_row(
            "SELECT art_automatic FROM songs WHERE id = ?1",
            rusqlite::params![song_id],
            |row| row.get(0),
        )
        .expect("art_automatic missing");
    assert!(art_auto.starts_with("album-"));

    let cached_path = temp_dir.path().join("covers").join(&art_auto);
    let cached_bytes = std::fs::read(&cached_path).unwrap_or_else(|e| {
        panic!(
            "expected cached remote cover art file at {}: {e}",
            cached_path.display()
        )
    });
    assert_eq!(
        cached_bytes, cover_bytes,
        "cached file content doesn't match the bytes served by the mock artwork endpoint"
    );
}
