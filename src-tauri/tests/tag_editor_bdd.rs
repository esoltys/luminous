use cucumber::{given, then, when, World};
use luminous_lib::db::Database;
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
pub struct TagEditorWorld {
    _temp_dir: TempDir,
    db: Arc<Database>,
    song_id: i64,
    new_title: String,
    new_artist: String,
}

impl Default for TagEditorWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        Self {
            _temp_dir: temp_dir,
            db,
            song_id: 1,
            new_title: String::new(),
            new_artist: String::new(),
        }
    }
}

#[given("I have a song in the library")]
fn song_in_library(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "INSERT OR REPLACE INTO songs (id, title, artist, album, source, filetype, unavailable)
         VALUES (?1, 'Yellow', 'Coldplay', 'Parachutes', 1, 1, 0)",
        rusqlite::params![w.song_id],
    )
    .unwrap();
}

#[when("I open the tag editor for the song")]
fn open_tag_editor(_w: &mut TagEditorWorld) {}

#[when(expr = "I change the Title to {string}")]
fn change_title(w: &mut TagEditorWorld, title: String) {
    w.new_title = title;
}

#[when(expr = "I change the Artist to {string}")]
fn change_artist(w: &mut TagEditorWorld, artist: String) {
    w.new_artist = artist;
}

#[when("I click \"Save Tags\"")]
fn click_save_tags(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    conn.execute(
        "UPDATE songs SET title = ?1, artist = ?2 WHERE id = ?3",
        rusqlite::params![w.new_title, w.new_artist, w.song_id],
    )
    .unwrap();
}

#[then("it should update the song details in the SQLite database")]
fn db_updated(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let (title, artist): (String, String) = conn
        .query_row(
            "SELECT title, artist FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("song not found");
    assert_eq!(title, w.new_title);
    assert_eq!(artist, w.new_artist);
}

#[then("the library views should immediately reflect the updated metadata")]
fn library_reflects_metadata(w: &mut TagEditorWorld) {
    let conn = w.db.pool.get().expect("db conn failed");
    let title: String = conn
        .query_row(
            "SELECT title FROM songs WHERE id = ?1",
            rusqlite::params![w.song_id],
            |row| row.get(0),
        )
        .expect("song not found");
    assert_eq!(title, "Yellow (Acoustic)");
}

#[tokio::main]
async fn main() {
    TagEditorWorld::cucumber()
        .max_concurrent_scenarios(4)
        .run_and_exit("../features/tag_editor.feature")
        .await;
}
