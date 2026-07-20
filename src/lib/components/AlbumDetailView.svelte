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
  import SongContextMenu from "./SongContextMenu.svelte";
  import { DiscAlbum, Play, Shuffle, Plus, Edit3, Clock, Music } from "lucide-svelte";
  import type { Song, AlbumItem } from "../types";
  import { i18n } from "../stores/i18n.svelte";

  let { albumName }: { albumName: string } = $props();

  let songs = $state<Song[]>([]);
  let loading = $state(true);
  let editingSongId = $state<number | null>(null);
  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  let selectedSongIds = $state<Set<number>>(new Set());
  let lastSelectedSongId = $state<number | null>(null);

  function handleContextMenu(event: MouseEvent, song: Song) {
    event.preventDefault();
    if (!selectedSongIds.has(song.id)) {
      selectedSongIds = new Set([song.id]);
      lastSelectedSongId = song.id;
    }
    contextMenuState = { x: event.clientX, y: event.clientY, song };
  }

  function handleSongClick(e: MouseEvent, song: Song) {
    if (e.shiftKey && lastSelectedSongId !== null) {
      const idx1 = songs.findIndex((s) => s.id === lastSelectedSongId);
      const idx2 = songs.findIndex((s) => s.id === song.id);
      if (idx1 !== -1 && idx2 !== -1) {
        const start = Math.min(idx1, idx2);
        const end = Math.max(idx1, idx2);
        const newSet = new Set(e.ctrlKey || e.metaKey ? selectedSongIds : []);
        for (let i = start; i <= end; i++) {
          newSet.add(songs[i].id);
        }
        selectedSongIds = newSet;
      }
    } else if (e.ctrlKey || e.metaKey) {
      const newSet = new Set(selectedSongIds);
      if (newSet.has(song.id)) {
        newSet.delete(song.id);
      } else {
        newSet.add(song.id);
      }
      selectedSongIds = newSet;
      lastSelectedSongId = song.id;
    } else {
      selectedSongIds = new Set([song.id]);
      lastSelectedSongId = song.id;
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      const target = e.target as HTMLElement;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) return;
      e.preventDefault();
      selectedSongIds = new Set(songs.map((s) => s.id));
    } else if (e.key === "Escape") {
      selectedSongIds = new Set();
    }
  }

  function handleWindowMouseDown(e: MouseEvent) {
    if (selectedSongIds.size === 0) return;
    const target = e.target as HTMLElement;
    if (!target) return;
    if (
      target.closest("[data-song-row]") ||
      target.closest("[role='menu']") ||
      target.closest("[data-floating-toolbar]") ||
      target.closest("button") ||
      target.closest("input")
    ) {
      return;
    }
    selectedSongIds = new Set();
  }

  async function handleBulkAddToPlaylist() {
    if (selectedSongIds.size === 0) return;
    if (playlistsStore.activePlaylistId !== null) {
      await playlistsStore.addSongsToPlaylist(playlistsStore.activePlaylistId, Array.from(selectedSongIds));
    } else {
      alert(i18n.t("collection.selectPlaylistFirstAlert"));
    }
  }

  function handlePlaySelected() {
    if (selectedSongIds.size === 0) return;
    const selectedList = songs.filter((s) => selectedSongIds.has(s.id));
    if (selectedList.length > 0) {
      playerStore.playSongs(selectedList.map((s) => s.id), 0);
    }
  }

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
  <!-- Album Hero & Summary Banner Header -->
  <div class="relative w-full border-b border-brand-border/60 bg-brand-main/60 backdrop-blur-md px-6 pt-6 pb-6">
    <div class="flex items-end justify-between gap-6 relative z-10">
      <!-- Left Title & Summary Metadata -->
      <div class="flex flex-col justify-end gap-2 min-w-0 max-w-xl">
        <button
          onclick={goBack}
          class="self-start flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-brand-accent-text hover:text-brand-accent-text-hover transition-colors cursor-pointer"
        >
          <DiscAlbum class="w-4 h-4" />
          <span>{i18n.t('albumDetail.backToAlbums')}</span>
        </button>

        <h1 class="text-3xl sm:text-4xl font-extrabold text-brand-text-primary leading-snug truncate py-0.5" title={albumName}>
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
          <span>{songs.length === 1 ? i18n.t('playlists.oneSong') : i18n.t('playlists.songsCount', { count: songs.length })}</span>
          <span>•</span>
          <span>{totalDurationLabel}</span>
        </div>

        <!-- Control Buttons -->
        <div class="flex items-center gap-3 mt-3 select-none">
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

      <!-- Right: Full Album Cover Art -->
      <div class="relative w-40 h-40 hidden sm:block shrink-0">
        <div class="absolute inset-0 rounded-2xl overflow-hidden border border-brand-border/60 shadow-2xl">
          <CoverArt
            songId={undefined}
            artEmbedded={albumItem?.art_embedded}
            artAutomatic={albumItem?.art_automatic}
            artManual={albumItem?.art_manual}
            sizeClass="w-full h-full object-cover"
          />
        </div>
      </div>
    </div>
  </div>

  <!-- Songs Table Section -->
  <div class="px-6 md:px-8 py-6" class:pb-24={playerStore.hasEverPlayed}>
    <div class="border border-brand-border rounded-lg bg-brand-sidebar/30">
      <!-- Table Header -->
      <div class="sticky top-0 z-10 flex flex-col rounded-t-lg bg-brand-sidebar border-b border-brand-border text-[10px] text-brand-text-secondary uppercase tracking-wider font-semibold select-none">
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
      <div class="divide-y divide-brand-border/40 rounded-b-lg overflow-hidden">
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
              data-song-row="true"
              onclick={(e) => handleSongClick(e, song)}
              ondblclick={() => handlePlaySong(song)}
              oncontextmenu={(e) => handleContextMenu(e, song)}
              class="grid grid-cols-[36px_40px_1fr_96px_60px_60px_80px] items-center hover:bg-brand-sidebar/40 group transition-colors py-2 px-4 text-sm cursor-pointer
                {selectedSongIds.has(song.id) ? 'bg-brand-accent/20 border-l-2 border-brand-accent text-brand-accent-text-hover' : (playerStore.currentSong && playerStore.currentSong.id === song.id ? 'bg-brand-accent/10 text-brand-accent-text-hover' : '')}"
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

