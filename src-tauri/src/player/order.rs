//! Playback order state machine (`PlayOrder`).
//!
//! Owns the loaded playlist items, the active shuffle permutation, the current
//! virtual position, the shuffle back-history, the ad-hoc "play next" queue,
//! and the shuffle/repeat modes. All operations are pure and synchronous so
//! navigation and playlist-edit invariants can be tested without a database or
//! audio engine.

use crate::models::{PlaylistItem, RepeatMode, ShuffleMode, Song};
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet, VecDeque};

/// How a resolved next-track decision advances `PlayOrder` when committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OrderTargetKind {
    /// `RepeatMode::Track` — replay the current track without advancing index.
    Replay,
    /// Advance to the item at this virtual (shuffle-order) index.
    Index(usize),
    /// Consume the first playable item in the ad-hoc play-next queue.
    Queue,
}

/// A resolved next-track candidate produced by peeking at `PlayOrder` without
/// mutating it.
#[derive(Debug, Clone)]
pub(crate) enum OrderTarget {
    Replay,
    Index {
        virtual_index: usize,
        song: Song,
        uuid: Option<String>,
    },
    Queue {
        song: Song,
        uuid: Option<String>,
    },
}

impl OrderTarget {
    /// Returns the `Song` for this target, falling back to `current_song` when
    /// the target is `Replay`.
    pub(crate) fn song<'a>(&'a self, current_song: Option<&'a Song>) -> Option<&'a Song> {
        match self {
            Self::Replay => current_song,
            Self::Index { song, .. } | Self::Queue { song, .. } => Some(song),
        }
    }
}

/// Synchronous playback-order state machine for a loaded playlist or ad-hoc
/// queue.
///
/// Invariants maintained across every public operation:
/// - `shuffle_order` is always a permutation of `0..items.len()`, and is the
///   identity permutation when `shuffle_mode == ShuffleMode::Off`.
/// - Every entry in `played_indices` is a valid virtual index (`< shuffle_order.len()`).
pub(crate) struct PlayOrder {
    items: Vec<PlaylistItem>,
    shuffle_order: Vec<usize>,
    current_index: Option<usize>,
    played_indices: Vec<usize>,
    queue: VecDeque<PlaylistItem>,
    shuffle_mode: ShuffleMode,
    repeat_mode: RepeatMode,
}

impl PlayOrder {
    /// Builds a `PlayOrder` from restored startup state and computes the initial
    /// `shuffle_order` permutation.
    pub(crate) fn new(
        items: Vec<PlaylistItem>,
        current_real_index: Option<usize>,
        shuffle_mode: ShuffleMode,
        repeat_mode: RepeatMode,
    ) -> Self {
        let mut order = Self {
            items,
            shuffle_order: Vec::new(),
            current_index: current_real_index,
            played_indices: Vec::new(),
            queue: VecDeque::new(),
            shuffle_mode,
            repeat_mode,
        };
        order.rebuild_shuffle_order();
        order
    }

    pub(crate) fn shuffle_mode(&self) -> ShuffleMode {
        self.shuffle_mode
    }

    pub(crate) fn repeat_mode(&self) -> RepeatMode {
        self.repeat_mode
    }

    pub(crate) fn items(&self) -> &[PlaylistItem] {
        &self.items
    }

    pub(crate) fn items_mut(&mut self) -> &mut [PlaylistItem] {
        &mut self.items
    }

    pub(crate) fn current_index(&self) -> Option<usize> {
        self.current_index
    }

    #[cfg(test)]
    pub(crate) fn set_current_index(&mut self, index: Option<usize>) {
        self.current_index = index;
    }

    #[cfg(test)]
    pub(crate) fn shuffle_order(&self) -> &[usize] {
        &self.shuffle_order
    }

    #[cfg(test)]
    pub(crate) fn set_shuffle_order(&mut self, order: Vec<usize>) {
        self.shuffle_order = order;
    }

    #[cfg(test)]
    pub(crate) fn played_indices(&self) -> &[usize] {
        &self.played_indices
    }

