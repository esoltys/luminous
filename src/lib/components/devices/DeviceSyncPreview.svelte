<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { CaretLeftIcon } from "phosphor-svelte";
  import { devicesStore } from "../../stores/devices.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { i18n, formatNumber } from "../../stores/i18n.svelte";
  import { formatStorageSize } from "../../utils/formatters";
  import { conversionLabel, deviceName, deviceStorage, storageName } from "../../utils/devices";
  import type { ConnectedDevice, SyncChangeList, SyncItem, SyncPlan, SyncReason, SyncSourceRef } from "../../types";
  import Button from "../Button.svelte";
  import DeviceStorageBar from "./DeviceStorageBar.svelte";
  import DeviceStorageLegend from "./DeviceStorageLegend.svelte";
  import { afterSyncSlices, beforeSyncSlices, planLegend } from "./planStorage";

  /**
   * The sync preview (#839): the verdict, the device now and after the
   * sync on one scale, and what is copied, removed and updated, with why.
   * Shows `plan` as the backend computed it. Nothing is written until
   * Start sync, which runs exactly this plan.
   */
  let {
    device,
    plan,
    onback,
    onplan,
    onstarted,
  }: {
    device: ConnectedDevice;
    plan: SyncPlan;
    onback: () => void;
    /** A newer plan replaces this one (after the suggested fix, or when this one went stale). */
    onplan: (plan: SyncPlan) => void;
    onstarted: () => void;
  } = $props();

  const name = $derived(deviceName(device));
  const storage = $derived(plan.storage);
  const target = $derived(
    plan.conversionTarget ? conversionLabel(plan.conversionTarget.format, plan.conversionTarget.bitrateKbps) : ""
  );

  const storageLabel = $derived.by(() => {
    const size = formatStorageSize(storage.capacityBytes);
    const s = deviceStorage(device, plan.storageId);
    // Which storage is synced only matters when there is a choice.
    if (!s || device.storages.length < 2) return size;
    return `${storageName(device, s)}, ${size}`;
  });

  const headline = $derived(
    storage.fits
      ? i18n.t("devices.verdictFits", { free: formatStorageSize(Math.max(0, storage.freeAfterBytes)) })
      : i18n.t("devices.verdictTooBig", { over: formatStorageSize(plan.shortfall?.bytes ?? 0), device: name })
  );

  const summary = $derived.by(() => {
    if (!storage.fits) {
      const parts: string[] = [];
      const saving = plan.shortfall?.convertAllSavingBytes;
      if (saving && target) parts.push(i18n.t("devices.fixConvertSaves", { target, saving: formatStorageSize(saving) }));
      const largest = plan.shortfall?.largestSource;
      if (largest) {
        parts.push(
          i18n.t(parts.length > 0 ? "devices.fixOrUncheckLargest" : "devices.fixUncheckLargest", {
            item: sourceName(largest.source),
            size: formatStorageSize(largest.bytes),
          })
        );
      }
      return parts.join(" ");
    }
    const parts = [i18n.plural("devices.summaryCopy", plan.copy.songs, { count: formatNumber(plan.copy.songs) })];
    if (plan.convertedSongs > 0 && target) {
      parts[0] = i18n.plural("devices.summaryCopyConverted", plan.copy.songs, {
        count: formatNumber(plan.copy.songs),
        converted: formatNumber(plan.convertedSongs),
        target,
      });
    }
    if (plan.remove.songs > 0) parts.push(i18n.plural("devices.summaryRemove", plan.remove.songs, { count: formatNumber(plan.remove.songs) }));
    return parts.join(" ");
  });

  const nowSlices = $derived(beforeSyncSlices(storage));
  const afterSlices = $derived(afterSyncSlices(storage));
  const total = (slices: { bytes: number }[]) => slices.reduce((sum, s) => sum + s.bytes, 0);
  const scale = $derived(Math.max(storage.capacityBytes, total(nowSlices), total(afterSlices)));
  const legend = $derived(planLegend(storage, true));

  function sourceName(source: SyncSourceRef): string {
    if (source.kind === "library") return i18n.t("devices.wholeLibrary");
    if (source.kind === "folder") return i18n.t("devices.reasonFolder");
    return source.name;
  }

  function reason(r: SyncReason): string {
    switch (r.code) {
      case "selected":
        return sourceName(r.source);
      case "renamedBesideExisting":
        return i18n.t("devices.reasonRenamed");
      case "adopted":
        return i18n.t("devices.reasonAdopted");
      case "leftSource":
        return i18n.t("devices.reasonLeft", { source: sourceName(r.source) });
      case "unselected":
        return r.source ? i18n.t(`devices.reasonUnchecked.${r.source.kind}`) : i18n.t("devices.reasonUnchecked.library");
      case "deletedFromLibrary":
        return i18n.t("devices.reasonDeleted");
      case "tagsChanged":
        return i18n.t("devices.reasonTags");
      case "fileChanged":
        return i18n.t("devices.reasonFileChanged");
      case "convertChanged":
        return i18n.t("devices.reasonConvertChanged");
      case "moved":
        return i18n.t("devices.reasonMoved");
      case "unreadable":
        return i18n.t("devices.reasonUnreadable");
    }
  }

  function itemLabel(item: SyncItem): string {
    const title = item.title ?? item.path.split("/").pop() ?? item.path;
    return item.artist ? `${item.artist}, ${title}` : title;
  }

  type ListKey = "copy" | "remove" | "update";
  let expanded = $state<Partial<Record<ListKey, SyncItem[]>>>({});
  $effect(() => {
    void plan.planId;
    expanded = {};
  });

  async function showAll(list: ListKey) {
    try {
      expanded[list] = await invoke<SyncItem[]>("get_device_sync_items", { planId: plan.planId, list });
    } catch (err) {
      if (String(err) === "sync-plan-stale") await replan();
      else console.error("Failed to read sync items:", err);
    }
  }

  const lists = $derived<{ key: ListKey; list: SyncChangeList; heading: string; more: string }[]>([
    {
      key: "copy",
      list: plan.copy,
      heading: i18n.plural("devices.listCopy", plan.copy.songs, { count: formatNumber(plan.copy.songs), size: formatStorageSize(plan.copy.bytes) }),
      more: i18n.t("devices.showAllCopy"),
    },
    {
      key: "remove",
      list: plan.remove,
      heading: i18n.plural("devices.listRemove", plan.remove.songs, { count: formatNumber(plan.remove.songs), size: formatStorageSize(plan.remove.bytes) }),
      more: i18n.t("devices.showAllRemove"),
    },
    {
      key: "update",
      list: plan.update,
      heading: i18n.plural("devices.listUpdate", plan.update.songs, { count: formatNumber(plan.update.songs) }),
      more: i18n.t("devices.showAllUpdate"),
    },
  ]);
  const playlistFiles = $derived(plan.playlistFilesWritten + plan.playlistFilesRemoved);

  let busy = $state(false);

  /** Plans the saved selection again and shows that plan instead. */
  async function replan() {
    const state = await devicesStore.refreshSyncState(device.id);
    if (!state) return;
    onplan(await devicesStore.planSync(device.id, state.settings));
  }

  /** The suggested fix: convert everything that gets smaller, then plan again. */
  async function convertEverything() {
    busy = true;
    try {
      await invoke("update_device_profile", {
        id: device.id,
        profile: { ...device.profile, convert: { ...device.profile.convert, mode: "saveSpace" } },
      });
      await replan();
    } catch (err) {
      console.error("Failed to switch Convert:", err);
      toastStore.show(i18n.t("devices.previewFailed", { device: name }), "error");
    } finally {
      busy = false;
    }
  }

  async function start() {
    busy = true;
    try {
      await invoke("start_device_sync", { planId: plan.planId });
      devicesStore.drawerHidden = false;
      await devicesStore.refreshSyncState(device.id);
      onstarted();
    } catch (err) {
      const code = String(err);
      if (code === "sync-plan-stale") {
        toastStore.show(i18n.t("devices.planChanged", { device: name }), "info");
        await replan().catch((e) => console.error("Failed to plan again:", e));
      } else {
        console.error("Failed to start device sync:", err);
        toastStore.show(i18n.t(code === "sync-in-progress" ? "devices.alreadySyncing" : "devices.startFailed", { device: name }), "error");
      }
    } finally {
      busy = false;
    }
  }
