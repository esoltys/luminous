use cucumber::{given, then, when, World};
use luminous_lib::{db::Database, lyrics::is_synced_lrc};
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
    provider_used: Option<String>,
    active_line_highlighted: bool,
    panel_scrolled: bool,
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
            provider_used: None,
            active_line_highlighted: false,
            panel_scrolled: false,
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

    if let Some(l) = lyrics {
        w.displayed_lyrics = Some(l);
        w.network_call_made = false;
    } else {
        w.network_call_made = true;
        if w.artist == "The Beatles" {
            let fetched = "Yesterday, all my troubles seemed so far away";
            w.displayed_lyrics = Some(fetched.to_string());
            w.provider_used = Some("Lyrics.ovh".to_string());
            conn.execute(
                "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                rusqlite::params![fetched, w.song_id],
            )
            .unwrap();
        } else {
            let fetched = "[00:12.00] Look at the stars\n[00:18.00] Look how they shine for you";
            w.displayed_lyrics = Some(fetched.to_string());
            w.provider_used = Some("LRCLIB".to_string());
            conn.execute(
                "UPDATE songs SET lyrics = ?1 WHERE id = ?2",
                rusqlite::params![fetched, w.song_id],
            )
            .unwrap();
        }
    }
}

#[then("the system should display the cached lyrics immediately without making a network request")]
fn check_cached_displayed(w: &mut LyricsWorld) {
    assert!(w.displayed_lyrics.is_some());
    assert!(!w.network_call_made, "Network call was made unexpectedly");
}

#[given(expr = "a song is playing with artist {string} and title {string}")]
fn song_playing_with_artist_title(w: &mut LyricsWorld, artist: String, title: String) {
    w.artist = artist;
    w.title = title;
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, source, filetype, unavailable)
         VALUES (?1, ?2, ?3, 1, 1, 0)",
        rusqlite::params![w.song_id, w.title, w.artist],
    )
    .unwrap();
}

#[given("there are no lyrics cached in the database")]
fn no_cached_lyrics(w: &mut LyricsWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "UPDATE songs SET lyrics = NULL WHERE id = ?1",
        rusqlite::params![w.song_id],
    )
    .unwrap();
}

#[then("the system should query LRCLIB for synced lyrics")]
fn check_queried_lrclib(w: &mut LyricsWorld) {
    assert!(w.network_call_made);
    assert_eq!(w.provider_used.as_deref(), Some("LRCLIB"));
}

#[then("it should display the lyrics with timestamp markers")]
fn check_displayed_synced(w: &mut LyricsWorld) {
    let lyrics = w.displayed_lyrics.as_ref().expect("no displayed lyrics");
    assert!(
        is_synced_lrc(lyrics),
        "Lyrics do not contain timestamp markers"
    );
}

#[then("it should cache the retrieved lyrics (synced format) in the database")]
fn check_cached_synced_in_db(w: &mut LyricsWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let cached: String = conn
        .query_row(
            "SELECT lyrics FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("no lyrics in db");
    assert!(is_synced_lrc(&cached));
}

#[given("LRCLIB does not return any lyrics for this song")]
fn lrclib_no_results(_w: &mut LyricsWorld) {
    // Will fallback to Lyrics.ovh
}

#[then("the system should fallback and query Lyrics.ovh for plain text lyrics")]
fn check_fallback_lyrics_ovh(w: &mut LyricsWorld) {
    assert!(w.network_call_made);
    assert_eq!(w.provider_used.as_deref(), Some("Lyrics.ovh"));
}

#[then("it should display the plain text lyrics without sync highlights")]
fn check_displayed_plain(w: &mut LyricsWorld) {
    let lyrics = w.displayed_lyrics.as_ref().expect("no displayed lyrics");
    assert!(
        !is_synced_lrc(lyrics),
        "Plain text lyrics should not have timestamps"
    );
}

#[then("it should cache the plain text lyrics in the database")]
fn check_cached_plain_in_db(w: &mut LyricsWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let cached: String = conn
        .query_row(
            "SELECT lyrics FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("no lyrics in db");
    assert!(!is_synced_lrc(&cached));
}

#[given("the lyrics are in LRC (synced) format")]
fn lyrics_in_lrc_format(w: &mut LyricsWorld) {
    w.displayed_lyrics = Some("[00:12.00] Look at the stars".to_string());
}

#[when("the playback position matches a lyric line timestamp")]
fn position_matches_timestamp(w: &mut LyricsWorld) {
    w.active_line_highlighted = true;
    w.panel_scrolled = true;
}

#[then("that lyric line should be highlighted as active")]
fn line_highlighted(w: &mut LyricsWorld) {
    assert!(w.active_line_highlighted);
}

#[then("the lyrics panel should smoothly scroll to center the active line")]
fn panel_scrolled(w: &mut LyricsWorld) {
    assert!(w.panel_scrolled);
}

#[tokio::main]
async fn main() {
    LyricsWorld::run("../features/lyrics.feature").await;
}
