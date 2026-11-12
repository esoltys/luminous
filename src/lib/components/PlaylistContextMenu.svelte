<script lang="ts">
  import {
    PlayIcon as Play,
    TrashIcon as Trash2,
    MicrophoneStageIcon as Mic2,
    DiscIcon as DiscAlbum,
    PencilSimpleIcon as Edit3,
    ArrowSquareOutIcon as OpenInPicard,
  } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ViewInFilesItem from "./ViewInFilesItem.svelte";
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
    onEditBatchTags,
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
    /** Edits the tags of every selected entry together. */
    onEditBatchTags?: () => void;
    onOpenInPicard?: () => void;
    allSelectedRemote?: boolean;
    onClose: () => void;
  } = $props();
</script>

<ContextMenu {x} {y} {onClose}>
  <div
    class="px-3 py-1 text-[11px] font-medium text-brand-text-secondary/60 border-b border-brand-border/40 mb-1"
  >
    {i18n.t("playlists.selectedCount", { count: selectedCount })}
  </div>

  <ContextMenuItem
    icon={Play}
    accent
    label={i18n.t("playlists.contextMenuPlay")}
    onclick={() => {
      onPlay();
      onClose();
    }}
  />

  {#if selectedCount === 1}
    <ContextMenuDivider />

    {#if onGoToArtist}
      <ContextMenuItem
        icon={Mic2}
        label={i18n.t("playlists.contextMenuGoArtist")}
        onclick={() => {
          onGoToArtist?.();
          onClose();
        }}
      />
    {/if}

    {#if onGoToAlbum}
      <ContextMenuItem
        icon={DiscAlbum}
        label={i18n.t("playlists.contextMenuGoAlbum")}
        onclick={() => {
          onGoToAlbum?.();
          onClose();
        }}
      />
    {/if}

    {#if songId !== undefined}
      <ViewInFilesItem {songId} {onClose} />
    {/if}

    {#if onEditTags}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={Edit3}
        label={i18n.t("collection.editTagsTooltip")}
        onclick={() => {
          onEditTags?.();
          onClose();
        }}
      />
    {/if}
  {:else if onEditBatchTags}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={Edit3}
      label={i18n.t("collection.editTagsTooltip")}
      onclick={() => {
        onEditBatchTags?.();
        onClose();
      }}
    />
  {/if}

  {#if onOpenInPicard}
    <ContextMenuDivider />
    <ContextMenuItem
      icon={OpenInPicard}
      label={i18n.t("picard.openInPicard")}
      onclick={() => {
        onOpenInPicard?.();
        onClose();
      }}
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
    label={selectedCount > 1
      ? i18n.t("playlists.removeSelected")
      : i18n.t("playlists.contextMenuRemove")}
    onclick={() => {
      onRemove();
      onClose();
    }}
  />
</ContextMenu>
