<script lang="ts">
  import { i18n } from "../../stores/i18n.svelte";
  import { formatStorageSize } from "../../utils/formatters";
  import { deviceName, storageName } from "../../utils/devices";
  import type { ConnectedDevice, SyncPlan, SyncSettings } from "../../types";
  import DeviceStorageBar from "./DeviceStorageBar.svelte";
  import DeviceStorageLegend from "./DeviceStorageLegend.svelte";
  import { afterSyncSlices, planLegend } from "./planStorage";

  /**
   * The synced storage as the latest preview leaves it (#839), laid out like
   * `DeviceStorageGauge` so the card keeps its size and place between the
   * Files and Sync tabs. Every figure is the plan's.
   */
  let {
    device,
    plan,
    unselected,
    failed,
  }: {
    device: ConnectedDevice;
    plan: SyncPlan | null;
    unselected: SyncSettings["unselected"] | undefined;
    /** The latest preview failed. */
    failed: boolean;
  } = $props();

  // Which storage is synced only matters when there is a choice.
  const target = $derived(
    plan && device.storages.length > 1 ? device.storages.find((s) => s.id === plan.storageId) : undefined
  );

  const verdict = $derived.by(() => {
    if (!plan) return "";
    const s = plan.storage;
    if (s.fits) {
      return i18n.t("devices.projectionFits", {
        copy: formatStorageSize(s.copiedBytes),
        remove: formatStorageSize(s.removedBytes),
        free: formatStorageSize(Math.max(0, s.freeAfterBytes)),
      });
    }
    return i18n.t(unselected === "leave" ? "devices.projectionTooBigLeave" : "devices.projectionTooBig", {
      over: formatStorageSize(plan.shortfall?.bytes ?? 0),
    });
  });

</script>

<div class="flex flex-col gap-2.5 min-w-0" data-testid="sync-projection">
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <span class="min-w-0 text-xs text-brand-text-secondary">
      {target ? i18n.t("devices.syncingTo", { storage: storageName(device, target) }) : ""}
    </span>
    <span class="text-xs font-semibold {plan ? 'text-brand-text-primary' : 'text-brand-text-secondary'}">
      {#if plan}
        {verdict}
      {:else if failed}
        {i18n.t("devices.previewFailed", { device: deviceName(device) })}
      {:else}
        {i18n.t("common.loading")}
      {/if}
    </span>
  </div>
  {#if plan}
    <DeviceStorageBar
      slices={afterSyncSlices(plan.storage)}
      capacityBytes={plan.storage.capacityBytes}
      reserveBytes={plan.storage.reserveBytes}
      label={verdict}
    />
    <DeviceStorageLegend items={planLegend(plan.storage, false)} />
  {:else}
    <div class="h-2.5 rounded-[3px] bg-brand-border/25" aria-hidden="true"></div>
    <!-- The legend's line, held while loading so the card doesn't grow when it arrives. -->
    <div class="h-4" aria-hidden="true"></div>
  {/if}
</div>
