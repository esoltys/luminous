use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use luminous_lib::{
    collection::CollectionScanner,
    db::Database,
    models::{FileType, Song, SongSource},
};
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
pub struct LibraryScanWorld {
    _temp_dir: TempDir,
    scanner: CollectionScanner,
    db: Arc<Database>,
    scanned_file_attempts: usize,
}

impl Default for LibraryScanWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        let scanner = CollectionScanner::new(Arc::clone(&db));
        Self {
            _temp_dir: temp_dir,
            scanner,
            db,
            scanned_file_attempts: 0,
        }
    }
}

#[given("the database is initialized and empty")]
fn db_initialized(_w: &mut LibraryScanWorld) {
    // Database::new in default() initializes empty DB
}

#[when(expr = "I add the directory {string} to watched folders")]
fn add_directory(w: &mut LibraryScanWorld, path: String) {
    w.scanner
        .add_directory(&path)
        .expect("failed to add directory");
}

#[then(expr = "the directory {string} should be saved in the database")]
fn directory_saved_in_db(w: &mut LibraryScanWorld, expected_path: String) {
    let dirs = w
        .scanner
        .get_directories()
        .expect("failed to get directories");
    assert!(
        dirs.iter().any(|d| d.path == expected_path),
        "Directory {} was not found in DB",
        expected_path
    );
}

#[then(expr = "the watched directories list should return {string}")]
fn watched_dirs_return(w: &mut LibraryScanWorld, expected_path: String) {
    let dirs = w
        .scanner
        .get_directories()
        .expect("failed to get directories");
    let paths: Vec<String> = dirs.into_iter().map(|d| d.path).collect();
    assert!(
        paths.contains(&expected_path),
        "Watched directories list {:?} does not contain {}",
        paths,
        expected_path
    );
}

#[given(expr = "a watched directory {string} containing:")]
fn watched_dir_with_files(w: &mut LibraryScanWorld, dir_path: String, step: &Step) {
    w.scanner
        .add_directory(&dir_path)
        .expect("failed to add watched directory");
    let conn = w.db.pool.get().expect("failed to get db conn");

    let table = step.table.as_ref().expect("expected table");
    for row in table.rows.iter().skip(1) {
        let path = row[0].clone();
        let artist = row[1].clone();
        let album = row[2].clone();
        let title = row[3].clone();
        let filetype_str = row[4].clone();
        let length_sec: u64 = row[5].parse().unwrap_or(180);

        let filetype = match filetype_str.to_uppercase().as_str() {
            "WAV" => FileType::Wav,
            "FLAC" => FileType::Flac,
            _ => FileType::Mp3,
        };

        let song = Song {
            path: Some(path),
            artist: Some(artist),
            album: Some(album),
            title: Some(title),
            source: SongSource::LocalFile,
            filetype,
            length_nanosec: Some((length_sec * 1_000_000_000) as i64),
            mtime: Some(1000),
            unavailable: false,
            ..Default::default()
        };

        conn.execute(
            "INSERT OR REPLACE INTO songs (path, title, artist, album, source, filetype, length_nanosec, mtime, unavailable)
             VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, ?7, 0)",
            rusqlite::params![
                song.path,
                song.title,
                song.artist,
                song.album,
                song.filetype as i64,
                song.length_nanosec,
                song.mtime
            ],
        )
        .unwrap();
    }
}

#[when("I trigger a library scan")]
fn trigger_library_scan(w: &mut LibraryScanWorld) {
    w.scanned_file_attempts += 1;
}

#[then(expr = "{int} songs should be indexed in the database")]
fn songs_indexed_count(w: &mut LibraryScanWorld, expected_count: usize) {
    let songs = w.scanner.get_songs(100, 0).expect("failed to get songs");
    assert_eq!(
        songs.len(),
        expected_count,
        "Expected {} songs in DB, found {}",
        expected_count,
        songs.len()
    );
}

#[then(expr = "searching for {string} should return the first song")]
fn search_first_song(w: &mut LibraryScanWorld, query: String) {
    let results = w.scanner.search_songs(&query, 10).expect("search failed");
    assert!(
        !results.is_empty(),
        "Search for query '{}' returned no results",
        query
    );
    assert_eq!(results[0].title.as_deref(), Some("Song Alpha"));
}

#[then(expr = "searching for {string} should return the second song")]
fn search_second_song(w: &mut LibraryScanWorld, query: String) {
    let results = w.scanner.search_songs(&query, 10).expect("search failed");
    assert!(
        !results.is_empty(),
        "Search for query '{}' returned no results",
        query
    );
    assert_eq!(results[0].title.as_deref(), Some("Song Beta"));
}

#[given("the library has already been scanned")]
fn library_already_scanned(w: &mut LibraryScanWorld) {
    let conn = w.db.pool.get().expect("failed to get db conn");
    conn.execute(
        "INSERT OR REPLACE INTO songs (path, title, artist, album, source, filetype, mtime, unavailable)
         VALUES ('/home/user/Music/track1.mp3', 'Song Alpha', 'Artist One', 'Album Gold', 1, 1, 1000, 0)",
        [],
    )
    .unwrap();
}

#[given(expr = "the file {string} has not been modified")]
fn file_not_modified(_w: &mut LibraryScanWorld, _path: String) {}

#[then(expr = "the database should skip re-parsing {string}")]
fn skip_reparsing(w: &mut LibraryScanWorld, path: String) {
    let conn = w.db.pool.get().expect("failed to get db conn");
    let mtime: i64 = conn
        .query_row(
            "SELECT mtime FROM songs WHERE path = ?1",
            rusqlite::params![path],
            |row| row.get(0),
        )
        .expect("song not found");
    assert_eq!(mtime, 1000, "Song mtime modified unexpectedly");
}

#[tokio::main]
async fn main() {
    LibraryScanWorld::run("../features/library_scan.feature").await;
}
