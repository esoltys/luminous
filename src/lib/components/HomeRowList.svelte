<script lang="ts">
  import type { HomeItem, Song, Playlist } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import { formatRelativeDate } from "../utils/date";
  import { getAlbumCategoryLabel } from "../utils/artist";
  import CoverArt from "./CoverArt.svelte";
  import PlaylistCoverThumb from "./PlaylistCoverThumb.svelte";
  import SongRating from "./SongRating.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import { Play } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    title?: string;
    items: HomeItem[];
    /** "rank" shows a 01-05 numeral + track duration; "added" shows a relative added date. */
    variant: "rank" | "added";
  }

  let { title, items, variant }: Props = $props();

  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  function keyFor(item: HomeItem): string {
    if (item.type === "song") return "s_" + item.song.id;
    if (item.type === "playlist") return "p_" + item.playlist.id;
    return "a_" + (item.album.album || "") + "_" + (item.album.artist || "");
  }

  function titleFor(item: HomeItem): string {
    if (item.type === "song") return item.song.title || i18n.t("collection.unknownSong");
    if (item.type === "album") return item.album.album || i18n.t("collection.unknownAlbum");
    return item.playlist.name;
  }

  function subtitleFor(item: HomeItem): string {
    if (item.type === "song") return item.song.artist || i18n.t("collection.unknownArtist");
    if (item.type === "album") return item.album.artist || i18n.t("collection.variousArtists");
    return i18n.t("sidebar.playlists");
  }

  function formatDuration(ns: number | undefined): string {
    if (!ns) return "0:00";
    const sec = Math.floor(ns / 1_000_000_000);
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return `${m}:${s < 10 ? "0" : ""}${s}`;
  }

  function trailingLabel(item: HomeItem): string {
    if (item.type === "song") {
      return variant === "rank" ? formatDuration(item.song.length_nanosec) : formatRelativeDate(item.song.added);
    }
    if (item.type === "album") {
      return getAlbumCategoryLabel(item.album.track_count, item.album.disc_count);
    }
    return item.playlist.track_count === 1
      ? i18n.t("playlists.oneSong")
      : i18n.t("playlists.songsCount", { count: item.playlist.track_count });
  }

  // Mirrors CarouselCard's openPlaylist: genre/decade auto-playlists open in
  // AutoPlaylistDetailView, custom playlists (including Smart Playlists) in
  // the regular PlaylistView.
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

  function openItem(item: HomeItem) {
    if (item.type === "song" && item.song.album) {
      collectionStore.viewAlbum(item.song.album);
    } else if (item.type === "album") {
      collectionStore.viewAlbum(item.album.album || "");
    } else if (item.type === "playlist") {
      openPlaylist(item.playlist);
    }
  }

  async function playItem(item: HomeItem) {
    if (item.type === "song") {
      await playerStore.playSong(item.song.id);
      return;
    }
    if (item.type === "album") {
      const songs = await invoke<Song[]>("get_songs_by_album", { album: item.album.album || "" });
      if (songs.length > 0) {
        playerStore.playSongs(songs.map((s) => s.id), 0, undefined, {
          type: "album",
          album: item.album.album || "",
          albumArtist: item.album.artist ?? undefined,
        });
      }
      return;
    }
    await playerStore.playPlaylistItem(item.playlist.id, 0);
  }

  function handleContextMenu(e: MouseEvent, item: HomeItem) {
    if (item.type !== "song") return;
    e.preventDefault();
    contextMenuState = { x: e.clientX, y: e.clientY, song: item.song };
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }
</script>

<div class="space-y-4">
  {#if title}
    <h2 class="text-xl font-semibold text-brand-text-primary">{title}</h2>
  {/if}

  <div class="flex flex-col gap-2">
    {#each items as item, i (keyFor(item))}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        role="button"
        tabindex="0"
        onclick={() => openItem(item)}
        oncontextmenu={(e) => handleContextMenu(e, item)}
        onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openItem(item); } }}
        class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 hover:border-brand-accent/40 hover:shadow-md hover:shadow-brand-accent/10 transition-all duration-200 cursor-pointer select-none"
      >
        {#if variant === "rank"}
          <span class="w-5 shrink-0 text-center text-sm font-bold text-brand-text-secondary tabular-nums">
            {String(i + 1).padStart(2, "0")}
          </span>
        {/if}

        <div class="relative shrink-0 rounded-md overflow-hidden">
          {#if item.type === "song"}
            <CoverArt
              songId={item.song.id}
              artEmbedded={item.song.art_embedded}
              artAutomatic={item.song.art_automatic}
              artManual={item.song.art_manual}
              sizeClass="w-11 h-11"
            />
          {:else if item.type === "album"}
            <CoverArt
              songId={undefined}
              artEmbedded={item.album.art_embedded}
              artAutomatic={item.album.art_automatic}
              artManual={item.album.art_manual}
              sizeClass="w-11 h-11"
            />
          {:else}
            <PlaylistCoverThumb playlist={item.playlist} sizeClass="w-11 h-11" />
          {/if}
          <button
            onclick={(e) => { e.stopPropagation(); playItem(item); }}
            class="absolute inset-0 flex items-center justify-center bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer"
            title={i18n.t('playerBar.play')}
          >
            <Play class="w-4 h-4 text-white fill-current" />
          </button>
        </div>

        <div class="min-w-0 flex-1">
          <p class="truncate text-sm font-semibold text-brand-text-primary">{titleFor(item)}</p>
          <p class="truncate text-xs text-brand-text-secondary font-medium">{subtitleFor(item)}</p>
        </div>

        {#if item.type === "song"}
          <SongRating rating={item.song.rating} onRate={(r) => rateSong(item.song, r)} />
        {/if}

        <span class="shrink-0 max-w-24 text-right text-xs text-brand-text-secondary font-medium tabular-nums truncate">
          {trailingLabel(item)}
        </span>
      </div>
    {/each}

    {#if items.length === 0}
      <p class="text-sm text-brand-text-secondary px-3 py-6 text-center">{i18n.t('home.emptyState')}</p>
    {/if}
  </div>
</div>

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    onPlay={() => playerStore.playSong(song.id)}
    onGoToArtist={() => collectionStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => collectionStore.viewAlbum(song.album || "")}
    onClose={() => { contextMenuState = null; }}
  />
{/if}
