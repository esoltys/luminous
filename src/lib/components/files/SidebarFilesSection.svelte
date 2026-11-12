<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { EjectIcon, FoldersIcon, PlusIcon, StopIcon } from "phosphor-svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { filesStore, sameSource, sourceKey } from "../../stores/files.svelte";
  import { deviceIcon, sourceIcon } from "./sourceIcon";
  import { devicesStore } from "../../stores/devices.svelte";
  import { songDrag } from "../../stores/songDrag.svelte";
  import { deviceName, syncStorage } from "../../utils/devices";
  import { formatStorageSize } from "../../utils/formatters";
  import DeviceProgressRing from "../devices/DeviceProgressRing.svelte";
  import { collectionStore } from "../../stores/collection.svelte";
  import { i18n, formatNumber } from "../../stores/i18n.svelte";
  import type { ConnectedDevice, FileSource } from "../../types";

  let { collapsed, showSubItems }: { collapsed: boolean; showSubItems: boolean } = $props();

  const active = $derived(navigationStore.activeTab === "files");
  const atOverview = $derived(active && navigationStore.filesLocation === null);

  function isOpen(source: FileSource): boolean {
    const location = navigationStore.filesLocation;
    return active && location !== null && sameSource(location.source, source);
  }

  /** How far the device's running sync is, 0–1; null when none is running. */
  function syncFraction(id: string): number | null {
    if (!devicesStore.isSyncing(id)) return null;
    const p = devicesStore.progress[id];
    return p && p.songsTotal > 0 ? p.songsDone / p.songsTotal : 0;
  }

  /** The storage the device syncs to: how full it is (0–1) and its free space, as shown. */
  function syncSpace(device: ConnectedDevice): { used: number; free: string } {
    const storage = syncStorage(device, devicesStore.syncStates[device.id]?.settings);
    if (!storage) return { used: 0, free: "" };
    return {
      used: storage.totalBytes > 0 ? 1 - storage.freeBytes / storage.totalBytes : 0,
      free: i18n.t("devices.free", { size: formatStorageSize(storage.freeBytes) }),
    };
  }

  function syncingText(id: string): string {
    const p = devicesStore.progress[id];
    return p
      ? i18n.t("devices.syncingCount", {
          done: formatNumber(p.songsDone),
          total: formatNumber(p.songsTotal),
        })
      : i18n.t("devices.syncingStarting");
  }

  const syncingFraction = $derived(
    devicesStore.syncingId ? syncFraction(devicesStore.syncingId) : null,
  );

  async function addMusicFolder() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: i18n.t("settings.selectMusicDirectory"),
      });
      if (selected && typeof selected === "string") await collectionStore.addDirectory(selected);
    } catch (err) {
      console.error("Failed to add a music folder:", err);
    }
  }
</script>

