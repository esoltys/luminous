<script lang="ts">
  import { onMount } from "svelte";
  import { BroadcastIcon as Broadcast } from "phosphor-svelte";
  import Plus from "../PlusIcon.svelte";
  import { formatUpdatedAgo, radioStore } from "../../stores/radio.svelte";
  import { playlistsStore } from "../../stores/playlists.svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { prefs } from "../../stores/prefs.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { isStationList } from "../../utils/playlist";
  import { rowListGap, useCardAppearance } from "../../utils/cardAppearance";
  import { startPointerDrag, type PointerDragHandle } from "../../utils/pointerDrag";
  import type { Playlist } from "../../types";
  import CardSection from "../CardSection.svelte";
  import DragGhost from "../DragGhost.svelte";
  import PlaylistRowCard from "../PlaylistRowCard.svelte";
  import PlaylistCardContextMenu from "../PlaylistCardContextMenu.svelte";
  import RadioStationCard from "./RadioStationCard.svelte";
  import PinnedStationRowCard from "./PinnedStationRowCard.svelte";
  import StationListModal from "./StationListModal.svelte";

  const appearance = useCardAppearance();

  let scrollEl: HTMLElement | null = $state(null);
  let showStationListModal = $state(false);
  let listMenu = $state<{ x: number; y: number; playlist: Playlist } | null>(null);

  let draggedIndex = $state<number | null>(null);
  let dragOverIndex = $state<number | null>(null);
  let ghostPos = $state<{ x: number; y: number } | null>(null);
  let stationDrag: PointerDragHandle | null = null;

  function commitSavedReorder(targetIndex: number) {
    if (draggedIndex === null || targetIndex === draggedIndex) return;
    const items = [...radioStore.savedStations];
    const [moved] = items.splice(draggedIndex, 1);
    if (!moved) return;
    items.splice(targetIndex, 0, moved);
    void radioStore.reorderSavedStations(items.map((s) => s.stationuuid));
  }

  function resetStationDrag() {
    draggedIndex = null;
    dragOverIndex = null;
    ghostPos = null;
    stationDrag = null;
  }

  function handleStationPointerDown(e: PointerEvent, index: number) {
    stationDrag = startPointerDrag(e, {
      captureOn: "arm",
      onMove: (pos) => {
        ghostPos = pos;
        const el = document
          .elementFromPoint(pos.x, pos.y)
          ?.closest("[data-saved-station-index]") as HTMLElement | null;
        const idx = el?.dataset.savedStationIndex;
        dragOverIndex = idx !== undefined ? Number(idx) : null;
      },
      onDrop: () => {
        if (dragOverIndex !== null) commitSavedReorder(dragOverIndex);
        resetStationDrag();
      },
      onTap: resetStationDrag,
      onCancel: resetStationDrag,
    });
    if (stationDrag) draggedIndex = index;
  }

  $effect(() => () => stationDrag?.cancel());

  const recentStations = $derived(radioStore.recentStations.slice(0, 10));

  const stationLists = $derived.by(() =>
    playlistsStore.playlists.filter((pl) => isStationList(pl)),
  );

  const hasDynamicStationLists = $derived(stationLists.some((pl) => pl.dynamic_enabled));

  const updatedAgoLabel = $derived(formatUpdatedAgo(radioStore.status?.lastSyncedAt ?? null));

  function openNewStationList() {
    showStationListModal = true;
  }

  onMount(() => {
    const savedScroll = radioStore.getScrollTop("my_stations");
    if (scrollEl && savedScroll > 0) {
      scrollEl.scrollTop = savedScroll;
    }
  });
</script>

<div
  bind:this={scrollEl}
  onscroll={(e) => radioStore.setScrollTop("my_stations", e.currentTarget.scrollTop)}
  data-testid="radio-my-stations-view"
  class="flex-1 min-h-0 overflow-y-auto pt-2 pb-dock space-y-12 pr-1"
