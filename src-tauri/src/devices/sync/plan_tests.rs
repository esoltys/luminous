use super::*;
use crate::devices::sync::selection::SourceKind;
use crate::models::SongSource;
use luminous_transcode::TargetFormat;

const GB: u64 = 1_000_000_000;

fn converting() -> Decision {
    Decision::Convert {
        format: TargetFormat::Aac,
        bitrate_kbps: 256,
        sample_rate: 44_100,
        channels: 2,
        max_art_px: None,
    }
}

fn song(id: i64, title: &str) -> Song {
    Song {
        id,
        source: SongSource::Collection,
        path: Some(format!("C:/Library/Amorphis/Halo/{title}.mp3")),
        artist: Some("Amorphis".into()),
        album_artist: Some("Amorphis".into()),
        album: Some("Halo".into()),
        track: Some(id as i32),
        title: Some(title.into()),
        filesize: Some(1_000),
        mtime: Some(100),
        ..Song::default()
    }
}

fn library_source() -> SourceRef {
    SourceRef {
        kind: SourceKind::Library,
        key: String::new(),
        name: String::new(),
    }
}

fn playlist_source(id: i64, name: &str) -> SourceRef {
    SourceRef {
        kind: SourceKind::Playlist,
        key: id.to_string(),
        name: name.into(),
    }
}

fn track_from(song: Song, source: SourceRef) -> TrackInput {
    let path = song.path.clone().unwrap();
    TrackInput {
        stamp: SourceStamp {
            path,
            size: 1_000,
            mtime: Some(100),
        },
        decision: Decision::Copy,
        size: 1_000,
        size_converting_all: None,
        wanted: WantedSong {
            song,
            sources: vec![source],
        },
    }
}

fn track(id: i64, title: &str) -> TrackInput {
    track_from(song(id, title), library_source())
}

fn profile() -> DeviceProfile {
    DeviceProfile {
        music_path: "Music".into(),
        playlists_path: "Playlists".into(),
        layout: luminous_devices::LayoutSettings {
            playlist_format: luminous_devices::PlaylistFileFormat::None,
            ..Default::default()
        },
        ..DeviceProfile::default()
    }
}

/// Where `track` goes on a device with `profile()`.
fn device_path(track: &TrackInput) -> String {
    let synced = [SyncedTrack {
        song: &track.wanted.song,
        extension: &track.extension(),
    }];
    DeviceLayout::plan(&profile(), &synced)
        .track_path(track.wanted.song.id)
        .unwrap()
        .to_string()
}

/// The manifest entry sync would have written for `track` at `path`.
fn synced(track: &TrackInput, path: &str) -> ManifestEntry {
    ManifestEntry {
        path: path.into(),
        kind: EntryKind::Track,
        size: track.size,
        source: Some(track.stamp.clone()),
        conversion: Some(track.decision),
        fingerprint: Some(tag_fingerprint(&track.wanted.song)),
        sources: track.wanted.sources.clone(),
        label: label(&track.wanted.song),
    }
}

fn inventory(files: &[(&str, u64)], manifest: Vec<ManifestEntry>) -> Inventory {
    Inventory {
        files: files.iter().map(|(p, s)| (p.to_lowercase(), *s)).collect(),
        roots: vec!["Music".into(), "Playlists".into()],
        manifest: manifest.into_iter().collect(),
        space: Space {
            total_bytes: 32 * GB,
            free_bytes: 10 * GB,
        },
    }
}

struct Case {
    profile: DeviceProfile,
    unselected: UnselectedSongs,
    tracks: Vec<TrackInput>,
    unreadable: Vec<Song>,
    playlists: Vec<WantedPlaylist>,
    sources: Vec<SourceRef>,
    inventory: Inventory,
    library: HashMap<String, LibraryFile>,
}

impl Case {
    fn new(tracks: Vec<TrackInput>, inventory: Inventory) -> Self {
        let library = tracks
            .iter()
            .map(|t| (t.stamp.path.clone(), LibraryFile::Available))
            .collect();
        Self {
            profile: profile(),
            unselected: UnselectedSongs::Remove,
            tracks,
            unreadable: Vec::new(),
            playlists: Vec::new(),
            sources: vec![library_source()],
            inventory,
            library,
        }
    }

