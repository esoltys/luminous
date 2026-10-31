<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { ArrowsClockwiseIcon, CaretUpIcon, CheckIcon, EjectIcon, PencilSimpleIcon, PlayIcon, StopIcon, XIcon } from "phosphor-svelte";
  import { navigationStore, type DeviceTab } from "../../stores/navigation.svelte";
  import { devicesStore } from "../../stores/devices.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { formatStorageSize } from "../../utils/formatters";
  import { formatDateAdded } from "../../utils/date";
  import { deviceName, deviceStorage, storageName, syncStorage } from "../../utils/devices";
  import type { ConnectedDevice, DirListing, FileEntry, SyncPlan } from "../../types";
  import Button from "../Button.svelte";
  import ConfirmDialog from "../ConfirmDialog.svelte";
  import FileBrowser from "../files/FileBrowser.svelte";
  import { deviceIcon } from "../files/sourceIcon";
  import DeviceStorageGauge from "./DeviceStorageGauge.svelte";
  import DeviceSyncSettings from "./DeviceSyncSettings.svelte";
  import DeviceSyncPreview from "./DeviceSyncPreview.svelte";

  /**
   * A connected device in the Files view (#839): its header and storage
   * gauge, the files on it and how it is synced. The tab, storage and
   * folder live in the saved Files location; the sync preview doesn't,
   * since its plan only stands until the next preview.
   */
  let { device }: { device: ConnectedDevice } = $props();

  const location = $derived(navigationStore.filesLocation!);
  const tab = $derived<DeviceTab>(location.deviceTab ?? "files");
  const name = $derived(deviceName(device));
  const Icon = $derived(deviceIcon(device));
  const syncState = $derived(devicesStore.syncStates[device.id]);
  const syncing = $derived(devicesStore.isSyncing(device.id));
  /** The storage browsed and measured: the one picked here, else the first. */
  const storage = $derived(
    deviceStorage(device, location.source.kind === "device" ? location.source.storage : null)
  );
  const targetStorage = $derived(syncStorage(device, syncState?.settings));
  const unnamedStorage = (n: number) => i18n.t("devices.storageNumbered", { n });

  let preview = $state<SyncPlan | null>(null);
  let opening = $state(false);

  // Another device, or another tab, closes the preview.
  $effect(() => {
    void device.id;
    void tab;
    preview = null;
  });

  const connection = $derived.by(() => {
    const mtp = device.kind === "mtp";
    return device.model
      ? i18n.t(mtp ? "devices.modelOverMtp" : "devices.modelOverUsb", { model: device.model })
      : i18n.t(mtp ? "devices.connectedMtp" : "devices.connectedUsb");
  });
  const subtitle = $derived.by(() => {
    // Which storage is synced only matters when there is a choice.
    if (tab === "sync" && targetStorage && device.storages.length > 1) {
      const where = i18n.t("devices.syncingTo", { storage: storageName(device, targetStorage, unnamedStorage) });
      return `${connection}. ${where}`;
    }
    const synced = device.lastSyncedAt
      ? i18n.t("devices.lastSynced", { when: formatDateAdded(device.lastSyncedAt) })
      : i18n.t("devices.neverSynced");
    return `${connection}. ${synced}`;
  });

  // Renaming: the nickname lives in the device's profile; a blank one goes back to the device's own name.
  let renaming = $state(false);
  let newName = $state("");

  function startRename() {
    newName = name;
    renaming = true;
  }

  async function saveRename() {
    if (!renaming) return;
    renaming = false;
    const nickname = newName.trim();
    if (nickname === name) return;
    try {
      await invoke("update_device_profile", { id: device.id, profile: { ...device.profile, nickname: nickname || null } });
    } catch (err) {
      console.error("Failed to rename device:", err);
    }
  }

  function focusAndSelect(node: HTMLInputElement) {
    node.focus();
    node.select();
  }

  function setTab(next: DeviceTab) {
    navigationStore.filesLocation = { ...location, deviceTab: next };
  }

  function setStorage(storageId: string) {
    navigationStore.filesLocation = { ...location, source: { kind: "device", id: device.id, storage: storageId }, path: [] };
  }

  /** Previews the saved selection and opens the preview; nothing is written before Start sync. */
  async function syncNow() {
    opening = true;
    try {
      const state = syncState ?? (await devicesStore.refreshSyncState(device.id));
      if (!state) return;
      preview = await invoke<SyncPlan>("preview_device_sync", { deviceId: device.id, settings: state.settings });
      if (tab !== "sync") setTab("sync");
    } catch (err) {
      console.error("Failed to preview device sync:", err);
      toastStore.show(i18n.t("devices.previewFailed", { device: name }), "error");
    } finally {
      opening = false;
    }
  }

  // Files tab.
  let listing = $state<DirListing | null>(null);
  let selectedName = $state<string | null>(null);
  let selection = $state<string[]>([]);
  let reloadToken = $state(0);
  let confirmingDelete = $state(false);

  const browsed = $derived({ kind: "device" as const, id: device.id, storage: storage?.id ?? null });
  const selectedEntries = $derived(
    (listing?.entries ?? []).filter((e) => selection.includes(e.name))
  );
  const playable = $derived(selectedEntries.filter((e) => e.songId !== null));

  // A sync rewrites the device: read its folders again once one ends.
  let wasSyncing = false;
  $effect(() => {
    if (wasSyncing && !syncing) reloadToken++;
    wasSyncing = syncing;
  });

  function navigate(path: string[]) {
    navigationStore.filesLocation = { ...location, path };
  }

  async function play(entries: FileEntry[]) {
    const ids = entries.map((e) => e.songId).filter((id): id is number => id !== null);
    if (ids.length === 0) {
      toastStore.show(i18n.t("devices.notPlayable"), "info");
      return;
    }
    try {
      await playerStore.playSongs(ids, 0, undefined, undefined, name);
    } catch (err) {
      console.error("Failed to play device files:", err);
      toastStore.show(i18n.t("files.playFailed"), "error");
    }
  }

  async function deleteSelected() {
    confirmingDelete = false;
    const names = selectedEntries.map((e) => e.name);
    try {
      await invoke("delete_device_files", { deviceId: device.id, storageId: storage?.id ?? null, path: location.path, names });
      toastStore.show(i18n.plural("devices.deleted", names.length, { device: name }), "success", 4000);
    } catch (err) {
      console.error("Failed to delete files from device:", err);
      toastStore.show(
        i18n.t(String(err) === "sync-in-progress" ? "devices.deleteWhileSyncing" : "devices.deleteFailed", { device: name }),
        "error"
      );
    } finally {
      selection = [];
      reloadToken++;
    }
  }