    #[cfg(test)]
    pub(crate) fn queue_mut(&mut self) -> &mut VecDeque<PlaylistItem> {
        &mut self.queue
    }

    /// Returns true if the playlist item has a playable (present + available) song.
    pub(crate) fn is_item_playable(item: &PlaylistItem) -> bool {
        match &item.song {
            Some(song) => !song.unavailable,
            None => false, // legacy ghost row (song_id = NULL)
        }
    }

    /// Replaces the loaded items and resets history/queue, returning the virtual
    /// index corresponding to `start_real_index` (if in range).
    pub(crate) fn load(
        &mut self,
        items: Vec<PlaylistItem>,
        start_real_index: Option<usize>,
    ) -> Option<usize> {
        self.items = items;
        self.played_indices.clear();
        self.queue.clear();
        self.shuffle_order.clear();
        self.current_index = start_real_index.filter(|&i| i < self.items.len());
        self.rebuild_shuffle_order();
        self.current_index
    }

    pub(crate) fn contains_uuid(&self, uuid: &str) -> bool {
        self.items.iter().any(|i| i.uuid == uuid)
    }

    /// Resolves a playlist item UUID to its current virtual index.
    pub(crate) fn virtual_index_for_uuid(&self, uuid: &str) -> Option<usize> {
        let real_idx = self.items.iter().position(|i| i.uuid == uuid)?;
        if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.iter().position(|&i| i == real_idx)
        } else {
            Some(real_idx)
        }
    }

    pub(crate) fn set_shuffle_mode(&mut self, mode: ShuffleMode) {
        self.shuffle_mode = mode;
        self.rebuild_shuffle_order();
    }

    pub(crate) fn set_repeat_mode(&mut self, mode: RepeatMode) {
        self.repeat_mode = mode;
    }

    /// Clears all in-memory playlist items and queue state.
    pub(crate) fn clear(&mut self) {
        self.items.clear();
        self.shuffle_order.clear();
        self.queue.clear();
        self.current_index = None;
        self.played_indices.clear();
    }

    /// Number of playlist items ahead of `current_index`.
    pub(crate) fn remaining_count(&self) -> usize {
        let total = self.items.len();
        match self.current_index {
            Some(idx) => total.saturating_sub(idx + 1),
            None => total,
        }
    }

    /// Returns `(1-based current position, total virtual length)` for `PlaybackState`.
    pub(crate) fn position_info(&self) -> (Option<usize>, usize) {
        let total = self.virtual_len();
        match self.current_index {
            Some(idx) => (Some(idx + 1), total),
            None => (None, total),
        }
    }

    /// The current playlist's items in actual playback order.
    pub(crate) fn tracks_in_playback_order(&self) -> Vec<PlaylistItem> {
        self.shuffle_order
            .iter()
            .filter_map(|&real_idx| self.items.get(real_idx).cloned())
            .collect()
    }

    /// Returns seed song IDs for Auto Continue when Repeat is off and at most
    /// `continue_mix::TOP_UP_THRESHOLD` songs remain (including play-next items).
    pub(crate) fn auto_continue_seeds(&self, current_song_id: i64) -> Option<Vec<i64>> {
        if self.repeat_mode != RepeatMode::Off {
            return None;
        }
        if self.remaining_count() + self.queue.len() > crate::continue_mix::TOP_UP_THRESHOLD {
            return None;
        }
        let mut seeds = vec![current_song_id];
        if let Some(pos) = self.current_index {
            seeds.extend(
                self.shuffle_order[..pos.min(self.shuffle_order.len())]
                    .iter()
                    .rev()
                    .filter_map(|&real| self.items.get(real)?.song.as_ref())
                    .map(|s| s.id)
                    .filter(|&id| id != current_song_id),
            );
        }
        seeds.truncate(crate::continue_mix::SEED_COUNT);
        Some(seeds)
    }

    /// Number of virtual indices in whichever order is currently active:
    /// `shuffle_order` while shuffling, `items` otherwise.
    pub(crate) fn virtual_len(&self) -> usize {
        if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.len()
        } else {
            self.items.len()
        }
    }

    /// Resolves a virtual index (position in the active play order) to its
    /// underlying `items` index — identity when shuffle is off, indirected
    /// through `shuffle_order` when shuffle is on.
    pub(crate) fn resolve_item_index(&self, virtual_index: usize) -> Option<usize> {
        if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.get(virtual_index).copied()
        } else {
            (virtual_index < self.items.len()).then_some(virtual_index)
        }
    }

    /// Resolves a virtual index to its first playable `OrderTarget` (wrapping
    /// through the active play order), or `None` if every item is unplayable.
    pub(crate) fn resolve_playable_target(&self, virtual_index: usize) -> Option<OrderTarget> {
        let candidate = self.find_playable_from(virtual_index)?;
        self.target_at_virtual_index(candidate)
    }

    /// True if the playlist item at virtual index `candidate` exists and is
    /// currently playable.
    fn is_playable_at(&self, candidate: usize) -> bool {
        self.resolve_item_index(candidate)
            .and_then(|i| self.items.get(i))
            .map(Self::is_item_playable)
            .unwrap_or(false)
    }

    /// Read-only walk forward from a virtual index (wrapping through the
    /// active play order) to the first playable item. `None` only if every
    /// item was checked without finding one.
    fn find_playable_from(&self, start: usize) -> Option<usize> {
        let total = self.virtual_len();
        if total == 0 {
            return None;
        }
        let mut candidate = start % total;
        for _ in 0..total {
            if self.is_playable_at(candidate) {
                return Some(candidate);
            }
            candidate = (candidate + 1) % total;
        }
        None
    }

    fn target_at_virtual_index(&self, candidate: usize) -> Option<OrderTarget> {
        let item_index = self.resolve_item_index(candidate)?;
        let item = self.items.get(item_index)?;
        let song = item.song.clone()?;
        Some(OrderTarget::Index {
            virtual_index: candidate,
            song,
            uuid: Some(item.uuid.clone()),
        })
    }

    fn peek_queue_front(&self) -> Option<OrderTarget> {
        let item = self.queue.iter().find(|i| Self::is_item_playable(i))?;
        let song = item.song.clone()?;
        Some(OrderTarget::Queue {
            song,
            uuid: Some(item.uuid.clone()),
        })
    }

    /// Determines what will play after the current track ends naturally,
    /// without mutating any state.
    pub(crate) fn peek_next_natural(&self, stop_after_current: bool) -> Option<OrderTarget> {
        if stop_after_current {
            return None;
        }

        if self.repeat_mode == RepeatMode::Track {
            self.current_index?;
            return Some(OrderTarget::Replay);
        }

        if let Some(queued) = self.peek_queue_front() {
            return Some(queued);
        }

        let idx = if self.repeat_mode == RepeatMode::Album {
            self.next_album_index()?
        } else if self.repeat_mode == RepeatMode::Playlist {
            self.get_next_index().unwrap_or(0)
        } else {
            self.get_next_index()?
        };
        let candidate = self.find_playable_from(idx)?;
        self.target_at_virtual_index(candidate)
    }

    /// Determines what plays on a manual "Next" skip without mutating state.
    /// Drains any unplayable play-next queue entries logically via peek; `commit`
    /// pops them when the track actually starts.
    pub(crate) fn next_manual(&self) -> Option<OrderTarget> {
        if let Some(queued) = self.peek_queue_front() {
            return Some(queued);
        }
        let next_idx = self.get_next_index()?;
        let candidate = self.find_playable_from(next_idx)?;
        self.target_at_virtual_index(candidate)
    }

    /// Steps backward one track, returning the virtual index to play (if any).
    /// In shuffle mode, walks back through `played_indices` (skipping the
    /// current track's own top-of-stack entry and unplayable tracks); otherwise
    /// steps backward in order, wrapping only under `RepeatMode::Playlist`.
    pub(crate) fn previous(&mut self) -> Option<usize> {
        if self.shuffle_mode != ShuffleMode::Off {
            while let Some(prev_index) = self.played_indices.pop() {
                if Some(prev_index) == self.current_index {
                    continue;
                }
                if self.is_playable_at(prev_index) {
                    return Some(prev_index);
                }
            }
        }

        let current = self.current_index?;
        let len = self.items.len();
        if len == 0 {
            return None;
        }
        let wrap = self.repeat_mode == RepeatMode::Playlist;
        let mut candidate = if current > 0 {
            current - 1
        } else if wrap {
            len - 1
        } else {
            return None;
        };
        for _ in 0..len {
            if self.is_playable_at(candidate) {
                return Some(candidate);
            }
            candidate = match candidate {
                0 if wrap => len - 1,
                0 => break,
                c => c - 1,
            };
        }
        None
    }

    /// Commits a started track target to `PlayOrder`, updating `current_index`
    /// and `played_indices` or pruning unplayable fronts and popping `queue`.
    pub(crate) fn commit(&mut self, kind: OrderTargetKind) {
        match kind {
            OrderTargetKind::Replay => {}
            OrderTargetKind::Index(candidate) => {
                self.current_index = Some(candidate);
                if !self.played_indices.contains(&candidate) {
                    self.played_indices.push(candidate);
                }
            }
            OrderTargetKind::Queue => {
                while let Some(front) = self.queue.front() {
                    if Self::is_item_playable(front) {
                        break;
                    }
                    log::debug!("Skipping unavailable queued item");
                    self.queue.pop_front();
                }
                self.queue.pop_front();
            }
        }
    }

    /// Appends songs directly to the in-memory items while keeping `shuffle_order`
    /// in sync.
    pub(crate) fn append(&mut self, items: Vec<PlaylistItem>) {
        self.items.extend(items);
        let new_start = if self.shuffle_mode != ShuffleMode::Off {
            self.shuffle_order.len()
        } else {
            0
        };
        let new_len = self.items.len();
        let existing_ordered: HashSet<usize> = self.shuffle_order.iter().copied().collect();
        for i in new_start..new_len {
            if !existing_ordered.contains(&i) {
                self.shuffle_order.push(i);
            }
        }
    }

    /// Removes items by UUID (never dropping `current_uuid`), keeping
    /// `shuffle_order`, `current_index`, and `played_indices` consistent.
    pub(crate) fn remove_by_uuids(&mut self, uuids: &[String], current_uuid: Option<&str>) {
        if self.items.is_empty() {
            return;
        }

        let uuid_set: HashSet<&str> = uuids.iter().map(String::as_str).collect();
        let old_uuids: Vec<String> = self.items.iter().map(|i| i.uuid.clone()).collect();

        self.items.retain(|item| {
            !uuid_set.contains(item.uuid.as_str()) || current_uuid == Some(item.uuid.as_str())
        });

        let uuid_to_new_idx: HashMap<&str, usize> = self
            .items
            .iter()
            .enumerate()
            .map(|(idx, item)| (item.uuid.as_str(), idx))
            .collect();

        let new_real_idx = current_uuid.and_then(|u| uuid_to_new_idx.get(u).copied());

        self.played_indices.clear();

        if self.shuffle_mode == ShuffleMode::Off {
            self.current_index = new_real_idx;
            self.shuffle_order = (0..self.items.len()).collect();
        } else {
            let mut new_shuffle_order = Vec::with_capacity(self.items.len());
            for &old_real_idx in &self.shuffle_order {
                if let Some(old_uuid) = old_uuids.get(old_real_idx) {
                    if let Some(&new_idx) = uuid_to_new_idx.get(old_uuid.as_str()) {
                        new_shuffle_order.push(new_idx);
                    }
                }
            }
            let mut seen = HashSet::new();
            for &idx in &new_shuffle_order {
                seen.insert(idx);
            }
            for i in 0..self.items.len() {
                if !seen.contains(&i) {
                    new_shuffle_order.push(i);
                }
            }
            self.shuffle_order = new_shuffle_order;
            self.current_index = new_real_idx
                .and_then(|real_idx| self.shuffle_order.iter().position(|&idx| idx == real_idx));
        }

        if let Some(idx) = self.current_index {
            self.played_indices.push(idx);
        }
    }

    /// Reorders a single item from `from` to `to`, keeping `current_index`,
    /// `shuffle_order`, and `played_indices` aligned.
    pub(crate) fn reorder(&mut self, from: usize, to: usize, current_uuid: Option<&str>) {
        if from >= self.items.len() || to >= self.items.len() || from == to {
            return;
        }

        let moved_item = self.items.remove(from);
        self.items.insert(to, moved_item);

        let new_real_idx = current_uuid.and_then(|u| self.items.iter().position(|i| i.uuid == u));

        // Where the item previously at real index `i` ends up (`remap` in
        // verification/lean/Luminous/Move.lean).
        let moved = |i: usize| {
            if i == from {
                to
            } else if from < to && i > from && i <= to {
                i - 1
            } else if from > to && i >= to && i < from {
                i + 1
            } else {
                i
            }
        };

        if self.shuffle_mode == ShuffleMode::Off {
            self.current_index = new_real_idx;
            self.shuffle_order = (0..self.items.len()).collect();
            for idx in &mut self.played_indices {
                *idx = moved(*idx);
            }
        } else {
            for idx in &mut self.shuffle_order {
                *idx = moved(*idx);
            }
            self.current_index = new_real_idx
                .and_then(|real_idx| self.shuffle_order.iter().position(|&i| i == real_idx));
        }
    }

    /// Rearranges the loaded items into `order` (item UUIDs), preserving the
    /// current track and play history.
    pub(crate) fn apply_order(&mut self, order: &[String], current_uuid: Option<&str>) {
        let rank: HashMap<&str, usize> = order
            .iter()
            .enumerate()
            .map(|(i, u)| (u.as_str(), i))
            .collect();
        let mut old_indices: Vec<usize> = (0..self.items.len()).collect();
        old_indices.sort_by_key(|&i| {
            rank.get(self.items[i].uuid.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });
        if old_indices.iter().enumerate().all(|(new, &old)| new == old) {
            return;
        }

        let mut new_index_of = vec![0; old_indices.len()];
        for (new, &old) in old_indices.iter().enumerate() {
            new_index_of[old] = new;
        }
        let mut items: Vec<Option<PlaylistItem>> = std::mem::take(&mut self.items)
            .into_iter()
            .map(Some)
            .collect();
        self.items = old_indices
            .iter()
            .filter_map(|&old| items[old].take())
            .collect();

        let new_real_idx = current_uuid.and_then(|u| self.items.iter().position(|i| i.uuid == u));
        if self.shuffle_mode == ShuffleMode::Off {
            self.current_index = new_real_idx;
            self.shuffle_order = (0..self.items.len()).collect();
            for idx in &mut self.played_indices {
                if let Some(&new) = new_index_of.get(*idx) {
                    *idx = new;
                }
            }
        } else {
            for idx in &mut self.shuffle_order {
                if let Some(&new) = new_index_of.get(*idx) {
                    *idx = new;
                }
            }
            self.current_index = new_real_idx
                .and_then(|real_idx| self.shuffle_order.iter().position(|&i| i == real_idx));
        }
    }

    /// Updates `is_instrumental` on any cached items or queued items matching `song_id`.
    pub(crate) fn update_song_instrumental(&mut self, song_id: i64, is_instrumental: bool) {
        for item in &mut self.items {
            if let Some(ref mut song) = item.song {
                if song.id == song_id {
                    song.is_instrumental = is_instrumental;
                }
            }
        }
        for item in &mut self.queue {
            if let Some(ref mut song) = item.song {
                if song.id == song_id {
                    song.is_instrumental = is_instrumental;
                }
            }
        }
    }

    fn album_key(item: &PlaylistItem) -> String {
        if let Some(ref song) = item.song {
            if let Some(ref album) = song.album {
                let key = crate::album_key::album_key(album);
                if !key.is_empty() {
                    return key;
                }
            }
        }
        item.uuid.clone()
    }

    fn next_album_index(&self) -> Option<usize> {
        let total = self.virtual_len();
        if total == 0 {
            return None;
        }
        let current_virtual = self.current_index?;
        let current_real = self.resolve_item_index(current_virtual)?;
        let key = Self::album_key(self.items.get(current_real)?);

        let album_virtual_indices: Vec<usize> = (0..total)
            .filter(|&v| {
                self.resolve_item_index(v)
                    .and_then(|r| self.items.get(r))
                    .map(|item| Self::album_key(item) == key)
                    .unwrap_or(false)
            })
            .collect();

        let pos = album_virtual_indices
            .iter()
            .position(|&v| v == current_virtual)?;
        let next_pos = (pos + 1) % album_virtual_indices.len();
        Some(album_virtual_indices[next_pos])
    }

    fn get_next_index(&self) -> Option<usize> {
        let total = self.virtual_len();
        if total == 0 {
            return None;
        }

        let current = self.current_index?;

        let next = current + 1;
        if next < total {
            Some(next)
        } else {
            match self.repeat_mode {
                RepeatMode::Playlist => Some(0),
                _ => None,
            }
        }
    }

    fn shuffle_grouped(
        &self,
        len: usize,
        current_real_idx: Option<usize>,
        rng: &mut impl rand::Rng,
        key_fn: impl Fn(&PlaylistItem) -> String,
        shuffle_groups: bool,
        shuffle_within_groups: bool,
    ) -> Vec<usize> {
        let current_key = current_real_idx.map(|idx| key_fn(&self.items[idx]));
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        let mut other_keys = Vec::new();

        for i in 0..len {
            if current_real_idx == Some(i) {
                continue;
            }
            let key = key_fn(&self.items[i]);
            groups
                .entry(key.clone())
                .or_insert_with(|| {
                    other_keys.push(key.clone());
                    Vec::new()
                })
                .push(i);
        }

        if shuffle_within_groups {
            for group_tracks in groups.values_mut() {
                group_tracks.shuffle(rng);
            }
        }
        if shuffle_groups {
            other_keys.shuffle(rng);
        }

        let mut order = Vec::with_capacity(len);

        if let Some(ref key) = current_key {
            if let Some(mut current_group) = groups.remove(key) {
                order.append(&mut current_group);
            }
        }
        for key in &other_keys {
            if let Some(mut group) = groups.remove(key) {
                order.append(&mut group);
            }
        }

        order
    }

    /// Replaces `shuffle_order` for the current `shuffle_mode`, keeping
    /// `current_index` and `played_indices` pointing at the same items.
    pub(crate) fn rebuild_shuffle_order(&mut self) {
        let len = self.items.len();
        if len == 0 {
            self.shuffle_order = Vec::new();
            return;
        }

        let old_order = std::mem::take(&mut self.shuffle_order);
        let to_real = |pos: usize| -> Option<usize> {
            let idx = if old_order.is_empty() {
                pos
            } else {
                old_order.get(pos).copied().unwrap_or(pos)
            };
            (idx < len).then_some(idx)
        };
        let current_real_idx = self.current_index.and_then(to_real);
        let played_real: Vec<usize> = self
            .played_indices
            .iter()
            .filter_map(|&v| to_real(v))
            .collect();

        let order = self.build_play_order(len, current_real_idx);

        let mut virtual_of_real = vec![None; len];
        for (virtual_idx, &real_idx) in order.iter().enumerate() {
            virtual_of_real[real_idx] = Some(virtual_idx);
        }
        self.current_index = current_real_idx.and_then(|r| virtual_of_real[r]);
        self.played_indices = played_real
            .into_iter()
            .filter_map(|r| virtual_of_real[r])
            .collect();
        self.shuffle_order = order;
    }

    fn build_play_order(&self, len: usize, current_real_idx: Option<usize>) -> Vec<usize> {
        let mut rng = rand::rng();

        let get_artist_key = |item: &PlaylistItem| -> String {
            if let Some(ref song) = item.song {
                if let Some(ref artist) = song.artist {
                    if !artist.trim().is_empty() {
                        return artist.to_lowercase();
                    }
                }
            }
            item.uuid.clone()
        };

        let mut order = if let Some(idx) = current_real_idx {
            vec![idx]
        } else {
            Vec::new()
        };

        match self.shuffle_mode {
            ShuffleMode::Off => return (0..len).collect(),
            ShuffleMode::All => {
                let mut remaining_indices: Vec<usize> =
                    (0..len).filter(|&i| current_real_idx != Some(i)).collect();
                remaining_indices.shuffle(&mut rng);
                order.extend(remaining_indices);
            }
            ShuffleMode::InsideAlbum => {
                order.extend(self.shuffle_grouped(
                    len,
                    current_real_idx,
                    &mut rng,
                    Self::album_key,
                    false,
                    true,
                ));
            }
            ShuffleMode::Albums => {
                order.extend(self.shuffle_grouped(
                    len,
                    current_real_idx,
                    &mut rng,
                    Self::album_key,
                    true,
                    false,
                ));
            }
            ShuffleMode::Artists => {
                order.extend(self.shuffle_grouped(
                    len,
                    current_real_idx,
                    &mut rng,
                    get_artist_key,
                    true,
                    false,
                ));
            }
        }

        order
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_song(id: i64, album: Option<&str>, unavailable: bool) -> Song {
        Song {
            id,
            title: Some(format!("Track {id}")),
            artist: Some("Artist".to_string()),
            album: album.map(str::to_string),
            unavailable,
            ..Default::default()
        }
    }

    fn make_items(songs: Vec<Song>) -> Vec<PlaylistItem> {
        songs
            .into_iter()
            .enumerate()
            .map(|(i, s)| PlaylistItem::new_song(0, i as i32, s))
            .collect()
    }

    fn advance_manual(order: &mut PlayOrder) -> Option<i64> {
        let target = order.next_manual()?;
        match target {
            OrderTarget::Replay => None,
            OrderTarget::Index {
                virtual_index,
                song,
                ..
            } => {
                order.commit(OrderTargetKind::Index(virtual_index));
                Some(song.id)
            }
            OrderTarget::Queue { song, .. } => {
                order.commit(OrderTargetKind::Queue);
                Some(song.id)
            }
        }
    }

    fn advance_natural(order: &mut PlayOrder) -> Option<i64> {
        let target = order.peek_next_natural(false)?;
        match target {
            OrderTarget::Replay => None,
            OrderTarget::Index {
                virtual_index,
                song,
                ..
            } => {
                order.commit(OrderTargetKind::Index(virtual_index));
                Some(song.id)
            }
            OrderTarget::Queue { song, .. } => {
                order.commit(OrderTargetKind::Queue);
                Some(song.id)
            }
        }
    }

    #[test]
    fn previous_walks_back_through_playlist() {
        let items = make_items(
            (1..=3)
                .map(|id| make_song(id, Some("Album"), false))
                .collect(),
        );
        let mut order = PlayOrder::new(items, None, ShuffleMode::Off, RepeatMode::Off);
        let start = order.load(
            make_items(
                (1..=3)
                    .map(|id| make_song(id, Some("Album"), false))
                    .collect(),
            ),
            Some(2),
        );
        assert_eq!(start, Some(2));
        order.commit(OrderTargetKind::Index(2));

        let prev = order.previous().unwrap();
        assert_eq!(prev, 1);
        order.commit(OrderTargetKind::Index(prev));

        let prev2 = order.previous().unwrap();
        assert_eq!(prev2, 0);
    }

    #[test]
    fn previous_does_not_wrap_at_start_without_repeat() {
        let items = make_items(
            (1..=3)
                .map(|id| make_song(id, Some("Album"), false))
                .collect(),
        );
        let mut order = PlayOrder::new(items, Some(0), ShuffleMode::Off, RepeatMode::Off);
        order.commit(OrderTargetKind::Index(0));

        assert_eq!(
            order.previous(),
            None,
            "previous at the start of the queue must not wrap to the last track"
        );

        order.set_repeat_mode(RepeatMode::Playlist);
        assert_eq!(order.previous(), Some(2));
    }

    #[test]
    fn previous_in_shuffle_mode_does_not_replay_current() {
        let items = make_items(
            (1..=4)
                .map(|id| make_song(id, Some("Album"), false))
                .collect(),
        );
        let mut order = PlayOrder::new(items, Some(0), ShuffleMode::All, RepeatMode::Playlist);
        let first_idx = order.current_index().unwrap();
        order.commit(OrderTargetKind::Index(first_idx));
        let first_id = order.items()[order.resolve_item_index(first_idx).unwrap()]
            .song
            .as_ref()
            .unwrap()
            .id;

        let second_id = advance_manual(&mut order).unwrap();
        let third_id = advance_manual(&mut order).unwrap();
        assert_ne!(second_id, first_id);
        assert_ne!(third_id, second_id);

        let prev_idx = order.previous().unwrap();
        order.commit(OrderTargetKind::Index(prev_idx));
        let prev_id = order.items()[order.resolve_item_index(prev_idx).unwrap()]
            .song
            .as_ref()
            .unwrap()
            .id;
        assert_eq!(prev_id, second_id);

        let prev_idx2 = order.previous().unwrap();
        let prev_id2 = order.items()[order.resolve_item_index(prev_idx2).unwrap()]
            .song
            .as_ref()
            .unwrap()
            .id;
        assert_eq!(prev_id2, first_id);
    }

    #[test]
    fn repeat_album_stays_within_album_and_wraps() {
        let items = make_items(
            (1..=4)
                .map(|id| {
                    let album = if id % 2 == 1 { "Album A" } else { "Album B" };
                    make_song(id, Some(album), false)
                })
                .collect(),
        );
        let mut order = PlayOrder::new(items, Some(0), ShuffleMode::Off, RepeatMode::Album);
        order.commit(OrderTargetKind::Index(0));

        assert_eq!(advance_natural(&mut order), Some(3));
        assert_eq!(advance_natural(&mut order), Some(1));
    }

    #[test]
    fn repeat_album_does_not_confine_manual_skip() {
        let items = make_items((1..=3).map(|id| make_song(id, None, false)).collect());
        let mut order = PlayOrder::new(items, Some(0), ShuffleMode::Off, RepeatMode::Album);
        order.commit(OrderTargetKind::Index(0));

        assert_eq!(advance_manual(&mut order), Some(2));
        assert_eq!(advance_manual(&mut order), Some(3));
    }

    #[test]
    fn shuffle_grouped_albums_keeps_track_order_inside_each_group() {
        let make = || {
            make_items(
                (1..=6)
                    .map(|id| {
                        let album = if id % 2 == 1 { "Album A" } else { "Album B" };
                        make_song(id, Some(album), false)
                    })
                    .collect(),
            )
        };

        let mut order = PlayOrder::new(make(), Some(0), ShuffleMode::Albums, RepeatMode::Off);
        let order_ids: Vec<i64> = order
            .shuffle_order()
            .iter()
            .map(|&idx| order.items()[idx].song.as_ref().unwrap().id)
            .collect();
        assert_eq!(order_ids, vec![1, 3, 5, 2, 4, 6]);

        order.set_shuffle_mode(ShuffleMode::InsideAlbum);
        order.load(make(), Some(0));
        let order_ids: Vec<i64> = order
            .shuffle_order()
            .iter()
            .map(|&idx| order.items()[idx].song.as_ref().unwrap().id)
            .collect();
        assert_eq!(order_ids[0], 1);
        let mut album_a_tail = order_ids[1..3].to_vec();
        album_a_tail.sort();
        assert_eq!(album_a_tail, vec![3, 5]);
        let mut album_b = order_ids[3..6].to_vec();
        album_b.sort();
        assert_eq!(album_b, vec![2, 4, 6]);
    }

    #[test]
    fn commit_queue_drops_unplayable_front_items() {
        let items = make_items(vec![make_song(1, Some("Album"), false)]);
        let mut order = PlayOrder::new(items, Some(0), ShuffleMode::Off, RepeatMode::Off);
        order.queue_mut().push_back(PlaylistItem::new_song(
            0,
            0,
            make_song(10, Some("Album"), true),
        ));
        order.queue_mut().push_back(PlaylistItem::new_song(
            0,
            1,
            make_song(11, Some("Album"), false),
        ));

        assert_eq!(advance_manual(&mut order), Some(11));
        assert!(order.queue_mut().is_empty());
    }
}