<svelte:window onkeydown={handleKeydown} onmousedown={handleWindowMouseDown} />

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    selectedCount={selectedSongIds.size}
    onPlay={() => {
      if (selectedSongIds.size > 1) {
        handlePlaySelected();
      } else {
        handlePlaySong(song);
      }
    }}
    onAddToPlaylist={() => {
      if (selectedSongIds.size > 1) {
        handleBulkAddToPlaylist();
      } else {
        handleAddSongToPlaylist(song.id);
      }
    }}
    onGoToArtist={() => collectionStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => collectionStore.viewAlbum(song.album || "")}
    onEditTags={() => openTagEditor(song.id)}
    onClose={() => { contextMenuState = null; }}
  />
{/if}

{#if selectedSongIds.size > 0}
  <div data-floating-toolbar="true" class="absolute bottom-6 left-1/2 -translate-x-1/2 z-40 bg-brand-sidebar/95 border border-brand-border/80 shadow-2xl rounded-full px-5 py-2.5 flex items-center gap-4 text-xs font-semibold backdrop-blur-xl animate-in fade-in slide-in-from-bottom-4 duration-200">
    <span class="text-brand-accent-text font-bold">
      {i18n.t('playlists.selectedCount', { count: selectedSongIds.size })}
    </span>
    <div class="h-4 w-px bg-brand-border/60"></div>
    <button
      onclick={handlePlaySelected}
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors cursor-pointer"
    >
      <Play class="w-3.5 h-3.5 fill-current text-brand-accent-text" />
      <span>{i18n.t('playlists.playSelected')}</span>
    </button>
    <button
      onclick={handleBulkAddToPlaylist}
      class="flex items-center gap-1.5 hover:text-brand-accent-text transition-colors cursor-pointer"
    >
      <Plus class="w-3.5 h-3.5 text-brand-accent-text" />
      <span>{i18n.t('playlists.contextMenuAddToPlaylist')}</span>
    </button>
    <div class="h-4 w-px bg-brand-border/60"></div>
    <button
      onclick={() => { selectedSongIds = new Set(); }}
      class="text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
    >
      {i18n.t('playlists.clearSelection')}
    </button>
  </div>
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
