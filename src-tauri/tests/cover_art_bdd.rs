use cucumber::{given, then, when, World};
use luminous_lib::{covermanager::CoverManager, db::Database};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Real synthetic audio files (see `scripts/generate_test_fixtures.sh`).
const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio");

#[derive(Debug, World)]
pub struct CoverArtWorld {
    temp_dir: TempDir,
    db: Arc<Database>,
    cover_manager: CoverManager,
    song_id: i64,
    song_path: Option<PathBuf>,
    folder_art_path: Option<PathBuf>,
    /// Exact bytes served by the mock iTunes artwork endpoint in
    /// `song_played_or_loaded` — compared byte-for-byte against the cached
    /// file so a bug in the artwork-URL rewrite (which would silently fetch
    /// a 404 from the wrong path instead) can't pass by merely caching
    /// *some* file.
    expected_remote_cover_bytes: Option<Vec<u8>>,
}

impl Default for CoverArtWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        let cover_manager = CoverManager::new(Arc::clone(&db), temp_dir.path().to_path_buf());
        Self {
            temp_dir,
            db,
            cover_manager,
            song_id: 1,
            song_path: None,
            folder_art_path: None,
            expected_remote_cover_bytes: None,
        }
    }
}

#[given("a watched directory containing a song with embedded cover art")]
fn dir_with_embedded_art(w: &mut CoverArtWorld) {
    let song_path = w.temp_dir.path().join("song_alpha.mp3");
    std::fs::copy(
        PathBuf::from(FIXTURES_DIR).join("song_alpha.mp3"),
        &song_path,
    )
    .expect("failed to copy fixture");
    w.song_path = Some(song_path.clone());

    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, path, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, ?2, 'Song Alpha', 'Artist One', 'Album Gold', 1, 0, 1, 1, 0)",
        rusqlite::params![w.song_id, song_path.to_string_lossy()],
    )
    .unwrap();
}

#[when("I trigger a library scan")]
fn trigger_scan(w: &mut CoverArtWorld) {
    let song_path = w.song_path.clone().expect("song_path not set");

    // Mirrors the order `scan_all_core`'s art-resolution phase tries: real
    // embedded art first, falling back to a real folder-art scan.
    let resolved_filename = w
        .cover_manager
        .extract_embedded_art(&song_path, "Artist One", "Album Gold")
        .expect("extract_embedded_art failed")
        .or_else(|| {
            w.cover_manager
                .scan_folder_art(&song_path)
                .map(|p| p.to_string_lossy().to_string())
        });

    if let Some(filename) = resolved_filename {
        let conn = w.db.pool.get().expect("db conn failed");
        conn.execute(
            "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
            rusqlite::params![filename, w.song_id],
        )
        .unwrap();
    }
}

#[then("save it to the covers cache directory with an FNV-1a hash filename")]
fn saves_to_hash_filename(w: &mut CoverArtWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let art_auto: String = conn
        .query_row(
            "SELECT art_automatic FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("art_automatic missing");
    assert!(art_auto.starts_with("album-"));

    let cached_path = w.temp_dir.path().join("covers").join(&art_auto);
    assert!(
        cached_path.exists(),
        "expected cached cover art file at {}",
        cached_path.display()
    );
}

#[then("the song in the database should have \"art_embedded\" set to true")]
fn art_embedded_is_true(w: &mut CoverArtWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let art_emb: bool = conn
        .query_row(
            "SELECT art_embedded FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("art_embedded missing");
    assert!(art_emb);
}

#[then("And \"art_automatic\" set to the cached filename")]
#[then(expr = "\"art_automatic\" set to the cached filename")]
fn art_auto_set_to_cached(w: &mut CoverArtWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let art_auto: Option<String> = conn
        .query_row(
            "SELECT art_automatic FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("art_automatic missing");
    assert!(art_auto.unwrap().starts_with("album-"));
}

#[given("a song without embedded cover art")]
fn song_without_embedded_art(w: &mut CoverArtWorld) {
    let song_path = w.temp_dir.path().join("song_gamma.flac");
    std::fs::copy(
        PathBuf::from(FIXTURES_DIR).join("song_gamma.flac"),
        &song_path,
    )
    .expect("failed to copy fixture");
    w.song_path = Some(song_path.clone());

    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, path, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, ?2, 'Song Beta', 'Artist Two', 'Album Silver', 0, 1, 1, 1, 0)",
        rusqlite::params![w.song_id, song_path.to_string_lossy()],
    )
    .unwrap();
}

#[given("the song's parent directory contains a file named \"cover.jpg\"")]
fn parent_dir_has_cover_jpg(w: &mut CoverArtWorld) {
    let cover_file = w.temp_dir.path().join("cover.jpg");
    std::fs::write(&cover_file, b"fake image bytes").unwrap();
    w.folder_art_path = Some(cover_file);
}

#[then("the scanner should find \"cover.jpg\" in the song's folder")]
fn scanner_finds_cover_jpg(w: &mut CoverArtWorld) {
    let song_path = w.song_path.clone().expect("song_path not set");
    let found = w
        .cover_manager
        .scan_folder_art(&song_path)
        .expect("scan_folder_art found nothing");
    assert!(found.to_string_lossy().contains("cover.jpg"));
}

#[then("the song in the database should have \"art_automatic\" set to the absolute path of \"cover.jpg\"")]
fn art_auto_set_to_absolute_path(w: &mut CoverArtWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let art_auto: String = conn
        .query_row(
            "SELECT art_automatic FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("art_automatic missing");
    assert!(art_auto.contains("cover.jpg"));
}

#[given("a song has no embedded cover art")]
fn song_has_no_embedded_art(w: &mut CoverArtWorld) {
    // art_unset = 0: no art resolved *yet*, but not yet checked either.
    // `fetch_remote_cover` treats art_unset = 1 as "already tried and
    // failed" and short-circuits without hitting the network at all — this
    // scenario is specifically about that first real fetch attempt.
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, 'Yellow', 'Coldplay', 'Parachutes', 0, 0, 1, 1, 0)",
        rusqlite::params![w.song_id],
    )
    .unwrap();
}

