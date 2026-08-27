use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use luminous_lib::{collection::CollectionScanner, db::Database};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Real synthetic audio files (see `scripts/generate_test_fixtures.sh`) that
/// these scenarios copy into a real watched directory and scan for real via
/// `CollectionScanner::scan_all_core`, instead of hand-seeding DB rows.
const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio");

#[derive(Debug, World)]
pub struct LibraryScanWorld {
    _temp_dir: TempDir,
    scan_dir: PathBuf,
    app_data_dir: PathBuf,
    scanner: CollectionScanner,
    db: Arc<Database>,
}

impl Default for LibraryScanWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        let scanner = CollectionScanner::new(Arc::clone(&db));
        let scan_dir = temp_dir.path().join("music");
        std::fs::create_dir_all(&scan_dir).expect("failed to create scan dir");
        let app_data_dir = temp_dir.path().join("app_data");
        Self {
            _temp_dir: temp_dir,
            scan_dir,
            app_data_dir,
            scanner,
            db,
        }
    }
}

/// Runs the real scan core against `w`'s temp watched directory, with the
/// network-dependent remote-cover-art fallback disabled so scans stay
/// deterministic and offline (local embedded/folder-art resolution still
/// runs for real).
async fn run_scan(w: &LibraryScanWorld, force: bool) {
    w.scanner
        .scan_all_core(w.app_data_dir.clone(), force, false, false, |_progress| {})
        .await
        .expect("scan_all_core failed");
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

#[given("a watched directory containing:")]
fn watched_dir_with_fixtures(w: &mut LibraryScanWorld, step: &Step) {
    w.scanner
        .add_directory(&w.scan_dir.to_string_lossy())
        .expect("failed to add watched directory");

    let table = step.table.as_ref().expect("expected table");
    for row in table.rows.iter().skip(1) {
        let fixture_name = &row[0];
        let src = PathBuf::from(FIXTURES_DIR).join(fixture_name);
        let dest = w.scan_dir.join(fixture_name);
        std::fs::copy(&src, &dest)
            .unwrap_or_else(|e| panic!("failed to copy fixture {fixture_name}: {e}"));
    }
}

#[when("I trigger a library scan")]
async fn trigger_library_scan(w: &mut LibraryScanWorld) {
    run_scan(w, false).await;
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
async fn library_already_scanned(w: &mut LibraryScanWorld) {
    w.scanner
        .add_directory(&w.scan_dir.to_string_lossy())
        .expect("failed to add watched directory");
    let src = PathBuf::from(FIXTURES_DIR).join("song_alpha.mp3");
    let dest = w.scan_dir.join("song_alpha.mp3");
    std::fs::copy(&src, &dest).expect("failed to copy fixture");

    // Real initial scan, so the DB's mtime/tags reflect the actual file.
    run_scan(w, true).await;
}

#[given(expr = "the file {string} has not been modified")]
fn file_not_modified(w: &mut LibraryScanWorld, fixture_name: String) {
    // Deliberately corrupt the cached title without touching the file's
    // mtime on disk. The following scan can only still show this corrupted
    // value if it genuinely skipped re-reading the file's tags — if the
    // skip logic is broken, the next scan overwrites it with the real tag
    // value ("Song Alpha") and the `Then` step below fails.
    let path = w.scan_dir.join(&fixture_name);
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "UPDATE songs SET title = 'STALE_TITLE_SHOULD_NOT_CHANGE' WHERE path = ?1",
        rusqlite::params![path.to_string_lossy()],
    )
    .unwrap();
}

#[then(expr = "the database should skip re-parsing {string}")]
async fn skip_reparsing(w: &mut LibraryScanWorld, fixture_name: String) {
    run_scan(w, false).await;

    let path = w.scan_dir.join(&fixture_name);
    let conn = w.db.pool.get().expect("db conn failed");
    let title: String = conn
        .query_row(
            "SELECT title FROM songs WHERE path = ?1",
            rusqlite::params![path.to_string_lossy()],
            |row| row.get(0),
        )
        .expect("song not found");
    assert_eq!(
        title, "STALE_TITLE_SHOULD_NOT_CHANGE",
        "song was re-parsed despite its mtime being unchanged"
    );
}

#[tokio::main]
async fn main() {
    LibraryScanWorld::cucumber()
        .max_concurrent_scenarios(4)
        .run_and_exit("../features/library_scan.feature")
        .await;
}
