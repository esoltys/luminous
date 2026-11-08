<script lang="ts">
  import { CaretRightIcon as CaretRight, DeviceMobileSpeakerIcon } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { filesStore } from "../stores/files.svelte";
  import { devicesStore } from "../stores/devices.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { deviceName, syncStorage } from "../utils/devices";
  import { formatStorageSize } from "../utils/formatters";
  import { VIEWPORT_EDGE_PADDING_PX, PLAYER_DOCK_CLEARANCE_PX } from "../constants";
  import type { SyncSource, ConnectedDevice } from "../types";
  import ContextMenuItem from "./ContextMenuItem.svelte";

  interface Props {
    source?: SyncSource;
    sourceName?: string;
    songIds?: number[];
    onClose: () => void;
  }

  let { source, sourceName, songIds, onClose }: Props = $props();

  const devices = $derived(filesStore.devices);

  const SUBMENU_MAX_HEIGHT_PX = 240;
  let isSubmenuOpen = $state(false);
  let submenuCloseTimer: ReturnType<typeof setTimeout> | null = null;
  let submenuTriggerEl = $state<HTMLDivElement | null>(null);
  let opensLeft = $state(false);
  let opensUp = $state(false);

  function openSubmenu() {
    if (submenuCloseTimer) {
      clearTimeout(submenuCloseTimer);
      submenuCloseTimer = null;
    }
    if (submenuTriggerEl && typeof window !== "undefined") {
      const rect = submenuTriggerEl.getBoundingClientRect();
      opensLeft = rect.right + 224 + VIEWPORT_EDGE_PADDING_PX > window.innerWidth;
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

  async function handleAddToDevice(device: ConnectedDevice) {
    closeSubmenuImmediately();
    if (songIds && songIds.length > 0) {
      await devicesStore.addSongsToSync(device, songIds);
    } else if (source) {
      await devicesStore.addToDeviceSync(device, source, sourceName ?? "");
    }
    onClose();
  }
</script>

{#if devices.length === 1}
  {@const dev = devices[0]}
  <ContextMenuItem
    icon={DeviceMobileSpeakerIcon}
    label={i18n.t("devices.addToDevice", { device: deviceName(dev) })}
    onclick={() => handleAddToDevice(dev)}
  />
{:else if devices.length > 1}
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
      <DeviceMobileSpeakerIcon class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary" />
      <span class="flex-1">{i18n.t("devices.addToPortablePlayer")}</span>
      <CaretRight class="w-3 h-3 shrink-0 text-brand-text-secondary/70 {isSubmenuOpen ? 'text-brand-text-primary' : ''}" />
    </button>

    {#if isSubmenuOpen}
      <div
        class="absolute z-50 w-56 max-h-60 overflow-y-auto bg-brand-sidebar border border-brand-border/80 rounded-xl shadow-2xl py-1.5 text-xs text-brand-text-primary select-none {opensLeft ? 'right-[calc(100%-2px)]' : 'left-[calc(100%-2px)]'} {opensUp ? 'bottom-0' : 'top-0'}"
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
        {#each devices as dev (dev.id)}
          {@const name = deviceName(dev)}
          {@const storage = syncStorage(dev, devicesStore.syncStates[dev.id]?.settings)}
          <button
            type="button"
            onclick={() => handleAddToDevice(dev)}
            class="w-full text-left px-3 py-1.5 flex items-center justify-between gap-2 transition-colors hover:bg-brand-main hover:text-brand-text-primary group"
            role="menuitem"
          >
            <span class="truncate flex-1 font-medium text-brand-text-primary" title={name}>{name}</span>
            {#if storage}
              <span class="text-[10px] text-brand-text-secondary/70 shrink-0">
                {formatStorageSize(storage.freeBytes)}
              </span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
  </div>
{/if}
