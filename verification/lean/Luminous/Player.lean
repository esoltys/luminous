import Luminous.Move

/-!
# Player index bookkeeping (`src-tauri/src/player.rs`)

`Player` addresses the playlist through two index spaces:

* a **real** index into `playlist_items`, and
* a **virtual** index — a position in the active play order `shuffle_order`
  (the identity `0..n` while shuffle is off).

`current_index` and every `played_indices` entry are *virtual*; the song that
is actually loaded is identified by `current_item_uuid`, i.e. by a *real*
index. The whole design relies on these staying in agreement. This file states
that agreement as invariants and checks which `Player` operations keep them.

Items are abstracted to their real index, and every non-`Off` shuffle mode is
collapsed into `shuffle = true`: the modes differ only in *which* permutation
`rebuild_shuffle_order` produces, which the model leaves as an arbitrary
input (`rest`).
-/

namespace Luminous.Player

/-- The slice of `Player` that index bookkeeping touches. -/
structure State where
  /-- `playlist_items.len()` -/
  n : Nat
  /-- `shuffle_mode != ShuffleMode::Off` -/
  shuffle : Bool
  /-- `shuffle_order` -/
  order : List Nat
  /-- `current_index` (virtual) -/
  cur : Option Nat
  /-- real index of the item whose uuid is `current_item_uuid`, or `none` when
  the loaded song is not a playlist item (e.g. a "play next" queue entry) -/
  playing : Option Nat
  /-- `played_indices` (virtual) -/
  hist : List Nat
deriving DecidableEq, Repr

/-- `resolve_item_index`: virtual → real. -/
def resolve (s : State) (v : Nat) : Option Nat :=
  if s.shuffle then s.order[v]? else if v < s.n then some v else none

/-- `l` is a permutation of `0..n`. -/
structure Perm (n : Nat) (l : List Nat) : Prop where
  length : l.length = n
  bound : ∀ x ∈ l, x < n
  nodup : l.Nodup
  complete : ∀ x, x < n → x ∈ l

/-- Well-formedness: `shuffle_order` is a permutation (the identity while
shuffle is off, which `rebuild_shuffle_order` maintains), and the loaded
song's real index is in range. -/
structure WF (s : State) : Prop where
  perm : Perm s.n s.order
  ident : s.shuffle = false → s.order = List.range s.n
  playing : ∀ r, s.playing = some r → r < s.n

/-- The playlist item `current_index` designates — where Next/Previous step from. -/
def curItem (s : State) : Option Nat := s.cur.bind (resolve s)

/-- The playlist items `previous_track` walks back through in shuffle mode. -/
def historyItems (s : State) : List Nat := s.hist.filterMap (resolve s)

/-- **Main invariant**: when the loaded song is a playlist item,
`current_index` designates exactly that item. When it breaks,
`next_track`/`previous_track` step from the wrong place. (It is deliberately
silent while a "play next" queue entry plays: `current_index` then keeps the
playlist position to resume from.) -/
def Synced (s : State) : Prop :=
  ∀ v r, s.cur = some v → s.playing = some r → resolve s v = some r

instance (s : State) : Decidable (Synced s) :=
  match hc : s.cur, hp : s.playing with
  | some v, some r =>
    if h : resolve s v = some r then
      isTrue fun v' r' hv hr => by rw [hc] at hv; rw [hp] at hr; cases hv; cases hr; exact h
    else isFalse fun hs => h (hs v r hc hp)
  | none, _ => isTrue fun _ _ hv _ => by rw [hc] at hv; cases hv
  | some _, none => isTrue fun _ _ _ hr => by rw [hp] at hr; cases hr

/-- `Synced` follows from `curItem` agreeing with the loaded song. -/
theorem synced_of_curItem {s : State} (h : ∀ r, s.playing = some r → s.cur.isSome →
    curItem s = some r) : Synced s := by
  intro v r hv hr
  have := h r hr (by simp [hv])
  simpa [curItem, hv] using this

/-! ## Helpers -/

/-- `iter().position(|&i| i == x)` -/
def pos? : List Nat → Nat → Option Nat
  | [], _ => none
  | y :: ys, x => if y = x then some 0 else (pos? ys x).map (· + 1)

