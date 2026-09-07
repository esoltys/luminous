<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import OrganizeFiles from "./OrganizeFiles.svelte";
  import Button from "./Button.svelte";
  import { EraserIcon as Eraser } from "phosphor-svelte";

  const PRUNE_MESSAGE_DURATION_MS = 8000;

  let pruneMsg = $state<string | null>(null);
  let organizeRefreshKey = $state(0);

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
