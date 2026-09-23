<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import { formatDuration } from "../utils/formatters";
  import CoverArt from "./CoverArt.svelte";
  import CoverMosaic from "./CoverMosaic.svelte";
  import GenreChips from "./GenreChips.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import TagEditor from "./TagEditor.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import ArtistProfileEditor from "./ArtistProfileEditor.svelte";
  import MarkdownBio from "./MarkdownBio.svelte";
  import SocialIcon from "./SocialIcon.svelte";
  import ArtistInformationPanel from "./ArtistInformationPanel.svelte";
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
    ArrowUpRightIcon as ArrowUpRight,
    ShareNetworkIcon as Share,
    ImageIcon as RetrieveImage
  } from "phosphor-svelte";
  const ExternalLink = OpenInPicard;
  import ShareModal from "./ShareModal.svelte";
  import type { Song, Playlist, AlbumItem, PlayContext, ArtistProfile, ExtendedArtworkResponse, SongContextEnrichment } from "../types";
  import { getCoverArtUrl } from "../types";
  import {
    resolveSocialUrl,
    formatDisplayLabel,
    normalizeWebsitePlatform,
    resolveArtistMbid,
    deriveMusicbrainzArtistUrl,
    deriveListenbrainzArtistUrl,
    deriveFanartTvUrlFromMbid,
  } from "../utils/artistSocials";
  import { getArtistAlbums, classifyRelease } from "../utils/artist";
  import { songsToCoverStack, resolveArtistPortraitUrl } from "../utils/covers";
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
  let hasWebsite = $derived(!!artistProfile?.website);
  let hasTags = $derived((artistProfile?.tags?.length ?? 0) > 0);
  let hasSocials = $derived((artistProfile?.social_links?.length ?? 0) > 0);

  // "Retrieve Artist Details" needs a MusicBrainz artist MBID: either
  // already captured on the profile (via "Retrieve Album Details" or a
  // previous run of this action), or resolvable from a tagged song.
  let hasMusicbrainzArtistId = $derived(
    !!artistProfile?.musicbrainz_artist_id ||
      songs.some((s) => (s.musicbrainz_artist_id ?? s.musicbrainz_album_artist_id ?? "").trim().length > 0)
  );

  // Fetched MusicBrainz/Wikipedia context (#23), keyed off a track by
  // this artist with a MusicBrainz ID (or first song) — neither ArtistProfile
  // nor a dedicated artist entity carry a MusicBrainz ID of their own,
  // so the backend resolves it from a song row.
  let contextData = $state<SongContextEnrichment | null>(null);
  $effect(() => {
    const songWithMb = songs.find((s) => s.musicbrainz_artist_id || s.musicbrainz_album_artist_id);
    const id = songWithMb?.id || songs[0]?.id;
    if (!id) {
      contextData = null;
      return;
    }
    let cancelled = false;
    invoke<SongContextEnrichment>("get_song_context", { songId: id })
      .then((data) => {
        if (!cancelled) contextData = data;
      })
      .catch(() => {
        if (!cancelled) contextData = null;
      });
    return () => {
      cancelled = true;
    };
  });

  // The user's own bio (personal curation) always wins over the fetched
  // Wikipedia extract — fetched data only fills the gap when nothing local
  // exists, per the canonical-lookup-vs-personal-curation split in AGENTS.md.
  let effectiveBio = $derived(artistProfile?.bio || contextData?.wikipedia_extract);
  let bioIsFromWikipedia = $derived(!artistProfile?.bio && !!contextData?.wikipedia_extract);
  let hasBio = $derived(!!effectiveBio);

  // Derived, read-only MusicBrainz/ListenBrainz/Fanart.tv links (#98/#761,
  // #1123) from the artist's resolved MBID — not a fetch, just computed
  // URLs, same as any other link in this section.
  let artistMbid = $derived(
    resolveArtistMbid(artistProfile?.musicbrainz_artist_id, artistProfile?.social_links)
  );
  let musicbrainzArtistUrl = $derived(deriveMusicbrainzArtistUrl(artistMbid));
  let listenbrainzArtistUrl = $derived(deriveListenbrainzArtistUrl(artistMbid));
  let fanartTvUrl = $derived(deriveFanartTvUrlFromMbid(artistMbid));

  let hasArtistInfo = $derived(
    !!contextData?.artist_gender ||
      !!contextData?.artist_begin_date ||
      !!contextData?.artist_end_date ||
      !!contextData?.artist_begin_area_name ||
      !!contextData?.artist_area_name
  );

  let hasProfileContent = $derived(hasWebsite || hasBio || hasSocials || !!artistMbid || hasArtistInfo);

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
  let bandLogoUrl = $derived(getCoverArtUrl(artistArtwork?.band_logo_uri));
  let fanartBannerUrl = $derived(getCoverArtUrl(artistArtwork?.fanart_uri));

  interface ArtistLinkItem {
    key: string;
    platform: string;
    url: string;
    label: string;
    /** The artist's own official site(s) — MusicBrainz's "official
     * homepage" relation(s) — rendered more prominently than a plain
     * cross-reference or social link (#1123). */
    isOfficial: boolean;
  }

  // Unifies the website, curated social links, and derived MusicBrainz/
  // ListenBrainz/Fanart.tv links into one alphabetically-sorted list — same
  // convention as AlbumDetailView's `releaseLinkItems` (#1122) — so the
  // derived links aren't stuck at a fixed spot at the end regardless of
  // what else is present. The official website always leads, ahead of the
  // alphabetical sort, since it's the artist's own page rather than one
  // more retrieved link.
  let artistLinkItems = $derived.by((): ArtistLinkItem[] => {
    const items: ArtistLinkItem[] = [];
    if (hasWebsite) {
      const website = artistProfile?.website ?? "";
      const url = resolveSocialUrl("website", website);
      items.push({
        key: "website",
        platform: normalizeWebsitePlatform("website", url),
        url,
        label: formatDisplayLabel("website", website),
        isOfficial: true,
      });
    }
    for (const link of artistProfile?.social_links ?? []) {
      const url = resolveSocialUrl(link.platform, link.handle_or_url);
      items.push({
        key: `${link.platform}:${link.handle_or_url}`,
        platform: normalizeWebsitePlatform(link.platform, url),
        url,
        label: formatDisplayLabel(link.platform, link.handle_or_url),
        // A secondary "official homepage" (multiple listed on MB) is still
        // stored under platform "website" before normalization — it's
        // official too, just not the primary one.
        isOfficial: link.platform === "website",
      });
    }
    if (musicbrainzArtistUrl) {
      items.push({ key: "musicbrainz-derived", platform: "musicbrainz", url: musicbrainzArtistUrl, label: "MusicBrainz", isOfficial: false });
    }
    if (listenbrainzArtistUrl) {
      items.push({ key: "listenbrainz-derived", platform: "listenbrainz", url: listenbrainzArtistUrl, label: "ListenBrainz", isOfficial: false });
    }
    if (fanartTvUrl) {
      items.push({ key: "fanart-tv", platform: "fanart_tv", url: fanartTvUrl, label: "Fanart.tv", isOfficial: false });
    }
    // Official homepage(s) lead as a group, ahead of the alphabetical sort —
    // they're the artist's own page(s) rather than retrieved cross-references,
    // and grouping them keeps a second/third homepage next to the first
    // instead of scattered wherever its label happens to sort (#1123).
    return items.sort((a, b) => {
      if (a.isOfficial !== b.isOfficial) return a.isOfficial ? -1 : 1;
      if (a.key === "website") return -1;
      if (b.key === "website") return 1;
      return a.label.localeCompare(b.label);
    });
  });

  function handleTagClick(tag: string) {
    collectionStore.searchQuery = `artist-tag:${tag}`;
    navigationStore.selectedArtistName = null;
    navigationStore.activeTab = "collection";
    navigationStore.activeSubTab = "artists";
  }

  async function handleOpenUrl(url: string) {
    if (!url) return;
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      window.open(url, "_blank");
    }
  }

  function handleAlbumContextMenu(event: MouseEvent, album: AlbumItem) {
    event.preventDefault();
    albumContextMenuState = { x: event.clientX, y: event.clientY, album };
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

  // Rescans the library for this artist's local portrait/band logo/fanart
  // banner (#761) and re-fetches its MusicBrainz/Wikipedia context (#23),
  // bypassing both caches — mirrors AlbumDetailView's handleRefreshAlbum.
  // Fixes #867: a portrait/logo/banner added, replaced, or removed on disk
  // otherwise never refreshes since both lookups are cached for the session.
  async function handleRescanArtist() {
    if (refreshing || collectionStore.isScanning) return;
    refreshing = true;
    try {
      await collectionStore.startScan(true);
      await collectionStore.refreshLibrary();
      await refetchSongs();
      const songWithMb = songs.find((s) => s.musicbrainz_artist_id || s.musicbrainz_album_artist_id);
      const contextSongId = songWithMb?.id ?? songs[0]?.id;
      const [artwork, context] = await Promise.all([
        collectionStore.getExtendedArtworkForArtist(artistName, true),
        contextSongId
          ? invoke<SongContextEnrichment>("get_song_context", { songId: contextSongId, forceRefresh: true }).catch(() => null)
          : Promise.resolve(null)
      ]);
      artistArtwork = artwork;
      if (context) contextData = context;
      toastStore.show(i18n.t("artistDetail.refreshSuccess", {}, "Artist artwork and bio refreshed"));
    } catch (err) {
      console.error("Failed to refresh artist:", err);
      toastStore.show(i18n.t("artistDetail.refreshError", {}, "Failed to refresh artist"), "error");
    } finally {
      refreshing = false;
    }
  }

  // The artist detail overflow menu's "Retrieve Artist Details" (#1123) —
  // the artist-level equivalent of AlbumDetailView's handleRetrieveAlbumDetails.
  async function handleRetrieveArtistDetails() {
    if (retrievingDetails || !hasMusicbrainzArtistId) return;
    retrievingDetails = true;
    try {
      const result = await collectionStore.retrieveArtistDetails(artistName);
      if (result.added_count === 1) {
        toastStore.show(
          i18n.t("artistDetail.retrieveDetailsSuccessOne", {}, "Added 1 link from MusicBrainz")
        );
      } else if (result.added_count > 1) {
        toastStore.show(
          i18n.t(
            "artistDetail.retrieveDetailsSuccessMany",
            { count: result.added_count },
            `Added ${result.added_count} links from MusicBrainz`
          )
        );
      } else {
        toastStore.show(
          i18n.t(
            "artistDetail.retrieveDetailsNoResults",
            {},
            "No additional details found on MusicBrainz"
          )
        );
      }
    } catch (err) {
      console.error("Failed to retrieve artist details:", err);
      // Surfaces the backend's actual error text (e.g. a MusicBrainz API
      // failure reason) rather than a generic message — same convention as
      // `openInPicard` — so a real failure is actionable instead of just
      // "something went wrong".
      toastStore.show(String(err), "error");
    } finally {
      retrievingDetails = false;
    }
  }

  // Artist detail overflow menu's "Retrieve Artist Image" (#1127) — fetches a
  // portrait from fanart.tv (if a key is configured) or, lacking one, the
  // Wikidata fallback, only ever filling the gap when no *local* portrait
  // exists (see `artistPortraitUrl`'s fallback ordering).
  async function handleRetrieveArtistImage() {
    if (retrievingImage || !hasMusicbrainzArtistId) return;
    retrievingImage = true;
    try {
      const result = await collectionStore.retrieveArtistImage(artistName);
      if (result.uri) {
        toastStore.show(
          result.source === "fanart"
            ? i18n.t("artistDetail.retrieveImageSuccessFanart", {}, "Artist image retrieved from fanart.tv")
            : i18n.t("artistDetail.retrieveImageSuccessWikidata", {}, "Artist image retrieved from Wikidata")
        );
      } else {
        toastStore.show(i18n.t("artistDetail.retrieveImageNoResults", {}, "No artist image found"));
      }
    } catch (err) {
      console.error("Failed to retrieve artist image:", err);
      toastStore.show(String(err), "error");
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
    contextData = null;
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
    const totalMinutes = Math.round(totalNs / 1_000_000_000 / 60);
    const h = Math.floor(totalMinutes / 60);
    const m = totalMinutes % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  });

  // Shares classifyRelease() with the per-card badge everywhere else in the
  // app, so the discography tabs agree with how a release is labeled
  // elsewhere: multi-disc releases are "Sets" regardless of duration, then
  // Albums/EPs by total duration (EP = under 30 minutes) and Singles by track count.
  let sets = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "set"));
  let fullAlbums = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "album"));
  let eps = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "ep"));
  let singles = $derived(albums.filter((a) => classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec) === "single"));

  let songsText = $derived(
    songs.length === 1 ? i18n.t("playlists.oneSong") : i18n.t("playlists.songsCount", { count: songs.length })
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
    const excluded = !statsExclusionsStore.isExcluded("artist", artistName);
    await statsExclusionsStore.setExcluded("artist", artistName, excluded);
    const message = excluded
      ? i18n.t("stats.excludedToast", { name: artistName })
      : i18n.t("stats.includedToast", { name: artistName });
    toastStore.show(message);
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
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 {windowLayoutStore.isDetailHeaderCollapsed ? 'py-3' : 'pt-6 pb-6'}">
    {#if fanartBannerUrl && !windowLayoutStore.isDetailHeaderCollapsed}
      <div class="absolute inset-0 z-0 overflow-hidden pointer-events-none" aria-hidden="true">
        <img src={fanartBannerUrl} alt="" class="w-full h-full object-cover opacity-25" />
        <div class="absolute inset-0 bg-gradient-to-t from-brand-main via-brand-main/70 to-brand-main/30"></div>
      </div>
    {/if}
    <div class="flex items-start justify-between gap-6 relative z-10">
      <div class="flex flex-col justify-end gap-1.5 min-w-0 max-w-xl">
        {#if !windowLayoutStore.isDetailHeaderCollapsed}
        {#if bandLogoUrl}
          <img
            src={bandLogoUrl}
            alt={artistName}
            class="h-10 sm:h-12 w-auto max-w-full object-contain object-left"
          />
        {:else}
          <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5">{artistName}</h1>
        {/if}

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

      {#if !windowLayoutStore.isDetailHeaderCollapsed && (artistPortraitUrl || headerCovers.length > 0)}
        <div class="hidden sm:flex items-start shrink-0 shadow-xl">
          <CoverMosaic covers={headerCovers} heroImageUrl={artistPortraitUrl} heroImageAlt={artistName} sizeClass="h-36" />
        </div>
      {/if}
    </div>
  </div>

  <div class="px-6 pt-6 flex flex-col gap-8">
    {#if !windowLayoutStore.isDetailHeaderCollapsed}
      {#if hasChips || (hasProfileContent && !windowLayoutStore.isOverviewExpanded)}
        <div class="flex flex-wrap items-center justify-between gap-3">
          {#if hasChips}
            <GenreChips
              curatedTags={artistOnlyTags}
              onCuratedTagClick={handleTagClick}
              curatedTagTitle={(tag) => `Filter artists tagged "${tag}"`}
              variant="full"
            />
          {/if}
          {#if hasProfileContent && !windowLayoutStore.isOverviewExpanded}
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

    <!-- Artist Profile Card (About & Links) -->
    {#if hasProfileContent && !windowLayoutStore.isDetailHeaderCollapsed && windowLayoutStore.isOverviewExpanded}
      <details
        open
        ontoggle={(e) => windowLayoutStore.setOverviewExpanded(e.currentTarget.open)}
        class="group/overview border border-brand-border rounded-xl bg-brand-sidebar/40 backdrop-blur-md overflow-hidden shadow-xs transition-all @container"
      >
        <summary class="flex items-center justify-between px-4 py-2.5 sm:px-5 sm:py-3 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none hover:text-brand-text-primary transition-colors">
          <span>{i18n.t('artistDetail.artistInfo', {}, 'Artist Info')}</span>
          <ArrowUpRight class="w-3.5 h-3.5 text-brand-text-secondary/70" />
        </summary>
        <div class="p-4 sm:p-5 md:p-6 border-t border-brand-border/60 flex flex-col @2xl:flex-row gap-5 md:gap-6 justify-between">
          <!-- About Column (Left) -->
          {#if hasBio}
            {@const bioText = effectiveBio ?? ""}
            <div class="flex-1 flex flex-col gap-3 min-w-0">
              <!-- Bio -->
              <div class="text-xs text-brand-text-secondary leading-relaxed">
                {#if bioIsFromWikipedia}
                  <button
                    type="button"
                    onclick={() => contextData?.wikipedia_page_url && handleOpenUrl(contextData.wikipedia_page_url)}
                    class="group/wiki relative inline-flex items-center gap-1 mb-1 text-[11px] font-semibold text-brand-text-secondary/70 hover:text-brand-accent transition-colors cursor-pointer"
                  >
                    <span class="underline decoration-brand-text-secondary/40 group-hover/wiki:decoration-brand-accent">{i18n.t('playerBar.wikipediaSectionLabel', {}, 'Wikipedia')}</span>
                    <ExternalLink class="w-3 h-3 opacity-0 group-hover/wiki:opacity-100 transition-opacity" />
                  </button>
                {/if}
                <MarkdownBio
                  text={bioText}
                  disableClamp={true}
                />
              </div>
            </div>
          {/if}

          <!-- Links & Facts Column (Right or Below) -->
          {#if hasWebsite || hasSocials || artistMbid || hasArtistInfo}
            <div
              class={hasBio
                ? "@2xl:w-[22rem] @3xl:w-[28rem] shrink-0 border-t border-brand-border/40 pt-4 @2xl:border-t-0 @2xl:border-l @2xl:border-brand-border/60 @2xl:pt-0 @2xl:pl-6 flex flex-col gap-4"
                : "w-full flex flex-col gap-4"}
            >
              {#if hasArtistInfo}
                <ArtistInformationPanel
                  sortName={contextData?.artist_sort_name}
                  gender={contextData?.artist_gender}
                  beginDate={contextData?.artist_begin_date}
                  endDate={contextData?.artist_end_date}
                  ended={contextData?.artist_ended}
                  artistType={contextData?.artist_type}
                  beginAreaName={contextData?.artist_begin_area_name}
                  beginAreaMbid={contextData?.artist_begin_area_mbid}
                  areaName={contextData?.artist_area_name}
                  areaMbid={contextData?.artist_area_mbid}
                  onOpenUrl={handleOpenUrl}
                  variant="plain"
                />
              {/if}

              {#if hasWebsite || hasSocials || artistMbid}
                <div class="grid grid-cols-1 @sm:grid-cols-2 {hasBio ? '@2xl:grid @2xl:grid-cols-2' : '@md:grid-cols-3 @xl:grid-cols-4'} gap-2.5">
                  <!-- Website, curated social links, and derived MusicBrainz/
                       ListenBrainz/Fanart.tv links, unified and sorted
                       alphabetically with the website first (#1122, #1123) -->
                  {#each artistLinkItems as item (item.key)}
                    <button
                      type="button"
                      onclick={() => handleOpenUrl(item.url)}
                      title={item.url}
                      class="flex items-center gap-2.5 sm:gap-3 group/link text-left transition-colors cursor-pointer min-w-0"
                    >
                      <div class="w-7 h-7 sm:w-8 sm:h-8 rounded-full bg-brand-main/60 {item.isOfficial ? 'border-[3px]' : 'border'} border-brand-border flex items-center justify-center text-brand-text-secondary group-hover/link:text-brand-accent group-hover/link:border-brand-accent/40 transition-colors shrink-0 shadow-2xs">
                        <SocialIcon platform={item.platform} size={14} />
                      </div>
                      <div class="flex items-center gap-1 min-w-0 flex-1">
                        <span class="text-xs font-medium text-brand-text-primary truncate transition-colors">
                          {item.label}
                        </span>
                        <ExternalLink class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover/link:opacity-100 transition-opacity shrink-0" />
                      </div>
                    </button>
                  {/each}
                </div>
              {/if}
            </div>
          {/if}
        </div>
      </details>
    {/if}

    {#if sets.length > 0}
      <HorizontalScrollRow title={i18n.t('artistDetail.setsFilter', { count: sets.length })}>
        {#each sets as album (album.album)}
          <AlbumCard
            {album}
            widthClass="w-48 shrink-0"
            onclick={() => openAlbum(album)}
            oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
          />
        {/each}
      </HorizontalScrollRow>
    {/if}

    {#if fullAlbums.length > 0}
      <HorizontalScrollRow title={i18n.t('artistDetail.albumsFilter', { count: fullAlbums.length })}>
        {#each fullAlbums as album (album.album)}
          <AlbumCard
            {album}
            widthClass="w-48 shrink-0"
            onclick={() => openAlbum(album)}
            oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
          />
        {/each}
      </HorizontalScrollRow>
    {/if}

    {#if eps.length > 0}
      <HorizontalScrollRow title={i18n.t('artistDetail.epsFilter', { count: eps.length })}>
        {#each eps as album (album.album)}
          <AlbumCard
            {album}
            widthClass="w-48 shrink-0"
            onclick={() => openAlbum(album)}
            oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
          />
        {/each}
      </HorizontalScrollRow>
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
      <HorizontalScrollRow title={i18n.t('artistDetail.compilationsFeaturing', { artist: artistName })}>
        {#each compilations as album (album.album)}
          <AlbumCard
            {album}
            widthClass="w-48 shrink-0"
            onclick={() => openAlbum(album)}
            oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
          />
        {/each}
      </HorizontalScrollRow>
    </div>
  {/if}

  {#if playlists.length > 0}
    <div class="px-6 pt-10 {playerStore.currentSong ? 'pb-28' : 'pb-6'}">
      <HorizontalScrollRow title={i18n.t('artistDetail.playlistsFeaturing', { artist: artistName })}>
        {#each playlists as playlist (playlist.id)}
          <PlaylistCard {playlist} widthClass="w-48 shrink-0" onClick={() => openPlaylist(playlist)} />
        {/each}
      </HorizontalScrollRow>
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

{#if singleContextMenuState}
  {@const song = singleContextMenuState.song}
  <SongContextMenu
    x={singleContextMenuState.x}
    y={singleContextMenuState.y}
    {song}
    selectedCount={selectedKeys.size}
    selectedSongIds={Array.from(selectedKeys, Number)}
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
      label={i18n.t("artistDetail.refresh", {}, "Refresh")}
      title={i18n.t('artistDetail.refreshTooltip')}
      onclick={() => { handleRescanArtist(); overflowMenuPos = null; }}
      disabled={loading || collectionStore.isScanning || refreshing}
    />
    <ContextMenuItem
      icon={OpenInPicard}
      label={i18n.t("picard.openAllInPicard")}
      onclick={() => { handleOpenAllInPicard(); overflowMenuPos = null; }}
      disabled={loading || songs.length === 0 || !picardStore.available}
      title={picardStore.available ? undefined : i18n.t("picard.notFoundTooltip")}
    />
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