</script>

<!-- Right padding belongs to each part, so scrollable ones scroll at the window edge. -->
<div class="flex-1 min-h-0 flex flex-col pl-6 pt-4 gap-4" class:pb-28={!!playerStore.currentSong} class:pb-6={!playerStore.currentSong} data-testid="device-view">
  {#if preview}
    <DeviceSyncPreview
      {device}
      plan={preview}
      onback={() => (preview = null)}
      onplan={(plan) => (preview = plan)}
      onstarted={() => (preview = null)}
    />
  {:else}
    <header class="mr-6 flex items-start justify-between gap-4 flex-wrap">
      <div class="flex items-center gap-4 min-w-0">
        {#if tab === "files"}
          <div class="w-14 h-14 shrink-0 flex items-center justify-center rounded-xl border border-brand-border/50 bg-brand-sidebar/60">
            <Icon class="w-7 h-7 text-brand-accent-text" />
          </div>
        {/if}
        <div class="min-w-0">
          {#if renaming}
            <div class="flex items-center gap-2">
              <input
                bind:value={newName}
                onkeydown={(e) => { if (e.key === "Enter") saveRename(); else if (e.key === "Escape") renaming = false; }}
                placeholder={device.defaultName}
                aria-label={i18n.t("devices.rename")}
                class="min-w-0 bg-brand-sidebar border border-brand-accent text-brand-text-primary px-3 py-1 text-2xl font-bold rounded-lg focus:outline-none"
                use:focusAndSelect
              />
              <button onclick={saveRename} class="p-1.5 text-brand-accent-text hover:text-brand-accent" title={i18n.t("common.save")} aria-label={i18n.t("common.save")}>
                <CheckIcon class="w-5 h-5" />
              </button>
              <button onclick={() => (renaming = false)} class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary" title={i18n.t("common.cancel")} aria-label={i18n.t("common.cancel")}>
                <XIcon class="w-5 h-5" />
              </button>
            </div>
          {:else}
            <div class="flex items-center gap-3 min-w-0 group/title">
              <h1
                class="text-3xl font-heading font-bold text-brand-text-primary leading-snug truncate py-0.5 hover:text-brand-accent-text transition-colors"
                ondblclick={startRename}
              >{name}</h1>
              <button
                onclick={startRename}
                class="shrink-0 p-1 text-brand-text-secondary hover:text-brand-text-primary opacity-0 group-hover/title:opacity-100 focus-visible:opacity-100 transition-opacity"
                title={i18n.t("devices.rename")}
                aria-label={i18n.t("devices.rename")}
              >
                <PencilSimpleIcon class="w-4 h-4" />
              </button>
            </div>
          {/if}
          <p class="text-sm text-brand-text-secondary truncate">{subtitle}</p>
        </div>
      </div>
      <div class="flex items-center gap-2 shrink-0">
        {#if syncing && devicesStore.drawerHidden}
          <Button onclick={() => devicesStore.showProgress()}>
            <CaretUpIcon class="w-4 h-4" />
            {i18n.t("devices.showProgress")}
          </Button>
        {/if}
        {#if syncing}
          <Button onclick={() => devicesStore.stopSync(device.id)}>
            <StopIcon weight="fill" class="w-4 h-4" />
            {i18n.t("devices.stopSync")}
          </Button>
        {:else if tab === "files"}
          <Button variant="primary" onclick={syncNow} disabled={opening}>
            <ArrowsClockwiseIcon class="w-4 h-4" />
            {i18n.t("devices.syncNow")}
          </Button>
        {/if}
        <Button onclick={() => devicesStore.eject(device.id)}>
          <EjectIcon class="w-4 h-4" />
          {i18n.t("devices.eject")}
        </Button>
      </div>
    </header>

    {#if tab === "files" && storage}
      <section class="mr-6 rounded-xl border border-brand-border/40 bg-brand-sidebar/40 px-4 py-3">
        <DeviceStorageGauge {device} {storage}>
          {#snippet switcher()}
            {#if device.storages.length > 1}
              <div class="inline-flex rounded-lg bg-brand-main/60 p-0.5" role="radiogroup" aria-label={i18n.t("devices.storage")}>
                {#each device.storages as s (s.id)}
                  <button
                    class="px-3 py-1 rounded-md text-xs font-semibold transition-colors {s.id === storage.id ? 'bg-brand-accent/20 text-brand-text-primary' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                    role="radio"
                    aria-checked={s.id === storage.id}
                    onclick={() => setStorage(s.id)}
                  >{storageName(device, s, unnamedStorage)}, {formatStorageSize(s.totalBytes)}</button>
                {/each}
              </div>
            {/if}
          {/snippet}
        </DeviceStorageGauge>
      </section>
    {/if}

    <div class="mr-6 flex items-center gap-6 border-b border-brand-border/40" role="tablist">
      {#each ["files", "sync"] as const as t (t)}
        <button
          class="-mb-px py-2 text-sm font-semibold border-b-2 transition-colors {tab === t ? 'border-brand-accent-text text-brand-text-primary' : 'border-transparent text-brand-text-secondary hover:text-brand-text-primary'}"
          role="tab"
          aria-selected={tab === t}
          onclick={() => setTab(t)}
        >{i18n.t(t === "files" ? "devices.tabFiles" : "devices.tabSync")}</button>
      {/each}
    </div>

    {#if tab === "files"}
      <div class="flex-1 min-h-0 flex mr-6">
        <FileBrowser
          source={browsed}
          path={location.path}
          onnavigate={navigate}
          onplay={(entry) => play([entry])}
          {reloadToken}
          unreachableLabel={i18n.t("files.unreachable", { name })}
          bind:listing
          bind:selectedName
          bind:selection
        >
          {#snippet footer()}
            <div class="flex items-center justify-between gap-3 px-4 py-2 text-xs text-brand-text-secondary">
              <span class="truncate">
                {selection.length > 0 ? i18n.plural("devices.selected", selection.length) : i18n.t("devices.onlyRemovesOwn")}
              </span>
              {#if selection.length > 0}
                <div class="flex items-center gap-2 shrink-0">
                  <Button size="sm" onclick={() => play(selectedEntries)} disabled={playable.length === 0}>
                    <PlayIcon weight="fill" class="w-3.5 h-3.5" />
                    {i18n.t("devices.play")}
                  </Button>
                  <Button size="sm" variant="warning" onclick={() => (confirmingDelete = true)} disabled={syncing}>
                    {i18n.t("devices.deleteFromDevice")}
                  </Button>
                </div>
              {/if}
            </div>
          {/snippet}
        </FileBrowser>
      </div>
    {:else}
      <DeviceSyncSettings {device} onpreview={(plan) => (preview = plan)} />
    {/if}
  {/if}
</div>

{#if confirmingDelete}
  <ConfirmDialog
    title={i18n.plural("devices.deleteTitle", selectedEntries.length, { device: name })}
    message={i18n.t("devices.deleteMessage", { device: name })}
    confirmLabel={i18n.t("devices.deleteFromDevice")}
    cancelLabel={i18n.t("common.cancel")}
    onConfirm={deleteSelected}
    onCancel={() => (confirmingDelete = false)}
  />
{/if}
