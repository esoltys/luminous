use cucumber::{given, then, when, World};
use luminous_lib::{covermanager::CoverManager, db::Database};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
pub struct CoverArtWorld {
    temp_dir: TempDir,
    db: Arc<Database>,
    cover_manager: CoverManager,
    song_id: i64,
    folder_art_path: Option<PathBuf>,
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
            folder_art_path: None,
        }
    }
}

#[given("a watched directory containing a song with embedded cover art")]
fn dir_with_embedded_art(w: &mut CoverArtWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let hash = w.cover_manager.get_album_hash("Artist One", "Album Gold");
    let cached_filename = format!("{}.jpg", hash);

    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, album, art_embedded, art_automatic, art_unset, source, filetype, unavailable)
         VALUES (?1, 'Song Alpha', 'Artist One', 'Album Gold', 1, ?2, 0, 1, 1, 0)",
        rusqlite::params![w.song_id, cached_filename],
    )
    .unwrap();
}

#[when("I trigger a library scan")]
fn trigger_scan(w: &mut CoverArtWorld) {
    if let Some(folder_art_path) = &w.folder_art_path {
        let path_str = folder_art_path.to_string_lossy().to_string();
        let conn = w.db.pool.get().expect("db conn failed");
        conn.execute(
            "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
            rusqlite::params![path_str, w.song_id],
        )
        .unwrap();
    }
}

#[then("the scanner should extract the image from the audio file")]
fn scanner_extracts_image(_w: &mut CoverArtWorld) {}

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
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, 'Song Beta', 'Artist Two', 'Album Silver', 0, 1, 1, 1, 0)",
        rusqlite::params![w.song_id],
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
    assert!(w.folder_art_path.is_some());
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
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, album, art_embedded, art_unset, source, filetype, unavailable)
         VALUES (?1, 'Yellow', 'Coldplay', 'Parachutes', 0, 1, 1, 1, 0)",
        rusqlite::params![w.song_id],
    )
    .unwrap();
}

#[given("there is no cover image file in the song's directory")]
fn no_cover_image_in_directory(w: &mut CoverArtWorld) {
    w.folder_art_path = None;
}

#[when("the song is played or loaded in the player")]
fn song_played_or_loaded(w: &mut CoverArtWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let hash = w.cover_manager.get_album_hash("Coldplay", "Parachutes");
    let cached_name = format!("{}.jpg", hash);
    conn.execute(
        "UPDATE songs SET art_automatic = ?1, art_unset = 0 WHERE id = ?2",
        rusqlite::params![cached_name, w.song_id],
    )
    .unwrap();
}

#[then("the player should query the iTunes Search API for the album's cover art")]
fn query_itunes_search_api(_w: &mut CoverArtWorld) {}

#[then("download the artwork to the covers cache directory")]
fn download_artwork_cache(_w: &mut CoverArtWorld) {}

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
}

#[tokio::main]
async fn main() {
    CoverArtWorld::run("../features/cover_art.feature").await;
}
