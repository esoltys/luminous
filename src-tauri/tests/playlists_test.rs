use luminous_lib::{db::Database, playlist::PlaylistManager};
use std::sync::Arc;
use tempfile::TempDir;

struct PlaylistsFixture {
    _temp_dir: TempDir,
    db: Arc<Database>,
    manager: PlaylistManager,
}

impl PlaylistsFixture {
    fn new() -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let db = Arc::new(Database::new(temp_dir.path().to_path_buf()).expect("failed to init db"));
        let manager =
            PlaylistManager::new(Arc::clone(&db)).expect("failed to init playlist manager");
        Self {
            _temp_dir: temp_dir,
            db,
            manager,
        }
    }

    fn get_titles(&self, pl_id: i64) -> Vec<String> {
        let items = self
            .manager
            .get_playlist_tracks(pl_id)
            .expect("failed to get tracks");
        items
            .into_iter()
            .map(|item| {
                item.song
                    .as_ref()
                    .and_then(|s| s.title.clone())
                    .unwrap_or_default()
            })
            .collect()
    }
}

#[test]
fn test_create_and_delete_playlist() {
    let fixture = PlaylistsFixture::new();

    let pl = fixture
        .manager
        .create_playlist("Study Vibes")
        .expect("failed to create playlist");

    let playlists = fixture
        .manager
        .get_playlists()
        .expect("failed to get playlists");
    assert!(
        playlists.iter().any(|p| p.name == "Study Vibes"),
        "Playlist 'Study Vibes' not found in database"
    );

    fixture
        .manager
        .delete_playlist(pl.id)
        .expect("failed to delete playlist");

    let playlists_after = fixture
        .manager
        .get_playlists()
        .expect("failed to get playlists");
    assert!(
        playlists_after.is_empty(),
        "Expected database to have 0 playlists"
    );
}

#[test]
fn test_playlist_reordering_with_undo_and_redo() {
    let mut fixture = PlaylistsFixture::new();

    let pl = fixture
        .manager
        .create_playlist("My Hits")
        .expect("failed to create playlist");

    let conn = fixture.db.pool.get().expect("failed to get db conn");

    let songs = [(101, "Song Alpha"), (102, "Song Beta"), (103, "Song Gamma")];
    let mut song_ids = Vec::new();

    for (id, title) in songs {
        conn.execute(
            "INSERT OR REPLACE INTO songs (id, title, source, filetype, unavailable)
             VALUES (?1, ?2, 1, 1, 0)",
            rusqlite::params![id, title],
        )
        .unwrap();
        song_ids.push(id);
    }

    fixture
        .manager
        .add_songs_to_playlist(pl.id, &song_ids)
        .expect("failed to add songs");

    // Move track at index 2 ("Song Gamma") to index 0
    fixture
        .manager
        .reorder_playlist_item(pl.id, 2, 0)
        .expect("reorder failed");
    assert_eq!(
        fixture.get_titles(pl.id),
        vec!["Song Gamma", "Song Alpha", "Song Beta"],
        "Track order mismatch after move"
    );

    // Click "Undo"
    fixture.manager.undo().expect("undo failed");
    assert_eq!(
        fixture.get_titles(pl.id),
        vec!["Song Alpha", "Song Beta", "Song Gamma"],
        "Track order mismatch after undo"
    );

    // Click "Redo"
    fixture.manager.redo().expect("redo failed");
    assert_eq!(
        fixture.get_titles(pl.id),
        vec!["Song Gamma", "Song Alpha", "Song Beta"],
        "Track order mismatch after redo"
    );
}
