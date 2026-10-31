<script lang="ts">
  import { untrack } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { CaretDownIcon, PlusIcon, WarningIcon } from "phosphor-svelte";
  import { devicesStore } from "../../stores/devices.svelte";
  import { playlistsStore } from "../../stores/playlists.svelte";
  import { collectionStore } from "../../stores/collection.svelte";
  import { i18n, formatNumber } from "../../stores/i18n.svelte";
  import { formatStorageSize, STORAGE_GB } from "../../utils/formatters";
  import { conversionLabel, deviceName, FORMAT_LABELS, storageName, syncStorage } from "../../utils/devices";
  import type {
    ConnectedDevice,
    ConvertMode,
    DeviceProfile,
    SyncPlan,
    SyncSettings,
    SyncSource,
    SyncSourceSize,
    TargetFormat,
    TranscodeCapabilities,
  } from "../../types";
  import Button from "../Button.svelte";
  import Select from "../Select.svelte";
  import TemplateInput from "../TemplateInput.svelte";
  import DeviceStorageBar from "./DeviceStorageBar.svelte";

  /**
   * A device's Sync tab (#839): what goes on it, how, and a live verdict.
   * Two writers, one owner each: the selection is saved by
   * `preview_device_sync`, the How settings by `update_device_profile`
   * (after which the device list brings the new profile and a new preview
   * follows). Every size, verdict and free-space figure is the backend
   * plan's; only the newest preview is shown.
   */
  let { device, onpreview }: { device: ConnectedDevice; onpreview: (plan: SyncPlan) => void } = $props();

  const PREVIEW_DEBOUNCE_MS = 250;
  const PROFILE_DEBOUNCE_MS = 400;

  const name = $derived(deviceName(device));
  const unnamedStorage = (n: number) => i18n.t("devices.storageNumbered", { n });

  let settings = $state<SyncSettings | null>(null);
  /** Everything listed, checked or not: the saved selection plus what was unchecked here. */
  let listed = $state<SyncSource[]>([]);
  let plan = $state<SyncPlan | null>(null);
  let previewError = $state(false);
  let capabilities = $state<TranscodeCapabilities | null>(null);
  let conversion = $state<{ substituted: boolean; devicePlays: boolean; format: TargetFormat; bitrateKbps: number } | null>(null);
  let examplePath = $state<string | null>(null);
  let moreOpen = $state(false);
  let picker = $state<"playlists" | "artists" | null>(null);
  let artistQuery = $state("");

  /** The profile as edited here; saved after a pause, then echoed back by `devices-changed`. */
  let draft = $state<DeviceProfile>(untrack(() => structuredClone($state.snapshot(device.profile))));
  let draftDirty = false;
  let profileTimer: ReturnType<typeof setTimeout> | undefined;

  // Load the saved selection once per device.
  $effect(() => {
    const id = device.id;
    settings = null;
    plan = null;
    devicesStore.refreshSyncState(id).then((state) => {
      if (!state || id !== device.id) return;
      settings = structuredClone($state.snapshot(state.settings));
      listed = [...settings.sources];
    });
  });

  // The device's saved profile, unless an edit here is still on its way.
  $effect(() => {
    const saved = device.profile;
    if (!draftDirty) draft = structuredClone($state.snapshot(saved));
  });

  $effect(() => {
    invoke<TranscodeCapabilities>("get_transcode_capabilities")
      .then((c) => (capabilities = c))
      .catch((err) => console.error("Failed to read transcode capabilities:", err));
  });

  // What conversions actually use here, and the example path, follow the profile being edited.
  let profileRequest = 0;
  $effect(() => {
    const profile = $state.snapshot(draft);
    const request = ++profileRequest;
    const timer = setTimeout(() => {
      invoke<typeof conversion>("get_conversion_target", { profile })
        .then((target) => { if (request === profileRequest) conversion = target; })
        .catch((err) => console.error("Failed to read the conversion target:", err));
      invoke<string | null>("preview_device_path", { profile, songId: null })
        .then((path) => { if (request === profileRequest) examplePath = path; })
        .catch((err) => console.error("Failed to preview the device path:", err));
    }, PREVIEW_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  // A new preview whenever the selection or the saved profile changes; late answers to older requests are dropped.
  let previewRequest = 0;
  $effect(() => {
    if (!settings) return;
    const snapshot = $state.snapshot(settings);
    void JSON.stringify(device.profile);
    const request = ++previewRequest;
    const timer = setTimeout(async () => {
      try {
        const result = await invoke<SyncPlan>("preview_device_sync", { deviceId: device.id, settings: snapshot });
        if (request === previewRequest) {
          plan = result;
          previewError = false;
        }
      } catch (err) {
        console.error("Failed to preview device sync:", err);
        if (request === previewRequest) previewError = true;
      }
    }, PREVIEW_DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  /** Previews the selection as it is now, skipping the debounce, and opens it. */
  let opening = $state(false);
  async function openPreview() {
    if (!settings) return;
    opening = true;
    const request = ++previewRequest;
    try {
      const result = await invoke<SyncPlan>("preview_device_sync", { deviceId: device.id, settings: $state.snapshot(settings) });
      if (request === previewRequest) plan = result;
      onpreview(result);
    } catch (err) {
      console.error("Failed to preview device sync:", err);
      previewError = true;
    } finally {
      opening = false;
    }
  }

  function editProfile(change: (profile: DeviceProfile) => void) {
    change(draft);
    draftDirty = true;
    clearTimeout(profileTimer);
    profileTimer = setTimeout(async () => {
      try {
        await invoke("update_device_profile", { id: device.id, profile: $state.snapshot(draft) });
      } catch (err) {
        console.error("Failed to save device profile:", err);
      } finally {
        draftDirty = false;
      }
    }, PROFILE_DEBOUNCE_MS);
  }

  // Selection.
  const sourceKey = (source: SyncSource) => JSON.stringify(source);
  const isChecked = (source: SyncSource) => !!settings?.sources.some((s) => sourceKey(s) === sourceKey(source));

  function toggle(source: SyncSource) {
    if (!settings) return;
    const key = sourceKey(source);
    settings.sources = isChecked(source)
      ? settings.sources.filter((s) => sourceKey(s) !== key)
      : [...settings.sources, source];
  }

  function add(source: SyncSource) {
    if (!settings) return;
    if (!listed.some((s) => sourceKey(s) === sourceKey(source))) listed = [...listed, source];
    if (!isChecked(source)) settings.sources = [...settings.sources, source];
  }

  async function addFolder() {
    try {
      const selected = await open({ directory: true, multiple: false, title: i18n.t("devices.addFolder") });
      if (selected && typeof selected === "string") add({ kind: "folder", path: selected });
    } catch (err) {
      console.error("Failed to pick a folder:", err);
    }
  }

  /** The plan's size for a listed source; absent while it is unchecked or the plan is loading. */
  function sizeOf(source: SyncSource): SyncSourceSize | undefined {
    if (!plan) return undefined;
    return plan.sources.find(({ source: ref }) => {
      if (ref.kind !== source.kind) return false;
      switch (source.kind) {
        case "playlist":
          return ref.key === String(source.id);
        case "artist":
          return ref.name === source.name;
        case "album":
          return ref.name === source.album;
        case "folder":
          return ref.name === source.path;
        default:
          return true;
      }
    });
  }

  function title(source: SyncSource): string {
    switch (source.kind) {
      case "library":
        return i18n.t("devices.wholeLibrary");
      case "playlist":
        return playlistsStore.playlists.find((p) => p.id === source.id)?.name ?? i18n.t("devices.deletedPlaylist");
      case "artist":
        return source.name;
      case "album":
        return i18n.t("devices.albumBy", { album: source.album, artist: source.artist });
      case "folder":
        return source.path;
    }
  }

  function detail(source: SyncSource, size: SyncSourceSize | undefined): string {
    if (!size) return "";
    if (size.capBytes !== null) {
      return i18n.plural("devices.cappedFit", size.songs, { cap: formatStorageSize(size.capBytes), count: formatNumber(size.songs) });
    }
    const songs = i18n.plural("files.songCount", size.songs, { count: formatNumber(size.songs) });
    if (source.kind === "playlist" && playlistsStore.playlists.find((p) => p.id === source.id)?.dynamic_enabled) {
      return i18n.t("devices.autoPlaylistSongs", { songs });
    }
    return source.kind === "folder" ? i18n.t("devices.folderSongs", { songs }) : songs;
  }

  const groups = $derived([
    { key: "playlists", label: i18n.t("devices.groupPlaylists"), sources: listed.filter((s) => s.kind === "playlist" || s.kind === "library") },
    { key: "artists", label: i18n.t("devices.groupArtists"), sources: listed.filter((s) => s.kind === "artist" || s.kind === "album") },
    { key: "folders", label: i18n.t("devices.groupFolders"), sources: listed.filter((s) => s.kind === "folder") },
  ].filter((g) => g.sources.length > 0));

  const playlistChoices = $derived(
    playlistsStore.playlists.filter((p) => !p.is_queue && !listed.some((s) => s.kind === "playlist" && s.id === p.id))
  );
  const artistChoices = $derived.by(() => {
    const q = artistQuery.trim().toLowerCase();
    return collectionStore.artists
      .filter((a): a is typeof a & { name: string } => !!a.name && (!q || a.name.toLowerCase().includes(q)))
      .filter((a) => !listed.some((s) => s.kind === "artist" && s.name === a.name))
      .slice(0, 50);
  });

  // How.
  const formats = $derived(capabilities?.formats ?? []);
  const bitrates = $derived(formats.find((f) => f.format === draft.convert.format)?.bitratesKbps ?? []);
  const CONVERT_MODES: ConvertMode[] = ["unsupported", "saveSpace", "never"];
  const ART_SIZES = [null, 1000, 500, 300];

  const convertHint = $derived.by(() => {
    if (draft.convert.mode === "never") return i18n.t("devices.convertNeverHint", { device: name });
    if (conversion?.substituted) {
      return i18n.t("devices.convertSubstituted", {
        format: FORMAT_LABELS[draft.convert.format],
        actual: conversionLabel(conversion.format, conversion.bitrateKbps),
      });
    }
    if (conversion && !conversion.devicePlays) {
      return i18n.t("devices.convertDeviceMayNotPlay", { device: name, format: FORMAT_LABELS[conversion.format] });
    }
    return i18n.t("devices.convertCacheHint");
  });
  const convertWarns = $derived(draft.convert.mode !== "never" && !!conversion && (conversion.substituted || !conversion.devicePlays));

  const playlistFolder = $derived(`/${draft.playlistsPath.replace(/^[/\\]+/, "")}`);
  const reserveGb = $derived(Math.round((draft.reserveBytes / STORAGE_GB) * 10) / 10);

  // Projection.
  const target = $derived(syncStorage(device, settings ?? undefined));
  const projection = $derived.by(() => {
    if (!plan) return "";
    const s = plan.storage;
    if (s.fits) {
      return i18n.t("devices.projectionFits", {
        copy: formatStorageSize(s.copiedBytes),
        remove: formatStorageSize(s.removedBytes),
        free: formatStorageSize(Math.max(0, s.freeAfterBytes)),
      });
    }
    return i18n.t(settings?.unselected === "leave" ? "devices.projectionTooBigLeave" : "devices.projectionTooBig", {
      over: formatStorageSize(plan.shortfall?.bytes ?? 0),
    });
  });
</script>

<div class="flex-1 min-h-0 flex flex-col gap-3" data-testid="device-sync-settings">
  {#if !settings}
    <p class="text-sm text-brand-text-secondary">{i18n.t("common.loading")}</p>
  {:else}
    <div class="flex-1 min-h-0 overflow-y-auto grid grid-cols-1 @container">
      <div class="grid grid-cols-1 @3xl:grid-cols-[minmax(0,1fr)_20rem] gap-8 pb-2">
        <!-- What -->
        <section class="min-w-0 flex flex-col gap-3">
          <div class="flex items-baseline justify-between gap-3">
            <h2 class="text-base font-bold text-brand-text-primary">{i18n.t("devices.whatGoesOn", { device: name })}</h2>
            <span class="text-xs text-brand-text-secondary">{i18n.t("devices.sizeAfterConverting")}</span>
          </div>
          {#if listed.length === 0}
            <p class="text-sm text-brand-text-secondary">{i18n.t("devices.nothingSelected")}</p>
          {/if}
          {#each groups as group (group.key)}
            <div class="flex flex-col">
              <h3 class="text-xs font-semibold text-brand-text-secondary px-2 py-1">{group.label}</h3>
              {#each group.sources as source (sourceKey(source))}
                {@const size = sizeOf(source)}
                {@const checked = isChecked(source)}
                <label class="flex items-center gap-3 px-2 py-2 rounded-lg hover:bg-brand-accent/5 cursor-pointer">
                  <input type="checkbox" class="w-4 h-4 accent-[var(--color-brand-accent)] shrink-0" {checked} onchange={() => toggle(source)} />
                  <span class="flex-1 min-w-0">
                    <span class="block text-sm text-brand-text-primary truncate">{title(source)}</span>
                    {#if checked && size}
                      <span class="block text-xs truncate {size.capBytes !== null ? 'text-brand-accent-text' : 'text-brand-text-secondary'}">{detail(source, size)}</span>
                    {/if}
                  </span>
                  <span class="text-xs tabular-nums text-brand-text-secondary shrink-0">{checked && size ? formatStorageSize(size.bytes) : ""}</span>
                </label>
              {/each}
            </div>
          {/each}
          <div class="flex flex-wrap gap-2 relative">
            <Button size="sm" onclick={() => (picker = picker === "playlists" ? null : "playlists")}>
              <PlusIcon class="w-3.5 h-3.5" />{i18n.t("devices.addPlaylists")}
            </Button>
            <Button size="sm" onclick={() => (picker = picker === "artists" ? null : "artists")}>
              <PlusIcon class="w-3.5 h-3.5" />{i18n.t("devices.addArtists")}
            </Button>
            <Button size="sm" onclick={addFolder}>
              <PlusIcon class="w-3.5 h-3.5" />{i18n.t("devices.addFolder")}
            </Button>
            {#if picker}
              <div class="absolute left-0 top-full mt-1 z-20 w-72 max-h-72 flex flex-col rounded-lg border border-brand-border/60 bg-brand-sidebar shadow-xl p-1" role="listbox">
                {#if picker === "artists"}
                  <input
                    type="search"
                    bind:value={artistQuery}
                    placeholder={i18n.t("devices.searchArtists")}
                    class="m-1 px-2.5 py-1.5 rounded-md bg-brand-main/60 border border-brand-border/60 text-sm focus:outline-none focus:border-brand-accent"
                  />
                {/if}
                <div class="overflow-y-auto">
                  {#if picker === "playlists"}
                    {#if !listed.some((s) => s.kind === "library")}
                      <button class="w-full text-left px-3 py-1.5 rounded-md text-sm hover:bg-brand-accent/10" onclick={() => add({ kind: "library" })}>{i18n.t("devices.wholeLibrary")}</button>
                    {/if}
                    {#each playlistChoices as p (p.id)}
                      <button class="w-full text-left px-3 py-1.5 rounded-md text-sm hover:bg-brand-accent/10 truncate" onclick={() => add({ kind: "playlist", id: p.id })}>{p.name}</button>
                    {/each}
                  {:else}
                    {#each artistChoices as a (a.name)}
                      <button class="w-full text-left px-3 py-1.5 rounded-md text-sm hover:bg-brand-accent/10 truncate" onclick={() => add({ kind: "artist", name: a.name })}>{a.name}</button>
                    {/each}
                  {/if}
                </div>
              </div>
            {/if}
          </div>
        </section>

        <!-- How -->
        <section class="min-w-0 flex flex-col gap-4 @3xl:border-l @3xl:border-brand-border/40 @3xl:pl-8">
          <h2 class="text-base font-bold text-brand-text-primary">{i18n.t("devices.how")}</h2>

          {#if device.storages.length > 1}
            <div class="flex flex-col gap-1.5">
              <label for="sync-storage" class="text-sm font-semibold text-brand-text-primary">{i18n.t("devices.syncToStorage")}</label>
              <Select
                id="sync-storage"
                value={target?.id ?? ""}
                onchange={(e) => { if (settings) settings.storageId = e.currentTarget.value; }}
                class="w-full px-3 py-2 pr-8 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm"
              >
                {#each device.storages as s (s.id)}
                  <option value={s.id}>{storageName(device, s, unnamedStorage)}</option>
                {/each}
              </Select>
            </div>
          {/if}

          <div class="flex flex-col gap-1.5">
            <span class="text-sm font-semibold text-brand-text-primary" id="unselected-label">{i18n.t("devices.songsYouUnselect")}</span>
            <div class="grid grid-cols-2 rounded-lg bg-brand-main/60 border border-brand-border/40 p-0.5" role="radiogroup" aria-labelledby="unselected-label">
              {#each ["remove", "leave"] as const as mode (mode)}
                <button
                  class="px-3 py-1.5 rounded-md text-sm font-semibold transition-colors {settings.unselected === mode ? 'bg-brand-accent/20 text-brand-text-primary' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
                  role="radio"
                  aria-checked={settings.unselected === mode}
                  onclick={() => { if (settings) settings.unselected = mode; }}
                >{i18n.t(mode === "remove" ? "devices.removeFromDevice" : "devices.leaveOnDevice")}</button>
              {/each}
            </div>
            <p class="text-xs text-brand-text-secondary">
              {i18n.t(settings.unselected === "remove" ? "devices.removeHint" : "devices.leaveHint", { device: name })}
            </p>
          </div>

          <div class="flex flex-col gap-1.5">
            <label for="convert-mode" class="text-sm font-semibold text-brand-text-primary">{i18n.t("devices.convert")}</label>
            <Select
              id="convert-mode"
              value={draft.convert.mode}
              onchange={(e) => editProfile((p) => (p.convert.mode = e.currentTarget.value as ConvertMode))}
              class="w-full px-3 py-2 pr-8 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm"
            >
              {#each CONVERT_MODES as mode (mode)}
                <option value={mode}>{i18n.t(`devices.convertMode.${mode}`, { device: name })}</option>
              {/each}
            </Select>
            {#if draft.convert.mode !== "never"}
              <div class="grid grid-cols-2 gap-2">
                <Select
                  value={draft.convert.format}
                  onchange={(e) => editProfile((p) => {
                    p.convert.format = e.currentTarget.value as TargetFormat;
                    const offered = formats.find((f) => f.format === p.convert.format)?.bitratesKbps ?? [];
                    if (offered.length > 0 && !offered.includes(p.convert.bitrateKbps)) p.convert.bitrateKbps = offered[offered.length - 1];
                  })}
                  class="w-full px-3 py-2 pr-8 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm"
                >
                  {#each formats as f (f.format)}
                    <option value={f.format}>{FORMAT_LABELS[f.format]}</option>
                  {/each}
                </Select>
                <Select
                  value={String(draft.convert.bitrateKbps)}
                  onchange={(e) => editProfile((p) => (p.convert.bitrateKbps = Number(e.currentTarget.value)))}
                  class="w-full px-3 py-2 pr-8 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm"
                >
                  {#each bitrates as kbps (kbps)}
                    <option value={String(kbps)}>{kbps} kbps</option>
                  {/each}
                </Select>
              </div>
            {/if}
            <p class="text-xs flex gap-1.5 {convertWarns ? 'text-amber-400' : 'text-brand-text-secondary'}">
              {#if convertWarns}<WarningIcon class="w-3.5 h-3.5 shrink-0 mt-px" />{/if}
              <span>{convertHint}</span>
            </p>
          </div>

          <div class="flex flex-col gap-1.5">
            <label for="device-folder-template" class="text-sm font-semibold text-brand-text-primary">{i18n.t("devices.foldersOnDevice")}</label>
            <TemplateInput
              id="device-folder-template"
              value={draft.layout.folderTemplate}
              onchange={(v) => editProfile((p) => (p.layout.folderTemplate = v))}
            />
            {#if examplePath}
              <p class="text-xs text-brand-text-secondary break-all">{examplePath}</p>
            {/if}
          </div>

          <div class="flex flex-col gap-1.5">
            <label for="playlist-files" class="text-sm font-semibold text-brand-text-primary">{i18n.t("devices.playlistFiles")}</label>
            <Select
              id="playlist-files"
              value={draft.layout.playlistFormat}
              onchange={(e) => editProfile((p) => (p.layout.playlistFormat = e.currentTarget.value as DeviceProfile["layout"]["playlistFormat"]))}
              class="w-full px-3 py-2 pr-8 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm"
            >
              {#each ["m3u8", "m3uAscii", "pls", "none"] as const as format (format)}
                <option value={format}>{i18n.t(`devices.playlistFormat.${format}`, { folder: playlistFolder })}</option>
              {/each}
            </Select>
          </div>

          <div class="flex flex-col gap-3">
            <button
              class="self-start flex items-center gap-1 text-sm text-brand-accent-text hover:text-brand-accent-text-hover"
              aria-expanded={moreOpen}
              onclick={() => (moreOpen = !moreOpen)}
            >
              {i18n.t("devices.more")}
              <CaretDownIcon class="w-3.5 h-3.5 {moreOpen ? 'rotate-180' : ''}" />
            </button>
            {#if moreOpen}
              <div class="flex flex-col gap-1.5">
                <label for="art-size" class="text-sm font-semibold text-brand-text-primary">{i18n.t("devices.artSize")}</label>
                <Select
                  id="art-size"
                  value={String(draft.convert.maxArtPx ?? "")}
                  onchange={(e) => editProfile((p) => (p.convert.maxArtPx = e.currentTarget.value ? Number(e.currentTarget.value) : null))}
                  class="w-full px-3 py-2 pr-8 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm"
                >
                  {#each ART_SIZES as px (px ?? "keep")}
                    <option value={String(px ?? "")}>{px === null ? i18n.t("devices.artKeep") : i18n.t("devices.artPx", { px })}</option>
                  {/each}
                </Select>
              </div>
              <div class="flex flex-col gap-1.5">
                <label for="path-separator" class="text-sm font-semibold text-brand-text-primary">{i18n.t("devices.pathSeparator")}</label>
                <Select
                  id="path-separator"
                  value={draft.layout.pathSeparator}
                  onchange={(e) => editProfile((p) => (p.layout.pathSeparator = e.currentTarget.value as DeviceProfile["layout"]["pathSeparator"]))}
                  class="w-full px-3 py-2 pr-8 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm"
                >
                  <option value="slash">{i18n.t("devices.separatorSlash")}</option>
                  <option value="backslash">{i18n.t("devices.separatorBackslash")}</option>
                </Select>
              </div>
              <div class="flex flex-col gap-1.5">
                <label for="reserve" class="text-sm font-semibold text-brand-text-primary">{i18n.t("devices.spaceToKeepFree")}</label>
                <div class="flex items-center gap-2">
                  <input
                    id="reserve"
                    type="number"
                    min="0"
                    step="0.5"
                    value={reserveGb}
                    onchange={(e) => editProfile((p) => (p.reserveBytes = Math.max(0, Math.round(Number(e.currentTarget.value) * STORAGE_GB))))}
                    class="w-24 px-3 py-2 rounded-lg bg-brand-main/60 border border-brand-border/60 text-sm tabular-nums"
                  />
                  <span class="text-sm text-brand-text-secondary">GB</span>
                </div>
              </div>
            {/if}
          </div>
        </section>
      </div>
    </div>

    <!-- Projection -->
    <div class="shrink-0 flex items-center gap-4 rounded-xl border border-brand-border/40 bg-brand-sidebar/60 px-4 py-3" data-testid="sync-projection">
      <div class="flex-1 min-w-0 flex flex-col gap-2">
        {#if plan}
          <DeviceStorageBar
            slices={[
              { tone: "music", bytes: plan.storage.staysBytes },
              { tone: "copied", bytes: plan.storage.copiedBytes },
              { tone: "other", bytes: plan.storage.otherBytes },
            ]}
            capacityBytes={plan.storage.capacityBytes}
            reserveBytes={plan.storage.reserveBytes}
            label={projection}
          />
          <p class="text-sm font-semibold {plan.storage.fits ? 'text-brand-text-primary' : 'text-amber-400'}">{projection}</p>
        {:else if previewError}
          <p class="text-sm text-brand-text-secondary">{i18n.t("devices.previewFailed", { device: name })}</p>
        {:else}
          <p class="text-sm text-brand-text-secondary">{i18n.t("common.loading")}</p>
        {/if}
      </div>
      <Button variant="primary" onclick={openPreview} disabled={!plan || opening}>
        {i18n.t("devices.previewChanges")}
      </Button>
    </div>
  {/if}
</div>

<svelte:window onclick={(e) => { if (picker && !(e.target as HTMLElement).closest('[role="listbox"], button')) picker = null; }} />