    fn build(&self) -> Built {
        build(PlanInput {
            profile: &self.profile,
            unselected: self.unselected,
            tracks: self.tracks.clone(),
            unreadable: self.unreadable.clone(),
            playlists: &self.playlists,
            sources: &self.sources,
            inventory: &self.inventory,
            library: &self.library,
            conversion_target: ConversionTarget {
                format: TargetFormat::Opus,
                bitrate_kbps: 160,
                substituted: true,
                device_plays: false,
            },
        })
    }
}

fn reasons(list: &ChangeList) -> Vec<&SyncReason> {
    list.items.iter().map(|i| &i.reason).collect()
}

#[test]
fn a_fresh_device_gets_every_song_copied() {
    let built = Case::new(
        vec![track(1, "Halo"), track(2, "Northbound")],
        inventory(&[], vec![]),
    )
    .build();
    let plan = &built.plan;
    assert_eq!(plan.copy.songs, 2);
    assert_eq!(plan.copy.bytes, 2_000);
    assert_eq!(
        reasons(&plan.copy),
        [&SyncReason::Selected {
            source: library_source()
        }; 2]
    );
    assert!(plan.storage.fits);
    assert_eq!(plan.storage.free_after_bytes, (10 * GB - 2_000) as i64);
    assert_eq!(plan.storage.other_bytes, 22 * GB);
    assert_eq!(built.steps.song_writes(), 2);
    assert!(built.steps.remove.is_empty());
    assert_eq!(plan.conversion_target, None, "nothing is converted");
}

#[test]
fn an_unchanged_synced_song_stays() {
    let t = track(1, "Halo");
    let path = device_path(&t);
    let built = Case::new(
        vec![t.clone()],
        inventory(&[(&path, 1_000)], vec![synced(&t, &path)]),
    )
    .build();
    assert_eq!(built.plan.copy.songs, 0);
    assert_eq!(built.plan.update.songs, 0);
    assert_eq!(built.plan.storage.stays_bytes, 1_000);
    assert!(built.steps.write.is_empty());
}

#[test]
fn a_synced_song_missing_from_the_device_is_copied_again() {
    let t = track(1, "Halo");
    let path = device_path(&t);
    let built = Case::new(vec![t.clone()], inventory(&[], vec![synced(&t, &path)])).build();
    assert_eq!(built.plan.copy.songs, 1);
    assert_eq!(built.plan.storage.removed_bytes, 0);
}

#[test]
fn changes_are_updates_with_their_reason() {
    let t = track(1, "Halo");
    let path = device_path(&t);
    let files = [(path.as_str(), 1_000)];

    let mut touched = synced(&t, &path);
    touched.source.as_mut().unwrap().mtime = Some(50);
    let built = Case::new(vec![t.clone()], inventory(&files, vec![touched.clone()])).build();
    assert_eq!(reasons(&built.plan.update), [&SyncReason::FileChanged]);

    touched.fingerprint = Some("old tags".into());
    let built = Case::new(vec![t.clone()], inventory(&files, vec![touched])).build();
    assert_eq!(reasons(&built.plan.update), [&SyncReason::TagsChanged]);

    let mut converted = synced(&t, &path);
    converted.conversion = Some(converting());
    let built = Case::new(vec![t], inventory(&files, vec![converted])).build();
    assert_eq!(reasons(&built.plan.update), [&SyncReason::ConvertChanged]);
    assert_eq!(
        built.plan.storage.removed_bytes, 1_000,
        "the old copy is replaced"
    );
    assert_eq!(built.plan.storage.copied_bytes, 1_000);
}

#[test]
fn a_song_whose_place_changed_moves() {
    let t = track(1, "Halo");
    let old = "Music/Old Layout/Halo.mp3";
    let built = Case::new(
        vec![t.clone()],
        inventory(&[(old, 1_000)], vec![synced(&t, old)]),
    )
    .build();
    assert_eq!(
        reasons(&built.plan.update),
        [&SyncReason::Moved { from: old.into() }]
    );
    assert_eq!(built.steps.remove, [old]);
    assert_eq!(built.plan.remove.songs, 0, "a move is not a removal");
}

