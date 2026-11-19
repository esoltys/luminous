<script lang="ts">
  import {
    BroadcastIcon as Broadcast,
    DownloadSimpleIcon as Download,
    WarningCircleIcon as AlertCircle,
    ArrowClockwiseIcon as RefreshCw,
  } from "phosphor-svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import type { RadioDirectorySyncProgress } from "../../types";

  let {
    syncing = false,
    progress = null,
    error = null,
    onDownload,
  }: {
    syncing?: boolean;
    progress?: RadioDirectorySyncProgress | null;
    error?: string | null;
    onDownload: () => void;
  } = $props();
</script>

<div
  data-testid="radio-opt-in-card"
  class="max-w-xl mx-auto my-8 p-6 rounded-2xl border border-brand-border/60 bg-brand-surface/60 text-center space-y-4"
>
  <div
    class="w-12 h-12 mx-auto rounded-2xl bg-brand-accent/15 text-brand-accent-text flex items-center justify-center"
  >
    <Broadcast class="w-6 h-6" />
  </div>

  <div class="space-y-2">
    <h2 class="text-lg font-semibold text-brand-text-primary">
      {i18n.t("radio.optInTitle")}
    </h2>
    <p class="text-sm text-brand-text-secondary leading-relaxed">
      {i18n.t("radio.optInDescription")}
    </p>
  </div>

  {#if error}
    <div
      data-testid="radio-sync-error"
      role="alert"
      class="flex items-start gap-2.5 text-left p-3 rounded-xl border border-brand-border bg-brand-surface text-sm text-brand-text-primary"
    >
      <AlertCircle class="w-4 h-4 mt-0.5 shrink-0 text-brand-text-secondary" />
      <div class="flex-1 min-w-0">
        <p class="font-medium">{i18n.t("radio.syncOfflineTitle")}</p>
        <p class="text-xs text-brand-text-secondary break-words mt-0.5">{error}</p>
      </div>
    </div>
  {/if}

  {#if syncing}
    <div
      data-testid="radio-sync-progress"
      class="p-3 rounded-xl bg-brand-surface/80 border border-brand-border/40 text-sm text-brand-text-secondary space-y-1.5"
    >
      <div class="flex items-center justify-center gap-2 font-medium text-brand-text-primary">
        <RefreshCw class="w-4 h-4 animate-spin" />
        <span>{i18n.t("radio.syncingIndex")}</span>
      </div>
      {#if progress}
        <p class="text-xs text-brand-text-secondary">
          {i18n.t("radio.syncProgressDetail", {
            count: progress.fetchedStations,
            page: progress.page,
          })}
        </p>
      {/if}
    </div>
  {:else}
    <button
      type="button"
      data-testid="radio-download-index-btn"
      onclick={onDownload}
      class="inline-flex items-center gap-2 px-5 py-2.5 rounded-xl bg-brand-accent text-brand-accent-contrast font-medium text-sm hover:opacity-90 transition-opacity"
    >
      <Download class="w-4 h-4" />
      <span>{error ? i18n.t("radio.retryDownload") : i18n.t("radio.downloadIndex")}</span>
    </button>
  {/if}
</div>
