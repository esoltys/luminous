<script lang="ts">
  import { onMount } from "svelte";
  import { Play, Trash2, User, Disc, Edit3 } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  let {
    x,
    y,
    selectedCount,
    onPlay,
    onRemove,
    onGoToArtist,
    onGoToAlbum,
    onEditTags,
    onClose,
  }: {
    x: number;
    y: number;
    selectedCount: number;
    onPlay: () => void;
    onRemove: () => void;
    onGoToArtist?: () => void;
    onGoToAlbum?: () => void;
    onEditTags?: () => void;
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
    const menuHeight = 220;
    return Math.min(y, window.innerHeight - menuHeight - 10);
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
  <div class="px-3 py-1 text-[11px] font-medium text-brand-text-secondary/60 border-b border-brand-border/40 mb-1">
    {i18n.t("playlists.selectedCount", { count: selectedCount })}
  </div>

  <button
    onclick={() => { onPlay(); onClose(); }}
    class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-accent/15 hover:text-brand-accent-text transition-colors cursor-pointer"
    role="menuitem"
  >
    <Play class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
    <span>{i18n.t("playlists.contextMenuPlay")}</span>
  </button>

  {#if selectedCount === 1}
    <div class="my-1 border-t border-brand-border/40"></div>

    {#if onGoToArtist}
      <button
        onclick={() => { onGoToArtist?.(); onClose(); }}
        class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
        role="menuitem"
      >
        <User class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
        <span>{i18n.t("playlists.contextMenuGoArtist")}</span>
      </button>
    {/if}

    {#if onGoToAlbum}
      <button
        onclick={() => { onGoToAlbum?.(); onClose(); }}
        class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
        role="menuitem"
      >
        <Disc class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
        <span>{i18n.t("playlists.contextMenuGoAlbum")}</span>
      </button>
    {/if}

    {#if onEditTags}
      <div class="my-1 border-t border-brand-border/40"></div>
      <button
        onclick={() => { onEditTags?.(); onClose(); }}
        class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-brand-sidebar/80 hover:text-brand-text-primary transition-colors cursor-pointer"
        role="menuitem"
      >
        <Edit3 class="w-3.5 h-3.5 text-brand-text-secondary shrink-0" />
        <span>{i18n.t("collection.editTagsTooltip")}</span>
      </button>
    {/if}
  {/if}

  <div class="my-1 border-t border-brand-border/40"></div>

  <button
    onclick={() => { onRemove(); onClose(); }}
    class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 hover:bg-red-500/10 hover:text-red-400 text-red-400 transition-colors cursor-pointer"
    role="menuitem"
  >
    <Trash2 class="w-3.5 h-3.5 text-red-400 shrink-0" />
    <span>{selectedCount > 1 ? i18n.t("playlists.removeSelected") : i18n.t("playlists.contextMenuRemove")}</span>
  </button>
</div>
