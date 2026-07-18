use cucumber::gherkin::Step;
use cucumber::{given, then, when, World};
use luminous_lib::{db::Database, playlist::PlaylistManager};
use std::sync::Arc;
use tempfile::TempDir;

#[derive(Debug, World)]
pub struct PlaylistsWorld {
    _temp_dir: TempDir,
    db: Arc<Database>,
    manager: PlaylistManager,
    active_playlist_id: Option<i64>,
}

impl Default for PlaylistsWorld {
    fn default() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        let manager =
            PlaylistManager::new(Arc::clone(&db)).expect("failed to init playlist manager");
        Self {
            _temp_dir: temp_dir,
            db,
            manager,
            active_playlist_id: None,
        }
    }
}

#[given("the playlist manager is initialized")]
fn playlist_manager_initialized(_w: &mut PlaylistsWorld) {}

#[when(expr = "I create a new playlist named {string}")]
fn create_playlist(w: &mut PlaylistsWorld, name: String) {
    let pl = w
        .manager
        .create_playlist(&name)
        .expect("failed to create playlist");
    w.active_playlist_id = Some(pl.id);
}

#[then(expr = "the database should store a new playlist with name {string}")]
fn check_playlist_stored(w: &mut PlaylistsWorld, expected_name: String) {
    let playlists = w.manager.get_playlists().expect("failed to get playlists");
    assert!(
        playlists.iter().any(|p| p.name == expected_name),
        "Playlist with name '{}' not found in database",
        expected_name
    );
}

#[when(expr = "I delete the playlist {string}")]
fn delete_playlist(w: &mut PlaylistsWorld, name: String) {
    let playlists = w.manager.get_playlists().expect("failed to get playlists");
    let target = playlists
        .iter()
        .find(|p| p.name == name)
        .expect("playlist not found");
    w.manager
        .delete_playlist(target.id)
        .expect("failed to delete playlist");
}

#[then("the playlist should be removed from the database")]
fn check_playlist_removed(w: &mut PlaylistsWorld) {
    let playlists = w.manager.get_playlists().expect("failed to get playlists");
    assert!(
        playlists.is_empty(),
        "Expected database to have 0 playlists"
    );
}

#[given(expr = "a playlist {string} containing:")]
fn create_playlist_with_tracks(w: &mut PlaylistsWorld, name: String, step: &Step) {
    let conn = w.db.pool.get().expect("failed to get db conn");

    let pl = w
        .manager
        .create_playlist(&name)
        .expect("failed to create playlist");
    w.active_playlist_id = Some(pl.id);

    let table = step.table.as_ref().expect("expected table");
    let mut song_ids = Vec::new();

    for row in table.rows.iter().skip(1) {
        let song_id: i64 = row[1].parse().unwrap();
        let title = &row[2];

        conn.execute(
            "INSERT OR REPLACE INTO songs (id, title, source, filetype, unavailable)
             VALUES (?1, ?2, 1, 1, 0)",
            rusqlite::params![song_id, title],
        )
        .unwrap();

        song_ids.push(song_id);
    }

    w.manager
        .add_songs_to_playlist(pl.id, &song_ids)
        .expect("failed to add songs");
}

#[when(expr = "I move track at index {int} to index {int}")]
fn move_track(w: &mut PlaylistsWorld, from: i32, to: i32) {
    let pl_id = w.active_playlist_id.expect("no active playlist");
    w.manager
        .reorder_playlist_item(pl_id, from, to)
        .expect("reorder failed");
}

fn assert_track_order(w: &mut PlaylistsWorld, step: &Step) {
    let pl_id = w.active_playlist_id.expect("no active playlist");
    let items = w
        .manager
        .get_playlist_tracks(pl_id)
        .expect("failed to get tracks");

    let table = step.table.as_ref().expect("expected table");
    for (idx, row) in table.rows.iter().skip(1).enumerate() {
        let expected_title = &row[1];
        let actual_item = &items[idx];
        let actual_title = actual_item
            .song
            .as_ref()
            .and_then(|s| s.title.as_deref())
            .unwrap_or("");
        assert_eq!(
            actual_title, expected_title,
            "Track at index {} expected '{}', got '{}'",
            idx, expected_title, actual_title
        );
    }
}

#[then("the playlist track order should become:")]
fn track_order_becomes(w: &mut PlaylistsWorld, step: &Step) {
    assert_track_order(w, step);
}

#[when("I click the \"Undo\" button")]
fn click_undo(w: &mut PlaylistsWorld) {
    w.manager.undo().expect("undo failed");
}

#[then("the playlist track order should restore to:")]
fn track_order_restores(w: &mut PlaylistsWorld, step: &Step) {
    assert_track_order(w, step);
}

#[when("I click the \"Redo\" button")]
fn click_redo(w: &mut PlaylistsWorld) {
    w.manager.redo().expect("redo failed");
}

#[then("the playlist track order should apply the move again:")]
fn track_order_reapplies(w: &mut PlaylistsWorld, step: &Step) {
    assert_track_order(w, step);
}

#[tokio::main]
async fn main() {
    PlaylistsWorld::run("../features/playlists.feature").await;
}
