<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Snippet } from "svelte";
  import { i18n, formatNumber } from "../../stores/i18n.svelte";
  import { formatStorageSize } from "../../utils/formatters";
  import type { ConnectedDevice, DeviceStorage, StorageUsage } from "../../types";
  import DeviceStorageBar from "./DeviceStorageBar.svelte";
  import DeviceStorageLegend from "./DeviceStorageLegend.svelte";

  /**
   * How one storage of a device is used: music Luminous synced, other
   * files, free space and the reserve, each named in the legend. Read from
   * the backend again whenever the storage's space or the reserve changes.
   */
  let {
    device,
    storage,
    switcher,
  }: {
    device: ConnectedDevice;
    storage: DeviceStorage;
    /** Drawn left of the free space: the storage switcher on devices with several. */
    switcher?: Snippet;
  } = $props();

  let usage = $state<StorageUsage | null>(null);
  let requested = "";

  $effect(() => {
    const deviceId = device.id;
    const key = `${deviceId}|${storage.id}|${storage.freeBytes}|${storage.totalBytes}|${device.profile.reserveBytes}`;
    if (key === requested) return;
    requested = key;
    invoke<StorageUsage>("get_device_storage_usage", { deviceId, storageId: storage.id })
      .then((result) => {
        if (requested === key) usage = result;
      })
      .catch((err) => console.error("Failed to read device storage usage:", err));
  });

  const legend = $derived(
    usage
      ? [
          {
            tone: "music" as const,
            label: i18n.plural("devices.gauge.music", usage.musicSongs, {
              size: formatStorageSize(usage.musicBytes),
              count: formatNumber(usage.musicSongs),
            }),
          },
          { tone: "other" as const, label: i18n.t("devices.gauge.other", { size: formatStorageSize(usage.otherBytes) }) },
          { tone: "free" as const, label: i18n.t("devices.gauge.free", { size: formatStorageSize(usage.freeBytes) }) },
          ...(usage.reserveBytes > 0
            ? [{ tone: "reserve" as const, label: i18n.t("devices.gauge.reserve", { size: formatStorageSize(usage.reserveBytes) }) }]
            : []),
        ]
      : []
  );
</script>

<div class="flex flex-col gap-2.5 min-w-0" data-testid="device-storage-gauge">
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <div class="min-w-0">{@render switcher?.()}</div>
    <div class="flex items-center gap-1 text-xs text-brand-text-secondary">
      <span>{storage.filesystem}.</span>
      <span class="font-semibold text-brand-text-primary">{i18n.t("devices.free", { size: formatStorageSize(storage.freeBytes) })}</span>
    </div>
  </div>
  {#if usage}
    <DeviceStorageBar
      slices={[
        { tone: "music", bytes: usage.musicBytes },
        { tone: "other", bytes: usage.otherBytes },
      ]}
      capacityBytes={usage.capacityBytes}
      reserveBytes={usage.reserveBytes}
      label={legend.map((l) => l.label).join(", ")}
    />
    <DeviceStorageLegend items={legend} />
  {:else}
    <div class="h-2.5 rounded-[3px] bg-brand-border/25" aria-hidden="true"></div>
    <!-- The legend's line, held while loading so the card doesn't grow when it arrives. -->
    <div class="h-4" aria-hidden="true"></div>
  {/if}
</div>
