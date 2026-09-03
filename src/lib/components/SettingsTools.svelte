<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import OrganizeFiles from "./OrganizeFiles.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import { Eraser, Sparkles, Check, Eye, EyeOff, Disc3, AlertTriangle, RefreshCw, FolderOpen } from "lucide-svelte";

  const PRUNE_MESSAGE_DURATION_MS = 8000;

  let pruneMsg = $state<string | null>(null);
  let organizeRefreshKey = $state(0);
  let showAcoustidKey = $state(false);
  let hasEnvKey = $state(false);
  let picardCustomPath = $state("");
  let isRecheckingPicard = $state(false);

  async function handlePicardCustomPathChange() {
    await invoke("set_app_setting", { key: "picard_path", value: picardCustomPath.trim() });
    await picardStore.refresh();
  }

  async function handleBrowsePicardPath() {
    const selected = await open({
      multiple: false,
      title: i18n.t("picard.browseBtn"),
      filters: [{ name: "Picard executable", extensions: ["exe"] }],
    });
    if (selected && typeof selected === "string") {
      picardCustomPath = selected;
      await handlePicardCustomPathChange();
    }
  }

  async function handleRecheckPicard() {
    isRecheckingPicard = true;
    try {
      await picardStore.refresh();
    } finally {
      isRecheckingPicard = false;
    }
  }

  async function handlePruneMissing() {
    const { deletedSongs, removedFolders, mergedDuplicates } = await collectionStore.pruneMissing();
    if (removedFolders > 0 && mergedDuplicates > 0) {
      pruneMsg = i18n.t('settings.pruneCompleteMsgWithFoldersAndDuplicates', { count: deletedSongs, folders: removedFolders, duplicates: mergedDuplicates });
    } else if (mergedDuplicates > 0) {
      pruneMsg = i18n.t('settings.pruneCompleteMsgWithDuplicates', { count: deletedSongs, duplicates: mergedDuplicates });
    } else if (removedFolders > 0) {
      pruneMsg = i18n.t('settings.pruneCompleteMsgWithFolders', { count: deletedSongs, folders: removedFolders });
    } else {
      pruneMsg = i18n.t('settings.pruneCompleteMsg', { count: deletedSongs });
    }
    organizeRefreshKey++;
    setTimeout(() => { pruneMsg = null; }, PRUNE_MESSAGE_DURATION_MS);
  }

  onMount(async () => {
    try {
      hasEnvKey = await invoke("has_acoustid_env_key");
    } catch (e) {
      console.error("Failed to check AcoustID env key on mount:", e);
    }
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      picardCustomPath = settings?.picard_path ?? "";
    } catch (e) {
      console.error("Failed to load Picard custom path on mount:", e);
    }
  });
</script>

<OrganizeFiles embedded songIds={[]} initialScope="library" refreshKey={organizeRefreshKey} />

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Disc3 class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('picard.integrationTitle')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t('picard.integrationDesc1')} <button onclick={() => openExternalUrl("https://picard.musicbrainz.org")} class="text-brand-accent hover:underline">MusicBrainz Picard</button> {i18n.t('picard.integrationDesc2')}
        </p>
      </div>
    </div>
  </div>

  <div class="flex items-center gap-2 text-xs font-medium">
    {#if picardStore.available}
      <Check class="w-3.5 h-3.5 text-brand-accent-text shrink-0" />
      <span class="text-brand-accent-text truncate" title={picardStore.path ?? undefined}>
        {i18n.t('picard.foundAt', { path: picardStore.path ?? '' })}
      </span>
    {:else}
      <AlertTriangle class="w-3.5 h-3.5 text-amber-500 shrink-0" />
      <span class="text-brand-text-secondary">{i18n.t('picard.notFound')}</span>
    {/if}
    <button
      onclick={handleRecheckPicard}
      disabled={isRecheckingPicard}
      class="ml-1 text-brand-text-secondary hover:text-brand-accent-text transition-colors disabled:opacity-50"
      title={i18n.t('picard.recheckTooltip')}
    >
      <RefreshCw class="w-3.5 h-3.5 {isRecheckingPicard ? 'animate-spin' : ''}" />
    </button>
  </div>

  <div class="flex flex-col gap-1.5">
    <label for="picard-custom-path-input" class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider">
      {i18n.t('picard.customPathLabel')}
    </label>
    <div class="flex items-center gap-2">
      <div class="max-w-md flex-1">
        <Input
          id="picard-custom-path-input"
          type="text"
          bind:value={picardCustomPath}
          onchange={handlePicardCustomPathChange}
          placeholder={i18n.t('picard.customPathPlaceholder')}
          class="w-full"
        />
      </div>
      <Button onclick={handleBrowsePicardPath} variant="secondary" size="sm">
        <FolderOpen class="w-4 h-4" />
        {i18n.t('picard.browseBtn')}
      </Button>
    </div>
  </div>
</div>

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4">
  <div class="pb-3 flex justify-between items-center">
    <div class="flex items-center gap-3">
      <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text shrink-0">
        <Sparkles class="w-5 h-5" />
      </div>
      <div class="space-y-1 min-w-0">
        <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t('settings.acoustidIntegration')}</h3>
        <p class="text-xs text-brand-text-secondary leading-relaxed">
          {i18n.t('settings.acoustidDesc1')}<button onclick={() => openExternalUrl("https://acoustid.org")} class="text-brand-accent hover:underline">AcoustID</button>{i18n.t('settings.acoustidDesc2')}
          <br />
          {i18n.t('settings.acoustidDesc3')}<button onclick={() => navigationStore.activeTab = 'help'} class="text-brand-accent hover:underline">{i18n.t('settings.acoustidUserGuide')}</button>{i18n.t('settings.acoustidDesc4')}
        </p>
      </div>
    </div>
  </div>

  <div>
    {#if hasEnvKey}
      <div class="mb-4 text-xs font-medium text-brand-accent-text flex items-center gap-2">
        <Check class="w-3.5 h-3.5" />
        <span>{i18n.t('settings.acoustidEnvKeyFound', { env: 'ACOUSTID_API_KEY' })}</span>
      </div>
    {/if}

    <div class="flex items-center gap-3 max-w-md">
      <div class="relative flex-1">
        <Input
          type={showAcoustidKey ? "text" : "password"}
          bind:value={prefs.acoustidApiKey}
          onchange={() => prefs.setAcoustidApiKey(prefs.acoustidApiKey)}
          placeholder={i18n.t('settings.acoustidPlaceholder')}
          class="w-full pr-10"
        />
        <button
          type="button"
          onclick={() => showAcoustidKey = !showAcoustidKey}
          class="absolute right-3 top-1/2 -translate-y-1/2 text-brand-text-secondary hover:text-brand-text-primary transition-colors"
          title={showAcoustidKey ? "Hide key" : "Show key"}
        >
          {#if showAcoustidKey}
            <EyeOff class="w-4 h-4" />
          {:else}
            <Eye class="w-4 h-4" />
          {/if}
        </button>
      </div>
    </div>
  </div>
</div>

<div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-5">
  <div class="flex flex-wrap items-center gap-3">
    <Button onclick={handlePruneMissing} disabled={collectionStore.isScanning} variant="secondary" size="sm">
      <Eraser class="w-4 h-4" />
      {i18n.t('settings.pruneMissingBtn')}
    </Button>
    <span class="text-xs text-brand-text-secondary">{i18n.t('settings.pruneMissingHint')}</span>

    {#if pruneMsg}
      <span class="text-xs text-brand-accent-text font-medium transition-all">{pruneMsg}</span>
    {/if}
  </div>
</div>
