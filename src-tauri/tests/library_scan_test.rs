use luminous_lib::{collection::CollectionScanner, db::Database};
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Real synthetic audio files (see `scripts/generate_test_fixtures.sh`).
const FIXTURES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/audio");

struct ScanTestEnv {
    _temp_dir: TempDir,
    scan_dir: PathBuf,
    app_data_dir: PathBuf,
    scanner: CollectionScanner,
    db: Arc<Database>,
}

impl ScanTestEnv {
    fn new() -> Self {
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

    async fn run_scan(&self, force: bool) {
        self.scanner
            .scan_all_core(
                self.app_data_dir.clone(),
                force,
                false,
                false,
                |_progress| {},
            )
            .await
            .expect("scan_all_core failed");
    }

    fn copy_fixture(&self, fixture_name: &str, relative_folder: &str) {
        let dir = if relative_folder.is_empty() {
            self.scan_dir.clone()
        } else {
            self.scan_dir.join(relative_folder)
        };
        std::fs::create_dir_all(&dir).expect("failed to create folder");
        std::fs::copy(
            PathBuf::from(FIXTURES_DIR).join(fixture_name),
            dir.join(fixture_name),
        )
        .unwrap_or_else(|e| panic!("failed to copy fixture {fixture_name}: {e}"));
    }

    fn available_titles(&self) -> Vec<String> {
        let conn = self.db.pool.get().expect("db conn failed");
        let mut stmt = conn
            .prepare("SELECT title FROM songs WHERE unavailable = 0")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }
}

#[test]
fn test_add_directory_to_watched_folders() {
    let env = ScanTestEnv::new();
    let music_path = "/home/user/Music";

    env.scanner
        .add_directory(music_path)
        .expect("failed to add directory");

    let dirs = env
        .scanner
        .get_directories()
        .expect("failed to get directories");
    assert!(
        dirs.iter().any(|d| d.path == music_path),
        "Directory {music_path} was not found in DB"
    );

    let paths: Vec<String> = dirs.into_iter().map(|d| d.path).collect();
    assert!(
        paths.contains(&music_path.to_string()),
        "Watched directories list {paths:?} does not contain {music_path}"
    );
}

#[tokio::test]
async fn test_rescan_folders_indexes_audio_files() {
    let env = ScanTestEnv::new();
    env.scanner
        .add_directory(&env.scan_dir.to_string_lossy())
        .expect("failed to add watched directory");

    env.copy_fixture("song_alpha.mp3", "");
    env.copy_fixture("song_beta.wav", "");

    env.run_scan(false).await;

    let songs = env.scanner.get_songs(100, 0).expect("failed to get songs");
    assert_eq!(
        songs.len(),
        2,
        "Expected 2 songs in DB, found {}",
        songs.len()
    );

    let results_alpha = env
        .scanner
        .search_songs("Song Alpha", 10)
        .expect("search failed");
    assert!(
        !results_alpha.is_empty(),
        "Search for 'Song Alpha' returned no results"
    );
    assert_eq!(results_alpha[0].title.as_deref(), Some("Song Alpha"));

    let results_silver = env
        .scanner
        .search_songs("Silver", 10)
        .expect("search failed");
    assert!(
        !results_silver.is_empty(),
        "Search for 'Silver' returned no results"
    );
    assert_eq!(results_silver[0].title.as_deref(), Some("Song Beta"));
}

#[tokio::test]
async fn test_incremental_scan_skips_unmodified_files() {
    let env = ScanTestEnv::new();
    env.scanner
        .add_directory(&env.scan_dir.to_string_lossy())
        .expect("failed to add watched directory");

    env.copy_fixture("song_alpha.mp3", "");

    // Real initial scan
    env.run_scan(true).await;

    // Deliberately corrupt the cached title without touching mtime on disk.
    // If the incremental scan skip logic works, the title remains corrupted.
    let path = env.scan_dir.join("song_alpha.mp3");
    let conn = env.db.pool.get().expect("db conn failed");
    conn.execute(
        "UPDATE songs SET title = 'STALE_TITLE_SHOULD_NOT_CHANGE' WHERE path = ?1",
        rusqlite::params![path.to_string_lossy()],
    )
    .unwrap();

    // Rescan without force
    env.run_scan(false).await;

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

#[tokio::test]
async fn test_scoped_scan_only_adds_files_inside_it() {
    let env = ScanTestEnv::new();
    env.scanner
        .add_directory(&env.scan_dir.to_string_lossy())
        .expect("failed to add watched directory");

    env.copy_fixture("song_alpha.mp3", "Alpha");
    env.run_scan(false).await;

    env.copy_fixture("song_gamma.flac", "Alpha");
    env.copy_fixture("song_beta.wav", "Beta");

    CollectionScanner::new(Arc::clone(&env.db))
        .scoped_to(env.scan_dir.join("Alpha"))
        .scan_all_core(
            env.app_data_dir.clone(),
            false,
            false,
            false,
            |_progress| {},
        )
        .await
        .expect("scoped scan failed");

    let titles = env.available_titles();
    assert_eq!(titles.len(), 2, "Expected 2 songs indexed, got: {titles:?}");
    assert!(titles.contains(&"Song Alpha".to_string()));
    assert!(titles.contains(&"Song Gamma".to_string()));
    assert!(
        !titles.contains(&"Song Beta".to_string()),
        "Song Beta unexpectedly indexed"
    );
}

#[tokio::test]
async fn test_scoped_scan_keeps_songs_elsewhere_in_library() {
    let env = ScanTestEnv::new();
    env.scanner
        .add_directory(&env.scan_dir.to_string_lossy())
        .expect("failed to add watched directory");

    env.copy_fixture("song_alpha.mp3", "Alpha");
    env.copy_fixture("song_beta.wav", "Beta");
    env.run_scan(false).await;

    // Scan only folder "Alpha"
    CollectionScanner::new(Arc::clone(&env.db))
        .scoped_to(env.scan_dir.join("Alpha"))
        .scan_all_core(
            env.app_data_dir.clone(),
            false,
            false,
            false,
            |_progress| {},
        )
        .await
        .expect("scoped scan failed");

    let titles = env.available_titles();
    assert_eq!(titles.len(), 2, "Expected 2 songs in DB, got: {titles:?}");
    assert!(
        titles.contains(&"Song Beta".to_string()),
        "Song Beta should be preserved in DB"
    );
}

#[tokio::test]
async fn test_reserved_folders_never_scanned() {
    let env = ScanTestEnv::new();
    env.scanner
        .add_directory(&env.scan_dir.to_string_lossy())
        .expect("failed to add watched directory");

    env.copy_fixture("song_alpha.mp3", "Alpha");
    env.copy_fixture("song_beta.wav", "Alpha/Duplicates");
    env.run_scan(false).await;

    // Scan only folder "Alpha/Duplicates"
    CollectionScanner::new(Arc::clone(&env.db))
        .scoped_to(env.scan_dir.join("Alpha/Duplicates"))
        .scan_all_core(
            env.app_data_dir.clone(),
            false,
            false,
            false,
            |_progress| {},
        )
        .await
        .expect("scoped scan failed");

    let titles = env.available_titles();
    assert_eq!(titles.len(), 1, "Expected 1 song in DB, got: {titles:?}");
    assert!(
        !titles.contains(&"Song Beta".to_string()),
        "Song Beta in Duplicates should not be indexed"
    );
}