<div class="w-full flex flex-col {collapsed ? 'items-center' : ''}" data-testid="sidebar-files">
  {#if collapsed}
    <button
      onclick={() => navigationStore.viewFiles()}
      class="flex items-center justify-center w-10 h-10 rounded-xl p-0 transition-colors duration-150 {active
        ? 'bg-brand-accent text-brand-accent-contrast shadow-lg shadow-brand-accent/20'
        : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}"
      title={i18n.t("sidebar.files")}
      aria-label={i18n.t("sidebar.files")}
    >
      <span class="relative">
        <FoldersIcon class="w-5 h-5" />
        {#if syncingFraction !== null}
          <DeviceProgressRing
            fraction={syncingFraction}
            class="absolute -top-1.5 -right-1.5 w-3.5 h-3.5"
          />
        {:else if filesStore.devices.length > 0}
          <span
            class="absolute -top-1 -right-1 w-2 h-2 rounded-full bg-brand-accent-text ring-2 ring-brand-sidebar"
            data-testid="files-device-dot"
          ></span>
        {/if}
      </span>
    </button>
  {:else}
    <div
      class="w-full flex items-center rounded-lg transition-colors duration-150 {atOverview
        ? 'bg-brand-accent/20 text-brand-accent-text'
        : 'text-brand-text-secondary hover:bg-brand-accent/10 hover:text-brand-accent-text-hover'}"
    >
      <button
        onclick={() => navigationStore.viewFiles()}
        class="flex-1 min-w-0 flex items-center gap-3 px-3 py-1.5 text-sm font-medium text-left"
      >
        <div class="w-7 h-7 flex items-center justify-center shrink-0">
          <FoldersIcon class="w-4 h-4 icon-align" />
        </div>
        <span class="truncate whitespace-nowrap">{i18n.t("sidebar.files")}</span>
      </button>
      <button
        onclick={() => navigationStore.openSettings("sources")}
        class="mr-1.5 w-6 h-6 flex items-center justify-center rounded-md text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 transition-colors"
        title={i18n.t("files.addSource")}
        aria-label={i18n.t("files.addSource")}
      >
        <PlusIcon class="w-3.5 h-3.5" />
      </button>
    </div>
  {/if}

  {#if showSubItems}
    <div
      class="pl-[9px] pr-1 py-0.5 space-y-0 border-l-2 border-brand-accent/30 ml-[calc(var(--sidebar-icon-axis)_-_1px)] my-0.5"
    >
      {#if filesStore.loaded && filesStore.sources.length === 0}
        <button
          onclick={addMusicFolder}
          class="w-full flex items-center gap-2 px-2.5 py-1 rounded-md text-xs font-semibold text-brand-accent-text hover:bg-brand-accent/10 transition-colors"
        >
          <PlusIcon class="w-3.5 h-3.5 icon-align" />
          <span class="truncate">{i18n.t("files.addMusicFolder")}</span>
        </button>
      {/if}
      {#each filesStore.sources as entry (sourceKey(entry.source))}
        {@const Icon = sourceIcon(entry.source)}
        <button
          onclick={() => navigationStore.viewFiles(entry.source)}
          class="w-full flex items-center justify-between px-2.5 py-1 rounded-md text-xs transition-colors {isOpen(
            entry.source,
          )
            ? 'bg-brand-accent/20 text-brand-accent-text font-semibold'
            : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'} {entry.available
            ? ''
            : 'opacity-60'}"
        >
          <div class="flex items-center gap-2 truncate">
            <Icon class="w-3.5 h-3.5 icon-align shrink-0" />
            <span class="truncate">{entry.name}</span>
          </div>
          <span class="text-[10px] text-brand-text-secondary/60 ml-1"
            >({formatNumber(entry.songCount)})</span
          >
        </button>
      {/each}
      {#if filesStore.devices.length > 0}
        <div class="mx-2.5 my-1.5 border-t border-brand-border/40" role="separator"></div>
      {/if}
      {#each filesStore.devices as device (device.id)}
        {@const Icon = deviceIcon(device)}
        {@const fraction = syncFraction(device.id)}
        {@const space = syncSpace(device)}
        {@const open = isOpen({ kind: "device", id: device.id })}
        {@const name = deviceName(device)}
        <div class="flex items-center gap-1" data-testid="sidebar-device">
          <button
            onclick={() => {
              navigationStore.viewDevice(device.id, navigationStore.filesLocation?.deviceTab);
              // A running sync's progress comes back with its device.
              if (fraction !== null) devicesStore.showProgress();
            }}
            data-drop-target="device"
            data-device-id={device.id}
            data-device-name={name}
            class="flex-1 min-w-0 flex items-start gap-2 px-2.5 py-1.5 rounded-md text-xs text-left transition-colors {open
              ? 'bg-brand-accent/20 text-brand-accent-text font-semibold'
              : 'text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10'} {songDrag.isDragging &&
            songDrag.dropTarget?.kind === 'device' &&
            songDrag.dropTarget.id === device.id
              ? 'bg-brand-accent/30 ring-2 ring-brand-accent text-brand-text-primary font-semibold'
              : ''}"
          >
            <Icon class="w-3.5 h-3.5 icon-align shrink-0 mt-px" />
            <span class="flex-1 min-w-0 flex flex-col gap-1">
              <span class="truncate">{name}</span>
              <span
                class="h-[3px] rounded-full bg-brand-border/50 overflow-hidden"
                aria-hidden="true"
              >
                <span
                  class="block h-full rounded-full {fraction === null
                    ? 'bg-brand-text-secondary/70'
                    : 'bg-brand-accent-text'}"
                  style:width="{Math.round((fraction ?? space.used) * 100)}%"
                ></span>
              </span>
              <span
                class="text-[10px] font-normal truncate {fraction === null
                  ? 'text-brand-text-secondary/80'
                  : 'text-brand-accent-text'}"
              >
                {fraction === null ? space.free : syncingText(device.id)}
              </span>
            </span>
          </button>
          {#if fraction === null}
            <button
              onclick={() => devicesStore.eject(device.id)}
              class="w-6 h-6 shrink-0 flex items-center justify-center rounded-md text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 transition-colors"
              title={i18n.t("devices.ejectNamed", { device: name })}
              aria-label={i18n.t("devices.ejectNamed", { device: name })}
            >
              <EjectIcon class="w-3.5 h-3.5" />
            </button>
          {:else}
            <button
              onclick={() => devicesStore.stopSync(device.id)}
              class="w-6 h-6 shrink-0 flex items-center justify-center rounded-md text-brand-text-primary hover:bg-brand-accent/10 transition-colors"
              title={i18n.t("devices.stopSyncNamed", { device: name })}
              aria-label={i18n.t("devices.stopSyncNamed", { device: name })}
            >
              <StopIcon weight="fill" class="w-3.5 h-3.5" />
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
