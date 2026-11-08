<script lang="ts">
  import {
    ListPlusIcon as ListPlus,
    CaretRightIcon as CaretRight,
    RadioIcon as Radio,
    PlaylistIcon,
    PlusIcon as Plus,
    XIcon as X,
  } from "phosphor-svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { VIEWPORT_EDGE_PADDING_PX, PLAYER_DOCK_CLEARANCE_PX } from "../constants";
  import type { Playlist } from "../types";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  interface Props {
    onSelectPlaylist: (playlist: Playlist) => Promise<void> | void;
    onCreatePlaylist: (name: string) => Promise<void> | void;
    onClose: () => void;
  }

  let { onSelectPlaylist, onCreatePlaylist, onClose }: Props = $props();

  const SUBMENU_MAX_HEIGHT_PX = 240;
  let isSubmenuOpen = $state(false);
  let submenuCloseTimer: ReturnType<typeof setTimeout> | null = null;
  let submenuTriggerEl = $state<HTMLDivElement | null>(null);
  let opensLeft = $state(false);
  let opensUp = $state(false);

  let showNewPlaylistModal = $state(false);
  let newPlaylistName = $state("");

  const activeCustom = $derived(playlistsStore.activeCustomPlaylist);
  const otherCustomPlaylists = $derived(
    activeCustom
      ? playlistsStore.customPlaylists.filter((p) => p.id !== activeCustom.id)
      : playlistsStore.customPlaylists
  );

  function openSubmenu() {
    if (submenuCloseTimer) {
      clearTimeout(submenuCloseTimer);
      submenuCloseTimer = null;
    }
    if (submenuTriggerEl && typeof window !== "undefined") {
      const rect = submenuTriggerEl.getBoundingClientRect();
      opensLeft = rect.right + 208 + VIEWPORT_EDGE_PADDING_PX > window.innerWidth;
      const dockClearance = playerStore.currentSong ? PLAYER_DOCK_CLEARANCE_PX : 0;
      opensUp = rect.top + SUBMENU_MAX_HEIGHT_PX > window.innerHeight - dockClearance - VIEWPORT_EDGE_PADDING_PX;
    }
    isSubmenuOpen = true;
  }

  function scheduleCloseSubmenu() {
    if (submenuCloseTimer) clearTimeout(submenuCloseTimer);
    submenuCloseTimer = setTimeout(() => {
      isSubmenuOpen = false;
    }, 150);
  }

  function closeSubmenuImmediately() {
    if (submenuCloseTimer) {
      clearTimeout(submenuCloseTimer);
      submenuCloseTimer = null;
    }
    isSubmenuOpen = false;
  }

  async function handleSelect(playlist: Playlist) {
    closeSubmenuImmediately();
    await onSelectPlaylist(playlist);
    onClose();
  }

  function handleOpenNewPlaylistModal() {
    closeSubmenuImmediately();
    newPlaylistName = "";
    showNewPlaylistModal = true;
  }

  async function handleConfirmCreatePlaylist() {
    const trimmed = newPlaylistName.trim();
    showNewPlaylistModal = false;
    await onCreatePlaylist(trimmed || i18n.t("playlists.untitledPlaylistName", {}, "Untitled Playlist"));
    onClose();
  }

  function autofocus(node: HTMLInputElement) {
    setTimeout(() => node.focus(), 50);
  }
</script>

<div
  bind:this={submenuTriggerEl}
  class="relative"
  role="none"
  onmouseenter={openSubmenu}
  onmouseleave={scheduleCloseSubmenu}
