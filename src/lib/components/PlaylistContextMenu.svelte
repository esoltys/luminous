<script lang="ts">
  import {
    PlayIcon as Play,
    TrashIcon as Trash2,
    MicrophoneStageIcon as Mic2,
    DiscIcon as DiscAlbum,
    PencilSimpleIcon as Edit3,
    ArrowSquareOutIcon as OpenInPicard,
    FolderIcon as Folder
  } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import type { FileNavigationTarget } from "../types";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuDivider from "./ContextMenuDivider.svelte";

  let {
    x,
    y,
    selectedCount,
    songId,
    onPlay,
    onRemove,
    onGoToArtist,
    onGoToAlbum,
    onEditTags,
    onOpenInPicard,
    allSelectedRemote = false,
    onClose,
  }: {
    x: number;
    y: number;
    selectedCount: number;
    /** The library song of the entry right-clicked, when it has one. */
    songId?: number;
    onPlay: () => void;
    onRemove: () => void;
    onGoToArtist?: () => void;
    onGoToAlbum?: () => void;
    onEditTags?: () => void;
    onOpenInPicard?: () => void;
    allSelectedRemote?: boolean;
    onClose: () => void;
  } = $props();

  /** Where the entry's song is in the Files view; null until known, and when
   * it has no place there, which hides View in Files. */
  let filesTarget = $state<FileNavigationTarget | null>(null);
  $effect(() => {
    if (selectedCount !== 1 || songId === undefined) return;
    let stale = false;
    invoke<FileNavigationTarget | null>("locate_song_in_files", { songId })
      .then((target) => { if (!stale) filesTarget = target ?? null; })
      .catch((err) => console.error("Failed to find the song in Files:", err));
    return () => { stale = true; };
  });
</script>

<ContextMenu {x} {y} {onClose}>
  <div class="px-3 py-1 text-[11px] font-medium text-brand-text-secondary/60 border-b border-brand-border/40 mb-1">
    {i18n.t("playlists.selectedCount", { count: selectedCount })}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlay")}
    onclick={() => { onPlay(); onClose(); }}
  />

  {#if selectedCount === 1}
    <ContextMenuDivider />

    {#if onGoToArtist}
      <ContextMenuItem
        icon={Mic2}
        label={i18n.t("playlists.contextMenuGoArtist")}
        onclick={() => { onGoToArtist?.(); onClose(); }}
      />
    {/if}

    {#if onGoToAlbum}
      <ContextMenuItem
        icon={DiscAlbum}
        label={i18n.t("playlists.contextMenuGoAlbum")}
        onclick={() => { onGoToAlbum?.(); onClose(); }}
      />
    {/if}

    {#if filesTarget}
      {@const target = filesTarget}
      <ContextMenuItem
        icon={Folder}
        label={i18n.t("files.viewInFiles")}
        onclick={() => { navigationStore.openInFiles(target); onClose(); }}
      />
    {/if}

    {#if onEditTags}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={Edit3}
        label={i18n.t("collection.editTagsTooltip")}
        onclick={() => { onEditTags?.(); onClose(); }}
      />
    {/if}
  {/if}

  {#if onOpenInPicard}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={OpenInPicard}
      label={i18n.t("picard.openInPicard")}
      onclick={() => { onOpenInPicard?.(); onClose(); }}
      disabled={!picardStore.available || allSelectedRemote}
      title={!picardStore.available
        ? i18n.t("picard.notFoundTooltip")
        : allSelectedRemote
          ? i18n.t("picard.remoteNotSupportedTooltip")
          : undefined}
    />
  {/if}

  <ContextMenuDivider />

  <ContextMenuItem
    icon={Trash2}
    destructive
    label={selectedCount > 1 ? i18n.t("playlists.removeSelected") : i18n.t("playlists.contextMenuRemove")}
    onclick={() => { onRemove(); onClose(); }}
  />
</ContextMenu>
