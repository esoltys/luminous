//! Property tests for `Player`'s index bookkeeping (#1226).
//!
//! Random operation sequences run against the real `Player`, and after every
//! step the invariants the Lean models in `verification/lean/Luminous/Player.lean`
//! rely on are checked (`WF`, `Synced`, `historyItems`). The Lean proofs show
//! the modelled logic is correct; these tests catch the Rust drifting away
//! from what the models describe.

use super::tests::setup_test_db;
use super::*;
use proptest::prelude::*;

const SONGS: i64 = 8;

#[derive(Debug, Clone)]
enum Op {
    Play {
        start: usize,
    },
    Next,
    Previous,
    TrackFinished,
    Shuffle(ShuffleMode),
    Repeat(RepeatMode),
    Reorder {
        from: usize,
        to: usize,
    },
    Remove {
        at: usize,
    },
    Append {
        song: i64,
    },
    /// Pushes onto the ad-hoc "play next" `queue` directly — nothing in
    /// production populates it today, but `next_track` and gapless still
    /// consume it.
    QueueNext {
        song: i64,
    },
}

fn op() -> impl Strategy<Value = Op> {
    let shuffle = prop_oneof![
        Just(ShuffleMode::Off),
        Just(ShuffleMode::All),
        Just(ShuffleMode::InsideAlbum),
        Just(ShuffleMode::Albums),
        Just(ShuffleMode::Artists),
    ];
    let repeat = prop_oneof![
        Just(RepeatMode::Off),
        Just(RepeatMode::Track),
        Just(RepeatMode::Album),
        Just(RepeatMode::Playlist),
    ];
    prop_oneof![
        1 => (0..SONGS as usize).prop_map(|start| Op::Play { start }),
        4 => Just(Op::Next),
        3 => Just(Op::Previous),
        2 => Just(Op::TrackFinished),
        2 => shuffle.prop_map(Op::Shuffle),
        1 => repeat.prop_map(Op::Repeat),
        2 => (0..16usize, 0..16usize).prop_map(|(from, to)| Op::Reorder { from, to }),
        1 => (0..16usize).prop_map(|at| Op::Remove { at }),
        1 => (1..=SONGS).prop_map(|song| Op::Append { song }),
        1 => (1..=SONGS).prop_map(|song| Op::QueueNext { song }),
    ]
}

/// The playlist items `played_indices` point back to (by uuid).
fn history_uuids(player: &Player) -> Vec<String> {
    player
        .played_indices
        .iter()
        .filter_map(|&v| player.resolve_item_index(v))
        .map(|r| player.playlist_items[r].uuid.clone())
        .collect()
}

fn check_invariants(player: &Player, after: &Op) -> Result<(), TestCaseError> {
    let len = player.playlist_items.len();

    // WF: shuffle_order is a permutation of 0..len — the identity while
    // shuffle is off.
    let mut sorted = player.shuffle_order.clone();
    sorted.sort_unstable();
    prop_assert_eq!(
        sorted,
        (0..len).collect::<Vec<_>>(),
        "shuffle_order is not a permutation after {:?}",
        after
    );
    if player.shuffle_mode == ShuffleMode::Off {
        prop_assert_eq!(
            &player.shuffle_order,
            &(0..len).collect::<Vec<_>>(),
            "shuffle_order is not the identity with shuffle off after {:?}",
            after
        );
    }

    // Synced: when the loaded song is a playlist item, current_index
    // designates exactly that item.
    let playing_real = player
        .current_item_uuid
        .as_deref()
        .and_then(|u| player.playlist_items.iter().position(|i| i.uuid == u));
    if let (Some(real), Some(virtual_idx)) = (playing_real, player.current_index) {
        prop_assert_eq!(
            player.resolve_item_index(virtual_idx),
            Some(real),
            "current_index does not designate the playing item after {:?}",
            after
        );
    }

    // Every history entry designates some item.
    for &v in &player.played_indices {
        prop_assert!(
            player.resolve_item_index(v).is_some(),
            "played_indices entry {} is out of range after {:?}",
            v,
            after
        );
    }
    Ok(())
}

async fn apply(player: &mut Player, op: &Op, songs: &[Song]) {
    let len = player.playlist_items.len();
    let item = |id: i64| PlaylistItem::new_song(0, 0, songs[(id - 1) as usize].clone());
    match *op {
        Op::Play { start } => {
            let items = (1..=SONGS).map(item).collect();
            let _ = player.play_playlist(items, start, 0, None).await;
        }
        Op::Next => {
            let _ = player.next_track().await;
        }
        Op::Previous => {
            let _ = player.previous_track().await;
        }
        Op::TrackFinished => {
            let _ = player.on_track_finished().await;
        }
        Op::Shuffle(mode) => player.set_shuffle_mode(mode),
        Op::Repeat(mode) => player.set_repeat_mode(mode),
        Op::Reorder { from, to } if len > 0 => player.reorder_playlist_items(from % len, to % len),
        Op::Remove { at } if len > 0 => {
            let uuid = player.playlist_items[at % len].uuid.clone();
            player.remove_songs_from_playlist_items(&[uuid]);
        }
        Op::Append { song } => player.append_songs_to_playlist_items(vec![item(song)]),
        Op::QueueNext { song } => player.queue.push_back(item(song)),
        Op::Reorder { .. } | Op::Remove { .. } => {}
    }
}

fn run(unavailable: Vec<bool>, ops: Vec<Op>) -> Result<(), TestCaseError> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let (db, temp_dir) = setup_test_db();
        let db = Arc::new(db);
        {
            let conn = db.pool.get().unwrap();
            for id in 1..=SONGS {
                conn.execute(
                    "INSERT INTO songs (id, path, title, artist, album, length_nanosec, unavailable) VALUES (?1, ?2, ?3, ?4, ?5, 180000000000, ?6)",
                    rusqlite::params![
                        id,
                        format!("/fake/path{id}.mp3"),
                        format!("Track {id}"),
                        format!("Artist {}", id % 2),
                        format!("Album {}", id % 3),
                        unavailable[(id - 1) as usize],
                    ],
                )
                .unwrap();
            }
        }
        let songs: Vec<Song> = {
            let conn = db.pool.get().unwrap();
            let sql = format!(
                "SELECT {} FROM songs WHERE id = ?1",
                crate::collection::SONG_SELECT_COLS
            );
            (1..=SONGS)
                .map(|id| {
                    conn.query_row(&sql, rusqlite::params![id], crate::collection::row_to_song)
                        .unwrap()
                })
                .collect()
        };
        let audio = Arc::new(Mutex::new(AudioEngine::new()));
        let mut player = Player::new(db, audio);

        let result = async {
            for op in &ops {
                let history_before = history_uuids(&player);
                apply(&mut player, op, &songs).await;
                check_invariants(&player, op)?;

                // historyItems: changing the play order (re-shuffling,
                // reordering, appending) or the repeat mode never changes
                // which songs Previous walks back through.
                if matches!(
                    op,
                    Op::Shuffle(_) | Op::Repeat(_) | Op::Reorder { .. } | Op::Append { .. }
                ) {
                    prop_assert_eq!(
                        history_uuids(&player),
                        history_before,
                        "Previous history changed after {:?}",
                        op
                    );
                }
            }
            Ok(())
        }
        .await;

        let _ = std::fs::remove_dir_all(temp_dir);
        result
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn player_index_invariants_hold(
        unavailable in proptest::collection::vec(prop::bool::weighted(0.2), SONGS as usize),
        ops in proptest::collection::vec(op(), 1..40),
    ) {
        run(unavailable, ops)?;
    }
}
