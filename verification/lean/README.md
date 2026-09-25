# Lean models of Luminous state machines

Machine-checked models of the index bookkeeping in `src-tauri/src/player.rs` and the move undo/redo stack in `src-tauri/src/playlist/mutation_undo.rs`. Core Lean only, no Mathlib.

```bash
cd verification/lean
lake build --wfail   # fails if any proof does not check or uses `sorry`
```

The toolchain is pinned in `lean-toolchain`. Install it with [elan](https://github.com/leanprover/elan). CI runs the same command in the `Lean Proofs` job of `.github/workflows/ci.yml`.

| File | Covers |
| --- | --- |
| `Luminous/Move.lean` | `Vec::remove` + `insert` and the `remap` index shift used by `reorder_playlist_items` and `reorder_item_internal`: in-range, injective, correct (`getElem?_move`), and self-inverting (`move_move'`). |
| `Luminous/Player.lean` | `current_index` / `played_indices` vs. the loaded song across `set_shuffle_mode` and `reorder_playlist_items`. |
| `Luminous/Nav.lean` | `get_next_index`, `find_playable_from` (sound and complete), and the linear walk in `previous_track`. |
| `Luminous/UndoRedo.lean` | Undo/redo of `PlaylistOp::Move`: undo after a move, redo∘undo, undo∘redo, and undoing any number of moves all restore the playlist. |

## Findings

Each finding is a `decide`-checked counterexample against a model of the code *before* its fix, and each was reproduced against the real `Player`. All three are now fixed, with regression tests in `src-tauri/src/player.rs`:

1. **`setShuffle_off_desyncs`**: turning shuffle off leaves `current_index` as a position in the discarded shuffle order. Shuffle on, start at track 3 of 6, shuffle off, Next → plays track 2 instead of track 4. Fixed in #1221 (`test_shuffle_off_keeps_next_relative_to_current_track`).
2. **`setShuffle_reshuffle_scrambles_history`**: re-shuffling (switching shuffle modes) never remaps `played_indices`, so Previous walks back through different songs. This happened in 18 of 20 randomized runs. Fixed in #1222 (`test_reshuffle_keeps_previous_history`).
3. **`prev_playlist_does_not_wrap`**: under `RepeatMode::Playlist`, Previous wraps past the start only when the first track is current. With `[unavailable, current, playable]` it does nothing. Fixed in #1223 (`test_previous_wraps_past_unavailable_tracks_with_repeat_playlist`).

`rebuildFixed_ok` / `rebuildFixed_synced` prove the fix now in `rebuild_shuffle_order` for 1 and 2: after building the new order, remap `current_index` and `played_indices` through the items they designate. This applies in the `Off` branch too, and holds for any permutation the shuffle produces.
