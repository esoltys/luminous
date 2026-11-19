<script lang="ts">
  import { onMount } from "svelte";
  import {
    ArrowClockwiseIcon as RefreshCw,
    WarningCircleIcon as AlertCircle,
  } from "phosphor-svelte";
  import { fade } from "../../utils/motion";
  import { radioStore } from "../../stores/radio.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import CollectionHeader from "../CollectionHeader.svelte";
  import RadioOptInCard from "./RadioOptInCard.svelte";
  import RadioDiscoverView from "./RadioDiscoverView.svelte";
  import RadioMyStationsView from "./RadioMyStationsView.svelte";

  function formatUpdatedAgo(lastSyncedAt: number | null): string {
    if (!lastSyncedAt) return "";
    const diffSecs = Math.max(0, Math.floor(Date.now() / 1000) - lastSyncedAt);
    const hours = Math.floor(diffSecs / 3600);
    if (hours < 24) {
      return i18n.t("radio.updatedHoursAgo", { count: Math.max(1, hours) });
    }
    const days = Math.floor(hours / 24);
    return i18n.t("radio.updatedDaysAgo", { count: days });
  }

  onMount(() => {
    void radioStore.init();
    return () => {
      radioStore.dispose();
    };
  });

  const needsOptIn = $derived(!radioStore.status?.optedIn || radioStore.status.stationCount === 0);
  const headerSubtitle = $derived.by(() => {
    const count = radioStore.status?.playableStationCount ?? 0;
    const countText = count > 0 ? `${count.toLocaleString()} stations` : "";
    if (radioStore.status?.optedIn && radioStore.isStale && radioStore.status.lastSyncedAt) {
      const ago = formatUpdatedAgo(radioStore.status.lastSyncedAt);
      return countText ? `${countText} · ${ago}` : ago;
    }
    return countText;
  });
</script>

<div class="@container flex flex-col h-full min-h-0" data-testid="radio-view">
  <CollectionHeader title={i18n.t("radio.title")} subtitle={headerSubtitle}>
    {#snippet actions()}
      {#if radioStore.status?.optedIn && radioStore.isStale && radioStore.status.lastSyncedAt}
        <span
          data-testid="radio-stale-indicator"
          class="label-small px-2.5 py-1 rounded-full bg-brand-sidebar border border-brand-border text-brand-text-secondary"
        >
          {formatUpdatedAgo(radioStore.status.lastSyncedAt)}
        </span>
      {/if}

      <!-- Sub-view switcher -->
      <div
        role="tablist"
        aria-label={i18n.t("radio.title")}
        class="inline-flex p-0.5 rounded-full bg-brand-sidebar border border-brand-border"
      >
        <button
          type="button"
          role="tab"
          aria-selected={radioStore.subView === "my_stations"}
          data-testid="radio-tab-my-stations"
          onclick={() => {
            radioStore.subView = "my_stations";
          }}
          class="px-3.5 py-1 rounded-full text-xs font-medium transition-colors duration-150 {radioStore.subView ===
          'my_stations'
            ? 'bg-brand-accent text-brand-accent-contrast'
            : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t("radio.myStationsTab")}
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={radioStore.subView === "discover"}
          data-testid="radio-tab-discover"
          onclick={() => {
            radioStore.subView = "discover";
          }}
          class="px-3.5 py-1 rounded-full text-xs font-medium transition-colors duration-150 {radioStore.subView ===
          'discover'
            ? 'bg-brand-accent text-brand-accent-contrast'
            : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t("radio.discoverTab")}
        </button>
      </div>

      {#if radioStore.status?.optedIn}
        <button
          type="button"
          disabled={radioStore.syncing}
          onclick={() => void radioStore.syncDirectory()}
          aria-label={i18n.t("radio.refreshIndex")}
          class="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-full border border-brand-border hover:border-brand-accent/60 bg-brand-sidebar text-xs font-medium text-brand-text-secondary hover:text-brand-text-primary disabled:opacity-50 transition-colors"
        >
          <RefreshCw class="w-3.5 h-3.5 {radioStore.syncing ? 'animate-spin' : ''}" />
          <span class="hidden @sm:inline">
            {radioStore.syncing ? i18n.t("radio.syncingShort") : i18n.t("radio.refreshIndex")}
          </span>
        </button>
      {/if}
    {/snippet}
  </CollectionHeader>

  <div class="flex-1 min-h-0 flex flex-col px-6 pb-4 gap-3">
    {#if radioStore.syncError && !needsOptIn}
      <div
        data-testid="radio-offline-banner"
        role="alert"
        class="flex items-center justify-between gap-3 px-3 py-2 rounded-xl border border-brand-border bg-brand-surface text-xs text-brand-text-primary shrink-0"
      >
        <div class="flex items-center gap-2 min-w-0">
          <AlertCircle class="w-4 h-4 shrink-0 text-brand-text-secondary" />
          <span class="truncate">{i18n.t("radio.syncOfflineTitle")}: {radioStore.syncError}</span>
        </div>
        <button
          type="button"
          onclick={() => void radioStore.syncDirectory()}
          class="px-2.5 py-1 rounded-lg bg-brand-accent/15 text-brand-accent-text font-medium shrink-0 hover:bg-brand-accent/25"
        >
          {i18n.t("radio.retryDownload")}
        </button>
      </div>
    {/if}

    <!-- Body -->
    <div class="flex-1 min-h-0 flex flex-col">
      {#if radioStore.subView === "my_stations"}
        <div class="flex-1 min-h-0 flex flex-col" in:fade={{ duration: 120 }}>
          <RadioMyStationsView />
        </div>
      {:else if needsOptIn}
        <div class="flex-1 min-h-0 overflow-y-auto" in:fade={{ duration: 120 }}>
          <RadioOptInCard
            syncing={radioStore.syncing}
            progress={radioStore.syncProgress}
            error={radioStore.syncError}
            onDownload={() => void radioStore.syncDirectory()}
          />
        </div>
      {:else}
        <div class="flex-1 min-h-0 flex flex-col" in:fade={{ duration: 120 }}>
          <RadioDiscoverView />
        </div>
      {/if}
    </div>
  </div>
</div>
