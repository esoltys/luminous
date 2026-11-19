<script lang="ts">
  import { onMount } from "svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import {
    ArrowClockwiseIcon as RefreshCw,
    WarningCircleIcon as AlertCircle,
    ExportIcon as Export,
  } from "phosphor-svelte";
  import { fade } from "../../utils/motion";
  import { CONTEXT_MENU_WIDTH_PX } from "../../constants";
  import { radioStore } from "../../stores/radio.svelte";
  import { toastStore } from "../../stores/toast.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import CollectionHeader from "../CollectionHeader.svelte";
  import MoreIcon from "../MoreIcon.svelte";
  import Plus from "../PlusIcon.svelte";
  import ContextMenu from "../ContextMenu.svelte";
  import ContextMenuItem from "../ContextMenuItem.svelte";
  import ContextMenuDivider from "../ContextMenuDivider.svelte";
  import RadioOptInCard from "./RadioOptInCard.svelte";
  import RadioDiscoverView from "./RadioDiscoverView.svelte";
  import RadioMyStationsView from "./RadioMyStationsView.svelte";
  import AddStationModal from "./AddStationModal.svelte";

  let overflowButtonEl: HTMLButtonElement | null = $state(null);
  let showOverflowMenu = $state(false);
  let overflowMenuPos = $state({ x: 0, y: 0 });
  let showAddStationModal = $state(false);

  function toggleOverflowMenu() {
    if (showOverflowMenu) {
      showOverflowMenu = false;
      return;
    }
    if (!overflowButtonEl) return;
    const rect = overflowButtonEl.getBoundingClientRect();
    overflowMenuPos = { x: rect.right - CONTEXT_MENU_WIDTH_PX, y: rect.bottom + 8 };
    showOverflowMenu = true;
  }

  async function handleExportSavedStations() {
    try {
      const savePath = await save({
        title: i18n.t("radio.exportStationsTooltip"),
        defaultPath: "saved-stations.pls",
        filters: [
          { name: "PLS Playlist (*.pls)", extensions: ["pls"] },
          { name: "M3U8 Playlist (*.m3u8)", extensions: ["m3u8"] },
          { name: "M3U Playlist (*.m3u)", extensions: ["m3u"] },
        ],
      });
      if (savePath && typeof savePath === "string") {
        await radioStore.exportSavedStations(savePath);
      }
    } catch (err) {
      toastStore.show(err instanceof Error ? err.message : String(err), "error");
    }
  }

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
    const countText =
      count > 0 ? i18n.t("radio.stationCount", { count: count.toLocaleString() }) : "";
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

      <button
        bind:this={overflowButtonEl}
        type="button"
        onclick={toggleOverflowMenu}
        data-testid="radio-overflow-btn"
        aria-label={i18n.t("playlists.moreActionsTooltip")}
        title={i18n.t("playlists.moreActionsTooltip")}
        class="flex items-center justify-center w-8 h-8 rounded-full border border-brand-border hover:border-brand-accent/60 bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary transition-colors cursor-pointer"
      >
        <MoreIcon class="w-4 h-4" />
      </button>
    {/snippet}
  </CollectionHeader>

  <div class="flex-1 min-h-0 flex flex-col px-6 pt-2 pb-4 gap-3">
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

{#if showOverflowMenu}
  <ContextMenu
    x={overflowMenuPos.x}
    y={overflowMenuPos.y}
    onClose={() => {
      showOverflowMenu = false;
    }}
  >
    <ContextMenuItem
      icon={Plus}
      label={i18n.t("radio.addStationBtn")}
      onclick={() => {
        showOverflowMenu = false;
        showAddStationModal = true;
      }}
    />
    {#if radioStore.savedStations.length > 0}
      <ContextMenuItem
        icon={Export}
        label={i18n.t("radio.exportStationsBtn")}
        onclick={() => {
          showOverflowMenu = false;
          void handleExportSavedStations();
        }}
      />
    {/if}
    {#if radioStore.status?.optedIn}
      <ContextMenuDivider />
      <ContextMenuItem
        icon={RefreshCw}
        label={radioStore.syncing ? i18n.t("radio.syncingShort") : i18n.t("radio.refreshIndex")}
        disabled={radioStore.syncing}
        onclick={() => {
          showOverflowMenu = false;
          void radioStore.syncDirectory();
        }}
      />
    {/if}
  </ContextMenu>
{/if}

{#if showAddStationModal || radioStore.editingStation}
  <AddStationModal
    station={radioStore.editingStation}
    onClose={() => {
      showAddStationModal = false;
      radioStore.editingStation = null;
    }}
  />
{/if}
