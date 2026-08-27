<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { openExternalUrl } from "../utils/openExternalUrl";
  import OrganizeFiles from "./OrganizeFiles.svelte";
  import Button from "./Button.svelte";
  import Input from "./Input.svelte";
  import { Eraser, Sparkles, Check, Eye, EyeOff } from "lucide-svelte";

  const PRUNE_MESSAGE_DURATION_MS = 8000;

  let pruneMsg = $state<string | null>(null);
  let organizeRefreshKey = $state(0);
  let showAcoustidKey = $state(false);
  let hasEnvKey = $state(false);

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
  });
</script>

<OrganizeFiles embedded songIds={[]} initialScope="library" refreshKey={organizeRefreshKey} />

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