theorem getElem?_of_pos? : ∀ {l : List Nat} {x v : Nat}, pos? l x = some v → l[v]? = some x
  | [], _, _, h => by simp [pos?] at h
  | y :: ys, x, v, h => by
    unfold pos? at h
    split at h
    · cases h; simp_all
    · cases h' : pos? ys x with
      | none => simp [h'] at h
      | some w =>
        simp [h'] at h; subst h
        simpa using getElem?_of_pos? h'

theorem pos?_isSome : ∀ {l : List Nat} {x : Nat}, x ∈ l → (pos? l x).isSome
  | [], _, h => by simp at h
  | y :: ys, x, h => by
    unfold pos?
    split
    · rfl
    · rename_i hne
      have : x ∈ ys := by
        rcases List.mem_cons.1 h with h | h
        · exact absurd h.symm hne
        · exact h
      simpa using pos?_isSome this

/-- Looking an in-range item up in a permutation and resolving it back is the
identity. -/
theorem pos?_roundtrip {n : Nat} {l : List Nat} (hp : Perm n l) {r : Nat} (hr : r < n) :
    (pos? l r).bind (l[·]?) = some r := by
  have := pos?_isSome (hp.complete r hr)
  cases h : pos? l r with
  | none => simp [h] at this
  | some v => simpa using getElem?_of_pos? h

theorem resolve_eq {s : State} (hw : WF s) (v : Nat) : resolve s v = s.order[v]? := by
  unfold resolve
  cases hs : s.shuffle
  · rw [hw.ident hs]
    by_cases hv : v < s.n
    · simp [hv]
    · simp [hv]
  · rfl

theorem perm_range (n : Nat) : Perm n (List.range n) where
  length := List.length_range
  bound := fun _ h => List.mem_range.1 h
  nodup := List.nodup_range
  complete := fun _ h => List.mem_range.2 h

/-! ## `rebuild_shuffle_order` / `set_shuffle_mode` -/

/-- `current_real_idx` in `rebuild_shuffle_order`. -/
def currentReal (s : State) : Option Nat :=
  match s.cur with
  | none => none
  | some pos =>
    let idx := if s.order.isEmpty then pos else (s.order[pos]?).getD pos
    if idx < s.n then some idx else none

/-- `current_real_idx` is exactly the item `current_index` designates. -/
theorem currentReal_eq {s : State} (hw : WF s) : currentReal s = curItem s := by
  unfold currentReal curItem
  cases hc : s.cur with
  | none => rfl
  | some pos =>
    simp only [Option.bind_some, resolve_eq hw]
    by_cases he : s.order.isEmpty
    · have : s.order = [] := List.isEmpty_iff.1 he
      have hn := hw.perm.length; rw [this] at hn
      simp [this, ← hn]
    · simp only [he, Bool.false_eq_true, if_false]
      cases hp : s.order[pos]? with
      | none =>
        have : s.order.length ≤ pos := List.getElem?_eq_none_iff.1 hp
        rw [hw.perm.length] at this
        simp [show ¬ pos < s.n by omega]
      | some r =>
        simp [hw.perm.bound r (List.mem_of_getElem? hp)]

/-- `rebuild_shuffle_order` as it was **before** the fix for #1221/#1222.
`rest` is whatever the shuffle produced for the tracks after the current one.
Note the early `return` in the `Off` branch, and that no branch touches
`played_indices`. The current code implements `rebuildFixed` below. -/
def rebuild (s : State) (rest : List Nat) : State :=
  if s.n = 0 then { s with order := [] }
  else if !s.shuffle then { s with order := List.range s.n }
  else
    let cr := currentReal s
    { s with order := cr.toList ++ rest, cur := cr.map fun _ => 0 }

/-- `set_shuffle_mode` before the fix for #1221/#1222. -/
def setShuffle (on : Bool) (s : State) (rest : List Nat) : State :=
  rebuild { s with shuffle := on } rest

/-- Shuffle on, order `[2, 0, 1]`, playing item 1 (virtual 2) after items 2
and 0 — so `played_indices = [0, 1, 2]`. -/
def witness : State :=
  { n := 3, shuffle := true, order := [2, 0, 1], cur := some 2, playing := some 1,
    hist := [0, 1, 2] }

theorem witness_ok : WF witness ∧ Synced witness :=
  ⟨⟨⟨rfl, by decide, by decide, by decide⟩, by decide, by decide⟩, by decide⟩

/-- **Finding 1.** Turning shuffle *off* leaves `current_index` as a position
in the discarded shuffle order: the loaded song is item 1 but `current_index`
now designates item 2, so Next stops/wraps as if the last track were playing.
Reproduced against the real `Player`: shuffle on, start at track 3 of 6,
shuffle off, Next → plays track 2 instead of track 4. -/
theorem setShuffle_off_desyncs :
    ¬ Synced (setShuffle false witness []) ∧
    curItem (setShuffle false witness []) ≠ curItem witness := by
  decide

/-- **Finding 2.** Re-shuffling (switching shuffle modes, e.g. All → Albums)
keeps `current_index` right but not `played_indices`: they are positions in
the *old* order, so Previous walks back through different songs than the ones
that played. Here Previous should go back to item 0 (then item 2); after the
re-shuffle it pops virtual index 2, which now holds item 2, and skips item 0. -/
theorem setShuffle_reshuffle_scrambles_history :
    historyItems witness = [2, 0, 1] ∧
    historyItems (setShuffle true witness [0, 2]) = [1, 0, 2] := by
  decide

/-- Turning shuffle *on* (or switching between shuffle modes) does keep
`current_index` on the same item, whatever permutation the shuffle picks. -/
theorem setShuffle_on_curItem {s : State} (hw : WF s) (rest : List Nat) :
    curItem (setShuffle true s rest) = curItem s := by
  unfold setShuffle rebuild
  by_cases hn : s.n = 0
  · have : s.order = [] := List.eq_nil_of_length_eq_zero (hw.perm.length.trans hn)
    simp only [hn, if_true, curItem]
    congr 1; funext v
    simp [resolve, this]; omega
  · have hcr : currentReal { s with shuffle := true } = curItem s := by
      rw [← currentReal_eq hw]; rfl
    simp only [hn, if_false, Bool.not_true, Bool.false_eq_true, hcr]
    cases h : curItem s with
    | none => simp [curItem]
    | some r => simp [curItem, resolve]

/-- The fix, as now implemented by `rebuild_shuffle_order` /
`build_play_order`: build the new order, then translate `current_index` and every
`played_indices` entry through the item it designates into the *new* order —
in the `Off` branch too. For shuffle-on this yields `current_index = Some(0)`,
which the code already does. -/
def rebuildFixed (s : State) (on : Bool) (newOrder : List Nat) : State :=
  { s with
    shuffle := on
    order := newOrder
    cur := (currentReal s).bind (pos? newOrder)
    hist := s.hist.filterMap fun v => s.order[v]?.bind (pos? newOrder) }

/-- The fixed rebuild keeps every invariant for any order the shuffle picks
(or the identity when shuffle is turned off): `current_index` and the whole
Previous history still designate the same items. -/
theorem rebuildFixed_ok {s : State} (hw : WF s) (on : Bool) (newOrder : List Nat)
    (hp : Perm s.n newOrder) (hid : on = false → newOrder = List.range s.n) :
    WF (rebuildFixed s on newOrder) ∧
    curItem (rebuildFixed s on newOrder) = curItem s ∧
    historyItems (rebuildFixed s on newOrder) = historyItems s := by
  have hw' : WF (rebuildFixed s on newOrder) := ⟨hp, hid, hw.playing⟩
  have hres : resolve (rebuildFixed s on newOrder) = (newOrder[·]?) := funext (resolve_eq hw')
  have key : ∀ o : Option Nat, (∀ r, o = some r → r < s.n) →
      (o.bind (pos? newOrder)).bind (resolve (rebuildFixed s on newOrder)) = o := by
    intro o ho
    cases o with
    | none => rfl
    | some r =>
      rw [Option.bind_some, hres]
      exact pos?_roundtrip hp (ho r rfl)
  refine ⟨hw', ?_, ?_⟩
  · show ((currentReal s).bind (pos? newOrder)).bind _ = _
    rw [key _ (fun r h => by
      rw [currentReal_eq hw] at h
      obtain ⟨v, -, hv⟩ := Option.bind_eq_some_iff.1 h
      rw [resolve_eq hw] at hv
      exact hw.perm.bound r (List.mem_of_getElem? hv)), currentReal_eq hw]
  · unfold historyItems
    rw [show (rebuildFixed s on newOrder).hist =
        s.hist.filterMap (fun v => s.order[v]?.bind (pos? newOrder)) from rfl,
      List.filterMap_filterMap]
    congr 1; funext v
    rw [key _ (fun r h => hw.perm.bound r (List.mem_of_getElem? h)), resolve_eq hw]

/-- …and therefore keeps `current_index` synced with the loaded song. -/
theorem rebuildFixed_synced {s : State} (hw : WF s) (hs : Synced s) (on : Bool)
    (newOrder : List Nat) (hp : Perm s.n newOrder)
    (hid : on = false → newOrder = List.range s.n) :
    Synced (rebuildFixed s on newOrder) := by
  have hc := (rebuildFixed_ok hw on newOrder hp hid).2.1
  apply synced_of_curItem
  intro r hr hsome
  rw [hc]
  have hsome' : s.cur.isSome := by
    have : (rebuildFixed s on newOrder).cur.isSome := hsome
    simp only [rebuildFixed] at this
    rw [currentReal_eq hw] at this
    cases h : s.cur <;> simp_all [curItem]
  obtain ⟨v, hv⟩ := Option.isSome_iff_exists.1 hsome'
  simpa [curItem, hv] using hs v r hv hr

/-- Sanity check: the fix repairs both witnesses. -/
example :
    Synced (rebuildFixed witness false (List.range 3)) ∧
    historyItems (rebuildFixed witness true [1, 0, 2]) = historyItems witness := by
  decide

/-! ## `reorder_playlist_items` -/

/-- `reorder_playlist_items(from, to)`: move the item, rewrite `shuffle_order`
with `remap` (shuffle on) or reset it to the identity (off), then re-find the
loaded song's uuid. -/
def reorder (s : State) (f t : Nat) : State :=
  if f < s.n ∧ t < s.n ∧ f ≠ t then
    let p := s.playing.map (remap f t)
    if s.shuffle then
      let o := s.order.map (remap f t)
      { s with order := o, playing := p, cur := p.bind (pos? o) }
    else { s with order := List.range s.n, playing := p, cur := p }
  else s

theorem perm_map_remap {n : Nat} {l : List Nat} (hp : Perm n l) {f t : Nat}
    (hf : f < n) (ht : t < n) : Perm n (l.map (remap f t)) where
  length := by simp [hp.length]
  bound := by
    intro x hx
    obtain ⟨y, hy, rfl⟩ := List.mem_map.1 hx
    exact remap_lt hf ht (hp.bound y hy)
  nodup := by
    unfold List.Nodup
    rw [List.pairwise_map]
    exact hp.nodup.imp fun h e => h (remap_injective e)
  complete := by
    intro x hx
    exact List.mem_map.2 ⟨remap t f x, hp.complete _ (remap_lt ht hf hx), remap_remap t f x⟩

/-- Reordering keeps every invariant. Combined with `getElem?_move` (the item
at real index `r` is found at `remap f t r` afterwards), this says the live
player keeps playing — and stepping from — the same song across a drag. -/
theorem reorder_ok {s : State} (hw : WF s) (hs : Synced s) (f t : Nat) :
    WF (reorder s f t) ∧ Synced (reorder s f t) := by
  unfold reorder
  split
  · rename_i h
    obtain ⟨hf, ht, -⟩ := h
    have hpl : ∀ r, s.playing.map (remap f t) = some r → r < s.n := by
      intro r hr
      obtain ⟨r0, h0, rfl⟩ := Option.map_eq_some_iff.1 hr
      exact remap_lt hf ht (hw.playing r0 h0)
    split
    · rename_i hsh
      refine ⟨⟨perm_map_remap hw.perm hf ht, fun h => by simp_all, hpl⟩, ?_⟩
      intro v r hv hr
      simp only at hv hr
      rw [hr] at hv
      simpa [resolve, hsh] using getElem?_of_pos? hv
    · rename_i hsh
      refine ⟨⟨perm_range _, fun _ => rfl, hpl⟩, ?_⟩
      intro v r hv hr
      simp only at hv hr
      rw [hv] at hr; cases hr
      simp [resolve, hsh, hpl v hv]
  · exact ⟨hw, hs⟩

end Luminous.Player
