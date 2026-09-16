<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import OrganizeFiles from "./OrganizeFiles.svelte";
  import Button from "./Button.svelte";
  import { FolderIcon as Folder, EraserIcon as Eraser } from "phosphor-svelte";

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

<div class="flex-1 flex flex-col h-full bg-brand-main text-brand-text-primary select-none overflow-hidden relative">
  <div class="flex-1 overflow-y-auto px-6 pb-12" class:pb-28={!!playerStore.currentSong} use:rememberScroll={"organize"}>
    <div class="pt-8 pb-4 max-w-3xl mx-auto">
      <h1 class="text-3xl font-heading font-bold text-brand-text-primary flex items-center gap-3">
        <Folder class="w-7 h-7 text-brand-accent" />
        {i18n.t("organizer.title")}
      </h1>
      <p class="text-sm text-brand-text-secondary mt-1">
        {i18n.t("organizer.subtitle")}
      </p>
    </div>

    <div class="max-w-3xl mx-auto space-y-4">
      <OrganizeFiles embedded songIds={[]} initialScope="library" refreshKey={organizeRefreshKey} />

      <!-- Lightweight inline maintenance action — deliberately not its own bordered card. -->
      <div class="flex flex-wrap items-center gap-3 px-1">
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
  </div>
</div>
