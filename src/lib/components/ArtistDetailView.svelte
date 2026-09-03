<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import { formatDuration } from "../utils/formatters";
  import CoverArt from "./CoverArt.svelte";
  import CoverStack from "./CoverStack.svelte";
  import GenreChips from "./GenreChips.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import TagEditor from "./TagEditor.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import IconActionButton from "./IconActionButton.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import ArtistProfileEditor from "./ArtistProfileEditor.svelte";
  import SocialIcon from "./SocialIcon.svelte";
  import SongSelectionToolbar from "./SongSelectionToolbar.svelte";
  import SongTable, { type SongTableRow } from "./SongTable.svelte";
  import {
    PencilSimpleIcon as Edit3,
    ArrowSquareOutIcon as ExternalLink,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff
  } from "phosphor-svelte";
  import type { Song, Playlist, AlbumItem, PlayContext, ArtistProfile } from "../types";
  import { resolveSocialUrl, formatDisplayLabel } from "../utils/artistSocials";
  import { getArtistAlbums, classifyRelease } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import { parseMultiValue, joinMultiValue } from "../utils/multiValue";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import { openInPicard } from "../utils/picard";
  import { compareSongs } from "../utils/songSort";

  let { artistName }: { artistName: string } = $props();

  let songs = $state<Song[]>([]);
  let playlists = $state<Playlist[]>([]);
  let compilations = $state<AlbumItem[]>([]);
  let loading = $state(true);

  let albumContextMenuState = $state<{ x: number; y: number; album: AlbumItem } | null>(null);
  let singleContextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  // "Not included" tracks stay visible/individually playable but drop out of
  // the whole-artist Play/Shuffle Play actions (#104).
  let playableSongs = $derived(songs.filter((s) => !s.not_included));
  let editingSongId = $state<number | null>(null);
  let isEditorOpen = $state(false);
  let isBioExpanded = $state(false);
  let selectedKeys = $state<Set<string>>(new Set());

  let artistProfile = $derived(collectionStore.getArtistProfile(artistName));
  let hasWebsite = $derived(!!artistProfile?.website);
  let hasTags = $derived((artistProfile?.tags?.length ?? 0) > 0);
  let hasBio = $derived(!!artistProfile?.bio);
  let hasSocials = $derived((artistProfile?.social_links?.length ?? 0) > 0);
  let hasProfileContent = $derived(hasWebsite || hasTags || hasBio || hasSocials);

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

  function refetchSongs() {
    invoke<Song[]>("get_songs_by_artist", { artist: artistName }).then((fetchedSongs) => {
      songs = Array.isArray(fetchedSongs) ? fetchedSongs : [];
    });
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    tagsStore.load();
    refetchSongs();
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
  let genreLabel = $derived(rawGenre ? undefined : i18n.t('artistDetail.unknownGenre'));

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
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", actions: "80px",
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

<div class="flex-1 flex flex-col overflow-y-auto bg-brand-main text-brand-text-secondary h-full carousel-scroll" use:rememberScroll={`artist-detail:${artistName}`}>
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 {windowLayoutStore.isDetailHeaderCollapsed ? 'py-3' : 'pt-6 pb-6'}">
    <div class="flex items-start justify-between gap-6 relative z-10">
      <div class="flex flex-col justify-end gap-1.5 max-w-xl">
        {#if !windowLayoutStore.isDetailHeaderCollapsed}
        <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5">{artistName}</h1>

        <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-secondary font-medium">
          {#if rawGenre}
            <GenreChips genre={rawGenre} variant="full" />
          {:else}
            <span>{genreLabel}</span>
          {/if}
          <span>•</span>
          <span>{songsText}</span>
          <span>•</span>
          <span>{totalDurationLabel}</span>
        </div>
        {/if}

        <div class="flex flex-wrap items-center gap-3 mt-3">
          <PlayShuffleButtons
            onPlayAll={handlePlayAll}
            onShufflePlay={handleShufflePlay}
            disabled={loading || songs.length === 0}
          />
          <IconActionButton
            onclick={() => { isEditorOpen = true; }}
            title={i18n.t("artistDetail.editArtistTooltip", {}, "Edit artist details, website, tags, and links")}
          >
            {#snippet icon()}<Edit3 class="w-4 h-4" />{/snippet}
          </IconActionButton>
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
          {#if singleSongs.length > 0}
            <ColumnSelector align="left" iconOnly />
          {/if}
        </div>
      </div>

      {#if !windowLayoutStore.isDetailHeaderCollapsed && headerCovers.length > 0}
        <div class="relative w-48 h-36 hidden sm:block shrink-0 flex items-center justify-end">
          <CoverStack covers={headerCovers} direction="left" sizeClass="w-28 h-28" />
        </div>
      {/if}
    </div>
  </div>

  <div class="px-6 pt-6 flex flex-col gap-8">
    <!-- Artist Profile Card (About & Links) -->
    {#if hasProfileContent && artistProfile}
      {@const profile = artistProfile}
      <div class="border border-brand-border rounded-xl bg-brand-sidebar/40 backdrop-blur-md p-4 sm:p-5 md:p-6 shadow-xs flex flex-col md:flex-row gap-5 md:gap-6 justify-between transition-all">
        <!-- About Column (Left) -->
        <div class="flex-1 flex flex-col gap-3 min-w-0">
          <h2 class="text-sm sm:text-base font-bold text-brand-text-primary font-heading">
            {i18n.t("artistDetail.about", {}, "About")}
          </h2>

          <!-- Tags Pills -->
          {#if hasTags}
            <div class="flex flex-wrap gap-1.5 sm:gap-2">
              {#each profile?.tags ?? [] as tag (tag)}
                <button
                  type="button"
                  onclick={() => handleTagClick(tag)}
                  class="px-2.5 sm:px-3 py-1 bg-brand-accent/10 hover:bg-brand-accent/25 hover:border-brand-accent/40 text-brand-text-primary rounded-full text-xs font-medium border border-brand-border/60 transition-all cursor-pointer flex items-center gap-1 shadow-2xs"
                  title={`Filter artists tagged "${tag}"`}
                >
                  <span>{tag}</span>
                </button>
              {/each}
            </div>
          {/if}

          <!-- Bio -->
          {#if hasBio}
            {@const bioText = profile?.bio ?? ""}
            {@const isLongBio = bioText.length > 200}
            <div class="text-xs text-brand-text-secondary leading-relaxed">
              <p class="{!isBioExpanded && isLongBio ? 'line-clamp-2 sm:line-clamp-3' : ''} whitespace-pre-line">
                {bioText}
              </p>
              {#if isLongBio}
                <button
                  type="button"
                  onclick={() => { isBioExpanded = !isBioExpanded; }}
                  class="mt-1 text-xs font-semibold text-brand-accent hover:underline inline-flex items-center gap-0.5 cursor-pointer"
                >
                  {isBioExpanded ? i18n.t("artistDetail.showLess", {}, "Show less") : i18n.t("artistDetail.showMore", {}, "Show more")}
                </button>
              {/if}
            </div>
          {/if}
        </div>

        <!-- Links Column (Right) -->
        {#if hasWebsite || hasSocials}
          <div class="md:w-60 lg:w-72 shrink-0 border-t border-brand-border/40 pt-4 md:border-t-0 md:border-l md:border-brand-border/60 md:pt-0 md:pl-6 flex flex-col gap-3">
            <h2 class="text-xs font-bold text-brand-text-secondary uppercase tracking-wider">
              {i18n.t("artistDetail.links", {}, "LINKS")}
            </h2>

            <div class="grid grid-cols-1 sm:grid-cols-2 md:flex md:flex-col gap-2.5">
              <!-- Primary Website Link -->
              {#if hasWebsite}
                {@const siteUrl = resolveSocialUrl("website", profile?.website ?? "")}
                <button
                  type="button"
                  onclick={() => handleOpenUrl(siteUrl)}
                  class="flex items-center gap-2.5 sm:gap-3 group text-left transition-colors cursor-pointer min-w-0"
                >
                  <div class="w-7 h-7 sm:w-8 sm:h-8 rounded-full bg-brand-main/60 border border-brand-border flex items-center justify-center text-brand-text-secondary group-hover:text-brand-accent group-hover:border-brand-accent/40 transition-colors shrink-0 shadow-2xs">
                    <SocialIcon platform="website" size={14} />
                  </div>
                  <div class="flex items-center gap-1 min-w-0 flex-1">
                    <span class="text-xs font-medium text-brand-text-primary group-hover:text-brand-accent truncate transition-colors">
                      {formatDisplayLabel("website", profile?.website ?? "")}
                    </span>
                    <ExternalLink class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover:opacity-100 transition-opacity shrink-0" />
                  </div>
                </button>
              {/if}

              <!-- Social Links -->
              {#each profile?.social_links ?? [] as link, idx (idx)}
                {@const resolvedUrl = resolveSocialUrl(link.platform, link.handle_or_url)}
                <button
                  type="button"
                  onclick={() => handleOpenUrl(resolvedUrl)}
                  class="flex items-center gap-2.5 sm:gap-3 group text-left transition-colors cursor-pointer min-w-0"
                >
                  <div class="w-7 h-7 sm:w-8 sm:h-8 rounded-full bg-brand-main/60 border border-brand-border flex items-center justify-center text-brand-text-secondary group-hover:text-brand-accent group-hover:border-brand-accent/40 transition-colors shrink-0 shadow-2xs">
                    <SocialIcon platform={link.platform} size={14} />
                  </div>
                  <div class="flex items-center gap-1 min-w-0 flex-1">
                    <span class="text-xs font-medium text-brand-text-primary group-hover:text-brand-accent truncate transition-colors">
                      {formatDisplayLabel(link.platform, link.handle_or_url)}
                    </span>
                    <ExternalLink class="w-3 h-3 text-brand-text-secondary opacity-0 group-hover:opacity-100 transition-opacity shrink-0" />
                  </div>
                </button>
              {/each}
            </div>
          </div>
        {/if}
      </div>
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
            onOpenInPicard={(song) => openInPicard([song.id])}
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

<style>
  :global(.carousel-scroll) {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  :global(.carousel-scroll::-webkit-scrollbar) {
    display: none;
  }
</style>
