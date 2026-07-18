<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { applySongStats, type SongStatsPayload } from "../utils/stats";
  import { collectionStore } from "../stores/collection.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import TagEditor from "./TagEditor.svelte";
  import { ArrowLeft, Play, Shuffle, Plus, Edit3, Clock, Music } from "lucide-svelte";
  import type { Song, AlbumItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";

  let { albumName }: { albumName: string } = $props();

  let songs = $state<Song[]>([]);
  let loading = $state(true);
  let editingSongId = $state<number | null>(null);

  let albumItem = $derived(
    collectionStore.albums.find((a) => a.album === albumName) || null
  );

  let artistName = $derived.by(() => {
    if (albumItem?.artist) return albumItem.artist;
    if (songs.length > 0) return songs[0].album_artist || songs[0].artist || "";
    return "";
  });

  let genreLabel = $derived.by(() => {
    if (songs.length > 0 && songs[0].genre) return songs[0].genre;
    return i18n.t('albumDetail.unknownGenre');
  });

  let yearLabel = $derived.by(() => {
    if (albumItem?.year) return albumItem.year;
    if (songs.length > 0 && songs[0].year) return songs[0].year;
    return null;
  });

  let isLossless = $derived.by(() => {
    const losslessFormats = ["FLAC", "WAV", "ALAC", "APE", "AIFF"];
    return songs.some(s => s.filetype && losslessFormats.includes(s.filetype.toUpperCase()));
  });

  let totalDurationLabel = $derived.by(() => {
    const totalNs = songs.reduce((sum, s) => sum + (s.length_nanosec ?? 0), 0);
    const totalMinutes = Math.round(totalNs / 1_000_000_000 / 60);
    const h = Math.floor(totalMinutes / 60);
    const m = totalMinutes % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  });

  $effect(() => {
    const requested = albumName;
    loading = true;
    invoke<Song[]>("get_songs_by_album", { album: requested })
      .then((fetchedSongs) => {
        if (requested !== albumName) return;
        let filtered = fetchedSongs.filter((s) => !collectionStore.isFormatExcluded(s.filetype));
        // Sort by disc, then by track
        filtered.sort((a, b) => {
          if (a.disc !== b.disc) {
            return (a.disc ?? 1) - (b.disc ?? 1);
          }
          return (a.track ?? 0) - (b.track ?? 0);
        });
        songs = filtered;
      })
      .catch((err) => {
        console.error("Failed to load album detail:", err);
      })
      .finally(() => {
        if (requested === albumName) loading = false;
      });
  });

  function goBack() {
    collectionStore.selectedAlbumName = null;
    collectionStore.activeSubTab = "albums";
  }

  function handlePlaySong(song: Song) {
    const index = songs.findIndex((s) => s.id === song.id);
    const songIds = songs.map((s) => s.id);
    playerStore.playSongs(songIds, index >= 0 ? index : 0);
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

  async function handleAddSongToPlaylist(songId: number) {
    if (playlistsStore.activePlaylistId !== null) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, [songId]);
    } else {
      alert(i18n.t('collection.selectPlaylistFirstAlert'));
    }
  }

  async function handleAddAlbumToPlaylist() {
    if (songs.length === 0) return;
    if (playlistsStore.activePlaylistId !== null) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, songs.map((s) => s.id));
    } else {
      alert(i18n.t('collection.selectPlaylistFirstAlert'));
    }
  }

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  function openAlbumTagEditor() {
    if (songs.length === 0) return;
    openTagEditor(songs[0].id);
  }

  function handleTagEditorSaved() {
    collectionStore.refreshLibrary();
    // Reload songs
    loading = true;
    invoke<Song[]>("get_songs_by_album", { album: albumName })
      .then((fetchedSongs) => {
        let filtered = fetchedSongs.filter((s) => !collectionStore.isFormatExcluded(s.filetype));
        filtered.sort((a, b) => {
          if (a.disc !== b.disc) {
            return (a.disc ?? 1) - (b.disc ?? 1);
          }
          return (a.track ?? 0) - (b.track ?? 0);
        });
        songs = filtered;
      })
      .catch((err) => console.error(err))
      .finally(() => loading = false);
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  // Sync rating/playcount changes from other views and scrobble bumps into
  // this view's locally fetched song list.
  $effect(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<SongStatsPayload>("song-stats-changed", (event) => {
      const song = songs.find((s) => s.id === event.payload.song_id);
      if (song) applySongStats(song, event.payload);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  });
</script>

<div class="flex-1 flex flex-col overflow-y-auto bg-brand-main text-brand-text-secondary h-full carousel-scroll">
  <!-- Hero Section -->
  <div class="p-6 md:p-8 flex flex-col md:flex-row gap-6 md:gap-8 items-start md:items-end border-b border-brand-border/30 bg-linear-to-b from-brand-sidebar/20 to-transparent">
    <!-- Album Art -->
    <div class="w-40 h-40 sm:w-48 sm:h-48 md:w-56 md:h-56 bg-brand-sidebar rounded-2xl overflow-hidden border border-brand-border/60 shadow-2xl shrink-0">
      <CoverArt
        songId={undefined}
        artEmbedded={albumItem?.art_embedded}
        artAutomatic={albumItem?.art_automatic}
        artManual={albumItem?.art_manual}
        sizeClass="w-full h-full object-cover"
      />
    </div>

    <!-- Metadata & Controls -->
    <div class="flex flex-col gap-2 min-w-0 flex-1">
      <button
        onclick={goBack}
        class="self-start flex items-center gap-1.5 text-xs text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer mb-2"
      >
        <ArrowLeft class="w-4 h-4" /> {i18n.t('albumDetail.backToAlbums')}
      </button>

      <h1 class="text-3xl sm:text-4xl font-black text-brand-text-primary leading-tight truncate" title={albumName}>
        {albumName}
      </h1>

      <div class="flex items-center gap-2 text-base font-semibold text-brand-accent-text">
        {#if artistName}
          <button
            onclick={() => collectionStore.viewArtist(artistName)}
            class="hover:underline cursor-pointer transition-colors text-left font-bold"
          >
            {artistName}
          </button>
        {:else}
          <span class="text-brand-text-secondary/50">{i18n.t('collection.unknownArtist')}</span>
        {/if}
      </div>

      <div class="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-brand-text-secondary/85 mt-1 font-medium">
        <span>{genreLabel}</span>
        <span>•</span>
        {#if yearLabel}
          <span>{yearLabel}</span>
          <span>•</span>
        {/if}
        <span>{i18n.t('playlists.songsCount', { count: songs.length })}</span>
        <span>•</span>
        {#if isLossless}
          <span class="px-1.5 py-0.5 text-[9px] font-bold rounded bg-brand-accent/15 text-brand-accent-text border border-brand-accent/30 shadow-sm shrink-0">
            {i18n.t('albumDetail.lossless')}
          </span>
          <span>•</span>
        {/if}
        <span>{totalDurationLabel}</span>
      </div>

      <!-- Control Buttons -->
      <div class="flex items-center gap-3 mt-4 select-none">
        <button
          onclick={handlePlayAll}
          disabled={loading || songs.length === 0}
          class="flex items-center gap-2 px-5 py-2.5 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-md shadow-brand-accent/10"
        >
          <Play class="w-4 h-4 fill-current" /> {i18n.t('artistDetail.playAll')}
        </button>
        <button
          onclick={handleShufflePlay}
          disabled={loading || songs.length === 0}
          class="flex items-center gap-2 px-5 py-2.5 rounded-full border border-brand-border text-brand-text-primary hover:bg-brand-sidebar font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-xs"
        >
          <Shuffle class="w-4 h-4" /> {i18n.t('artistDetail.shuffleAndPlay')}
        </button>
        <button
          onclick={handleAddAlbumToPlaylist}
          disabled={loading || songs.length === 0}
          title={i18n.t('albumDetail.addAllToPlaylistTooltip')}
          class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-xs"
        >
          <Plus class="w-4 h-4" />
        </button>
        <button
          onclick={openAlbumTagEditor}
          disabled={loading || songs.length === 0}
          title={i18n.t('albumDetail.editInfoTooltip')}
          class="flex items-center justify-center w-10 h-10 rounded-full border border-brand-border text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-sidebar transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-xs"
        >
          <Edit3 class="w-4 h-4" />
        </button>
      </div>
    </div>
  </div>

  <!-- Songs Table Section -->
  <div class="px-6 md:px-8 py-6" class:pb-24={playerStore.hasEverPlayed}>
    <div class="border border-brand-border rounded-lg bg-brand-sidebar/30 overflow-hidden">
      <!-- Table Header -->
      <div class="sticky top-0 z-10 flex flex-col bg-brand-sidebar border-b border-brand-border text-[10px] text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
        <div class="grid grid-cols-[36px_40px_1fr_96px_60px_60px_80px] items-center py-2.5 px-4">
          <div class="text-center w-9"></div>
          <div class="text-left">{i18n.t('collection.tableHeaderTrack')}</div>
          <div class="text-left">{i18n.t('collection.tableHeaderTitle')}</div>
          <div class="text-center">{i18n.t('collection.tableHeaderRating')}</div>
          <div class="text-center">{i18n.t('collection.tableHeaderPlays')}</div>
          <div class="text-center">
            <Clock class="w-3.5 h-3.5 mx-auto" />
          </div>
          <div class="text-center">{i18n.t('collection.tableHeaderActions')}</div>
        </div>
      </div>

      <!-- Table Body -->
      <div class="divide-y divide-brand-border/40">
        {#if loading}
          <div class="flex items-center justify-center py-16">
            <div class="text-brand-text-secondary text-sm">{i18n.t('home.loading')}</div>
          </div>
        {:else if songs.length === 0}
          <div class="py-16 text-center select-none">
            <Music class="w-12 h-12 text-brand-accent-text/40 mb-3 mx-auto animate-pulse" />
            <h3 class="text-sm font-semibold text-brand-text-primary mb-1">{i18n.t('collection.noSongsTitle')}</h3>
          </div>
        {:else}
          {#each songs as song, index (song.id)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <div
              ondblclick={() => handlePlaySong(song)}
              class="grid grid-cols-[36px_40px_1fr_96px_60px_60px_80px] items-center hover:bg-brand-sidebar/40 group transition-colors py-2 px-4 text-sm
                {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : ''}"
            >
              <div class="text-center flex justify-center relative w-9 h-6 items-center">
                {#if playerStore.currentSong && playerStore.currentSong.id === song.id && playerStore.state === 'playing'}
                  <div class="flex items-center justify-center gap-0.5 h-3.5 w-3.5 absolute group-hover:opacity-0 transition-opacity">
                    <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.1s"></span>
                    <span class="w-0.5 bg-brand-accent animate-bounce h-2/3" style="animation-delay: 0.2s"></span>
                    <span class="w-0.5 bg-brand-accent animate-bounce h-full" style="animation-delay: 0.3s"></span>
                  </div>
                {/if}
                <button
                  onclick={() => handlePlaySong(song)}
                  class="absolute flex items-center justify-center opacity-0 group-hover:opacity-100 text-brand-accent-text hover:text-brand-accent-text-hover transition-all duration-150 cursor-pointer"
                  title={i18n.t('collection.playSong')}
                >
                  <Play class="w-3.5 h-3.5 fill-current" />
                </button>
              </div>

              <div class="text-brand-text-secondary/70 truncate pr-4 font-medium">
                {song.track !== undefined && song.track !== null ? song.track : index + 1}
              </div>

              <div class="font-medium truncate pr-4 flex items-center gap-2 min-w-0">
                <span class="truncate {playerStore.currentSong && playerStore.currentSong.id === song.id ? 'text-brand-accent-text-hover' : 'text-brand-text-primary'}">
                  {song.title || i18n.t('collection.unknownSong')}
                </span>
              </div>

              <div class="flex justify-center">
                <SongRating rating={song.rating} onRate={(r) => rateSong(song, r)} />
              </div>

              <div class="text-center text-brand-text-secondary/80 font-medium">
                {song.playcount ?? 0}
              </div>

              <div class="text-center text-brand-text-secondary/80 font-medium">
                {formatDuration(song.length_nanosec)}
              </div>

              <div class="flex items-center justify-center gap-2.5">
                <button
                  onclick={() => handleAddSongToPlaylist(song.id)}
                  class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors cursor-pointer"
                  title={i18n.t('collection.addPlaylistTooltip')}
                >
                  <Plus class="w-4 h-4" />
                </button>
                <button
                  onclick={() => openTagEditor(song.id)}
                  class="text-brand-text-secondary/60 hover:text-brand-accent-text transition-colors cursor-pointer"
                  title={i18n.t('collection.editTagsTooltip')}
                >
                  <Edit3 class="w-4 h-4" />
                </button>
              </div>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
</div>

{#if editingSongId !== null}
  <TagEditor
    songId={editingSongId}
    onClose={() => { editingSongId = null; }}
    onSave={handleTagEditorSaved}
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
