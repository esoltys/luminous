<script lang="ts">
  import {
    PlayIcon as Play,
    BookmarkSimpleIcon as Bookmark,
    PushPinIcon as Pin,
    PushPinSlashIcon as PinOff,
    PencilSimpleIcon as Pencil,
  } from "phosphor-svelte";
  import { onMount } from "svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { pinnedStore } from "../../stores/pinned.svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import type { RadioDirectoryStation } from "../../types";
  import ContextMenu from "../ContextMenu.svelte";
  import ContextMenuItem from "../ContextMenuItem.svelte";
  import ContextMenuDivider from "../ContextMenuDivider.svelte";

  interface Props {
    x: number;
    y: number;
    station: RadioDirectoryStation;
    onClose: () => void;
  }

  let { x, y, station, onClose }: Props = $props();

  const pinned = $derived(pinnedStore.isPinned("station", station.stationuuid));
  const saved = $derived(radioStore.isSaved(station.stationuuid));
  const isCustom = $derived(station.stationuuid.startsWith("custom-"));

  onMount(() => {
    void radioStore.ensureSavedLoaded();
  });
</script>

<ContextMenu {x} {y} {onClose}>
  <div
    class="px-3 py-1 text-[11px] font-bold text-brand-text-primary border-b border-brand-border/40 mb-1 truncate"
  >
    {station.name || i18n.t("radio.unnamedStation")}
  </div>

  {#if station.playable}
    <ContextMenuItem
      icon={Play}
      accent
      label={i18n.t("radio.playStationMenu")}
      onclick={() => {
        void radioStore.playStation(station);
        onClose();
      }}
    />
    <ContextMenuDivider />
  {/if}

  <ContextMenuItem
    icon={Bookmark}
    label={saved ? i18n.t("radio.unsaveStation") : i18n.t("radio.saveStation")}
    onclick={() => {
      void radioStore.toggleSaveByUuid(station.stationuuid);
      onClose();
    }}
  />
  <ContextMenuItem
    icon={pinned ? PinOff : Pin}
    label={pinned
      ? i18n.t("playlists.contextMenuUnpinHome")
      : i18n.t("playlists.contextMenuPinHome")}
    onclick={() => {
      void pinnedStore.toggle("station", station.stationuuid);
      onClose();
    }}
  />
  {#if isCustom}
    <ContextMenuItem
      icon={Pencil}
      label={i18n.t("radio.editStationMenu")}
      onclick={() => {
        radioStore.editingStation = station;
        onClose();
      }}
    />
  {/if}
</ContextMenu>
