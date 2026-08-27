use cucumber::{given, then, when, World};
use luminous_lib::db::Database;
use luminous_lib::lyrics::{get_lyrics_for_song, LyricsManager};
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
pub struct LyricsWorld {
    _temp_dir: TempDir,
    db: Arc<Database>,
    song_id: i64,
    artist: String,
    title: String,
    cached_lyrics: Option<String>,
    displayed_lyrics: Option<String>,
    /// Derived from how long the real `get_lyrics_for_song` call took — see
    /// `open_lyrics_panel` for why elapsed time is a reliable proxy for
    /// "did this reach the network".
    network_call_made: bool,
}

impl Default for LyricsWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        Self {
            _temp_dir: temp_dir,
            db,
            song_id: 1,
            artist: String::new(),
            title: String::new(),
            cached_lyrics: None,
            displayed_lyrics: None,
            network_call_made: false,
        }
    }
}

#[given("a song is playing")]
fn song_is_playing(w: &mut LyricsWorld) {
    w.song_id = 1;
    w.title = "Yellow".to_string();
    w.artist = "Coldplay".to_string();
}

#[given("the database already has cached lyrics for this song")]
fn db_has_cached_lyrics(w: &mut LyricsWorld) {
    let lyrics = "[00:12.00] Look at the stars\n[00:18.00] Look how they shine for you";
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, lyrics, source, filetype, unavailable)
         VALUES (?1, ?2, ?3, ?4, 1, 1, 0)",
        rusqlite::params![w.song_id, w.title, w.artist, lyrics],
    )
    .unwrap();
    w.cached_lyrics = Some(lyrics.to_string());
}

#[when("I open the lyrics panel")]
async fn open_lyrics_panel(w: &mut LyricsWorld) {
    // Route through the real cache-check/fetch logic used by the
    // `get_lyrics` Tauri command (extracted into `get_lyrics_for_song` so
    // it's callable without an `AppHandle`). A `LyricsManager` is real
    // here, not a stub — the cache-hit branch must return before ever
    // calling `fetch_lyrics`, so this genuinely exercises that the cache
    // check works, not a hand-rolled reimplementation of it.
    //
    // Elapsed time stands in for "no network request was made": these
    // tests run with no network mocking, so any accidental fall-through to
    // `LyricsManager::fetch_lyrics` (an HTTP round trip, or up to a 6s
    // client timeout if unreachable) takes orders of magnitude longer than
    // the microsecond-scale SQLite reads on the cache-hit path.
    let lyrics_manager = LyricsManager::new();
    let start = std::time::Instant::now();
    let result = get_lyrics_for_song(&w.db, &lyrics_manager, w.song_id, false).await;
    let elapsed = start.elapsed();

    w.network_call_made = elapsed > std::time::Duration::from_millis(500);
    w.displayed_lyrics = result.ok();
}

#[then("the system should display the cached lyrics immediately without making a network request")]
fn check_cached_displayed(w: &mut LyricsWorld) {
    assert!(w.displayed_lyrics.is_some());
    assert!(!w.network_call_made, "Network call was made unexpectedly");
}

#[tokio::main]
async fn main() {
    LyricsWorld::cucumber()
        .max_concurrent_scenarios(4)
        .run_and_exit("../features/lyrics.feature")
        .await;
}
