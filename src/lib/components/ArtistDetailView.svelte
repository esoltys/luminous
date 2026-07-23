<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import CoverStack from "./CoverStack.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import CarouselCard from "./CarouselCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import { Play, Shuffle } from "lucide-svelte";
  import type { Song, Playlist, AlbumItem, PlayContext } from "../types";
  import { getArtistAlbums } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { i18n } from "../stores/i18n.svelte";

  let { artistName }: { artistName: string } = $props();

  let songs = $state<Song[]>([]);
  let playlists = $state<Playlist[]>([]);
  let loading = $state(true);

  let albumContextMenuState = $state<{ x: number; y: number; album: AlbumItem } | null>(null);

  function handleAlbumContextMenu(event: MouseEvent, album: AlbumItem) {
    event.preventDefault();
    albumContextMenuState = { x: event.clientX, y: event.clientY, album };
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
      invoke<Playlist[]>("get_playlists_by_artist", { artist: requested })
    ])
      .then(([fetchedSongs, fetchedPlaylists]) => {
        if (requested !== artistName) return;
        songs = fetchedSongs;
        playlists = fetchedPlaylists;
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

  // Mirrors getAlbumCategoryLabel()'s canonical release categories (used for
  // the per-card badge everywhere else in the app) so the discography tabs
  // agree with how a release is labeled elsewhere: multi-disc releases are
  // "Sets" regardless of track count, then Albums/EPs/Singles by track count.
  let sets = $derived(albums.filter((a) => a.disc_count > 1));
  let fullAlbums = $derived(albums.filter((a) => a.disc_count <= 1 && a.track_count >= 7));
  let eps = $derived(albums.filter((a) => a.disc_count <= 1 && a.track_count >= 2 && a.track_count <= 6));
  let singles = $derived(albums.filter((a) => a.disc_count <= 1 && a.track_count === 1));

  let albumsText = $derived(
    fullAlbums.length === 1 ? i18n.t("collection.oneAlbum") : i18n.t("collection.albumsCount", { count: fullAlbums.length })
  );
  let songsText = $derived(
    songs.length === 1 ? i18n.t("playlists.oneSong") : i18n.t("playlists.songsCount", { count: songs.length })
  );

  let discographyFilter = $state<"sets" | "albums" | "eps" | "singles">("albums");
  let showAll = $state(false);
  const POPULAR_CAP = 8;

  function setDiscographyFilter(filter: "sets" | "albums" | "eps" | "singles") {
    discographyFilter = filter;
    showAll = false;
  }

  // Some artists have no proper album releases at all (every track is a loose
  // single with no album tag), so getArtistAlbums() returns nothing and the
  // Albums/Singles/Popular filters all end up empty. Fall back to showing the
  // artist's individual songs directly rather than an empty "no releases" state.
  // Mutually exclusive with `singles` (albums.length is 0 whenever this is
  // populated), so combining the two counts is safe.
  let looseSongs = $derived(
    albums.length === 0 ? [...songs].sort((a, b) => (a.title || "").localeCompare(b.title || "")) : []
  );
  let singlesCount = $derived(singles.length + looseSongs.length);

  // Land on whichever category actually has releases when switching artists,
  // rather than defaulting to a possibly-empty "Albums" tab (e.g. an
  // EP-only or singles-only artist). Keyed on artistName so a manual tab
  // pick while browsing the same artist isn't overridden.
  $effect(() => {
    const _artist = artistName;
    discographyFilter = untrack(() => {
      if (fullAlbums.length > 0) return "albums";
      if (sets.length > 0) return "sets";
      if (eps.length > 0) return "eps";
      if (singlesCount > 0) return "singles";
      return "albums";
    });
    showAll = false;
  });

  let discographySource = $derived(
    discographyFilter === "sets" ? sets : discographyFilter === "eps" ? eps : discographyFilter === "singles" ? singles : fullAlbums
  );
  let discographyItems = $derived(showAll ? discographySource : discographySource.slice(0, POPULAR_CAP));

  let displayedLooseSongs = $derived(showAll ? looseSongs : looseSongs.slice(0, POPULAR_CAP));

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
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(songs.map((s) => s.id), 0);
  }

  async function handleShufflePlay() {
    if (songs.length === 0) return;
    const ids = songs.map((s) => s.id);
    const randomIndex = Math.floor(Math.random() * ids.length);
    await playerStore.setShuffleMode("all");
    await playerStore.playSongs(ids, randomIndex);
  }
</script>

<div class="flex-1 flex flex-col overflow-y-auto bg-brand-main text-brand-text-secondary h-full carousel-scroll">
  <!-- Stacked Cover Art Hero & Summary Banner Header -->
  <div class="relative w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6">
    <div class="flex items-end justify-between gap-6 relative z-10">
      <!-- Left Title & Summary Metadata -->
      <div class="flex flex-col justify-end gap-2 max-w-xl">
        <h1 class="text-3xl sm:text-4xl font-extrabold text-brand-text-primary leading-snug truncate py-0.5">{artistName}</h1>

        <!-- Summary Metadata Line -->
        <div class="flex items-center gap-3 text-xs text-brand-text-secondary font-medium mt-1">
          <span>{i18n.t('artistDetail.statsLine', { genre: genreLabel, albums: albumsText, songs: songsText, duration: totalDurationLabel })}</span>
        </div>

        <!-- Action Buttons: Play All & Shuffle Play -->
        <div class="flex items-center gap-3 mt-3">
          <button
            onclick={handlePlayAll}
            disabled={loading || songs.length === 0}
            class="flex items-center gap-2 px-5 py-2 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-md shadow-brand-accent/20"
          >
            <Play class="w-4 h-4 fill-current" /> {i18n.t('artistDetail.playAll')}
          </button>
          <button
            onclick={handleShufflePlay}
            disabled={loading || songs.length === 0}
            class="flex items-center gap-2 px-5 py-2 rounded-full border border-brand-border text-brand-text-primary hover:bg-brand-sidebar font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Shuffle class="w-4 h-4" /> {i18n.t('artistDetail.shuffleAndPlay')}
          </button>
        </div>
      </div>

      <!-- Right: 3D Stacked Album Cover Preview Header -->
      {#if headerCovers.length > 0}
        <div class="relative w-48 h-36 hidden sm:block shrink-0 flex items-center justify-end">
          <CoverStack covers={headerCovers} direction="left" sizeClass="w-28 h-28" />
        </div>
      {/if}
    </div>
  </div>

  <!-- Discography -->
  <div class="px-6 pt-8">
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-xl font-semibold text-brand-text-primary">{i18n.t('artistDetail.discography')}</h2>
      {#if discographySource.length > POPULAR_CAP || looseSongs.length > POPULAR_CAP}
        <button
          onclick={() => { showAll = !showAll; }}
          class="text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
        >
          {showAll ? i18n.t('artistDetail.showLess') : i18n.t('artistDetail.showAll')}
        </button>
      {/if}
    </div>

    {#if albums.length > 0 || looseSongs.length > 0}
      <div class="flex items-center gap-2 mb-4">
        {#if sets.length > 0}
          <button
            onclick={() => setDiscographyFilter("sets")}
            class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {discographyFilter === 'sets' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
          >
            {i18n.t('artistDetail.setsFilter', { count: sets.length })}
          </button>
        {/if}
        {#if fullAlbums.length > 0}
          <button
            onclick={() => setDiscographyFilter("albums")}
            class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {discographyFilter === 'albums' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
          >
            {i18n.t('artistDetail.albumsFilter', { count: fullAlbums.length })}
          </button>
        {/if}
        {#if eps.length > 0}
          <button
            onclick={() => setDiscographyFilter("eps")}
            class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {discographyFilter === 'eps' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
          >
            {i18n.t('artistDetail.epsFilter', { count: eps.length })}
          </button>
        {/if}
        {#if singlesCount > 0}
          <button
            onclick={() => setDiscographyFilter("singles")}
            class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {discographyFilter === 'singles' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
          >
            {i18n.t('artistDetail.singlesFilter', { count: singlesCount })}
          </button>
        {/if}
      </div>
    {/if}

    {#if discographyItems.length > 0}
      <HorizontalScrollRow>
        {#each discographyItems as album (album.album)}
          <AlbumCard
            {album}
            widthClass="w-40 shrink-0"
            showArtist={false}
            onclick={() => openAlbum(album)}
            oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
          />
        {/each}
      </HorizontalScrollRow>
    {:else if displayedLooseSongs.length > 0}
      <HorizontalScrollRow>
        {#each displayedLooseSongs as song (song.id)}
          <CarouselCard item={{ type: "song", song }} />
        {/each}
      </HorizontalScrollRow>
    {:else if !loading}
      <p class="text-xs text-brand-text-secondary/60 py-8 text-center">{i18n.t('artistDetail.noReleasesFound')}</p>
    {/if}
  </div>

  <!-- Playlists featuring this artist -->
  {#if playlists.length > 0}
    <div class="px-6 pt-10" class:pb-28={!!playerStore.currentSong}>
      <HorizontalScrollRow title={i18n.t('artistDetail.playlistsFeaturing', { artist: artistName })}>
        {#each playlists as playlist (playlist.id)}
          <PlaylistCard {playlist} widthClass="w-44 shrink-0" onClick={() => openPlaylist(playlist)} />
        {/each}
      </HorizontalScrollRow>
    </div>
  {:else}
    <div class:pb-28={!!playerStore.currentSong}></div>
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
      if (songs.length > 0 && playlistsStore.activeCustomPlaylist) {
        await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songs.map(s => s.id));
      } else if (songs.length > 0) {
        alert(i18n.t("collection.selectPlaylistFirstAlert"));
      }
    }}
    onGoToArtist={album.artist && album.artist !== artistName ? () => collectionStore.viewArtist(album.artist || "") : undefined}
    onClose={() => { albumContextMenuState = null; }}
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
