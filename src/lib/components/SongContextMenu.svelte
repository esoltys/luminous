<script lang="ts">
  import { isRemoteSource } from "../utils/remoteSource";
  import {
    PlayIcon as Play,
    PlusIcon as Plus,
    ListPlusIcon as ListPlus,
    MicrophoneStageIcon as Mic2,
    DiscIcon as DiscAlbum,
    PencilSimpleIcon as Edit3,
    FolderIcon as Folder,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    ArrowSquareOutIcon as OpenInPicard,
    EyeSlashIcon as EyeSlash,
    EyeIcon as Eye,
    ChartBarIcon as BarChart2,
    ShareNetworkIcon as Share,
    CaretRightIcon as CaretRight,
    RadioIcon as Radio,
    PlaylistIcon as PlaylistIcon,
    XIcon as X,
    HeartIcon,
    HeartBreakIcon
  } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { VIEWPORT_EDGE_PADDING_PX, PLAYER_DOCK_CLEARANCE_PX } from "../constants";
  import type { Song, Playlist } from "../types";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import ShareModal from "./ShareModal.svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";

  let {
    x,
    y,
    song,
    selectedCount = 1,
    selectedSongIds,
    selectedSongs,
    onPlay,
    onAddToQueue,
    onAddToPlaylist,
    onGoToArtist,
    onGoToAlbum,
    onEditTags,
    onOpenInPicard,
    onOrganizeFiles,
    onClose,
  }: {
    x: number;
    y: number;
    song: Song;
    selectedCount?: number;
    selectedSongIds?: number[];
    selectedSongs?: Song[];
    onPlay: () => void;
    onAddToQueue?: () => void;
    onAddToPlaylist?: ((playlistId: number) => void) | (() => void);
    onGoToArtist?: () => void;
    onGoToAlbum?: () => void;
    onEditTags?: () => void;
    onOpenInPicard?: () => void;
    onOrganizeFiles?: () => void;
    onClose: () => void;
  } = $props();

  async function handleDefaultAddToQueue() {
    const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
    await playlistsStore.addSongsToQueue(ids);
    const name = ids.length > 1 ? `${ids.length} songs` : (song.title || i18n.t("collection.unknownSong"));
    toastStore.show(i18n.t("playlists.addedToQueueSuccess", { name }, `Added ${name} to Queue`));
  }

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
      opensUp = rect.top + 200 > window.innerHeight - dockClearance - VIEWPORT_EDGE_PADDING_PX;
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

  async function handleAddToCustomPlaylist(targetPlaylist: Playlist) {
    const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
    await playlistsStore.addSongsToPlaylist(targetPlaylist.id, ids);
    toastStore.show(
      i18n.t("playlists.addedToPlaylistSuccess", { name: targetPlaylist.name }, `Added to ${targetPlaylist.name}`)
    );
    if (onAddToPlaylist && onAddToPlaylist.length > 0) {
      (onAddToPlaylist as (id: number) => void)(targetPlaylist.id);
    }
    onClose();
  }

  function handleOpenNewPlaylistModal() {
    newPlaylistName = "";
    menuVisible = false;
    showNewPlaylistModal = true;
  }

  async function handleConfirmCreatePlaylist() {
    const trimmed = newPlaylistName.trim();
    const finalName = trimmed || i18n.t("playlists.untitledPlaylistName", {}, "Untitled Playlist");
    try {
      const created = await playlistsStore.createPlaylist(finalName);
      if (created) {
        const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
        await playlistsStore.addSongsToPlaylist(created.id, ids);
        toastStore.show(
          i18n.t("playlists.addedToPlaylistSuccess", { name: created.name }, `Added to ${created.name}`)
        );
      }
    } catch (err) {
      console.error("Failed to create playlist from song menu:", err);
    }
    showNewPlaylistModal = false;
    onClose();
  }

  function autofocus(node: HTMLInputElement) {
    setTimeout(() => node.focus(), 50);
  }

  async function handleToggleNotIncluded() {
    const ids = selectedSongIds && selectedSongIds.length > 0 ? selectedSongIds : [song.id];
    const notIncluded = !song.not_included;
    await invoke("set_songs_not_included", { songIds: ids, notIncluded });
    const name = ids.length > 1 ? `${ids.length} songs` : (song.title || i18n.t("collection.unknownSong"));
    const message = notIncluded
      ? i18n.t("playlists.markedNotIncluded", { name })
      : i18n.t("playlists.unmarkedNotIncluded", { name });
    toastStore.show(message);
  }

  const picardSelection = $derived(selectedSongs && selectedSongs.length > 0 ? selectedSongs : [song]);
  const allSelectedRemote = $derived(picardSelection.every(isRemoteSource));

  let showShareModal = $state(false);
  // See AlbumContextMenu.svelte for why the menu must be hidden (not left
  // mounted) the instant Share is clicked: ContextMenu's outside-click
  // listener would otherwise treat any click inside the portalled
  // ShareModal as "outside this menu" and tear the whole thing down.
  let menuVisible = $state(true);

  async function handleToggleStatsExcluded() {
    const reason = statsExclusionsStore.getSongExclusionReason(song);
    if (reason === "album" && song.album) {
      await statsExclusionsStore.toggleWithToast("album", song.album);
    } else if (reason === "artist") {
      const artist = song.album_artist || song.artist;
      if (artist) {
        await statsExclusionsStore.toggleWithToast("artist", artist);
      }
    } else {
      await statsExclusionsStore.toggleWithToast(
        "song",
        String(song.id),
        song.title || i18n.t("collection.unknownSong")
      );
    }
  }

  async function handleToggleFavourite() {
    const isFav = song.loved === 1 || (song.loved === undefined && song.rating === 5);
    const newLoved = isFav ? 0 : 1;
    try {
      song.loved = await invoke<number>("set_song_loved", { songId: song.id, loved: newLoved });
    } catch (e) {
      console.error("Failed to update favourite:", e);
    }
  }

  async function handleToggleDislike() {
    const newLoved = song.loved === -1 ? 0 : -1;
    try {
      song.loved = await invoke<number>("set_song_loved", { songId: song.id, loved: newLoved });
    } catch (e) {
      console.error("Failed to update dislike:", e);
    }
  }
