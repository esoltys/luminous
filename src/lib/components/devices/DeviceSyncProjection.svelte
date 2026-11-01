<script lang="ts">
  import { i18n } from "../../stores/i18n.svelte";
  import { formatStorageSize } from "../../utils/formatters";
  import { deviceName, storageName } from "../../utils/devices";
  import type { ConnectedDevice, SyncPlan, SyncSettings } from "../../types";
  import DeviceStorageBar from "./DeviceStorageBar.svelte";
  import DeviceStorageLegend from "./DeviceStorageLegend.svelte";

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

  const unnamedStorage = (n: number) => i18n.t("devices.storageNumbered", { n });
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

  const legend = $derived(
    plan
      ? [
          { tone: "music" as const, label: i18n.t("devices.legendStays", { size: formatStorageSize(plan.storage.staysBytes) }) },
          { tone: "copied" as const, label: i18n.t("devices.legendCopied", { size: formatStorageSize(plan.storage.copiedBytes) }) },
          { tone: "other" as const, label: i18n.t("devices.legendOther", { size: formatStorageSize(plan.storage.otherBytes) }) },
          ...(plan.storage.reserveBytes > 0
            ? [{ tone: "reserve" as const, label: i18n.t("devices.gauge.reserve", { size: formatStorageSize(plan.storage.reserveBytes) }) }]
            : []),
        ]
      : []
  );
</script>

<div class="flex flex-col gap-2.5 min-w-0" data-testid="sync-projection">
  <div class="flex items-center justify-between gap-3 flex-wrap">
    <span class="min-w-0 text-xs text-brand-text-secondary">
      {target ? i18n.t("devices.syncingTo", { storage: storageName(device, target, unnamedStorage) }) : ""}
    </span>
    <span class="text-xs font-semibold {plan && !plan.storage.fits ? 'text-amber-400' : plan ? 'text-brand-text-primary' : 'text-brand-text-secondary'}">
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
      slices={[
        { tone: "music", bytes: plan.storage.staysBytes },
        { tone: "copied", bytes: plan.storage.copiedBytes },
        { tone: "other", bytes: plan.storage.otherBytes },
      ]}
      capacityBytes={plan.storage.capacityBytes}
      reserveBytes={plan.storage.reserveBytes}
      label={verdict}
    />
    <DeviceStorageLegend items={legend} />
  {:else}
    <div class="h-2.5 rounded-[3px] bg-brand-border/25" aria-hidden="true"></div>
  {/if}
</div>
