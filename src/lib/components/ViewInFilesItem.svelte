<script lang="ts">
  import { FolderIcon } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { i18n } from "../stores/i18n.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import type { FileNavigationTarget } from "../types";
  import ContextMenuItem from "./ContextMenuItem.svelte";

  /**
   * The View in Files menu item (#1883) for a library song or an album, by
   * title. It looks up its place in the Files view when shown and renders
   * nothing until that is known, or at all when there is none (a stream, a
   * file outside every watched folder).
   */
  let {
    songId,
    album,
    onmouseenter,
    onClose,
  }: {
    songId?: number;
    album?: string;
    onmouseenter?: () => void;
    onClose: () => void;
  } = $props();

  let target = $state<FileNavigationTarget | null>(null);

  $effect(() => {
    const lookup =
      songId !== undefined
        ? invoke<FileNavigationTarget | null>("locate_song_in_files", { songId })
        : album
          ? invoke<FileNavigationTarget | null>("locate_album_in_files", { album })
          : null;
    if (!lookup) return;
    let stale = false;
    lookup
      .then((found) => { if (!stale) target = found ?? null; })
      .catch((err) => console.error("Failed to find this in Files:", err));
    return () => { stale = true; };
  });
</script>

{#if target}
  {@const found = target}
  <ContextMenuItem
    icon={FolderIcon}
    label={i18n.t("files.viewInFiles")}
    {onmouseenter}
    onclick={() => { navigationStore.openInFiles(found); onClose(); }}
  />
{/if}
