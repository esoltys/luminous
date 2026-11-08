<script lang="ts">
  import {
    PlayIcon as Play,
    StackIcon as Layers,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    ArrowSquareOutIcon as OpenInPicard,
    ChartBarIcon as BarChart2,
    ShareNetworkIcon as Share,
    RadioIcon as Radio
  } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { pinnedStore } from "../stores/pinned.svelte";
  import { statsExclusionsStore } from "../stores/statsExclusions.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { openInPicard } from "../utils/picard";
  import { queueArtistAsPlaylist, addArtistToQueue, startArtistRadio } from "../utils/playlist";
  import type { Song, Playlist } from "../types";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";
  import ShareModal from "./ShareModal.svelte";
  import AddToDeviceMenuItem from "./AddToDeviceMenuItem.svelte";
  import AddToPlaylistSubmenu from "./AddToPlaylistSubmenu.svelte";

  interface Props {
    x: number;
    y: number;
    artistName: string;
    onPlay?: () => void;
    onAddToQueue?: () => void;
    onClose: () => void;
  }

  let {
    x,
    y,
    artistName,
    onPlay,
    onAddToQueue,
    onClose,
  }: Props = $props();

  let showShareModal = $state(false);
  // Same reasoning as AlbumContextMenu: hide the menu (and its outside-click
  // listener) when Share opens, but keep this component mounted so the
  // sibling ShareModal survives.
  let menuVisible = $state(true);

  async function handleOpenInPicard() {
    try {
      const songs = await invoke<Song[]>("get_songs_by_artist", { artist: artistName });
      if (songs.length > 0) {
        await openInPicard(songs.map((s) => s.id));
      }
    } catch (err) {
      console.error("Failed to open artist in Picard:", err);
    }
  }

  async function handleToggleStatsExcluded() {
    await statsExclusionsStore.toggleWithToast("artist", artistName);
  }

  async function fetchPlayableSongIds(): Promise<number[]> {
    const songs = await invoke<Song[]>("get_songs_by_artist", { artist: artistName });
    return songs.filter((s) => !s.not_included).map((s) => s.id);
  }

  async function handleAddToCustomPlaylist(targetPlaylist: Playlist) {
    try {
      const songIds = await fetchPlayableSongIds();
      if (songIds.length > 0) {
        await playlistsStore.addSongsToPlaylist(targetPlaylist.id, songIds);
      }
    } catch (err) {
      console.error("Failed to add artist songs to playlist:", err);
    }
  }

  async function handleCreateAndAddToPlaylist(name: string) {
    try {
      const songIds = await fetchPlayableSongIds();
      await playlistsStore.createPlaylistWithSongs(name, songIds);
    } catch (err) {
      console.error("Failed to create playlist and add artist songs:", err);
    }
  }
</script>

{#if menuVisible}
<ContextMenu {x} {y} {onClose}>
  <div class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate">
    {artistName || i18n.t("collection.unknownArtist")}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlayArtist", {}, "Play Artist")}
    onclick={async () => {
      if (onPlay) {
        onPlay();
      } else {
        await queueArtistAsPlaylist(artistName);
      }
      onClose();
    }}
  />

  <ContextMenuItem
    icon={Layers}
    label={i18n.t("playlists.contextMenuAddQueue", {}, "Add to Queue")}
    onclick={async () => {
      if (onAddToQueue) {
        onAddToQueue();
      } else {
        await addArtistToQueue(artistName);
      }
      onClose();
    }}
  />

  <ContextMenuItem
    icon={Radio}
    label={i18n.t("playlists.contextMenuStartArtistRadio", {}, "Start Artist Radio")}
    onclick={async () => {
      await startArtistRadio(artistName);
      onClose();
    }}
  />

  <AddToPlaylistSubmenu
    onSelectPlaylist={handleAddToCustomPlaylist}
    onCreatePlaylist={handleCreateAndAddToPlaylist}
    {onClose}
  />

  {#if artistName}
    <AddToDeviceMenuItem
      source={{ kind: "artist", name: artistName }}
      sourceName={artistName}
      {onClose}
    />
  {/if}

  {#if artistName}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={OpenInPicard}
      label={i18n.t("picard.openInPicard")}
      onclick={async () => { await handleOpenInPicard(); onClose(); }}
      disabled={!picardStore.available}
      title={picardStore.available ? undefined : i18n.t("picard.notFoundTooltip")}
    />
    <ContextMenuItem
      icon={Share}
      label={i18n.t("shareModal.menuItem")}
      onclick={() => { menuVisible = false; showShareModal = true; }}
    />
    <ContextMenuDivider />
    <ContextMenuItem
      icon={pinnedStore.isPinned("artist", artistName) ? PinOff : Pin}
      label={pinnedStore.isPinned("artist", artistName)
        ? i18n.t("playlists.contextMenuUnpinHome")
        : i18n.t("playlists.contextMenuPinHome")}
      onclick={() => {
        pinnedStore.toggle("artist", artistName);
        onClose();
      }}
    />
    <ContextMenuItem
      icon={BarChart2}
      label={statsExclusionsStore.isExcluded("artist", artistName)
        ? i18n.t("stats.includeInStats")
        : i18n.t("stats.excludeFromStats")}
      onclick={() => { handleToggleStatsExcluded(); onClose(); }}
    />
  {/if}
</ContextMenu>
{/if}

{#if showShareModal}
  <ShareModal entity={{ kind: "artist", artistName }} onClose={() => { showShareModal = false; onClose(); }} />
{/if}
