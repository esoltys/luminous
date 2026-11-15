<script lang="ts">
  import { DeviceMobileSpeakerIcon, CheckIcon } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { filesStore } from "../stores/files.svelte";
  import { devicesStore } from "../stores/devices.svelte";
  import { deviceName, syncStorage } from "../utils/devices";
  import { formatStorageSize } from "../utils/formatters";
  import type { SyncSource, ConnectedDevice } from "../types";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import ContextMenuSubmenu from "./ContextMenuSubmenu.svelte";

  interface Props {
    source?: SyncSource;
    sourceName?: string;
    songIds?: number[];
    onClose: () => void;
  }

  let { source, sourceName, songIds, onClose }: Props = $props();

  const devices = $derived(filesStore.devices);

  $effect(() => {
    for (const dev of devices) {
      if (!devicesStore.syncStates[dev.id]) {
        devicesStore.refreshSyncState(dev.id);
      }
    }
  });

  function isDeviceInSync(dev: ConnectedDevice): boolean {
    if (songIds && songIds.length > 0) {
      return devicesStore.areSongsInSync(dev.id, songIds);
    }
    if (source) {
      return devicesStore.isSourceInSync(dev.id, source);
    }
    return false;
  }

  const allInSync = $derived(devices.length > 0 && devices.every((d) => isDeviceInSync(d)));

  async function handleToggleDevice(device: ConnectedDevice, closeSubmenu?: () => void) {
    closeSubmenu?.();
    await devicesStore.toggleDeviceSync(device, source, sourceName, songIds);
    onClose();
  }
</script>

{#if devices.length === 1}
  {@const dev = devices[0]}
  {@const inSync = isDeviceInSync(dev)}
  <ContextMenuItem
    icon={DeviceMobileSpeakerIcon}
    label={inSync
      ? i18n.t("devices.removeFromDeviceName", { device: deviceName(dev) })
      : i18n.t("devices.addToDevice", { device: deviceName(dev) })}
    onclick={() => handleToggleDevice(dev)}
  />
{:else if devices.length > 1}
  <ContextMenuSubmenu
    icon={DeviceMobileSpeakerIcon}
    label={allInSync
      ? i18n.t("devices.removeFromPortablePlayer")
      : i18n.t("devices.addToPortablePlayer")}
    width={224}
  >
    {#snippet children(close)}
      {#each devices as dev (dev.id)}
        {@const name = deviceName(dev)}
        {@const storage = syncStorage(dev, devicesStore.syncStates[dev.id]?.settings)}
        {@const inSync = isDeviceInSync(dev)}
        <button
          type="button"
          onclick={() => handleToggleDevice(dev, close)}
          class="w-full text-left px-3 py-1.5 flex items-center justify-between gap-2 transition-colors hover:bg-brand-main hover:text-brand-text-primary group"
          role="menuitem"
        >
          <div class="flex items-center gap-2 truncate">
            {#if inSync}
              <CheckIcon class="w-3.5 h-3.5 shrink-0 text-brand-accent-text" />
            {:else}
              <DeviceMobileSpeakerIcon class="w-3.5 h-3.5 shrink-0 text-brand-text-secondary" />
            {/if}
            <span
              class="truncate font-medium {inSync
                ? 'text-brand-accent-text'
                : 'text-brand-text-primary'}"
              title={name}>{name}</span
            >
          </div>
          <div class="flex items-center gap-1.5 shrink-0">
            {#if inSync}
              <span class="text-[10px] text-brand-text-secondary">{i18n.t("devices.inSync")}</span>
            {:else if storage}
              <span class="text-[10px] text-brand-text-secondary/70 shrink-0">
                {formatStorageSize(storage.freeBytes)}
              </span>
            {/if}
          </div>
        </button>
      {/each}
    {/snippet}
  </ContextMenuSubmenu>
{/if}
