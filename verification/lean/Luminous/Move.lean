/-!
# Single-item move (`Vec::remove` + `Vec::insert`)

Models the reorder primitive shared by

* `Player::reorder_playlist_items` (`src-tauri/src/player.rs`), which moves an
  item in the live `playlist_items` and rewrites every `shuffle_order` entry
  with the index-shift arithmetic captured here as `remap`, and
* `PlaylistOp::Move` in `src-tauri/src/playlist/mutation_undo.rs`, whose undo
  is the reverse move `to → from`.
-/

namespace Luminous

/-- `let x = v.remove(f); v.insert(t, x)` — a no-op when either index is out
of range, matching the guard in `reorder_playlist_items`. -/
def move (l : List α) (f t : Nat) : List α :=
  if h : f < l.length ∧ t < l.length then (l.eraseIdx f).insertIdx t l[f] else l

/-- Where the item previously at index `i` ends up after `move · f t`: the
exact `if/else if` chain `reorder_playlist_items` applies to each
`shuffle_order` entry. -/
def remap (f t i : Nat) : Nat :=
  if i = f then t
  else if f < t ∧ f < i ∧ i ≤ t then i - 1
  else if t < f ∧ t ≤ i ∧ i < f then i + 1
  else i

theorem remap_lt {f t i n : Nat} (hf : f < n) (ht : t < n) (hi : i < n) :
    remap f t i < n := by
  unfold remap; repeat' split <;> try omega

/-- The reverse move undoes the index shift (for *every* `i`, not just in-range
ones), so `remap f t` is injective. -/
theorem remap_remap (f t i : Nat) : remap t f (remap f t i) = i := by
  unfold remap; repeat' split <;> try omega

theorem remap_injective {f t a b : Nat} (h : remap f t a = remap f t b) : a = b := by
  have := congrArg (remap t f) h
  simpa [remap_remap] using this

theorem length_move (l : List α) (f t : Nat) : (move l f t).length = l.length := by
  unfold move
  split
  · rename_i h
    rw [List.length_insertIdx, List.length_eraseIdx_of_lt h.1]
    split <;> omega
  · rfl

/-- **Soundness of the shuffle-order rewrite**: after the move, the item that
was at index `i` is found at `remap f t i`. This is what makes rewriting
`shuffle_order` with `remap` keep every entry pointing at the same song. -/
theorem getElem?_move {l : List α} {f t i : Nat}
    (hf : f < l.length) (ht : t < l.length) (hi : i < l.length) :
    (move l f t)[remap f t i]? = l[i]? := by
  have h : f < l.length ∧ t < l.length := ⟨hf, ht⟩
  simp only [move, h, and_self, dite_true]
  rw [List.getElem?_insertIdx]
  have hlen := List.length_eraseIdx_of_lt hf
  unfold remap
  by_cases h1 : i = f
  · subst h1
    have : t ≤ (l.eraseIdx i).length := by omega
    simp [this]
  · simp only [h1, if_false]
    repeat' split
    all_goals first
      | omega
      | (rw [List.getElem?_eraseIdx]; split <;> first | omega | (congr 1; try omega))

theorem move_of_not_lt {l : List α} {f t : Nat} (h : ¬ (f < l.length ∧ t < l.length)) :
    move l f t = l := by
  simp [move, h]

/-- Undoing a `PlaylistOp::Move { from, to }` by replaying `to → from`
restores the original list exactly. -/
theorem move_move (l : List α) {f t : Nat} (hf : f < l.length) (ht : t < l.length) :
    move (move l f t) t f = l := by
  have hlen := length_move l f t
  apply List.ext_getElem?
  intro j
  by_cases hj : j < l.length
  · have e := getElem?_move (l := move l f t) (f := t) (t := f) (i := remap f t j)
      (hlen ▸ ht) (hlen ▸ hf) (hlen ▸ remap_lt hf ht hj)
    rw [remap_remap] at e
    rw [e, getElem?_move hf ht hj]
  · rw [List.getElem?_eq_none (by rw [length_move, hlen]; omega),
        List.getElem?_eq_none (by omega)]

/-- …and so does replaying an out-of-range move, which is a no-op both ways. -/
theorem move_move' (l : List α) (f t : Nat) : move (move l f t) t f = l := by
  by_cases h : f < l.length ∧ t < l.length
  · exact move_move l h.1 h.2
  · rw [move_of_not_lt h, move_of_not_lt (by omega)]

end Luminous
