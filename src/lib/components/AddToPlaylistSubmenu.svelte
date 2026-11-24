<script lang="ts">
  import {
    ListPlusIcon as ListPlus,
    RadioIcon as Radio,
    PlaylistIcon,
    XIcon as X,
  } from "phosphor-svelte";
  import Plus from "./PlusIcon.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import type { Playlist } from "../types";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import ContextMenuSubmenu from "./ContextMenuSubmenu.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  interface Props {
    onSelectPlaylist: (playlist: Playlist) => Promise<void> | void;
    onCreatePlaylist: (name: string) => Promise<void> | void;
    onClose: () => void;
  }

  let { onSelectPlaylist, onCreatePlaylist, onClose }: Props = $props();

  let showNewPlaylistModal = $state(false);
  let newPlaylistName = $state("");

  const activeCustom = $derived(playlistsStore.activeCustomPlaylist);
  const otherCustomPlaylists = $derived(
    activeCustom
      ? playlistsStore.customPlaylists.filter((p) => p.id !== activeCustom.id)
      : playlistsStore.customPlaylists,
  );

  async function handleSelect(playlist: Playlist, closeSubmenu: () => void) {
    closeSubmenu();
    await onSelectPlaylist(playlist);
    onClose();
  }

  function handleOpenNewPlaylistModal(closeSubmenu: () => void) {
    closeSubmenu();
    newPlaylistName = "";
    showNewPlaylistModal = true;
  }

  async function handleConfirmCreatePlaylist() {
    const trimmed = newPlaylistName.trim();
    showNewPlaylistModal = false;
    await onCreatePlaylist(
      trimmed || i18n.t("playlists.untitledPlaylistName", {}, "Untitled Playlist"),
    );
    onClose();
  }

  function autofocus(node: HTMLInputElement) {
    setTimeout(() => node.focus(), 50);
  }
</script>

<ContextMenuSubmenu
  icon={ListPlus}
  label={i18n.t("playlists.contextMenuAddToPlaylistGeneric", {}, "Add to Playlist")}
>
  {#snippet children(close)}
    {#if playlistsStore.customPlaylists.length === 0}
      <div class="px-3 py-1.5 text-brand-text-secondary italic select-none">
        {i18n.t("playlists.contextMenuNoCustomPlaylists", {}, "No custom playlists")}
      </div>
    {:else}
      {#if activeCustom}
        <button
          type="button"
          onclick={() => handleSelect(activeCustom, close)}
          class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-main hover:text-brand-text-primary group"
          role="menuitem"
        >
          <Radio class="w-3.5 h-3.5 shrink-0 text-brand-accent-text" />
          <span
            class="truncate flex-1 font-medium text-brand-text-primary"
            title={activeCustom.name}>{activeCustom.name}</span
          >
          <span
            class="label-small text-brand-accent-text px-1 py-0.5 rounded bg-brand-accent/15 shrink-0"
          >
            {i18n.t("playlists.activeBadgeLabel", {}, "Active")}
          </span>
        </button>
        {#if otherCustomPlaylists.length > 0}
          <ContextMenuDivider />
        {/if}
      {/if}

      {#each activeCustom ? otherCustomPlaylists : playlistsStore.customPlaylists as pl (pl.id)}
        <button
          type="button"
          onclick={() => handleSelect(pl, close)}
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
      onclick={() => handleOpenNewPlaylistModal(close)}
      class="w-full text-left px-3 py-1.5 flex items-center gap-2 transition-colors hover:bg-brand-accent/15 hover:text-brand-accent-text text-brand-accent-text font-medium"
      role="menuitem"
    >
      <Plus class="w-3.5 h-3.5 shrink-0" />
      <span>{i18n.t("playlists.contextMenuNewPlaylist", {}, "New Playlist...")}</span>
    </button>
  {/snippet}
</ContextMenuSubmenu>

{#if showNewPlaylistModal}
  <Modal
    onClose={() => {
      showNewPlaylistModal = false;
      onClose();
    }}
    maxWidth="max-w-sm"
  >
    <div
      class="h-14 flex items-center justify-between px-6 border-b border-brand-border shrink-0 bg-brand-main"
    >
      <div class="flex items-center gap-2">
        <ListPlus class="w-4 h-4 text-brand-accent-text" />
        <h3 class="text-sm font-bold">
          {i18n.t("playlists.newPlaylistModalTitle", {}, "New Playlist")}
        </h3>
      </div>
      <button
        type="button"
        onclick={() => {
          showNewPlaylistModal = false;
          onClose();
        }}
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
        <Button
          variant="secondary"
          onclick={() => {
            showNewPlaylistModal = false;
            onClose();
          }}
        >
          {i18n.t("common.cancel", {}, "Cancel")}
        </Button>
        <Button variant="primary" onclick={handleConfirmCreatePlaylist}>
          {i18n.t("common.create")}
        </Button>
      </div>
    </div>
  </Modal>
{/if}
