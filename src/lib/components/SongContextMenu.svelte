<script lang="ts">
  import { onMount } from "svelte";
  import { Play, Plus, User, Disc, Edit3, Folder } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import type { Song } from "../types";
  import { portal } from "../utils/portal";

  let {
    x,
    y,
    song,
    selectedCount = 1,
    onPlay,
    onAddToPlaylist,
    onGoToArtist,
    onGoToAlbum,
    onEditTags,
    onOrganizeFiles,
    onClose,
  }: {
    x: number;
    y: number;
    song: Song;
    selectedCount?: number;
    onPlay: () => void;
    onAddToPlaylist?: () => void;
    onGoToArtist?: () => void;
    onGoToAlbum?: () => void;
    onEditTags?: () => void;
    onOrganizeFiles?: () => void;
    onClose: () => void;
  } = $props();

  let menuEl = $state<HTMLDivElement | null>(null);

  // Keep menu inside viewport boundaries
  let adjustedX = $derived.by(() => {
    if (typeof window === "undefined") return x;
    const menuWidth = 200;
    return Math.min(x, window.innerWidth - menuWidth - 10);
  });

  let adjustedY = $derived.by(() => {
    if (typeof window === "undefined") return y;
    const menuHeight = 250;
    const dockClearance = playerStore.currentSong ? 96 : 0;
    return Math.min(y, window.innerHeight - menuHeight - dockClearance - 10);
  });

  function handleWindowClick(e: MouseEvent) {
    if (menuEl && !menuEl.contains(e.target as Node)) {
      onClose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      onClose();
    }
  }

  onMount(() => {
    window.addEventListener("mousedown", handleWindowClick);
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("mousedown", handleWindowClick);
      window.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

<div
  use:portal
  bind:this={menuEl}
  style="left: {adjustedX}px; top: {adjustedY}px;"
  class="fixed z-50 w-52 bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary backdrop-blur-xl animate-in fade-in zoom-in-95 duration-100 select-none"
  role="menu"
  tabindex="-1"
>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {#if selectedCount > 1}
      {i18n.t("playlists.selectedCount", { count: selectedCount })}
    {:else}
      {song.title || i18n.t("collection.unknownSong")}
    {/if}
  </div>

  <button
    onclick={() => { onPlay(); onClose(); }}
    class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-accent/15 hover:text-brand-accent-text transition-colors cursor-pointer"
    role="menuitem"
  >
    <Play class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
    <span>
      {selectedCount > 1
        ? i18n.t("playlists.contextMenuPlay")
        : i18n.t("playlists.contextMenuPlaySong")}
    </span>
  </button>

  {#if onAddToPlaylist}
    <button
      onclick={() => { onAddToPlaylist?.(); onClose(); }}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
      role="menuitem"
    >
      <Plus class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
      <span>
        {playlistsStore.activeCustomPlaylist
          ? i18n.t("playlists.contextMenuAddToPlaylist", { name: playlistsStore.activeCustomPlaylist.name })
          : i18n.t("playlists.contextMenuAddToPlaylistDefault")}
      </span>
    </button>
  {/if}

  {#if selectedCount === 1}
    <div class="my-1 border-t border-brand-border/40"></div>

    {#if onGoToArtist && song.artist}
      <button
        onclick={() => { onGoToArtist?.(); onClose(); }}
        class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
        role="menuitem"
      >
        <User class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
        <span>{i18n.t("playlists.contextMenuGoArtist")}</span>
      </button>
    {/if}

    {#if onGoToAlbum && song.album}
      <button
        onclick={() => { onGoToAlbum?.(); onClose(); }}
        class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
        role="menuitem"
      >
        <Disc class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
        <span>{i18n.t("playlists.contextMenuGoAlbum")}</span>
      </button>
    {/if}
  {/if}

  <div class="my-1 border-t border-brand-border/40"></div>

  {#if onOrganizeFiles}
    <button
      onclick={() => { onOrganizeFiles?.(); onClose(); }}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
      role="menuitem"
    >
      <Folder class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
      <span>{i18n.t("organizer.title")}</span>
    </button>
  {/if}

  {#if selectedCount === 1 && onEditTags}
    <button
      onclick={() => { onEditTags?.(); onClose(); }}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
      role="menuitem"
    >
      <Edit3 class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
      <span>{i18n.t("collection.editTagsTooltip")}</span>
    </button>
  {/if}
</div>
