<script lang="ts">
  import { collectionStore } from "../stores/collection.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { rememberScroll } from "../utils/scrollMemory";
  import OrganizeFiles from "./OrganizeFiles.svelte";
  import Button from "./Button.svelte";
  import { EraserIcon as Eraser, ArrowsClockwiseIcon as RefreshCw, BroomIcon as Broom, StarIcon as Star } from "phosphor-svelte";

  const PRUNE_MESSAGE_DURATION_MS = 8000;

  let pruneMsg = $state<string | null>(null);
  let organizeRefreshKey = $state(0);
  let organizeReadyCount = $state(0);
  let organizeCanApply = $state(false);
  let organizeIsApplying = $state(false);
  let organizeApplyRequestKey = $state(0);

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
    <div class="pt-4 pb-4 flex items-start justify-between gap-4">
      {#if organizeReadyCount === 0}
        <span class="flex items-center gap-2 text-xl font-bold text-brand-accent-text whitespace-nowrap">
          <span class="relative inline-flex shrink-0">
            <span class="absolute inset-0 rounded-full anim-gold-ring"></span>
            <Star weight="fill" class="w-5 h-5 anim-milestone-bounce" />
          </span>
          {i18n.t("organizer.nothingToOrganize")}
        </span>
      {:else}
        <span class="flex items-center gap-2 text-xl font-bold text-brand-accent-text whitespace-nowrap">
          <Broom class="w-5 h-5 shrink-0" />
          {i18n.t("organizer.summaryReady", { count: organizeReadyCount })}
        </span>
      {/if}

      <Button
        variant="primary"
        onclick={() => { organizeApplyRequestKey++; }}
        disabled={!organizeCanApply || organizeIsApplying}
      >
        {#if organizeIsApplying}
          <RefreshCw class="w-4 h-4 animate-spin" />
          <span>{i18n.t("organizer.applying")}</span>
        {:else}
          <span>{i18n.t("organizer.applyButton")}</span>
        {/if}
      </Button>
    </div>

    <div class="space-y-4">
      <OrganizeFiles
        embedded
        songIds={[]}
        initialScope="library"
        refreshKey={organizeRefreshKey}
        bind:summaryReadyCount={organizeReadyCount}
        bind:summaryCanApply={organizeCanApply}
        bind:summaryIsApplying={organizeIsApplying}
        applyRequestKey={organizeApplyRequestKey}
      />

      <!-- Lightweight inline maintenance action — deliberately not its own bordered card. -->
      <div class="max-w-3xl mx-auto flex flex-wrap items-center gap-3 px-1">
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