#[test]
fn only_files_sync_copied_are_removed() {
    let gone = track(9, "Gone");
    let gone_path = device_path(&gone);
    let mine = "Music/Mine/Bootleg.mp3";
    let mut case = Case::new(
        vec![track(1, "Halo")],
        inventory(
            &[(&gone_path, 1_000), (mine, 5_000)],
            vec![synced(&gone, &gone_path)],
        ),
    );
    case.library
        .insert(gone.stamp.path.clone(), LibraryFile::Available);
    // The whole library was unchecked; a playlist is selected now.
    case.sources = vec![playlist_source(3, "Commute")];
    let built = case.build();
    assert_eq!(built.plan.remove.songs, 1);
    assert_eq!(
        reasons(&built.plan.remove),
        [&SyncReason::Unselected {
            source: Some(library_source())
        }]
    );
    assert_eq!(built.steps.remove, [gone_path]);
    assert_eq!(built.plan.storage.other_bytes, 22 * GB - 1_000);
}

#[test]
fn leave_on_device_removes_nothing() {
    let gone = track(9, "Gone");
    let gone_path = device_path(&gone);
    let mut case = Case::new(
        vec![],
        inventory(&[(&gone_path, 1_000)], vec![synced(&gone, &gone_path)]),
    );
    case.unselected = UnselectedSongs::Leave;
    let built = case.build();
    assert_eq!(built.plan.remove.songs, 0);
    assert!(built.steps.remove.is_empty());
    assert_eq!(built.plan.storage.stays_bytes, 1_000);
}

#[test]
fn removal_reasons_say_why_a_song_left() {
    let commute = playlist_source(3, "Commute");
    let left = track_from(song(7, "Left"), commute.clone());
    let deleted = track(8, "Deleted");
    let offline = track(9, "Offline");
    let (left_path, deleted_path, offline_path) = (
        device_path(&left),
        device_path(&deleted),
        device_path(&offline),
    );
    let mut case = Case::new(
        vec![],
        inventory(
            &[
                (&left_path, 1_000),
                (&deleted_path, 1_000),
                (&offline_path, 1_000),
            ],
            vec![
                synced(&left, &left_path),
                synced(&deleted, &deleted_path),
                synced(&offline, &offline_path),
            ],
        ),
    );
    case.sources = vec![commute.clone()];
    case.library = HashMap::from([
        (left.stamp.path.clone(), LibraryFile::Available),
        (offline.stamp.path.clone(), LibraryFile::Unavailable),
    ]);
    let built = case.build();
    let mut got = reasons(&built.plan.remove);
    got.sort_by_key(|r| format!("{r:?}"));
    assert_eq!(
        got,
        [
            &SyncReason::DeletedFromLibrary,
            &SyncReason::LeftSource { source: commute }
        ]
    );
    assert!(
        !built.steps.remove.contains(&offline_path),
        "a song whose library drive is offline stays"
    );
}

#[test]
fn an_unreadable_selected_song_is_skipped_and_its_copy_kept() {
    let t = track(1, "Halo");
    let path = device_path(&t);
    let mut case = Case::new(
        vec![],
        inventory(&[(&path, 1_000)], vec![synced(&t, &path)]),
    );
    case.unreadable = vec![t.wanted.song.clone()];
    case.library
        .insert(t.stamp.path.clone(), LibraryFile::Available);
    let built = case.build();
    assert_eq!(reasons(&built.plan.skipped), [&SyncReason::Unreadable]);
    assert_eq!(built.plan.remove.songs, 0);
}

#[test]
fn a_users_file_with_the_same_song_is_adopted() {
    let t = track(1, "Halo");
    let path = device_path(&t);
    let built = Case::new(vec![t], inventory(&[(&path, 1_000)], vec![])).build();
    assert_eq!(reasons(&built.plan.adopted), [&SyncReason::Adopted]);
    assert_eq!(built.plan.copy.songs, 0);
    assert_eq!(built.steps.record.len(), 1);
    assert_eq!(built.steps.record[0].path, path);
    assert_eq!(built.plan.storage.stays_bytes, 1_000);
}

