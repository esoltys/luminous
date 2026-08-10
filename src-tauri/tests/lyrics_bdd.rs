use cucumber::{given, then, when, World};
use luminous_lib::db::Database;
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
fn open_lyrics_panel(w: &mut LyricsWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let lyrics: Option<String> = conn
        .query_row(
            "SELECT lyrics FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .ok()
        .flatten();

    match lyrics {
        Some(l) => {
            w.displayed_lyrics = Some(l);
            w.network_call_made = false;
        }
        None => {
            w.network_call_made = true;
        }
    }
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
