/-!
# Next / Previous navigation (`src-tauri/src/player.rs`)

Pure index arithmetic over the active play order, with items abstracted to a
`playable : Nat → Bool` predicate (`is_playable_at`: present and not flagged
`unavailable`). `total` is `virtual_len()`.
-/

namespace Luminous.Nav

inductive RepeatMode | off | track | album | playlist
deriving DecidableEq, Repr

/-! ## `get_next_index` -/

def nextIndex (total : Nat) (cur : Option Nat) (rep : RepeatMode) : Option Nat :=
  if total = 0 then none
  else match cur with
    | none => none
    | some c => if c + 1 < total then some (c + 1)
                else if rep = .playlist then some 0 else none

theorem nextIndex_lt {total : Nat} {cur : Option Nat} {rep : RepeatMode} {j : Nat}
    (h : nextIndex total cur rep = some j) : j < total := by
  unfold nextIndex at h
  split at h; · cases h
  split at h; · cases h
  split at h
  · cases h; omega
  · split at h <;> cases h; omega

/-- Without `RepeatMode::Playlist`, manual Next only ever moves forward, so a
run of Next presses reaches the end and stops within `total` steps. -/
theorem nextIndex_forward {total c : Nat} {rep : RepeatMode} {j : Nat}
    (hrep : rep ≠ .playlist) (h : nextIndex total (some c) rep = some j) : j = c + 1 := by
  unfold nextIndex at h
  split at h; · cases h
  simp only at h
  split at h
  · cases h; rfl
  · simp_all

/-! ## `find_playable_from` -/

/-- The loop in `find_playable_from`: try `c`, then `(c + 1) % total`, …,
at most `fuel` candidates. -/
def scan (total : Nat) (playable : Nat → Bool) : Nat → Nat → Option Nat
  | 0, _ => none
  | fuel + 1, c => if playable c then some c else scan total playable fuel ((c + 1) % total)

def findPlayable (total : Nat) (playable : Nat → Bool) (start : Nat) : Option Nat :=
  if total = 0 then none else scan total playable total (start % total)

theorem scan_sound {total : Nat} {p : Nat → Bool} :
    ∀ {fuel c j}, c < total → scan total p fuel c = some j → j < total ∧ p j = true
  | 0, _, _, _, h => by cases h
  | fuel + 1, c, j, hc, h => by
    unfold scan at h
    split at h
    · cases h; exact ⟨hc, by assumption⟩
    · exact scan_sound (Nat.mod_lt _ (by omega)) h

/-- Whatever `find_playable_from` returns is in range and playable. -/
theorem findPlayable_sound {total : Nat} {p : Nat → Bool} {start j : Nat}
    (h : findPlayable total p start = some j) : j < total ∧ p j = true := by
  unfold findPlayable at h
  split at h
  · cases h
  · exact scan_sound (Nat.mod_lt _ (by omega)) h

theorem scan_complete {total : Nat} {p : Nat → Bool} :
    ∀ {fuel c d}, 0 < total → d < fuel → p ((c + d) % total) = true →
      (scan total p fuel (c % total)).isSome
  | 0, _, _, _, hd, _ => absurd hd (Nat.not_lt_zero _)
  | fuel + 1, c, d, ht, hd, hp => by
    unfold scan
    split
    · rfl
    · rename_i hc
      cases d with
      | zero => simp_all
      | succ d =>
        have e : (c % total + 1) % total = (c + 1) % total := by
          rw [Nat.add_mod, Nat.mod_mod, ← Nat.add_mod]
        rw [e]
        exact scan_complete (c := c + 1) (d := d) ht (by omega)
          (by rw [show c + 1 + d = c + (d + 1) by omega]; exact hp)

/-- `find_playable_from` never gives up while *any* track in the order is
playable — it only returns `None` (and playback stops) for an
all-unavailable playlist. -/
theorem findPlayable_complete {total : Nat} {p : Nat → Bool} {start k : Nat}
    (hk : k < total) (hp : p k = true) : (findPlayable total p start).isSome := by
  unfold findPlayable
  have ht : total ≠ 0 := by omega
  simp only [ht, if_false]
  have hs : start % total < total := Nat.mod_lt _ (by omega)
  refine scan_complete (d := (k + total - start % total) % total) (by omega)
    (Nat.mod_lt _ (by omega)) ?_
  rw [← Nat.mod_add_mod, Nat.add_mod_mod, show start % total + (k + total - start % total) = k + total by omega,
    Nat.add_mod_right, Nat.mod_eq_of_lt hk]
  exact hp

/-! ## `previous_track` (the non-history, playlist-order walk) -/

/-- The linear walk at the end of `previous_track` as it was **before** the
fix for #1223 (it now wraps wherever it crosses the start): start one before
`current` (wrapping to the last track only under `RepeatMode::Playlist` when
`current == 0`), then step backwards, `break`ing at index 0. -/
def prevLinear (total : Nat) (playable : Nat → Bool) (cur : Nat) (rep : RepeatMode) :
    Option Nat :=
  if total = 0 then none
  else
    let start? := if cur > 0 then some (cur - 1)
                  else if rep = .playlist then some (total - 1) else none
    match start? with
    | none => none
    | some c => go total c
where
  go : Nat → Nat → Option Nat
    | 0, _ => none
    | fuel + 1, c => if playable c then some c else if c = 0 then none else go fuel (c - 1)

/-- **Finding 3.** Under `RepeatMode::Playlist`, Previous only wraps past the
start when the *current* track is the first one. If every track before the
current one is unavailable, the walk hits index 0 and `break`s instead of
wrapping, so Previous does nothing — even though the last track is playable
and Next would wrap. (Order: `[unavailable, current, playable]`.) -/
theorem prev_playlist_does_not_wrap :
    let p : Nat → Bool := fun i => i ≠ 0
    prevLinear 3 p 1 .playlist = none ∧ (findPlayable 3 p 2).isSome := by
  decide

end Luminous.Nav
