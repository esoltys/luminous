<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import CoverArt from "./CoverArt.svelte";
  import CoverStack from "./CoverStack.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import GenreChips from "./GenreChips.svelte";
  import { tagsStore } from "../stores/tags.svelte";
  import TagEditor from "./TagEditor.svelte";
  import SongRating from "./SongRating.svelte";
  import SortableHeader from "./SortableHeader.svelte";
  import NowPlayingBars from "./NowPlayingBars.svelte";
  import ColumnSelector from "./ColumnSelector.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import ArtistProfileEditor from "./ArtistProfileEditor.svelte";
  import SocialIcon from "./SocialIcon.svelte";
  import { Play, Plus, Edit3, Clock, ExternalLink } from "lucide-svelte";
  import type { Song, Playlist, AlbumItem, PlayContext, ArtistProfile } from "../types";
  import { resolveSocialUrl, formatDisplayLabel } from "../utils/artistSocials";
  import { getArtistAlbums, classifyRelease, formatTrackNumber } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import { formatDate, formatFileSize, formatSampleRate, formatBitDepth, formatChannels } from "../utils/formatters";
  import { formatDateAdded } from "../utils/date";
  import { SONG_TABLE_COLUMNS } from "../utils/songColumns";
  import { columnResize } from "../utils/columnResize";

  let { artistName }: { artistName: string } = $props();

  let songs = $state<Song[]>([]);
  let playlists = $state<Playlist[]>([]);
  let compilations = $state<AlbumItem[]>([]);
  let loading = $state(true);

  let albumContextMenuState = $state<{ x: number; y: number; album: AlbumItem } | null>(null);
  let singleContextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);
  let editingSongId = $state<number | null>(null);
  let isEditorOpen = $state(false);
  let isBioExpanded = $state(false);

  let artistProfile = $derived(collectionStore.getArtistProfile(artistName));
  let hasWebsite = $derived(!!artistProfile?.website);
  let hasTags = $derived((artistProfile?.tags?.length ?? 0) > 0);
  let hasBio = $derived(!!artistProfile?.bio);
  let hasSocials = $derived((artistProfile?.social_links?.length ?? 0) > 0);
  let hasProfileContent = $derived(hasWebsite || hasTags || hasBio || hasSocials);

  function handleTagClick(tag: string) {
    collectionStore.searchQuery = `artist-tag:${tag}`;
    collectionStore.selectedArtistName = null;
    collectionStore.activeTab = "collection";
    collectionStore.activeSubTab = "artists";
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

  function handleSingleContextMenu(event: MouseEvent, song: Song) {
    event.preventDefault();
    singleContextMenuState = { x: event.clientX, y: event.clientY, song };
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function refetchSongs() {
    invoke<Song[]>("get_songs_by_artist", { artist: artistName }).then((fetchedSongs) => {
      songs = fetchedSongs;
    });
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    tagsStore.load();
    refetchSongs();
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
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

  function toggleSingleSort(field: SingleSortField) {
    if (singleSortField === field) {
      singleSortAsc = !singleSortAsc;
    } else {
      singleSortField = field;
      singleSortAsc = true;
    }
  }

  async function handleAddSingleToPlaylist(songId: number) {
    const targetPlaylist = playlistsStore.activeCustomPlaylist;
    const isQueue = !targetPlaylist || targetPlaylist.is_queue;
    const songIds = [songId];

    if (isQueue) {
      {
        await playlistsStore.addSongsToQueue(songIds);
        const songObj = songs.find((s) => s.id === songId);
        const name = songObj?.title || "Song";
        toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
      }
    } else {
      await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
      toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`));
    }
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
    loading = true;
    Promise.all([
      invoke<Song[]>("get_songs_by_artist", { artist: requested }),
      invoke<Playlist[]>("get_playlists_by_artist", { artist: requested }),
      invoke<AlbumItem[]>("get_compilations_by_artist", { artist: requested }),
      invoke<ArtistProfile>("get_artist_profile", { artist: requested })
    ])
      .then(([fetchedSongs, fetchedPlaylists, fetchedCompilations, fetchedProfile]) => {
        if (requested !== artistName) return;
        songs = fetchedSongs;
        playlists = fetchedPlaylists.filter((p) => !p.is_queue);
        compilations = fetchedCompilations;
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
    collectionStore.selectedArtistName = null;
    collectionStore.activeSubTab = "artists";
  }

  function deriveGenreLabel(list: Song[]): string {
    const counts = new Map<string, number>();
    for (const s of list) {
      const g = (s.genre ?? "").trim();
      if (g !== "") counts.set(g, (counts.get(g) ?? 0) + 1);
    }
    if (counts.size === 0) return i18n.t('artistDetail.unknownGenre');
    const maxCount = Math.max(...counts.values());
    const top = [...counts.entries()]
      .filter(([, c]) => c === maxCount)
      .map(([g]) => g)
      .sort((a, b) => a.localeCompare(b));
    return top.slice(0, 2).join(" / ");
  }

  let genreLabel = $derived(deriveGenreLabel(songs));

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

  // Some artists have no proper album releases at all (every track is a loose
  // single with no album tag), so getArtistAlbums() returns nothing and the
  // Albums/Singles/Popular filters all end up empty. Fall back to showing the
  // artist's individual songs directly rather than an empty "no releases" state.
  // Mutually exclusive with `singles` (albums.length is 0 whenever this is
  // populated), so combining the two counts is safe.
  let looseSongs = $derived(
    albums.length === 0 ? [...songs].sort((a, b) => (a.title || "").localeCompare(b.title || "")) : []
  );

  // Grouped singles are AlbumItems (track_count === 1), but they render as a
  // song table alongside loose singles — resolve each back to its one song.
  let singleSongs = $derived(
    singles.length > 0
      ? singles
          .map((a) => songs.find((s) => s.album === a.album))
          .filter((s): s is Song => s !== undefined)
      : looseSongs
  );

  let sortedSingleSongs = $derived.by(() => {
    if (singleSortField === "track") {
      if (singleSortAsc) return singleSongs;
      return [...singleSongs].reverse();
    }

    const field = singleSortField as keyof Song;
    return [...singleSongs].sort((a, b) => {
      const valA = a[field];
      const valB = b[field];

      if (valA === undefined || valA === null) return singleSortAsc ? 1 : -1;
      if (valB === undefined || valB === null) return singleSortAsc ? -1 : 1;

      if (typeof valA === "string" && typeof valB === "string") {
        const cmp = valA.localeCompare(valB);
        return singleSortAsc ? cmp : -cmp;
      }
      if (typeof valA === "number" && typeof valB === "number") {
        return singleSortAsc ? valA - valB : valB - valA;
      }
      return 0;
    });
  });

  // Mirrors AlbumDetailView/CollectionView/PlaylistView/AutoPlaylistDetailView's
  // identical formula so this table's columns match what's shown everywhere else.
  // Default column widths (px or fr) — used when no saved width exists for a column.
  const ARTIST_COL_DEFAULTS: Partial<Record<keyof typeof collectionStore.visibleColumns, string>> = {
    track: "48px", title: "2fr", artist: "1.5fr", album: "1.5fr",
    composer: "1.5fr", album_artist: "1.5fr", format: "64px", year: "60px",
    genre: "1.2fr", grouping: "1.2fr", bpm: "60px", initial_key: "60px",
    bitrate: "70px", samplerate: "75px", bitdepth: "65px", channels: "70px",
    filesize: "75px", rating: "96px", playcount: "70px", skipcount: "70px",
    lastplayed: "90px", added: "90px", duration: "80px", path: "2fr", actions: "80px",
  };

  let gridColsStyle = $derived.by(() => {
    const vc = collectionStore.visibleColumns;
    const cw = collectionStore.columnWidths;
    const cols: string[] = ["36px"]; // play indicator always present
    for (const { key } of SONG_TABLE_COLUMNS) {
      if (!vc[key]) continue;
      const saved = cw[key];
      cols.push(saved !== undefined ? `${saved}px` : (ARTIST_COL_DEFAULTS[key] ?? "80px"));
    }
    return `grid-template-columns: ${cols.join(" ")}`;
  });

  let singleDiscCount = $derived(singleSongs.reduce((max, s) => Math.max(max, s.disc ?? 1), 1));

  function openAlbum(album: AlbumItem) {
    collectionStore.viewAlbum(album.album || "");
  }

  // Mirrors PlaylistsCollectionView's openAuto/openPlaylist split so genre/decade
  // auto-playlists open in AutoPlaylistDetailView (Auto-Play toggle, etc.) here too,
  // instead of always falling through to the custom-playlist detail view. Smart
  // Playlists are also dynamic_enabled but are user-authored rule playlists, not
  // system auto-playlists, so they must go through the normal viewPlaylist path.
  function openPlaylist(playlist: Playlist) {
    if (playlist.dynamic_enabled && !isSmartPlaylistSpec(playlist.dynamic_spec)) {
      const isDecade = playlist.dynamic_spec?.startsWith("decade:") ?? false;
      collectionStore.viewAutoPlaylist(
        isDecade
          ? { kind: "decade", decade: playlist.dynamic_spec?.replace(/^decade:/, "") ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
          : { kind: "genre", genre: playlist.dynamic_spec ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
      );
      return;
    }
    playlistsStore.selectPlaylist(playlist.id);
    collectionStore.viewPlaylist(playlist.id);
  }

  async function handlePlayAll() {
    if (songs.length === 0) return;
    const queuePl = await playlistsStore.requireQueue();
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(songs.map((s) => s.id), 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      collectionStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleShufflePlay() {
    if (songs.length === 0) return;
    const queuePl = await playlistsStore.requireQueue();
    const shuffledIds = shuffleArray(songs.map((s) => s.id));
    await playerStore.setShuffleMode("all");
    await playerStore.playSongs(shuffledIds, 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      collectionStore.viewPlaylist(queuePl.id);
    }
  }
</script>

<div class="flex-1 flex flex-col overflow-y-auto bg-brand-main text-brand-text-secondary h-full carousel-scroll" use:rememberScroll={`artist-detail:${artistName}`}>
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 {collectionStore.isDetailHeaderCollapsed ? 'py-3' : 'pt-6 pb-6'}">
    <div class="flex items-start justify-between gap-6 relative z-10">
      <div class="flex flex-col justify-end gap-1.5 max-w-xl">
        {#if !collectionStore.isDetailHeaderCollapsed}
        <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5">{artistName}</h1>

        <div class="flex items-center gap-3 text-xs text-brand-text-secondary font-medium">
          <span>{i18n.t('artistDetail.statsLine', { genre: genreLabel, songs: songsText, duration: totalDurationLabel })}</span>
        </div>
        {/if}

        <div class="flex flex-wrap items-center gap-3 mt-3">
          <PlayShuffleButtons
            onPlayAll={handlePlayAll}
            onShufflePlay={handleShufflePlay}
            disabled={loading || songs.length === 0}
          />
          <button
            type="button"
            onclick={() => { isEditorOpen = true; }}
            class="px-3.5 py-1.5 rounded-full text-xs font-semibold bg-brand-sidebar/80 hover:bg-brand-sidebar text-brand-text-primary border border-brand-border hover:border-brand-accent/40 shadow-xs flex items-center gap-1.5 transition-all cursor-pointer"
            title={i18n.t("artistDetail.editArtistTooltip", {}, "Edit artist details, website, tags, and links")}
          >
            <Edit3 class="w-3.5 h-3.5 text-brand-accent" />
            <span>{i18n.t("artistDetail.editArtist", {}, "Edit")}</span>
          </button>
          {#if singleSongs.length > 0}
            <ColumnSelector align="left" iconOnly />
          {/if}
        </div>
      </div>

      {#if !collectionStore.isDetailHeaderCollapsed && headerCovers.length > 0}
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
        <div class="border border-brand-border rounded-lg bg-brand-sidebar/50 backdrop-blur-xl shadow-2xl overflow-hidden">
          <div class="sticky top-0 z-10 flex flex-col rounded-t-lg bg-brand-sidebar/80 backdrop-blur-md border-b border-brand-border text-[10px] text-brand-text-primary uppercase tracking-wider font-semibold select-none">
            <div class="grid items-center py-2.5 px-4" style={gridColsStyle}>
              <div class="text-center w-9"></div>
              {#if collectionStore.visibleColumns.track}
                <div use:columnResize={{ column: "track", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "track"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("track")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderTrack')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.title}
                <div use:columnResize={{ column: "title", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "title"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("title")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderTitle')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.artist}
                <div use:columnResize={{ column: "artist", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "artist"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("artist")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderArtist')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.album}
                <div use:columnResize={{ column: "album", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "album"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("album")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-1rem)]">{i18n.t('collection.tableHeaderAlbum')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.composer}
                <div use:columnResize={{ column: "composer", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "composer"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("composer")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderComposer')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.album_artist}
                <div use:columnResize={{ column: "album_artist", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "album_artist"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("album_artist")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderAlbumArtist')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.format}
                <div use:columnResize={{ column: "format", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "filetype"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("filetype")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFormat')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.year}
                <div use:columnResize={{ column: "year", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "year"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("year")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderYear')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.genre}
                <div use:columnResize={{ column: "genre", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "genre"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("genre")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGenre')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.grouping}
                <div use:columnResize={{ column: "grouping", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "grouping"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("grouping")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderGrouping')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bpm}
                <div use:columnResize={{ column: "bpm", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "bpm"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("bpm")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBpm')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.initial_key}
                <div use:columnResize={{ column: "initial_key", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "initial_key"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("initial_key")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderInitialKey')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bitrate}
                <div use:columnResize={{ column: "bitrate", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "bitrate"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("bitrate")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitrate')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.samplerate}
                <div use:columnResize={{ column: "samplerate", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "samplerate"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("samplerate")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderSampleRate')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.bitdepth}
                <div use:columnResize={{ column: "bitdepth", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "bitdepth"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("bitdepth")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderBitDepth')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.channels}
                <div use:columnResize={{ column: "channels", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "channels"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("channels")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderChannels')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.filesize}
                <div use:columnResize={{ column: "filesize", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "filesize"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("filesize")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderFileSize')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.rating}
                <div use:columnResize={{ column: "rating", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "rating"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("rating")}
                  class="flex items-center justify-center hover:text-brand-text-primary transition-colors font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderRating')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.playcount}
                <div use:columnResize={{ column: "playcount", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "playcount"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("playcount")}
                  class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderPlays')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.skipcount}
                <div use:columnResize={{ column: "skipcount", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "skipcount"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("skipcount")}
                  class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderSkips')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.lastplayed}
                <div use:columnResize={{ column: "lastplayed", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "lastplayed"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("lastplayed")}
                  class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderLastPlayed')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.added}
                <div use:columnResize={{ column: "added", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "added"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("added")}
                  class="text-center hover:text-brand-text-primary transition-colors flex items-center justify-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate">{i18n.t('collection.tableHeaderAdded')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.duration}
                <div use:columnResize={{ column: "duration", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "length_nanosec"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("length_nanosec")}
                  class="flex items-center justify-center hover:text-brand-text-primary transition-colors font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<Clock class="w-3.5 h-3.5 shrink-0" /> {arrow}{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.path}
                <div use:columnResize={{ column: "path", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden">
                <SortableHeader
                  active={singleSortField === "path"}
                  sortAsc={singleSortAsc}
                  onclick={() => toggleSingleSort("path")}
                  class="text-left hover:text-brand-text-primary transition-colors flex items-center gap-1 font-semibold uppercase tracking-wider min-w-0 w-full"
                >
                  {#snippet label(arrow)}<span class="truncate max-w-[calc(100%-0.5rem)]">{i18n.t('collection.tableHeaderPath')} {arrow}</span>{/snippet}
                </SortableHeader>
                </div>
              {/if}
              {#if collectionStore.visibleColumns.actions}
                <div use:columnResize={{ column: "actions", onResize: collectionStore.setColumnWidth.bind(collectionStore), onReset: collectionStore.resetColumnWidth.bind(collectionStore) }} class="relative overflow-hidden text-center">{i18n.t('collection.tableHeaderActions')}</div>
              {/if}
            </div>
          </div>

          <div class="divide-y divide-brand-border/40 rounded-b-lg overflow-hidden">
            {#each sortedSingleSongs as song, index (song.id)}
              {@const disconnected = !song.unavailable && collectionStore.isPathOnDisconnectedDrive(song.path)}
              {@const disabled = song.unavailable || disconnected}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <!-- svelte-ignore a11y_click_events_have_key_events -->
              <div
                data-song-row="true"
                ondblclick={() => !disabled && handlePlaySingle(song)}
                oncontextmenu={(e) => !disabled && handleSingleContextMenu(e, song)}
                style={gridColsStyle}
                title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : undefined}
                class="grid items-center hover:bg-brand-sidebar/40 group transition-colors py-2 px-4 text-sm
                  {disabled ? 'opacity-50 cursor-not-allowed' : ''}
                  {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : ''}"
              >
                <div class="text-center flex justify-center relative w-9 h-6 items-center">
                  {#if playerStore.currentSong && playerStore.currentSong.id === song.id && playerStore.state === 'playing'}
                    <div class="flex items-center justify-center gap-0.5 h-3.5 w-3.5 absolute group-hover:opacity-0 transition-opacity">
                      <NowPlayingBars />
                    </div>
                  {/if}
                  <button
                    onclick={(e) => { e.stopPropagation(); if (!disabled) handlePlaySingle(song); }}
                    class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-all duration-150 disabled:opacity-0 disabled:cursor-not-allowed"
                    disabled={disabled}
                    title={disconnected ? i18n.t('collection.driveDisconnectedTooltip') : i18n.t('collection.playSong')}
                  >
                    <Play class="w-3.5 h-3.5 fill-current" />
                  </button>
                </div>

                {#if collectionStore.visibleColumns.track}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {formatTrackNumber(song.track, song.disc, singleDiscCount, index)}
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.title}
                  <div class="font-medium truncate pr-4 flex items-center gap-2 min-w-0">
                    <CoverArt
                      songId={song.id}
                      artEmbedded={song.art_embedded}
                      artAutomatic={song.art_automatic}
                      artManual={song.art_manual}
                      sizeClass="w-7 h-7 rounded shrink-0"
                    />
                    <span class="truncate {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}">
                      {song.title || i18n.t('collection.unknownSong')}
                    </span>
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.artist}
                  <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-medium">
                    {song.artist || i18n.t('collection.unknownArtist')}
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.album}
                  <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-medium">
                    {song.album || i18n.t('collection.unknownAlbum')}
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.composer}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.composer}>
                    {song.composer || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.album_artist}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.album_artist}>
                    {song.album_artist || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.format}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-semibold uppercase">
                    {song.filetype ? song.filetype.toUpperCase() : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.year}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {song.year || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.genre}
                  <div class="truncate pr-2 min-w-0" title={song.genre}>
                    {#if song.genre}
                      <GenreChips genre={song.genre} />
                    {:else}
                      <span class="text-brand-text-secondary text-xs font-medium">—</span>
                    {/if}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.grouping}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium" title={song.grouping}>
                    {song.grouping || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.bpm}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {song.bpm || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.initial_key}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {song.initial_key || "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.bitrate}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {song.bitrate ? `${song.bitrate}k` : "—"}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.samplerate}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {formatSampleRate(song.samplerate)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.bitdepth}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {formatBitDepth(song.bitdepth)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.channels}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {formatChannels(song.channels)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.filesize}
                  <div class="text-brand-text-primary truncate pr-2 min-w-0 text-xs font-medium">
                    {formatFileSize(song.filesize)}
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.rating}
                  <div class="flex justify-center">
                    <SongRating rating={song.rating} onRate={(r) => rateSingle(song, r)} />
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.playcount}
                  <div class="text-center text-brand-text-primary font-medium">
                    {song.playcount ?? 0}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.skipcount}
                  <div class="text-center text-brand-text-primary font-medium text-xs">
                    {song.skipcount ?? 0}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.lastplayed}
                  <div class="text-center text-brand-text-primary text-xs whitespace-nowrap">
                    {formatDate(song.lastplayed)}
                  </div>
                {/if}
                {#if collectionStore.visibleColumns.added}
                  <div class="text-center text-brand-text-primary text-xs whitespace-nowrap">
                    {formatDateAdded(song.added)}
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.duration}
                  <div class="text-center text-brand-text-primary text-xs font-medium">
                    {formatDuration(song.length_nanosec)}
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.path}
                  <div class="text-brand-text-primary truncate pr-4 min-w-0 text-xs font-medium" title={song.path}>
                    {song.path || "—"}
                  </div>
                {/if}

                {#if collectionStore.visibleColumns.actions}
                  <div class="flex items-center justify-center gap-2.5">
                    <button
                      onclick={(e) => { e.stopPropagation(); handleAddSingleToPlaylist(song.id); }}
                      class="text-brand-text-primary hover:text-brand-accent-text transition-colors"
                      title={playlistsStore.activeCustomPlaylist
                        ? i18n.t('collection.addPlaylistTooltip', { name: playlistsStore.activeCustomPlaylist.name })
                        : i18n.t('collection.addPlaylistTooltipDefault')}
                    >
                      <Plus class="w-4 h-4" />
                    </button>
                    <button
                      onclick={(e) => { e.stopPropagation(); openTagEditor(song.id); }}
                      class="text-brand-text-primary hover:text-brand-accent-text transition-colors"
                      title={i18n.t('collection.editTagsTooltip')}
                    >
                      <Edit3 class="w-4 h-4" />
                    </button>
                  </div>
                {/if}
              </div>
            {/each}
          </div>
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
      if (songs.length > 0) {
        const context: PlayContext = { type: "album", album: album.album || "", albumArtist: album.artist || undefined };
        playerStore.playSongs(songs.map(s => s.id), 0, undefined, context);
      }
    }}
    onAddToPlaylist={async () => {
      let songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
      if (songs.length > 0) {
        const targetPlaylist = playlistsStore.activeCustomPlaylist;
        const isQueue = !targetPlaylist || targetPlaylist.is_queue;
        const songIds = songs.map(s => s.id);

        if (isQueue) {
          {
            await playlistsStore.addSongsToQueue(songIds);
            const name = album.album || i18n.t("collection.unknownAlbum");
            toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
          }
        } else {
          await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
          toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`));
        }
      }
    }}
    onGoToArtist={album.artist && album.artist !== artistName ? () => collectionStore.viewArtist(album.artist || "") : undefined}
    onClose={() => { albumContextMenuState = null; }}
  />
{/if}

{#if singleContextMenuState}
  {@const song = singleContextMenuState.song}
  <SongContextMenu
    x={singleContextMenuState.x}
    y={singleContextMenuState.y}
    {song}
    onPlay={() => handlePlaySingle(song)}
    onAddToPlaylist={() => handleAddSingleToPlaylist(song.id)}
    onEditTags={() => openTagEditor(song.id)}
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

<style>
  :global(.carousel-scroll) {
    scrollbar-width: none;
    -ms-overflow-style: none;
  }
  :global(.carousel-scroll::-webkit-scrollbar) {
    display: none;
  }
</style>
