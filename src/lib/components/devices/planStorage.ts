import { i18n } from "../../stores/i18n.svelte";
import { formatStorageSize } from "../../utils/formatters";
import type { SyncPlan } from "../../types";
import type { StorageSlice, StorageTone } from "./DeviceStorageBar.svelte";

type PlanStorage = SyncPlan["storage"];

/** A plan's storage as the sync leaves it, in bar order. */
export function afterSyncSlices(storage: PlanStorage): StorageSlice[] {
  return [
    { tone: "music", bytes: storage.staysBytes },
    { tone: "copied", bytes: storage.copiedBytes },
    { tone: "other", bytes: storage.otherBytes },
  ];
}

/** A plan's storage before the sync, with what it removes, in bar order. */
export function beforeSyncSlices(storage: PlanStorage): StorageSlice[] {
  return [
    { tone: "music", bytes: storage.staysBytes },
    { tone: "removed", bytes: storage.removedBytes },
    { tone: "other", bytes: storage.otherBytes },
  ];
}

/**
 * Names every slice of a plan's bars with its size. `withRemoved` names
 * what the sync removes too, for when the before bar is drawn.
 */
export function planLegend(
  storage: PlanStorage,
  withRemoved: boolean,
): { tone: StorageTone; label: string }[] {
  const size = formatStorageSize;
  return [
    {
      tone: "music" as const,
      label: i18n.t("devices.legendStays", { size: size(storage.staysBytes) }),
    },
    ...(withRemoved
      ? [
          {
            tone: "removed" as const,
            label: i18n.t("devices.legendRemoved", { size: size(storage.removedBytes) }),
          },
        ]
      : []),
    {
      tone: "copied" as const,
      label: i18n.t("devices.legendCopied", { size: size(storage.copiedBytes) }),
    },
    {
      tone: "other" as const,
      label: i18n.t("devices.legendOther", { size: size(storage.otherBytes) }),
    },
    ...(storage.reserveBytes > 0
      ? [
          {
            tone: "reserve" as const,
            label: i18n.t("devices.gauge.reserve", { size: size(storage.reserveBytes) }),
          },
        ]
      : []),
  ];
}
