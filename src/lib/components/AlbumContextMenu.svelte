<script lang="ts">
  import { onMount } from "svelte";
  import { Play, Plus, User } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";

  let {
    x,
    y,
    albumName,
    artistName,
    onPlay,
    onAddToPlaylist,
    onGoToArtist,
    onClose,
  }: {
    x: number;
    y: number;
    albumName: string;
    artistName?: string;
    onPlay: () => void;
    onAddToPlaylist?: () => void;
    onGoToArtist?: () => void;
    onClose: () => void;
  } = $props();

  let menuEl = $state<HTMLDivElement | null>(null);

  let adjustedX = $derived.by(() => {
    if (typeof window === "undefined") return x;
    const menuWidth = 200;
    return Math.min(x, window.innerWidth - menuWidth - 10);
  });

  let adjustedY = $derived.by(() => {
    if (typeof window === "undefined") return y;
    const menuHeight = 180;
    // The floating PlayerBar dock (h-20 + bottom-4 inset ≈ 96px) sits on top
    // of page content once a track has ever played this session — clamp
    // above it so the menu's lower items aren't hidden underneath it.
    const dockClearance = playerStore.hasEverPlayed ? 96 : 0;
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
  bind:this={menuEl}
  style="left: {adjustedX}px; top: {adjustedY}px;"
  class="fixed z-50 w-52 bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary backdrop-blur-xl animate-in fade-in zoom-in-95 duration-100 select-none"
  role="menu"
  tabindex="-1"
>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {albumName || i18n.t("collection.unknownAlbum")}
  </div>

  <button
    onclick={() => { onPlay(); onClose(); }}
    class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-accent/15 hover:text-brand-accent-text transition-colors cursor-pointer"
    role="menuitem"
  >
    <Play class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
    <span>{i18n.t("playlists.contextMenuPlayAlbum")}</span>
  </button>

  {#if onAddToPlaylist}
    <button
      onclick={() => { onAddToPlaylist?.(); onClose(); }}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
      role="menuitem"
    >
      <Plus class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
      <span>{i18n.t("playlists.contextMenuAddToPlaylist")}</span>
    </button>
  {/if}

  {#if onGoToArtist && artistName}
    <div class="my-1 border-t border-brand-border/40"></div>
    <button
      onclick={() => { onGoToArtist?.(); onClose(); }}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
      role="menuitem"
    >
      <User class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
      <span>{i18n.t("playlists.contextMenuGoArtist")}</span>
    </button>
  {/if}
</div>