>
  <!-- Saved stations -->
  <section class="space-y-4">
    {#if radioStore.savedStations.length === 0}
      <h2 class="text-xl font-semibold text-brand-text-primary">
        {i18n.t("radio.savedStationsTitle")}
      </h2>
      <div
        data-testid="radio-saved-empty"
        class="p-6 rounded-xl border border-brand-border/40 bg-brand-surface/40 text-center space-y-2"
      >
        <p class="text-sm text-brand-text-secondary">
          {i18n.t("radio.noSavedStations")}
        </p>
        {#if radioStore.status?.optedIn}
          <button
            type="button"
            onclick={() => {
              radioStore.subView = "discover";
            }}
            class="px-3.5 py-1.5 rounded-lg bg-brand-accent/15 text-brand-accent-text text-xs font-medium hover:bg-brand-accent/25"
          >
            {i18n.t("radio.browseStationsAction")}
          </button>
        {/if}
      </div>
    {:else}
      <CardSection
        title={i18n.t("radio.savedStationsTitle")}
        viewMode={prefs.radioSavedViewMode}
        onViewModeChange={(m) => prefs.setRadioSavedViewMode(m)}
      >
        {#snippet children(mode)}
          {@const rows = mode === "rows"}
          {#each radioStore.savedStations as station, index (station.stationuuid)}
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              data-saved-station-index={index}
              onpointerdown={(e) => handleStationPointerDown(e, index)}
              ondragstart={(e) => e.preventDefault()}
              class="relative h-full [&>*:first-child]:h-full transition-opacity cursor-grab active:cursor-grabbing [&_[role=button]]:cursor-[inherit] {rows
                ? 'rounded-lg'
                : 'rounded-xl'} {draggedIndex === index ? 'opacity-40' : ''}"
            >
              {#if rows}
                <PinnedStationRowCard
                  {station}
                  playing={radioStore.isPlayingStation(station.stationuuid)}
                  onPlay={(s) => void radioStore.playStation(s)}
                />
              {:else}
                <RadioStationCard
                  {station}
                  playing={radioStore.isPlayingStation(station.stationuuid)}
                  onPlay={(s) => void radioStore.playStation(s)}
                />
              {/if}
              {#if dragOverIndex === index && draggedIndex !== null && draggedIndex !== index}
                <div
                  class="absolute inset-0 bg-brand-accent/30 pointer-events-none {rows
                    ? 'rounded-lg'
                    : 'rounded-xl'}"
                ></div>
              {/if}
            </div>
          {/each}
        {/snippet}
      </CardSection>
    {/if}
  </section>

  <!-- Recently played + Station lists side-by-side (matching HomeView Recently Added / Top Albums) -->
  <div class="grid grid-cols-1 @5xl:grid-cols-2 gap-8">
    <!-- Recently played stations (10 items) -->
    <section class="h-full flex flex-col gap-4">
      <h2 class="text-xl font-semibold text-brand-text-primary">
        {i18n.t("radio.recentStationsTitle")}
      </h2>

      {#if recentStations.length === 0}
        <p class="text-xs text-brand-text-secondary px-1">
          {i18n.t("radio.noRecentStations")}
        </p>
      {:else}
        <div class="flex-1 flex flex-col {rowListGap(appearance)}">
          {#each recentStations as station (station.stationuuid)}
            <PinnedStationRowCard
              {station}
              playing={radioStore.isPlayingStation(station.stationuuid)}
              onPlay={(st) => void radioStore.playStation(st)}
            />
          {/each}
        </div>
      {/if}
    </section>

    <!-- Station lists (auto-updating genre lists + manual radio playlists) -->
    <section class="h-full flex flex-col gap-4" data-testid="radio-station-lists-section">
      <div class="flex items-center justify-between gap-3">
        <div class="flex items-baseline gap-2 min-w-0">
          <h2 class="text-xl font-semibold text-brand-text-primary truncate">
            {i18n.t("radio.stationListsTitle")}
          </h2>
          {#if updatedAgoLabel && hasDynamicStationLists}
            <span
              class="text-xs text-brand-text-secondary font-medium shrink-0"
              data-testid="station-lists-updated-ago"
            >
              {updatedAgoLabel}
            </span>
          {/if}
        </div>
        {#if radioStore.status?.optedIn}
          <button
            type="button"
            data-testid="new-station-list-btn"
            onclick={openNewStationList}
            class="inline-flex items-center gap-1.5 px-3 py-1 rounded-lg bg-brand-accent/15 text-brand-accent-text text-xs font-medium hover:bg-brand-accent/25 shrink-0"
          >
            <Plus class="w-3.5 h-3.5" />
            <span>{i18n.t("radio.newStationList")}</span>
          </button>
        {/if}
      </div>

      {#if stationLists.length === 0}
        <div
          class="p-5 rounded-xl border border-brand-border/40 bg-brand-surface/40 text-center space-y-1.5"
        >
          <p class="text-sm text-brand-text-secondary">
            {i18n.t("radio.noStationLists")}
          </p>
        </div>
      {:else}
        <div class="flex-1 flex flex-col {rowListGap(appearance)}">
          <!-- Station lists read like playlists elsewhere (PlaylistRowCard, no
               inline controls): open one to play, refresh, edit or delete it,
               or right-click it. Imported lists (#2279) count stations too. -->
          {#each stationLists as pl (pl.id)}
            <PlaylistRowCard
              playlist={pl}
              countLabel={i18n.t("radio.stationCount", { count: pl.track_count })}
              onClick={() => navigationStore.viewPlaylist(pl.id)}
              oncontextmenu={(e) => {
                e.preventDefault();
                listMenu = { x: e.clientX, y: e.clientY, playlist: pl };
              }}
            />
          {/each}
        </div>
      {/if}
    </section>
  </div>
</div>

{#if ghostPos && draggedIndex !== null && radioStore.savedStations[draggedIndex]}
  {@const dragged = radioStore.savedStations[draggedIndex]}
  <DragGhost
    x={ghostPos.x}
    y={ghostPos.y}
    overTarget={dragOverIndex !== null && dragOverIndex !== draggedIndex}
    testId="saved-station-drag-ghost"
  >
    <Broadcast class="w-3.5 h-3.5 text-brand-accent shrink-0" />
    <span class="max-w-[180px] truncate">{dragged.name || i18n.t("radio.unnamedStation")}</span>
  </DragGhost>
{/if}

{#if showStationListModal}
  <StationListModal
    onClose={() => {
      showStationListModal = false;
    }}
  />
{/if}

{#if listMenu}
  <PlaylistCardContextMenu
    x={listMenu.x}
    y={listMenu.y}
    playlist={listMenu.playlist}
    onClose={() => (listMenu = null)}
  />
{/if}