#[given("there is no cover image file in the song's directory")]
fn no_cover_image_in_directory(w: &mut CoverArtWorld) {
    w.folder_art_path = None;
}

#[when("the song is played or loaded in the player")]
async fn song_played_or_loaded(w: &mut CoverArtWorld) {
    // Real `fetch_remote_cover` call, pointed at a local mock iTunes Search
    // API server (see `CoverManager::with_itunes_base_url`) so this
    // exercises the real request/parse/download/cache code path without
    // touching the network.
    let mock_server = MockServer::start().await;
    let artwork_url_100 = format!("{}/img/100x100bb.jpg", mock_server.uri());

    Mock::given(method("GET"))
        .and(path("/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "results": [{ "artworkUrl100": artwork_url_100 }]
        })))
        .mount(&mock_server)
        .await;

    // JPEG magic bytes are all `detect_image_format_and_clean` needs to
    // recognize this as a JPEG payload worth caching.
    let mut cover_bytes = vec![0xFF, 0xD8, 0xFF, 0xE0];
    cover_bytes.extend(std::iter::repeat_n(0u8, 64));
    Mock::given(method("GET"))
        .and(path("/img/600x600bb.jpg"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(cover_bytes.clone()))
        .expect(1)
        .mount(&mock_server)
        .await;
    w.expected_remote_cover_bytes = Some(cover_bytes);

    let cover_manager = CoverManager::new(Arc::clone(&w.db), w.temp_dir.path().to_path_buf())
        .with_itunes_base_url(mock_server.uri());

    let result = cover_manager
        .fetch_remote_cover(w.song_id)
        .await
        .expect("fetch_remote_cover failed");
    assert!(
        result.is_some(),
        "fetch_remote_cover returned no cover art from the mock server"
    );

    // `.expect(1)` above only gets checked when the server is dropped, which
    // happens at the end of this function — force that now so a wrong
    // artwork URL (hitting the search mock but never the image mock) fails
    // this step instead of silently passing.
    mock_server.verify().await;
}

#[then("update the database with the cached artwork filename in \"art_automatic\"")]
fn update_db_art_automatic(w: &mut CoverArtWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let art_auto: String = conn
        .query_row(
            "SELECT art_automatic FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("art_automatic missing");
    assert!(art_auto.starts_with("album-"));

    let cached_path = w.temp_dir.path().join("covers").join(&art_auto);
    let cached_bytes = std::fs::read(&cached_path).unwrap_or_else(|e| {
        panic!(
            "expected cached remote cover art file at {}: {e}",
            cached_path.display()
        )
    });
    assert_eq!(
        cached_bytes,
        w.expected_remote_cover_bytes
            .clone()
            .expect("no expected bytes recorded"),
        "cached file content doesn't match the bytes served by the mock artwork endpoint \
         (the code likely fetched the wrong URL)"
    );
}

#[tokio::main]
async fn main() {
    CoverArtWorld::cucumber()
        .max_concurrent_scenarios(4)
        .run_and_exit("../features/cover_art.feature")
        .await;
}
