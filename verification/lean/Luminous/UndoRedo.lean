import Luminous.Move

/-!
# Playlist undo/redo for moves (`src-tauri/src/playlist/mutation_undo.rs`)

`reorder_playlist_item` applies `reorder_item_internal(from, to)` — whose SQL
shifts positions in `(from, to]` down / `[to, from)` up and puts the moved row
at `to`, i.e. rewrites every position `i` to `remap from to i` — pushes
`PlaylistOp::Move { from, to }` and clears the redo stack. `undo` pops it and
replays `to → from`; `redo` replays `from → to`. By `getElem?_move`, that
position rewrite is exactly `move`, so the stack is modelled over `move`.
-/

namespace Luminous.UndoRedo

structure Stack (α : Type) where
  items : List α
  /-- `undo_stack` (head = top) -/
  undo : List (Nat × Nat)
  /-- `redo_stack` (head = top) -/
  redo : List (Nat × Nat)

/-- `reorder_playlist_item` -/
def exec (s : Stack α) (f t : Nat) : Stack α :=
  { items := move s.items f t, undo := (f, t) :: s.undo, redo := [] }

def undo (s : Stack α) : Stack α :=
  match s.undo with
  | [] => s
  | (f, t) :: u => { items := move s.items t f, undo := u, redo := (f, t) :: s.redo }

def redo (s : Stack α) : Stack α :=
  match s.redo with
  | [] => s
  | (f, t) :: r => { items := move s.items f t, undo := (f, t) :: s.undo, redo := r }

/-- Undo right after a move restores the playlist exactly. -/
theorem undo_exec (s : Stack α) (f t : Nat) : (undo (exec s f t)).items = s.items := by
  simp [undo, exec, move_move']

/-- Redo re-applies exactly what Undo took back… -/
theorem redo_undo (s : Stack α) (h : s.undo ≠ []) : redo (undo s) = s := by
  cases s with
  | mk items u r =>
    cases u with
    | nil => contradiction
    | cons op u => obtain ⟨f, t⟩ := op; simp [undo, redo, move_move']

/-- …and Undo takes back exactly what Redo re-applied. -/
theorem undo_redo (s : Stack α) (h : s.redo ≠ []) : undo (redo s) = s := by
  cases s with
  | mk items u r =>
    cases r with
    | nil => contradiction
    | cons op r => obtain ⟨f, t⟩ := op; simp [undo, redo, move_move']

def execAll (s : Stack α) : List (Nat × Nat) → Stack α
  | [] => s
  | (f, t) :: ms => execAll (exec s f t) ms

/-- Press Undo `n` times. -/
def undoN : Nat → Stack α → Stack α
  | 0, s => s
  | n + 1, s => undo (undoN n s)

/-- Undoing every recorded move, most recent first, returns the playlist (and
the undo stack) to where it started, however many moves were made. -/
theorem undoN_execAll (s : Stack α) (moves : List (Nat × Nat)) :
    (undoN moves.length (execAll s moves)).items = s.items ∧
    (undoN moves.length (execAll s moves)).undo = s.undo := by
  induction moves generalizing s with
  | nil => exact ⟨rfl, rfl⟩
  | cons m ms ih =>
    obtain ⟨f, t⟩ := m
    obtain ⟨hi, hu⟩ := ih (exec s f t)
    simp only [List.length_cons, undoN, execAll]
    generalize undoN ms.length (execAll (exec s f t) ms) = x at hi hu
    cases x with
    | mk items u r =>
      simp only [exec] at hi hu
      subst hi hu
      simp [undo, move_move']

end Luminous.UndoRedo
