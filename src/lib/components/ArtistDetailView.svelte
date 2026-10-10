<script lang="ts">
  import { isRemoteSource } from "../utils/remoteSource";
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import { formatDuration, formatHoursMinutes } from "../utils/formatters";
  import CoverArt from "./CoverArt.svelte";
  import CoverMosaic from "./CoverMosaic.svelte";
  import GenreChips from "./GenreChips.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import AlbumRowCard from "./AlbumRowCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import PlaylistRowCard from "./PlaylistRowCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import PlaylistCardContextMenu from "./PlaylistCardContextMenu.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import { tasksStore } from "../stores/tasks.svelte";
  import { prefs, type CollectionViewMode } from "../stores/prefs.svelte";
  import TagEditor from "./TagEditor.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import CardSection from "./CardSection.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import ArtistProfileEditor from "./ArtistProfileEditor.svelte";
  import SongSelectionToolbar from "./SongSelectionToolbar.svelte";
  import SongTable, { type SongTableRow } from "./SongTable.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import {
    PencilSimpleIcon as Edit3,
    ArrowSquareOutIcon as OpenInPicard,
    ArrowsClockwiseIcon as RefreshCw,
    DownloadSimpleIcon as RetrieveDetails,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    DotsThreeIcon as MoreHorizontal,
    ChartBarIcon as BarChart2,
    ArrowDownLeftIcon as ArrowDownLeft,
    ShareNetworkIcon as Share,
    ImageIcon as RetrieveImage
  } from "phosphor-svelte";
  import EntityInfoCard from "./EntityInfoCard.svelte";
  import { contextStore, entitySubject } from "../stores/context.svelte";
  import ShareModal from "./ShareModal.svelte";
  import type { Song, Playlist, AlbumItem, PlayContext, ArtistProfile, ExtendedArtworkResponse } from "../types";
  import { getArtistAlbums, classifyRelease } from "../utils/artist";
  import {
    songsToCoverStack,
    resolveArtistPortraitUrl,
    resolveArtistLogoUrl,
    resolveArtistBackgroundUrl,
  } from "../utils/covers";
  import { parseMultiValue, joinMultiValue } from "../utils/multiValue";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { i18n } from "../stores/i18n.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import { openInPicard } from "../utils/picard";
  import { compareSongs } from "../utils/songSort";

  let { artistName }: { artistName: string } = $props();

  let songs = $state<Song[]>([]);
  let playlists = $state<Playlist[]>([]);
  let compilations = $state<AlbumItem[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let retrievingDetails = $state(false);
  let retrievingImage = $state(false);

  let albumContextMenuState = $state<{ x: number; y: number; album: AlbumItem } | null>(null);
  let playlistContextMenuState = $state<{ x: number; y: number; playlist: Playlist } | null>(null);
  let singleContextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  // "Not included" tracks stay visible/individually playable but drop out of
  // the whole-artist Play/Shuffle Play actions (#104).
  let playableSongs = $derived(songs.filter((s) => !s.not_included));
  let editingSongId = $state<number | null>(null);
  let isEditorOpen = $state(false);
  let showShareModal = $state(false);
  let isBioExpanded = $state(false);
  let selectedKeys = $state<Set<string>>(new Set());

  let overflowMenuPos = $state<{ x: number; y: number } | null>(null);

  function toggleOverflowMenu(e: MouseEvent) {
    if (overflowMenuPos) {
      overflowMenuPos = null;
    } else {
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      overflowMenuPos = { x: rect.left, y: rect.bottom + 4 };
    }
  }

  function handleOpenAllInPicard() {
    if (songs.length === 0) return;
    openInPicard(songs.map((s) => s.id));
  }

  let artistProfile = $derived(collectionStore.getArtistProfile(artistName));
  let hasTags = $derived((artistProfile?.tags?.length ?? 0) > 0);

  // "Retrieve Artist Details" needs a MusicBrainz artist MBID: either
  // already captured on the profile (via "Retrieve Album Details" or a
  // previous run of this action), or resolvable from a tagged song.
  let hasMusicbrainzArtistId = $derived(
    !!artistProfile?.musicbrainz_artist_id ||
      songs.some((s) => (s.musicbrainz_artist_id ?? s.musicbrainz_album_artist_id ?? "").trim().length > 0)
  );

  // Everything fetched about the artist (facts, bio, events, links) lives in contextStore and shows
  // in the info sidebar; this view only renders it, as a card, while the sidebar is hidden.
  let infoSubject = $derived(entitySubject("artist", artistName, songs));
  const info = contextStore.for(() => infoSubject);
  let showInfoCard = $derived(
    !windowLayoutStore.isInfoSidebarVisible && (info.sections.length > 0 || info.loading)
  );

  // Locally-discovered artist visuals (#98/#761) — portrait/logo/fanart,
  // fetched on demand per artist since scanning every artist's folder
  // eagerly would be far too expensive (see #758's design notes).
  let artistArtwork = $state<ExtendedArtworkResponse | null>(null);
  $effect(() => {
    const name = artistName;
    let cancelled = false;
    collectionStore.getExtendedArtworkForArtist(name).then((result) => {
      if (!cancelled) artistArtwork = result;
    });
    return () => {
      cancelled = true;
    };
  });
  let artistPortraitUrl = $derived(
    resolveArtistPortraitUrl(artistArtwork?.artist_portrait_uri, artistProfile?.fetched_image_filename)
  );
  let mosaicSpareWidth = $state(0);
  let bandLogoUrl = $derived(
    resolveArtistLogoUrl(artistArtwork?.band_logo_uri, artistProfile?.fetched_logo_filename)
  );
  let fanartBannerUrl = $derived(
    resolveArtistBackgroundUrl(artistArtwork?.fanart_uri, artistProfile?.fetched_background_filename)
  );

  function handleTagClick(tag: string) {
    collectionStore.searchQuery = `artist-tag:${tag}`;
    navigationStore.selectedArtistName = null;
    navigationStore.activeTab = "collection";
    navigationStore.activeSubTab = "artists";
  }

  function handleAlbumContextMenu(event: MouseEvent, album: AlbumItem) {
    event.preventDefault();
    albumContextMenuState = { x: event.clientX, y: event.clientY, album };
  }

  function handlePlaylistContextMenu(event: MouseEvent, playlist: Playlist) {
    event.preventDefault();
    playlistContextMenuState = { x: event.clientX, y: event.clientY, playlist };
  }

  function handleRowContextMenu(event: MouseEvent, row: SongTableRow) {
    if (row.song) singleContextMenuState = { x: event.clientX, y: event.clientY, song: row.song };
  }

  function handlePlaySelected() {
    if (selectedKeys.size === 0) return;
    const selectedList = singleSongs.filter((s) => selectedKeys.has(String(s.id)));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0);
    }
  }

  async function handleBulkAddSinglesToPlaylist() {
    if (selectedKeys.size === 0) return;
    const songIds = Array.from(selectedKeys, Number);
    const label = songIds.length === 1 ? "1 song" : `${songIds.length} songs`;
    await playlistsStore.addSongsToActiveTarget(songIds, label);
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  async function refetchSongs() {
    const fetchedSongs = await invoke<Song[]>("get_songs_by_artist", { artist: artistName });
    songs = Array.isArray(fetchedSongs) ? fetchedSongs : [];
  }

  // Rescans disk for this artist's tracks, local portrait/band logo/fanart
  // banner (#761) and re-fetches its MusicBrainz/Wikipedia context (#23),
  // bypassing both caches — mirrors AlbumDetailView's handleRefreshAlbum.
  // Fixes #867 & #1399: rescans only this artist's songs via `rescan_songs`
  // rather than triggering a whole-library scan.
  async function handleRescanArtist() {
    if (refreshing || collectionStore.isScanning) return;
    refreshing = true;
    try {
      if (songs.length > 0) {
        await invoke("rescan_songs", { songIds: songs.map((s) => s.id) });
      }
      await collectionStore.refreshLibrary();
      await refetchSongs();
      const [artwork] = await Promise.all([
        collectionStore.getExtendedArtworkForArtist(artistName, true),
        info.refresh(),
      ]);
      artistArtwork = artwork;
      toastStore.show(i18n.t("artistDetail.refreshSuccess", {}, "Artist artwork and bio refreshed"));
    } catch (err) {
      console.error("Failed to refresh artist:", err);
      toastStore.show(i18n.t("artistDetail.refreshError", {}, "Failed to refresh artist"), "error");
    } finally {
      refreshing = false;
    }
  }

  let retrievingAll = $state(false);

  // Multi-step artist enrichment (#1143): fetches MusicBrainz links,
  // Wikipedia summary, and artist image sequentially, showing progress
  // in a single unified task notification instead of multiple stacked toasts.
  async function handleRetrieveArtistAll() {
    if (retrievingAll || !hasMusicbrainzArtistId) return;
    retrievingAll = true;
    const taskId = `artist-enrichment-${artistName.toLowerCase()}`;
    const taskName = i18n.t("artistDetail.retrievingArtistTask", {}, "Retrieving artist information");
    tasksStore.startTask({
      id: taskId,
      label: i18n.t("artistDetail.retrievingDetails", {}, "Retrieving artist details..."),
      taskName,
      total: 3,
    });

    try {
      // Step 1: MusicBrainz relations & links
      await collectionStore.retrieveArtistDetails(artistName).catch((e) => {
        console.warn("Failed to retrieve artist details:", e);
      });

      // Step 2: Wikipedia summary & context
      tasksStore.updateTask(taskId, {
        current: 1,
        label: i18n.t("artistDetail.retrievingBio", {}, "Retrieving artist summary..."),
      });
      await info.refresh().catch((e) => {
        console.warn("Failed to retrieve artist context:", e);
      });

      // Step 3: Artist portrait image
      tasksStore.updateTask(taskId, {
        current: 2,
        label: i18n.t("artistDetail.retrievingImage", {}, "Retrieving artist image..."),
      });
      await collectionStore.retrieveArtistImage(artistName, { onlyMissing: true }).catch((e) => {
        console.warn("Failed to retrieve artist image:", e);
      });

      tasksStore.completeTask(
        taskId,
        i18n.t("artistDetail.enrichmentComplete", {}, "Artist information retrieved")
      );
    } catch (err) {
      console.error("Failed to retrieve artist data:", err);
      tasksStore.failTask(taskId, String(err));
    } finally {
      retrievingAll = false;
    }
  }

  let lastAutoFetchedArtist = $state<string | null>(null);
  $effect(() => {
    const currentArtist = artistName;
    if (!currentArtist || songs.length === 0 || !prefs.onlineEnabled) return;
    if (lastAutoFetchedArtist === currentArtist) return;

    const profile = artistProfile;
    // Image types enabled in Settings that haven't been attempted yet (#1276)
    // — also backfills a logo/background for artists fetched before those
    // existed, or after a type is switched back on.
    const imagesMissing =
      (prefs.fanartFetchPhoto && !profile?.image_fetched) ||
      (prefs.fanartFetchLogo && !profile?.logo_fetched) ||
      (prefs.fanartFetchBackground && !profile?.background_fetched);
    if (profile?.details_fetched && !imagesMissing) {
      lastAutoFetchedArtist = currentArtist;
      return;
    }
    if (!hasMusicbrainzArtistId) return;

    lastAutoFetchedArtist = currentArtist;
    const detailsFetched = !!profile?.details_fetched;
    collectionStore.isContextEnrichmentEnabled().then((enabled) => {
      if (!enabled || retrievingAll || tasksStore.isTaskActive(`artist-enrichment-${currentArtist.toLowerCase()}`)) return;
      if (detailsFetched) {
        // Details are done — quietly fill in just the missing images.
        collectionStore.retrieveArtistImage(currentArtist, { onlyMissing: true }).catch((e) => {
          console.warn("Failed to retrieve artist images:", e);
        });
      } else {
        handleRetrieveArtistAll();
      }
    });
  });

  // The artist detail overflow menu's "Retrieve Artist Details" (#1123) —
  // the artist-level equivalent of AlbumDetailView's handleRetrieveAlbumDetails.
  async function handleRetrieveArtistDetails() {
    if (retrievingDetails || !hasMusicbrainzArtistId || !prefs.onlineEnabled) return;
    retrievingDetails = true;
    const taskId = `artist-details-${artistName.toLowerCase()}`;
    tasksStore.startTask({
      id: taskId,
      label: i18n.t("artistDetail.retrievingDetails", {}, "Retrieving artist details..."),
      taskName: i18n.t("artistDetail.retrieveArtistDetails", {}, "Retrieve Artist Details"),
      total: 1,
    });
    try {
      const result = await collectionStore.retrieveArtistDetails(artistName);
      const label = result.added_count > 1
          ? i18n.plural("artistDetail.retrieveDetailsSuccess", result.added_count)
          : i18n.t("artistDetail.retrieveDetailsNoResults", {}, "No additional details found on MusicBrainz");
      tasksStore.completeTask(taskId, label);
    } catch (err) {
      console.error("Failed to retrieve artist details:", err);
      tasksStore.failTask(taskId, String(err));
    } finally {
      retrievingDetails = false;
    }
  }

  // Artist detail overflow menu's "Retrieve Artist Image" (#1127) — fetches the
  // photo, logo and background from fanart.tv (if a key is configured), whatever
  // the Settings toggles say, with Wikidata as the photo fallback.
  async function handleRetrieveArtistImage() {
    if (retrievingImage || !hasMusicbrainzArtistId || !prefs.onlineEnabled) return;
    retrievingImage = true;
    const taskId = `artist-image-${artistName.toLowerCase()}`;
    tasksStore.startTask({
      id: taskId,
      label: i18n.t("artistDetail.retrievingImage", {}, "Retrieving artist image..."),
      taskName: i18n.t("artistDetail.retrieveArtistImage", {}, "Retrieve Artist Image"),
      total: 1,
    });
    try {
      const result = await collectionStore.retrieveArtistImage(artistName);
      const label = result.uri
        ? (result.source === "fanart"
            ? i18n.t("artistDetail.retrieveImageSuccessFanart", {}, "Artist image retrieved from fanart.tv")
            : i18n.t("artistDetail.retrieveImageSuccessWikidata", {}, "Artist image retrieved from Wikidata"))
        : result.logo_uri || result.background_uri
          ? i18n.t("artistDetail.retrieveArtworkSuccessFanart", {}, "Artist artwork retrieved from fanart.tv")
          : i18n.t("artistDetail.retrieveImageNoResults", {}, "No artist image found");
      tasksStore.completeTask(taskId, label);
    } catch (err) {
      console.error("Failed to retrieve artist image:", err);
      tasksStore.failTask(taskId, String(err));
    } finally {
      retrievingImage = false;
    }
  }

  async function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    tagsStore.load();

    const editedSongId = editingSongId;
    await refetchSongs();

    // Renaming the just-edited track's (album) artist can leave this artist
    // with none of its previously-loaded songs still matching `artistName` —
    // get_songs_by_artist then comes back empty even though the artist still
    // exists, just under a new name. Follow it instead of showing a stale,
    // empty page.
    if (songs.length === 0 && editedSongId !== null) {
      try {
        const details = await invoke<{ artist: string; album_artist: string }>("get_song_details", { songId: editedSongId });
        const newArtist = details.album_artist || details.artist;
        if (newArtist && newArtist !== artistName) {
          navigationStore.selectedArtistName = newArtist;
        }
      } catch (err) {
        console.error("Failed to resolve current artist name after tag edit:", err);
      }
    }
  }

  async function rateSingle(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  async function handlePlaySingle(song: Song) {
    const queuePl = await playlistsStore.requireQueue();
    await playerStore.playSongs([song.id], 0, queuePl?.id, undefined, "Queue");
  }

  type SingleSortField = keyof Song | "track";
  let singleSortField = $state<SingleSortField>("track");
  let singleSortAsc = $state(true);

  function toggleSingleSort(field: string) {
    const f = field as SingleSortField;
    if (singleSortField === f) {
      singleSortAsc = !singleSortAsc;
    } else {
      singleSortField = f;
      singleSortAsc = true;
    }
  }

  async function handleAddSingleToPlaylist(songId: number) {
    const songObj = songs.find((s) => s.id === songId);
    await playlistsStore.addSongsToActiveTarget([songId], songObj?.title || "Song");
  }

  let albums = $derived(getArtistAlbums(collectionStore.albums, artistName));
  // Artists with no proper album releases (loose singles only) have nothing
  // in `albums` to draw covers from — fall back to the songs' own art.
  let headerCovers = $derived(
    albums.length > 0
      ? albums.map((a) => ({
          artEmbedded: a.art_embedded,
          artAutomatic: a.art_automatic,
          artManual: a.art_manual,
        }))
      : songsToCoverStack(songs)
  );

  $effect(() => {
    const requested = artistName;
    isBioExpanded = false;
    // Track collectionStore.songs so artist details update when the library changes (e.g. new albums added)
    const _libraryVersion = collectionStore.songs;
    loading = true;
    Promise.all([
      invoke<Song[]>("get_songs_by_artist", { artist: requested }),
      invoke<Playlist[]>("get_playlists_by_artist", { artist: requested }),
      invoke<AlbumItem[]>("get_compilations_by_artist", { artist: requested }),
      invoke<ArtistProfile>("get_artist_profile", { artist: requested })
    ])
      .then(([fetchedSongs, fetchedPlaylists, fetchedCompilations, fetchedProfile]) => {
        if (requested !== artistName) return;
        songs = Array.isArray(fetchedSongs) ? fetchedSongs : [];
        playlists = Array.isArray(fetchedPlaylists) ? fetchedPlaylists.filter((p) => !p.is_queue) : [];
        compilations = Array.isArray(fetchedCompilations) ? fetchedCompilations : [];
        if (fetchedProfile?.artist_key) {
          collectionStore.artistProfiles[fetchedProfile.artist_key.toLowerCase()] = fetchedProfile;
        }
      })
      .catch((err) => {
        console.error("Failed to load artist detail:", err);
      })
      .finally(() => {
        if (requested === artistName) loading = false;
      });
  });

  function goBackToArtists() {
    navigationStore.selectedArtistName = null;
    navigationStore.activeSubTab = "artists";
  }

  function deriveArtistGenres(list: Song[]): string {
    const counts = new Map<string, number>();
    for (const s of list) {
      if (!s.genre) continue;
      for (const g of parseMultiValue(s.genre)) {
        const trimmed = g.trim();
        if (trimmed) {
          counts.set(trimmed, (counts.get(trimmed) ?? 0) + 1);
        }
      }
    }
    if (counts.size === 0) return "";
    const sorted = [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([g]) => g);
    return joinMultiValue(sorted);
  }

  let rawGenre = $derived(deriveArtistGenres(songs));
  // Header shows curated artist tags only, not the file-embedded genre --
  // that's already covered by the Genres page and every album/song beneath
  // this artist, so repeating it here is just noise. A tag is excluded if
  // it duplicates any genre *anywhere in the library* (not just this
  // artist's own songs) -- matching the "Artist Only Tags" filter on the
  // Genres page -- since a name like "Electronic" is still a real genre
  // even if this particular artist's own files don't happen to use it.
  let artistOnlyTags = $derived.by(() => {
    const tags = artistProfile?.tags;
    if (!tags?.length) return tags;
    const genreNames = new Set(tagsStore.allTags.map((t) => t.name.toLowerCase()));
    return tags.filter((t) => !genreNames.has(t.trim().toLowerCase()));
  });
  let hasChips = $derived((artistOnlyTags?.length ?? 0) > 0);

  let totalDurationLabel = $derived.by(() => {
    const totalNs = songs.reduce((sum, s) => sum + (s.length_nanosec ?? 0), 0);
    return formatHoursMinutes(Math.round(totalNs / 1_000_000_000 / 60));
  });

  // Shares classifyRelease() with the per-card badge everywhere else in the
  // app, so the discography tabs agree with how a release is labeled
  // elsewhere: multi-disc releases are "Sets" regardless of duration, then
  // Albums/EPs by total duration (EP = under 30 minutes) and Singles by track count.
  let sets = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "set"));
  let fullAlbums = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "album"));
  let eps = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "ep"));
  let singles = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "single"));

  // One Cards/Rows toggle serves every release and playlist section; it sits
  // on the header of the top-most one that is showing.
  let viewModeSection = $derived(
    sets.length > 0
      ? "sets"
      : fullAlbums.length > 0
        ? "albums"
        : eps.length > 0
          ? "eps"
          : compilations.length > 0
            ? "compilations"
            : "playlists"
  );

  function viewModeChangeFor(section: string) {
    return section === viewModeSection ? (mode: CollectionViewMode) => prefs.setArtistReleasesViewMode(mode) : undefined;
  }

  let songsText = $derived(
    i18n.plural("playlists.songsCount", songs.length)
  );

  // Songs with no album tag at all are excluded from get_albums() entirely
  // (it requires a non-empty album), so they'd never surface via `albums`/
  // `singles`. Surface each such song individually as its own "loose
  // single" — computed directly from this artist's songs rather than gated
  // on "this artist has zero proper albums", which used to make every
  // blank-album song vanish the moment the artist had even one real album
  // elsewhere (its `albums.length` going from 0 to 1 turned this fallback
  // off entirely, even though the loose songs and real albums are disjoint
  // sets and can coexist).
  let looseSongs = $derived(
    songs.filter((s) => !s.album).sort((a, b) => (a.title || "").localeCompare(b.title || ""))
  );

  // Grouped singles are AlbumItems (track_count === 1), but they render as a
  // song table alongside loose singles — resolve each back to its one song.
  let singleSongs = $derived([
    ...singles.map((a) => songs.find((s) => s.album === a.album)).filter((s): s is Song => s !== undefined),
    ...looseSongs,
  ]);

  let sortedSingleSongs = $derived.by(() => {
    if (singleSortField === "track") {
      if (singleSortAsc) return singleSongs;
      return [...singleSongs].reverse();
    }
    const field = singleSortField as keyof Song;
    return [...singleSongs].sort((a, b) => compareSongs(a, b, field, singleSortAsc));
  });

  // Mirrors AlbumDetailView/CollectionView/PlaylistView/AutoPlaylistDetailView's
  // identical formula so this table's columns match what's shown everywhere else.
  // Default column widths (px or fr) — used when no saved width exists for a column.
  const ARTIST_COL_DEFAULTS: Partial<Record<keyof typeof collectionStore.visibleColumns, string>> = {
    track: "48px", title: "2fr", artist: "1.5fr", album: "1.5fr",
    composer: "1.5fr", album_artist: "1.5fr", format: "64px", year: "60px", originalyear: "60px",
    genre: "1.2fr", grouping: "1.2fr", bpm: "60px", initial_key: "60px",
    bitrate: "70px", samplerate: "75px", bitdepth: "65px", channels: "70px",
    filesize: "75px", rating: "96px", playcount: "70px", skipcount: "70px",
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", library: "130px", actions: "80px",
  };

  let singleDiscCount = $derived(singleSongs.reduce((max, s) => Math.max(max, s.disc ?? 1), 1));

  function songToRow(song: Song): SongTableRow {
    const disconnected = !song.unavailable && collectionStore.isPathOnDisconnectedDrive(song.path);
    return {
      key: String(song.id),
      song,
      disabled: song.unavailable || disconnected,
      disabledTooltip: disconnected ? i18n.t("collection.driveDisconnectedTooltip") : undefined,
    };
  }

  let singleTableRows = $derived(sortedSingleSongs.map(songToRow));

  function openAlbum(album: AlbumItem) {
    navigationStore.viewAlbum(album.album || "");
  }

  // Mirrors PlaylistsCollectionView's openAuto/openPlaylist split so genre/decade
  // auto-playlists open in AutoPlaylistDetailView (Auto-Play toggle, etc.) here too,
  // instead of always falling through to the custom-playlist detail view. Smart
  // Playlists are also dynamic_enabled but are user-authored rule playlists, not
  // system auto-playlists, so they must go through the normal viewPlaylist path.
  function openPlaylist(playlist: Playlist) {
    if (playlist.dynamic_enabled && !isSmartPlaylistSpec(playlist.dynamic_spec)) {
      const isDecade = playlist.dynamic_spec?.startsWith("decade:") ?? false;
      navigationStore.viewAutoPlaylist(
        isDecade
          ? { kind: "decade", decade: playlist.dynamic_spec?.replace(/^decade:/, "") ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
          : { kind: "genre", genre: playlist.dynamic_spec?.replace(/^tag:/, "") ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
      );
      return;
    }
    playlistsStore.selectPlaylist(playlist.id);
    navigationStore.viewPlaylist(playlist.id);
  }

  async function handleToggleStatsExcluded() {
    await statsExclusionsStore.toggleWithToast("artist", artistName);
  }

  async function handlePlayAll() {
    if (playableSongs.length === 0) return;
    const queuePl = await playlistsStore.requireQueue();
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(playableSongs.map((s) => s.id), 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      navigationStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleShufflePlay() {
    if (playableSongs.length === 0) return;
    const queuePl = await playlistsStore.requireQueue();
    const shuffledIds = shuffleArray(playableSongs.map((s) => s.id));
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(shuffledIds, 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      navigationStore.viewPlaylist(queuePl.id);
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-y-auto bg-brand-main text-brand-text-secondary h-full" use:rememberScroll={`artist-detail:${artistName}`}>
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md table-surface-blur px-6 {windowLayoutStore.isDetailHeaderCollapsed ? 'py-3' : 'pt-6 pb-6'}">
    {#if fanartBannerUrl && !windowLayoutStore.isDetailHeaderCollapsed}
      <div class="absolute inset-0 z-0 overflow-hidden pointer-events-none" aria-hidden="true">
        <img src={fanartBannerUrl} alt="" class="w-full h-full object-cover opacity-25" />
        <div class="absolute inset-0 bg-gradient-to-t from-brand-main via-brand-main/70 to-brand-main/30"></div>
      </div>
    {/if}
    <div class="flex items-start justify-between gap-6 relative z-10">
      <div class="flex flex-col justify-end gap-1.5 min-w-0 max-w-xl">
        {#if !windowLayoutStore.isDetailHeaderCollapsed}
        <h1 class="text-3xl @xl:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5">{artistName}</h1>

        <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-primary font-medium">
          <span>{songsText}</span>
          <span>•</span>
          <span>{totalDurationLabel}</span>
        </div>
        {/if}

        <div class="flex flex-wrap items-center gap-3 {windowLayoutStore.isDetailHeaderCollapsed ? '' : 'mt-3'} select-none">
          <PlayShuffleButtons
            onPlayAll={handlePlayAll}
            onShufflePlay={handleShufflePlay}
            disabled={loading || songs.length === 0}
          />
          <IconActionButton
            onclick={() => pinnedStore.toggle("artist", artistName)}
            title={pinnedStore.isPinned("artist", artistName)
              ? i18n.t("artistDetail.unpinHome")
              : i18n.t("artistDetail.pinHome")}
          >
            {#snippet icon()}
              {#if pinnedStore.isPinned("artist", artistName)}
                <PinOff class="w-4 h-4" />
              {:else}
                <Pin class="w-4 h-4" />
              {/if}
            {/snippet}
          </IconActionButton>
          <IconActionButton
            onclick={() => { showShareModal = true; }}
            title={i18n.t("shareModal.menuItem")}
          >
            {#snippet icon()}<Share class="w-4 h-4" />{/snippet}
          </IconActionButton>
          <button
            onclick={toggleOverflowMenu}
            title={i18n.t("playlists.moreActionsTooltip", {}, "More actions")}
            class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors shadow-xs cursor-pointer"
          >
            <MoreHorizontal class="w-4 h-4" />
          </button>
        </div>
      </div>

      {#if !windowLayoutStore.isDetailHeaderCollapsed && (bandLogoUrl || artistPortraitUrl || headerCovers.length > 0)}
        <!-- The logo sits beside the photo rather than replacing the name: logos are often hard to read.
             The mosaic's box is wider than its grid, so the logo shifts right by half the empty
             width to centre between the action buttons and the covers. That is only right while
             the logo is directly left of the end-aligned mosaic and both gaps are this row's gap-6. -->
        <div class="hidden @xl:flex items-center justify-end gap-6 min-w-0 flex-1">
          {#if bandLogoUrl}
            <img
              src={bandLogoUrl}
              alt=""
              class="hidden @4xl:block h-16 @5xl:h-20 @6xl:h-24 w-auto max-w-64 @5xl:max-w-80 @6xl:max-w-96 min-w-0 shrink object-contain object-right"
              style:transform="translateX({mosaicSpareWidth / 2}px)"
            />
          {/if}
          {#if artistPortraitUrl || headerCovers.length > 0}
            <!-- Fills the header's spare width: more albums = more columns/rows, never overflowing (#1496). -->
            <CoverMosaic covers={headerCovers} heroImageUrl={artistPortraitUrl} heroImageAlt={artistName} sizeClass="h-36" fit align="end" maxCovers={16} bind:spareWidth={mosaicSpareWidth} />
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <div class="px-6 pt-6 flex flex-col gap-8">
    {#if !windowLayoutStore.isDetailHeaderCollapsed}
      {#if hasChips || (showInfoCard && !windowLayoutStore.isOverviewExpanded)}
        <div class="flex flex-wrap items-center justify-between gap-3">
          {#if hasChips}
            <GenreChips
              curatedTags={artistOnlyTags}
              onCuratedTagClick={handleTagClick}
              curatedTagTitle={(tag) => `Filter artists tagged "${tag}"`}
              variant="full"
            />
          {/if}
          {#if showInfoCard && !windowLayoutStore.isOverviewExpanded}
            <button
              type="button"
              onclick={() => windowLayoutStore.setOverviewExpanded(true)}
              class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border border-brand-border bg-brand-sidebar text-brand-text-secondary text-xs font-medium hover:text-brand-text-primary hover:border-brand-accent/40 transition-colors cursor-pointer shrink-0 ml-auto"
            >
              <ArrowDownLeft class="w-3.5 h-3.5" />
              <span>{i18n.t('artistDetail.artistInfo', {}, 'Artist Info')}</span>
            </button>
          {/if}
        </div>
      {/if}
    {/if}

    {#if showInfoCard && !windowLayoutStore.isDetailHeaderCollapsed && windowLayoutStore.isOverviewExpanded}
      <EntityInfoCard view={info}>
        {#snippet title()}<span>{i18n.t('artistDetail.artistInfo', {}, 'Artist Info')}</span>{/snippet}
      </EntityInfoCard>
    {/if}

    {#if sets.length > 0}
      <CardSection
        title={i18n.t('artistDetail.setsFilter', { count: sets.length })}
        viewMode={prefs.artistReleasesViewMode}
        onViewModeChange={viewModeChangeFor("sets")}
      >
        {#snippet children(mode)}
          {#each sets as album (album.album)}
            {#if mode === "rows"}
              <AlbumRowCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {:else}
              <AlbumCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {/if}
          {/each}
        {/snippet}
      </CardSection>
    {/if}

    {#if fullAlbums.length > 0}
      <CardSection
        title={i18n.t('artistDetail.albumsFilter', { count: fullAlbums.length })}
        viewMode={prefs.artistReleasesViewMode}
        onViewModeChange={viewModeChangeFor("albums")}
      >
        {#snippet children(mode)}
          {#each fullAlbums as album (album.album)}
            {#if mode === "rows"}
              <AlbumRowCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {:else}
              <AlbumCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {/if}
          {/each}
        {/snippet}
      </CardSection>
    {/if}

    {#if eps.length > 0}
      <CardSection
        title={i18n.t('artistDetail.epsFilter', { count: eps.length })}
        viewMode={prefs.artistReleasesViewMode}
        onViewModeChange={viewModeChangeFor("eps")}
      >
        {#snippet children(mode)}
          {#each eps as album (album.album)}
            {#if mode === "rows"}
              <AlbumRowCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {:else}
              <AlbumCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {/if}
          {/each}
        {/snippet}
      </CardSection>
    {/if}

    {#if singleSongs.length > 0}
      <div class="flex flex-col gap-3">
        <h2 class="text-xl font-semibold text-brand-text-primary">{i18n.t('artistDetail.singlesFilter', { count: singleSongs.length })}</h2>
        <div class="border border-brand-border rounded-lg bg-brand-sidebar/50 backdrop-blur-xl shadow-2xl overflow-hidden table-surface-blur">
          <SongTable
            rows={singleTableRows}
            mode="track"
            discCount={singleDiscCount}
            leadingColumnWidth="36px"
            colDefaults={ARTIST_COL_DEFAULTS}
            sortField={singleSortField}
            sortAsc={singleSortAsc}
            onToggleSort={toggleSingleSort}
            bind:selectedKeys
            onRowDoubleClick={(row) => row.song && handlePlaySingle(row.song)}
            onRowContextMenu={handleRowContextMenu}
            onRate={rateSingle}
            onAddToPlaylist={(song) => handleAddSingleToPlaylist(song.id)}
            onEditTags={(song) => openTagEditor(song.id)}
          />
        </div>
      </div>
    {/if}

    {#if albums.length === 0 && singleSongs.length === 0 && !loading}
      <p class="text-xs text-brand-text-secondary py-8 text-center">{i18n.t('artistDetail.noReleasesFound')}</p>
    {/if}
  </div>

  {#if compilations.length > 0}
    <div class="px-6 pt-10">
      <CardSection
        title={i18n.t('artistDetail.compilationsFeaturing', { artist: artistName })}
        viewMode={prefs.artistReleasesViewMode}
        onViewModeChange={viewModeChangeFor("compilations")}
      >
        {#snippet children(mode)}
          {#each compilations as album (album.album)}
            {#if mode === "rows"}
              <AlbumRowCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {:else}
              <AlbumCard
                {album}
                onclick={() => openAlbum(album)}
                oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
              />
            {/if}
          {/each}
        {/snippet}
      </CardSection>
    </div>
  {/if}

  {#if playlists.length > 0}
    <div class="px-6 pt-10 {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
      <CardSection
        title={i18n.t('artistDetail.playlistsFeaturing', { artist: artistName })}
        viewMode={prefs.artistReleasesViewMode}
        onViewModeChange={viewModeChangeFor("playlists")}
      >
        {#snippet children(mode)}
          {#each playlists as playlist (playlist.id)}
            {#if mode === "rows"}
              <PlaylistRowCard
                {playlist}
                onClick={() => openPlaylist(playlist)}
                oncontextmenu={(e) => handlePlaylistContextMenu(e, playlist)}
              />
            {:else}
              <PlaylistCard
                {playlist}
                onClick={() => openPlaylist(playlist)}
                oncontextmenu={(e) => handlePlaylistContextMenu(e, playlist)}
              />
            {/if}
          {/each}
        {/snippet}
      </CardSection>
    </div>
  {:else}
    <div class="{playerStore.currentSong ? 'pb-28' : 'pb-6'}"></div>
  {/if}
</div>

{#if albumContextMenuState}
  {@const album = albumContextMenuState.album}
  <AlbumContextMenu
    x={albumContextMenuState.x}
    y={albumContextMenuState.y}
    albumName={album.album || i18n.t("collection.unknownAlbum")}
    artistName={album.artist || artistName}
    onPlay={async () => {
      let songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
      const playable = songs.filter((s) => !s.not_included);
      if (playable.length > 0) {
        const context: PlayContext = { type: "album", album: album.album || "", albumArtist: album.artist || undefined };
        playerStore.playSongs(playable.map(s => s.id), 0, undefined, context);
      }
    }}
    onAddToPlaylist={async () => {
      let songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
      const playable = songs.filter((s) => !s.not_included);
      if (playable.length > 0) {
        await playlistsStore.addSongsToActiveTarget(
          playable.map(s => s.id),
          album.album || i18n.t("collection.unknownAlbum")
        );
      }
    }}
    onGoToArtist={album.artist && album.artist !== artistName ? () => navigationStore.viewArtist(album.artist || "") : undefined}
    onClose={() => { albumContextMenuState = null; }}
  />
{/if}

{#if playlistContextMenuState}
  <PlaylistCardContextMenu
    x={playlistContextMenuState.x}
    y={playlistContextMenuState.y}
    playlist={playlistContextMenuState.playlist}
    onClose={() => { playlistContextMenuState = null; }}
  />
{/if}

{#if singleContextMenuState}
  {@const song = singleContextMenuState.song}
  {@const selectedSongs = selectedKeys.size > 1 ? singleSongs.filter((s) => selectedKeys.has(String(s.id))) : undefined}
  <SongContextMenu
    x={singleContextMenuState.x}
    y={singleContextMenuState.y}
    {song}
    selectedCount={selectedKeys.size}
    selectedSongIds={Array.from(selectedKeys, Number)}
    {selectedSongs}
    onPlay={() => {
      if (selectedKeys.size > 1) {
        handlePlaySelected();
      } else {
        handlePlaySingle(song);
      }
    }}
    onAddToPlaylist={() => {
      if (selectedKeys.size > 1) {
        handleBulkAddSinglesToPlaylist();
      } else {
        handleAddSingleToPlaylist(song.id);
      }
    }}
    onEditTags={() => openTagEditor(song.id)}
    onOpenInPicard={() => openInPicard(selectedKeys.size > 1 ? Array.from(selectedKeys, Number) : [song.id])}
    onClose={() => { singleContextMenuState = null; }}
  />
{/if}

{#if overflowMenuPos}
  <ContextMenu
    x={overflowMenuPos.x}
    y={overflowMenuPos.y}
    onClose={() => { overflowMenuPos = null; }}
  >
    <ContextMenuItem
      icon={Edit3}
      label={i18n.t("artistDetail.editArtistDetails", {}, "Edit Artist Details")}
      onclick={() => { isEditorOpen = true; overflowMenuPos = null; }}
    />
    <ContextMenuItem
      icon={RefreshCw}
      label={i18n.t("artistDetail.refresh", {}, "Refresh Artist")}
      title={i18n.t('artistDetail.refreshTooltip')}
      onclick={() => { handleRescanArtist(); overflowMenuPos = null; }}
      disabled={loading || collectionStore.isScanning || refreshing}
    />
    <ContextMenuItem
      icon={OpenInPicard}
      label={i18n.t("picard.openAllInPicard")}
      onclick={() => { handleOpenAllInPicard(); overflowMenuPos = null; }}
      disabled={loading || songs.length === 0 || !picardStore.available || songs.every(isRemoteSource)}
      title={!picardStore.available
        ? i18n.t("picard.notFoundTooltip")
        : songs.length > 0 && songs.every(isRemoteSource)
          ? i18n.t("picard.remoteNotSupportedTooltip")
          : undefined}
    />
    {#if prefs.onlineEnabled}
    <ContextMenuItem
      icon={RetrieveDetails}
      label={i18n.t("artistDetail.retrieveArtistDetails", {}, "Retrieve Artist Details")}
      title={hasMusicbrainzArtistId ? i18n.t("artistDetail.retrieveArtistDetailsTooltip", {}, "Fetch Discogs, AllMusic, Wikidata, IMDb and social links from MusicBrainz") : i18n.t("artistDetail.retrieveArtistDetailsNoMbidTooltip", {}, "No MusicBrainz artist ID found for this artist")}
      onclick={() => { handleRetrieveArtistDetails(); overflowMenuPos = null; }}
      disabled={loading || retrievingDetails || !hasMusicbrainzArtistId}
    />
    <ContextMenuItem
      icon={RetrieveImage}
      label={i18n.t("artistDetail.retrieveArtistImage", {}, "Retrieve Artist Image")}
      title={hasMusicbrainzArtistId ? i18n.t("artistDetail.retrieveArtistImageTooltip", {}, "Retrieve an artist portrait from fanart.tv or Wikidata") : i18n.t("artistDetail.retrieveArtistDetailsNoMbidTooltip", {}, "No MusicBrainz artist ID found for this artist")}
      onclick={() => { handleRetrieveArtistImage(); overflowMenuPos = null; }}
      disabled={loading || retrievingImage || !hasMusicbrainzArtistId}
    />
    {/if}
    <ContextMenuItem
      icon={BarChart2}
      label={statsExclusionsStore.isExcluded("artist", artistName)
        ? i18n.t("stats.includeInStats")
        : i18n.t("stats.excludeFromStats")}
      onclick={() => { handleToggleStatsExcluded(); overflowMenuPos = null; }}
    />
  </ContextMenu>
{/if}

{#if showShareModal}
  <ShareModal entity={{ kind: "artist", artistName }} onClose={() => { showShareModal = false; }} />
{/if}

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
  />
{/if}

<ArtistProfileEditor
  {artistName}
  isOpen={isEditorOpen}
  onClose={() => { isEditorOpen = false; }}
/>

{#if selectedKeys.size > 0}
  <SongSelectionToolbar
    count={selectedKeys.size}
    onPlaySelected={handlePlaySelected}
    onAddToPlaylist={handleBulkAddSinglesToPlaylist}
    onClear={() => { selectedKeys = new Set(); }}
  />
{/if}

