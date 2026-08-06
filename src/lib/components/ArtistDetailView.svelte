<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { shuffleArray } from "../utils/shuffle";
  import CoverArt from "./CoverArt.svelte";
  import CoverStack from "./CoverStack.svelte";
  import AlbumCard from "./AlbumCard.svelte";
  import CarouselCard from "./CarouselCard.svelte";
  import PlaylistCard from "./PlaylistCard.svelte";
  import AlbumContextMenu from "./AlbumContextMenu.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";
  import PlayShuffleButtons from "./PlayShuffleButtons.svelte";
  import type { Song, Playlist, AlbumItem, PlayContext } from "../types";
  import { getArtistAlbums } from "../utils/artist";
  import { songsToCoverStack } from "../utils/covers";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { rememberScroll } from "../utils/scrollMemory";

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

  // Some artists have no proper album releases at all (every track is a loose
  // single with no album tag), so getArtistAlbums() returns nothing and the
  // Albums/Singles/Popular filters all end up empty. Fall back to showing the
  // artist's individual songs directly rather than an empty "no releases" state.
  // Mutually exclusive with `singles` (albums.length is 0 whenever this is
  // populated), so combining the two counts is safe.
  let looseSongs = $derived(
    albums.length === 0 ? [...songs].sort((a, b) => (a.title || "").localeCompare(b.title || "")) : []
  );

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
    const queuePl = await playlistsStore.ensureQueuePlaylist();
    await playerStore.setShuffleMode("off");
    await playerStore.playSongs(songs.map((s) => s.id), 0, queuePl?.id, undefined, "Queue");
    if (queuePl) {
      playlistsStore.selectPlaylist(queuePl.id);
      collectionStore.viewPlaylist(queuePl.id);
    }
  }

  async function handleShufflePlay() {
    if (songs.length === 0) return;
    const queuePl = await playlistsStore.ensureQueuePlaylist();
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
  <!-- Stacked Cover Art Hero & Summary Banner Header -->
  <div class="relative z-30 w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6">
    <div class="flex items-start justify-between gap-6 relative z-10">
      <!-- Left Title & Summary Metadata -->
      <div class="flex flex-col justify-end gap-2 max-w-xl">
        <h1 class="text-3xl sm:text-4xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5">{artistName}</h1>

        <!-- Summary Metadata Line -->
        <div class="flex items-center gap-3 text-xs text-brand-text-secondary font-medium mt-1">
          <span>{i18n.t('artistDetail.statsLine', { genre: genreLabel, albums: albumsText, songs: songsText, duration: totalDurationLabel })}</span>
        </div>

        <!-- Action Buttons: Play All & Shuffle Play -->
        <div class="flex items-center gap-3 mt-3">
          <PlayShuffleButtons
            onPlayAll={handlePlayAll}
            onShufflePlay={handleShufflePlay}
            disabled={loading || songs.length === 0}
          />
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

  <!-- Release Categories (Sets, Albums, EPs, Singles) -->
  <div class="px-6 pt-6 flex flex-col gap-8">
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

    {#if singles.length > 0}
      <HorizontalScrollRow title={i18n.t('artistDetail.singlesFilter', { count: singles.length })}>
        {#each singles as album (album.album)}
          <AlbumCard
            {album}
            widthClass="w-48 shrink-0"
            onclick={() => openAlbum(album)}
            oncontextmenu={(e) => handleAlbumContextMenu(e, album)}
          />
        {/each}
      </HorizontalScrollRow>
    {:else if looseSongs.length > 0}
      <HorizontalScrollRow title={i18n.t('artistDetail.singlesFilter', { count: looseSongs.length })}>
        {#each looseSongs as song (song.id)}
          <CarouselCard item={{ type: "song", song }} />
        {/each}
      </HorizontalScrollRow>
    {/if}

    {#if albums.length === 0 && looseSongs.length === 0 && !loading}
      <p class="text-xs text-brand-text-secondary py-8 text-center">{i18n.t('artistDetail.noReleasesFound')}</p>
    {/if}
  </div>

  <!-- Playlists featuring this artist -->
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
        if (playlistsStore.activeCustomPlaylist) {
          await playlistsStore.addSongsToPlaylist(playlistsStore.activeCustomPlaylist.id, songs.map(s => s.id));
          toastStore.show(i18n.t("playlists.addedToPlaylistSuccess", { name: playlistsStore.activeCustomPlaylist.name }, `Added to ${playlistsStore.activeCustomPlaylist.name}`));
        } else {
          const queuePl = await playlistsStore.ensureQueuePlaylist();
          if (queuePl) {
            const songIds = songs.map(s => s.id);
            await playlistsStore.addSongsToPlaylist(queuePl.id, songIds);
            await invoke("append_songs_to_player_playlist", { songIds });
            const name = album.album || i18n.t("collection.unknownAlbum");
            toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
          }
        }
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
