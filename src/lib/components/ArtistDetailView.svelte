<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import { ArrowLeft, Play, Shuffle, ListMusic } from "lucide-svelte";
  import type { Song, Playlist, AlbumItem } from "../types";
  import { getArtistAlbums, getArtistGradient } from "../utils/artist";
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

  $effect(() => {
    const requested = artistName;
    loading = true;
    Promise.all([
      invoke<Song[]>("get_songs_by_artist", { artist: requested }),
      invoke<Playlist[]>("get_playlists_by_artist", { artist: requested })
    ])
      .then(([fetchedSongs, fetchedPlaylists]) => {
        if (requested !== artistName) return;
        songs = fetchedSongs.filter((s) => !collectionStore.isFormatExcluded(s.filetype));
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

  let fullAlbums = $derived(albums.filter((a) => a.track_count > 7));
  let singlesAndEps = $derived(albums.filter((a) => a.track_count <= 7));

  let albumPopularity = $derived.by(() => {
    const map = new Map<string, number>();
    for (const s of songs) {
      if (!s.album) continue;
      map.set(s.album, (map.get(s.album) ?? 0) + (s.playcount ?? 0));
    }
    return map;
  });

  let hasAnyPlaycount = $derived(songs.some((s) => (s.playcount ?? 0) > 0));

  let popularReleases = $derived.by(() => {
    const list = [...albums];
    if (hasAnyPlaycount) {
      list.sort((a, b) => (albumPopularity.get(b.album ?? "") ?? 0) - (albumPopularity.get(a.album ?? "") ?? 0));
    } else {
      list.sort((a, b) => (b.year ?? 0) - (a.year ?? 0));
    }
    return list;
  });

  let discographyFilter = $state<"popular" | "albums" | "singles">("popular");
  let showAll = $state(false);
  const POPULAR_CAP = 8;

  function setDiscographyFilter(filter: "popular" | "albums" | "singles") {
    discographyFilter = filter;
    showAll = false;
  }

  let discographySource = $derived(
    discographyFilter === "albums" ? fullAlbums : discographyFilter === "singles" ? singlesAndEps : popularReleases
  );
  let discographyItems = $derived(showAll ? discographySource : discographySource.slice(0, POPULAR_CAP));

  function openAlbum(album: AlbumItem) {
    collectionStore.viewAlbum(album.album || "");
  }

  function openPlaylist(id: number) {
    playlistsStore.selectPlaylist(id);
    collectionStore.activeTab = "playlists";
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
  <!-- Hero -->
  <div class="relative h-72 sm:h-80 w-full overflow-hidden shrink-0">
    <div class="absolute inset-0 z-0 opacity-25 blur-3xl scale-110 pointer-events-none">
      {#if albums[0]}
        <CoverArt
          songId={undefined}
          artEmbedded={albums[0].art_embedded}
          artAutomatic={albums[0].art_automatic}
          artManual={albums[0].art_manual}
          sizeClass="w-full h-full object-cover"
        />
      {:else}
        <div class="w-full h-full bg-gradient-to-br {getArtistGradient(artistName)}"></div>
      {/if}
    </div>

    {#if albums.length > 0}
      <div class="absolute right-8 top-8 w-64 h-64 hidden md:block">
        {#each albums.slice(0, 6) as album, i (((album.album ?? "unknown")) + i)}
          <div
            class="absolute inset-0 rounded-lg overflow-hidden border border-brand-border/50 shadow-2xl"
            style="z-index: {10 - i}; transform: translate({i * 22}px, {i * -16}px) rotate({i * 7}deg) scale({1 - i * 0.06}); opacity: {1 - i * 0.08};"
          >
            <CoverArt
              songId={undefined}
              artEmbedded={album.art_embedded}
              artAutomatic={album.art_automatic}
              artManual={album.art_manual}
              sizeClass="w-full h-full"
            />
          </div>
        {/each}
      </div>
    {/if}

    <div class="absolute inset-0 z-10 bg-gradient-to-t from-brand-main via-brand-main/70 to-transparent"></div>

    <div class="relative z-20 flex flex-col justify-end h-full p-8 gap-3">
      <button
        onclick={goBackToArtists}
        class="self-start flex items-center gap-1.5 text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
      >
        <ArrowLeft class="w-4 h-4" /> {i18n.t('artistDetail.backToArtists')}
      </button>
      <h1 class="text-4xl sm:text-5xl font-black text-brand-text-primary leading-snug truncate py-0.5">{artistName}</h1>
      <p class="text-sm text-brand-text-secondary">
        {i18n.t('artistDetail.statsLine', { genre: genreLabel, albumCount: albums.length, songCount: songs.length, duration: totalDurationLabel })}
      </p>
      <div class="flex items-center gap-3 mt-2">
        <button
          onclick={handlePlayAll}
          disabled={loading || songs.length === 0}
          class="flex items-center gap-2 px-5 py-2 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
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
  </div>

  <!-- Discography -->
  <div class="px-8 pt-8">
    <div class="flex items-center justify-between mb-4">
      <h2 class="text-xl font-semibold text-brand-text-primary">{i18n.t('artistDetail.discography')}</h2>
      {#if discographySource.length > POPULAR_CAP}
        <button
          onclick={() => { showAll = !showAll; }}
          class="text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
        >
          {showAll ? i18n.t('artistDetail.showLess') : i18n.t('artistDetail.showAll')}
        </button>
      {/if}
    </div>

    <div class="flex items-center gap-2 mb-4">
      <button
        onclick={() => setDiscographyFilter("popular")}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {discographyFilter === 'popular' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        {i18n.t('artistDetail.popularReleases')}
      </button>
      <button
        onclick={() => setDiscographyFilter("albums")}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {discographyFilter === 'albums' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        {i18n.t('artistDetail.albumsFilter')}
      </button>
      <button
        onclick={() => setDiscographyFilter("singles")}
        class="px-3 py-1 rounded-full text-xs font-medium border transition-all cursor-pointer flex-shrink-0 {discographyFilter === 'singles' ? 'bg-brand-border border-brand-border text-white font-semibold shadow-sm' : 'border-transparent text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
      >
        {i18n.t('artistDetail.singlesAndEps')}
      </button>
    </div>

    {#if discographyItems.length > 0}
      <HorizontalScrollRow>
        {#each discographyItems as album (album.album)}
          <button
            onclick={() => openAlbum(album)}
            oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
            class="w-40 shrink-0 bg-brand-sidebar border border-brand-border/60 rounded-xl p-3 flex flex-col text-left hover:border-brand-accent/40 transition-all duration-200 cursor-pointer"
          >
            <div class="aspect-square bg-brand-main rounded-lg mb-2 overflow-hidden border border-brand-border">
              <CoverArt
                songId={undefined}
                artEmbedded={album.art_embedded}
                artAutomatic={album.art_automatic}
                artManual={album.art_manual}
                sizeClass="w-full h-full"
              />
            </div>
            <span class="font-semibold text-xs text-brand-text-primary truncate">{album.album || i18n.t('collection.unknownAlbum')}</span>
            <span class="text-[10px] text-brand-text-secondary/50 mt-0.5">
              {album.year || ""}{album.year ? " • " : ""}{album.track_count <= 7 ? i18n.t('artistDetail.singleEp') : i18n.t('collection.tableHeaderAlbum')}
            </span>
          </button>
        {/each}
      </HorizontalScrollRow>
    {:else if !loading}
      <p class="text-xs text-brand-text-secondary/60 py-8 text-center">{i18n.t('artistDetail.noReleasesFound')}</p>
    {/if}
  </div>

  <!-- Playlists featuring this artist -->
  {#if playlists.length > 0}
    <div class="px-8 pt-10" class:pb-24={playerStore.hasEverPlayed}>
      <h2 class="text-xl font-semibold text-brand-text-primary mb-4">{i18n.t('artistDetail.playlistsFeaturing', { artist: artistName })}</h2>
      <HorizontalScrollRow>
        {#each playlists as playlist (playlist.id)}
          <PlaylistCard {playlist} onClick={() => openPlaylist(playlist.id)} />
        {/each}
      </HorizontalScrollRow>
    </div>
  {:else}
    <div class:pb-24={playerStore.hasEverPlayed}></div>
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
      songs = songs.filter(s => !collectionStore.isFormatExcluded(s.filetype));
      if (songs.length > 0) {
        playerStore.playSongs(songs.map(s => s.id), 0);
      }
    }}
    onAddToPlaylist={async () => {
      let songs = await invoke<Song[]>("get_songs_by_album", { album: album.album || "" });
      songs = songs.filter(s => !collectionStore.isFormatExcluded(s.filetype));
      if (songs.length > 0 && playlistsStore.activePlaylistId !== null) {
        await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, songs.map(s => s.id));
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