</script>

<div class="flex-1 min-h-0 flex flex-col gap-5" data-testid="device-sync-preview">
  <div class="flex-1 min-h-0 overflow-y-auto pr-6 flex flex-col gap-5">
    <header>
      <button class="flex items-center gap-1 text-sm text-brand-accent-text hover:text-brand-accent-text-hover" onclick={onback}>
        <CaretLeftIcon class="w-3.5 h-3.5" />{i18n.t("devices.backToSettings", { device: name })}
      </button>
      <h1 class="text-3xl font-heading font-bold text-brand-text-primary leading-snug">{i18n.t("devices.reviewTitle", { device: name })}</h1>
    </header>

    <div>
      <p class="text-4xl font-heading font-bold leading-tight text-brand-text-primary" data-testid="sync-verdict">{headline}</p>
      {#if summary}<p class="mt-2 text-sm text-brand-text-secondary">{summary}</p>{/if}
    </div>

    <div class="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-2">
      <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t("devices.now")}</span>
      <DeviceStorageBar slices={nowSlices} capacityBytes={storage.capacityBytes} reserveBytes={storage.reserveBytes} scaleBytes={scale} height="h-10" label={i18n.t("devices.now")} />
      <span class="text-xs font-semibold text-brand-text-secondary">{i18n.t("devices.afterSync")}</span>
      <DeviceStorageBar slices={afterSlices} capacityBytes={storage.capacityBytes} reserveBytes={storage.reserveBytes} scaleBytes={scale} height="h-10" label={headline} />
      <span></span>
      <div class="relative h-4 text-[11px] font-semibold text-brand-text-primary">
        <span class="absolute top-0 -translate-x-full whitespace-nowrap" style:left="{(storage.capacityBytes / Math.max(1, scale)) * 100}%">{storageLabel}</span>
      </div>
      <span></span>
      <DeviceStorageLegend items={legend} />
    </div>

    <div class="grid grid-cols-1 @container">
      <div class="grid grid-cols-1 @3xl:grid-cols-3 gap-6">
        {#each lists as { key, list, heading, more } (key)}
          {@const items = expanded[key] ?? list.items}
          <section class="min-w-0 flex flex-col">
            <h2 class="text-sm font-bold text-brand-text-primary mb-1">{heading}</h2>
            <ul class="flex flex-col">
              {#each items as item, i (item.path + i)}
                <li class="flex items-baseline justify-between gap-3 py-1.5 border-b border-brand-border/30 text-sm">
                  <span class="truncate text-brand-text-primary">{itemLabel(item)}</span>
                  <span class="shrink-0 text-xs text-brand-text-secondary">{reason(item.reason)}</span>
                </li>
              {/each}
              {#if key === "update" && playlistFiles > 0}
                <li class="flex items-baseline justify-between gap-3 py-1.5 border-b border-brand-border/30 text-sm">
                  <span class="text-brand-text-primary">{i18n.t("devices.playlistsRow")}</span>
                  <span class="shrink-0 text-xs text-brand-text-secondary">{i18n.plural("devices.playlistFilesRewritten", playlistFiles, { count: formatNumber(playlistFiles) })}</span>
                </li>
              {/if}
            </ul>
            {#if !expanded[key] && list.songs > list.items.length}
              <button class="self-start mt-2 text-sm text-brand-accent-text hover:text-brand-accent-text-hover" onclick={() => showAll(key)}>{more}</button>
            {/if}
            {#if key === "update"}
              <p class="mt-2 text-xs text-brand-text-secondary">{i18n.t("devices.othersLeftAlone", { device: name })}</p>
            {/if}
          </section>
        {/each}
      </div>
    </div>
  </div>

  <footer class="mr-6 shrink-0 flex items-center justify-end gap-2 border-t border-brand-border/40 pt-4">
    {#if storage.fits}
      <Button onclick={onback}>{i18n.t("devices.changeSelection")}</Button>
      <Button variant="primary" onclick={start} disabled={busy}>{i18n.t("devices.startSync")}</Button>
    {:else}
      {#if plan.shortfall?.convertAllSavingBytes && target}
        <Button onclick={convertEverything} disabled={busy}>{i18n.t("devices.convertEverything", { target })}</Button>
      {/if}
      <Button variant="primary" onclick={onback}>{i18n.t("devices.changeSelection")}</Button>
    {/if}
  </footer>
</div>