</script>

{#if menuVisible}
<ContextMenu {x} {y} {onClose} estimatedHeight={560}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {#if selectedCount > 1}
      {i18n.t("playlists.selectedCount", { count: selectedCount })}
    {:else}
      {song.title || i18n.t("collection.unknownSong")}
    {/if}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={selectedCount > 1
      ? i18n.t("playlists.contextMenuPlay")
      : i18n.t("playlists.contextMenuPlaySong")}
    onmouseenter={closeSubmenuImmediately}
    onclick={() => { onPlay(); onClose(); }}
  />

  <ContextMenuItem
    icon={Layers}
    label={i18n.t("playlists.contextMenuAddQueue", {}, "Add to Queue")}
    onmouseenter={closeSubmenuImmediately}
    onclick={async () => {
      if (onAddToQueue) {
        onAddToQueue();
      } else {
        await handleDefaultAddToQueue();
      }
      onClose();
    }}
  />

  <!-- Add to Playlist submenu trigger & flyout -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    bind:this={submenuTriggerEl}
    class="relative"
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
        if (isSubmenuOpen) {
          closeSubmenuImmediately();
        } else {
          openSubmenu();
        }
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
        class="absolute z-50 w-52 max-h-60 overflow-y-auto bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary backdrop-blur-xl select-none {opensLeft ? 'right-[calc(100%-2px)]' : 'left-[calc(100%-2px)]'} {opensUp ? 'bottom-0' : 'top-0'}"
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
              onclick={() => handleAddToCustomPlaylist(activeCustom)}
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
              onclick={() => handleAddToCustomPlaylist(pl)}
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

  <ContextMenuItem
    icon={song.not_included ? Eye : EyeSlash}
    label={song.not_included
      ? i18n.t("playlists.contextMenuIncludeInPlaylists")
      : i18n.t("playlists.contextMenuMarkNotIncluded")}
    onmouseenter={closeSubmenuImmediately}
    onclick={() => { handleToggleNotIncluded(); onClose(); }}
  />

  {#if selectedCount === 1}
    <ContextMenuItem
      icon={HeartIcon}
      label={(song.loved === 1 || (song.loved === undefined && song.rating === 5))
        ? i18n.t("rating.unfavoriteTooltip")
        : i18n.t("rating.favoriteTooltip")}
      onmouseenter={closeSubmenuImmediately}
      onclick={() => { handleToggleFavourite(); onClose(); }}
    />

    <ContextMenuItem
      icon={HeartBreakIcon}
      label={song.loved === -1
        ? i18n.t("rating.clearHateTooltip")
        : i18n.t("rating.hateAction")}
      onmouseenter={closeSubmenuImmediately}
      onclick={() => { handleToggleDislike(); onClose(); }}
    />

    <ContextMenuItem
      icon={BarChart2}
      label={statsExclusionsStore.isSongExcluded(song)
        ? i18n.t("stats.includeInStats")
        : i18n.t("stats.excludeFromStats")}
      onmouseenter={closeSubmenuImmediately}
      onclick={() => { handleToggleStatsExcluded(); onClose(); }}
    />

    <ContextMenuDivider />

    {#if onGoToArtist && song.artist}
      <ContextMenuItem
        icon={Mic2}
        label={i18n.t("playlists.contextMenuGoArtist")}
        onmouseenter={closeSubmenuImmediately}
        onclick={() => { onGoToArtist?.(); onClose(); }}
      />
    {/if}

    {#if onGoToAlbum && song.album}
      <ContextMenuItem
        icon={DiscAlbum}
        label={i18n.t("playlists.contextMenuGoAlbum")}
        onmouseenter={closeSubmenuImmediately}
        onclick={() => { onGoToAlbum?.(); onClose(); }}
      />
    {/if}

    {#if song.artist}
      <ContextMenuItem
        icon={Share}
        label={i18n.t("shareModal.menuItem")}
        onmouseenter={closeSubmenuImmediately}
        onclick={() => { menuVisible = false; showShareModal = true; }}
      />
    {/if}

    <ContextMenuItem
      icon={pinnedStore.isPinned("song", String(song.id)) ? PinOff : Pin}
      label={pinnedStore.isPinned("song", String(song.id))
        ? i18n.t("playlists.contextMenuUnpinHome")
        : i18n.t("playlists.contextMenuPinHome")}
      onmouseenter={closeSubmenuImmediately}
      onclick={() => { pinnedStore.toggle("song", String(song.id)); onClose(); }}
    />
  {/if}

  <ContextMenuDivider />

  {#if onOrganizeFiles}
    <ContextMenuItem
      icon={Folder}
      label={i18n.t("organizer.title")}
      onmouseenter={closeSubmenuImmediately}
      onclick={() => { onOrganizeFiles?.(); onClose(); }}
    />
  {/if}

  {#if selectedCount === 1 && onEditTags}
    <ContextMenuItem
      icon={Edit3}
      label={i18n.t("collection.editTagsTooltip")}
      onmouseenter={closeSubmenuImmediately}
      onclick={() => { onEditTags?.(); onClose(); }}
    />
  {/if}

  {#if onOpenInPicard}
    <ContextMenuItem
      icon={OpenInPicard}
      label={i18n.t("picard.openInPicard")}
      onmouseenter={closeSubmenuImmediately}
      onclick={() => { onOpenInPicard?.(); onClose(); }}
      disabled={!picardStore.available || allSelectedRemote}
      title={!picardStore.available
        ? i18n.t("picard.notFoundTooltip")
        : allSelectedRemote
          ? i18n.t("picard.remoteNotSupportedTooltip")
          : undefined}
    />
  {/if}
</ContextMenu>
{/if}

{#if showShareModal && song.artist}
  <ShareModal entity={{ kind: "artist", artistName: song.artist }} onClose={() => { showShareModal = false; onClose(); }} />
{/if}

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
      <label class="text-xs text-brand-text-secondary font-medium" for="new-playlist-name-input">
        {i18n.t("playlists.newPlaylistPrompt", {}, "Enter a name for the new playlist:")}
      </label>
      <input
        id="new-playlist-name-input"
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