#[test]
fn a_users_other_file_is_kept_and_the_song_goes_beside_it() {
    let t = track(1, "Halo");
    let path = device_path(&t);
    let built = Case::new(vec![t], inventory(&[(&path, 4_321)], vec![])).build();
    let copy = &built.plan.copy.items[0];
    assert_eq!(
        copy.reason,
        SyncReason::RenamedBesideExisting {
            source: library_source(),
            existing_path: path.clone()
        }
    );
    assert_eq!(copy.path, path.replace(".mp3", " (2).mp3"));
    assert!(built.steps.remove.is_empty());
}

#[test]
fn a_selection_too_big_says_by_how_much_and_what_helps() {
    let big = playlist_source(4, "Favourites");
    let mut a = track_from(song(1, "Halo"), big.clone());
    let mut b = track(2, "Northbound");
    a.size = 7 * GB;
    a.decision = converting();
    a.size_converting_all = Some(2 * GB);
    b.size = 4 * GB;
    b.size_converting_all = Some(4 * GB);
    let mut case = Case::new(vec![a, b], inventory(&[], vec![]));
    case.sources = vec![big.clone(), library_source()];
    case.profile.reserve_bytes = GB;
    let plan = case.build().plan;
    assert!(!plan.storage.fits);
    assert_eq!(plan.storage.free_after_bytes, -(GB as i64));
    let shortfall = plan.shortfall.unwrap();
    assert_eq!(shortfall.bytes, 2 * GB, "1 GB over, plus the reserve");
    assert_eq!(shortfall.largest_source.unwrap().source, big);
    assert_eq!(shortfall.convert_all_saving_bytes, Some(5 * GB));
    assert_eq!(plan.converted_songs, 1);
    assert!(
        plan.conversion_target.is_some(),
        "a flagged target is shown"
    );
}

#[test]
fn playlist_files_are_written_when_changed_and_removed_when_stale() {
    let t = track(1, "Halo");
    let mut case = Case::new(vec![t.clone()], inventory(&[], vec![]));
    case.profile.layout.playlist_format = luminous_devices::PlaylistFileFormat::M3u8;
    case.playlists = vec![WantedPlaylist {
        name: "Commute".into(),
        songs: vec![t.wanted.song.clone()],
    }];
    let built = case.build();
    assert_eq!(built.plan.playlist_files_written, 1);
    let written = built
        .steps
        .write
        .iter()
        .find(|w| w.entry.kind == EntryKind::Playlist)
        .unwrap()
        .entry
        .clone();
    assert_eq!(written.path, "Playlists/Commute.m3u8");

    // Synced, unchanged: nothing to write.
    let path = device_path(&t);
    case.inventory = inventory(
        &[(&path, 1_000), (&written.path, written.size)],
        vec![synced(&t, &path), written.clone()],
    );
    assert_eq!(case.build().plan.playlist_files_written, 0);

    // The playlist was unselected.
    case.playlists.clear();
    let built = case.build();
    assert_eq!(built.plan.playlist_files_removed, 1);
    assert!(built.steps.remove.contains(&written.path));
}

#[test]
fn previews_carry_only_the_first_items() {
    let tracks = (1..=60).map(|i| track(i, &format!("Song {i}"))).collect();
    let plan = Case::new(tracks, inventory(&[], vec![])).build().plan;
    let preview = plan.preview();
    assert_eq!(preview.copy.items.len(), PREVIEW_ITEMS);
    assert_eq!(preview.copy.songs, 60);
}

#[test]
fn leave_on_device_still_moves_a_song_rather_than_duplicating_it() {
    let t = track(1, "Halo");
    let old = "Music/Old Layout/Halo.mp3";
    let mut case = Case::new(
        vec![t.clone()],
        inventory(&[(old, 1_000)], vec![synced(&t, old)]),
    );
    case.unselected = UnselectedSongs::Leave;
    let built = case.build();
    assert_eq!(
        reasons(&built.plan.update),
        [&SyncReason::Moved { from: old.into() }]
    );
    assert_eq!(built.steps.remove, [old]);
    assert_eq!(built.plan.remove.songs, 0);
}