>
  <button
    type="button"
    class="w-full text-left px-3 py-1.5 flex items-center gap-2.5 transition-colors hover:bg-brand-main hover:text-brand-text-primary {isSubmenuOpen ? 'bg-brand-main text-brand-text-primary' : 'text-brand-text-primary'}"
    role="menuitem"
    aria-haspopup="menu"
    aria-expanded={isSubmenuOpen}
    onclick={() => {
      if (isSubmenuOpen) closeSubmenuImmediately();
      else openSubmenu();
    }}
    onkeydown={(e) => {
      if (e.key === "ArrowRight" || e.key === "Enter") {
        e.preventDefault();
        openSubmenu();
      }
    }}
  >
    <ListPlus class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary" />
    <span class="flex-1">{i18n.t("playlists.contextMenuAddToPlaylistGeneric", {}, "Add to Playlist")}</span>
    <CaretRight class="w-3 h-3 shrink-0 text-brand-text-secondary/70 {isSubmenuOpen ? 'text-brand-text-primary' : ''}" />
  </button>

  {#if isSubmenuOpen}
    <div
      class="absolute z-50 w-52 max-h-60 overflow-y-auto bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary select-none {opensLeft ? 'right-[calc(100%-2px)]' : 'left-[calc(100%-2px)]'} {opensUp ? 'bottom-0' : 'top-0'}"
      role="menu"
      tabindex="-1"
      onmouseenter={openSubmenu}
      onmouseleave={scheduleCloseSubmenu}
      onkeydown={(e) => {
        if (e.key === "ArrowLeft" || e.key === "Escape") {
          e.preventDefault();
          closeSubmenuImmediately();
        }
      }}
    >
      {#if playlistsStore.customPlaylists.length === 0}
        <div class="px-3 py-1.5 text-brand-text-secondary/60 italic select-none">
          {i18n.t("playlists.contextMenuNoCustomPlaylists", {}, "No custom playlists")}
        </div>
      {:else}
        {#if activeCustom}
          <button
            type="button"
            onclick={() => handleSelect(activeCustom)}
            class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-main hover:text-brand-text-primary group"
            role="menuitem"
          >
            <Radio class="w-3.5 h-3.5 shrink-0 text-brand-accent-text" />
            <span class="truncate flex-1 font-medium text-brand-text-primary" title={activeCustom.name}>{activeCustom.name}</span>
            <span class="text-[9px] text-brand-accent-text font-semibold uppercase px-1 py-0.5 rounded bg-brand-accent/15 shrink-0">
              {i18n.t("playlists.activeBadgeLabel", {}, "Active")}
            </span>
          </button>
          {#if otherCustomPlaylists.length > 0}
            <ContextMenuDivider />
          {/if}
        {/if}

        {#each (activeCustom ? otherCustomPlaylists : playlistsStore.customPlaylists) as pl (pl.id)}
          <button
            type="button"
            onclick={() => handleSelect(pl)}
            class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-main hover:text-brand-text-primary"
            role="menuitem"
          >
            <PlaylistIcon class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary" />
            <span class="truncate flex-1" title={pl.name}>{pl.name}</span>
          </button>
        {/each}
      {/if}

      <ContextMenuDivider />

      <button
        type="button"
        onclick={handleOpenNewPlaylistModal}
        class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-accent/15 hover:text-brand-accent-text text-brand-accent-text font-medium"
        role="menuitem"
      >
        <Plus class="w-3.5 h-3.5 shrink-0" />
        <span>{i18n.t("playlists.contextMenuNewPlaylist", {}, "New Playlist...")}</span>
      </button>
    </div>
  {/if}
</div>

{#if showNewPlaylistModal}
  <Modal onClose={() => { showNewPlaylistModal = false; onClose(); }} maxWidth="max-w-sm">
    <div class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main">
      <div class="flex items-center gap-2">
        <ListPlus class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">{i18n.t("playlists.newPlaylistModalTitle", {}, "New Playlist")}</h3>
      </div>
      <button
        type="button"
        onclick={() => { showNewPlaylistModal = false; onClose(); }}
        class="text-brand-text-secondary hover:text-brand-text-primary transition-colors"
      >
        <X class="w-4 h-4" />
      </button>
    </div>

    <div class="p-6 flex flex-col gap-4">
      <label class="text-xs text-brand-text-secondary font-medium" for="new-playlist-submenu-input">
        {i18n.t("playlists.newPlaylistPrompt", {}, "Enter a name for the new playlist:")}
      </label>
      <input
        id="new-playlist-submenu-input"
        type="text"
        use:autofocus
        bind:value={newPlaylistName}
        placeholder={i18n.t("playlists.untitledPlaylistName", {}, "Untitled Playlist")}
        onkeydown={(e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            handleConfirmCreatePlaylist();
          } else if (e.key === "Escape") {
            e.preventDefault();
            showNewPlaylistModal = false;
            onClose();
          }
        }}
        class="w-full bg-brand-sidebar border border-brand-border rounded-lg px-3 py-2 text-sm text-brand-text-primary focus:outline-none focus:border-brand-accent transition-colors"
      />
      <div class="flex justify-end gap-2 mt-2">
        <Button variant="secondary" onclick={() => { showNewPlaylistModal = false; onClose(); }}>
          {i18n.t("common.cancel", {}, "Cancel")}
        </Button>
        <Button variant="primary" onclick={handleConfirmCreatePlaylist}>
          {i18n.t("common.create")}
        </Button>
      </div>
    </div>
  </Modal>
{/if}
