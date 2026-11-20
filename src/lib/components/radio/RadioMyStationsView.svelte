<script lang="ts">
  import { onMount } from "svelte";
  import {
    BroadcastIcon as Broadcast,
    PlayIcon as Play,
    PauseIcon as Pause,
    PencilSimpleIcon as Pencil,
    TrashIcon as Trash,
  } from "phosphor-svelte";
  import Plus from "../PlusIcon.svelte";
  import { formatUpdatedAgo, radioStore } from "../../stores/radio.svelte";
  import { playlistsStore } from "../../stores/playlists.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { prefs } from "../../stores/prefs.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { isStationList, parseStationListSpec } from "../../utils/playlist";
  import { rowClasses, rowListGap, useCardAppearance } from "../../utils/cardAppearance";
  import { startPointerDrag, type PointerDragHandle } from "../../utils/pointerDrag";
  import type { Playlist, RadioDirectoryStation, RadioStationListSpec } from "../../types";
  import CardSection from "../CardSection.svelte";
  import DragGhost from "../DragGhost.svelte";
  import RadioStationCard from "./RadioStationCard.svelte";
  import PinnedStationRowCard from "./PinnedStationRowCard.svelte";
  import RadioSaveButton from "./RadioSaveButton.svelte";
  import StationFavicon from "./StationFavicon.svelte";
  import PlaylistCoverThumb from "../PlaylistCoverThumb.svelte";
  import StationListModal from "./StationListModal.svelte";

  const appearance = useCardAppearance();

  let scrollEl: HTMLElement | null = $state(null);
  let showStationListModal = $state(false);
  let editingList = $state<{ id: number; name: string; spec: RadioStationListSpec } | null>(null);

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

  const dynamicStationLists = $derived.by(() =>
    playlistsStore.playlists.filter((pl) => isStationList(pl)),
  );

  const manualRadioPlaylists = $derived.by(() => {
    const ids = new Set(radioStore.radioPlaylistIds);
    return playlistsStore.customPlaylists.filter((pl) => ids.has(pl.id) && !isStationList(pl));
  });

  const updatedAgoLabel = $derived(formatUpdatedAgo(radioStore.status?.lastSyncedAt ?? null));

  function formatSpecSummary(pl: Playlist): string {
    const spec = parseStationListSpec(pl.dynamic_spec);
    if (!spec) return "";
    const parts: string[] = [];
    if (spec.tags.length > 0) parts.push(spec.tags.join(", "));
    if (spec.country) parts.push(spec.country);
    if (spec.lang) parts.push(spec.lang);
    if (spec.minBitrate) parts.push(`${spec.minBitrate}+ kbps`);
    return parts.join(" · ");
  }

  function formatStationSubtitle(station: RadioDirectoryStation): string {
    const parts: string[] = [];
    if (station.countrycode || station.country) {
      parts.push(station.countrycode || station.country);
    }
    if (station.codec) {
      parts.push(station.codec);
    }
    if (station.tags) {
      const firstTags = station.tags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean)
        .slice(0, 2)
        .join(", ");
      if (firstTags) parts.push(firstTags);
    }
    return parts.join(" · ");
  }

  function openNewStationList() {
    editingList = null;
    showStationListModal = true;
  }

  function openEditStationList(pl: Playlist) {
    const spec = parseStationListSpec(pl.dynamic_spec);
    if (!spec) return;
    editingList = { id: pl.id, name: pl.name, spec };
    showStationListModal = true;
  }

  function onRowKeydown(e: KeyboardEvent, action: () => void) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      action();
    }
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
  class="@container flex-1 min-h-0 overflow-y-auto pt-2 pb-dock space-y-12 pr-1"
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
            {@const playing = radioStore.isPlayingStation(station.stationuuid)}
            {@const subtitle = formatStationSubtitle(station)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              role="button"
              tabindex="0"
              onclick={() => void radioStore.playStation(station)}
              onkeydown={(e) => onRowKeydown(e, () => void radioStore.playStation(station))}
              class={rowClasses(appearance)}
            >
              <div class="relative shrink-0 overflow-hidden">
                <StationFavicon favicon={station.favicon} sizeClass="w-11 h-11" />
              </div>

              <div class="min-w-0 flex-1 flex flex-col gap-0.5">
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">
                    {station.name || i18n.t("radio.unnamedStation")}
                  </p>
                  {#if station.bitrate > 0}
                    <span
                      class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0"
                    >
                      {station.bitrate} kbps
                    </span>
                  {/if}
                </div>
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">
                    {subtitle}
                  </p>
                  <div
                    class="flex items-center gap-1 shrink-0"
                    onclick={(e) => e.stopPropagation()}
                  >
                    <RadioSaveButton stationuuid={station.stationuuid} size="sm" />
                    <button
                      type="button"
                      onclick={() => void radioStore.playStation(station)}
                      aria-label={playing
                        ? i18n.t("radio.playingLabel", { name: station.name })
                        : i18n.t("radio.playStation", { name: station.name })}
                      class="p-1 rounded-md text-brand-text-secondary hover:text-brand-accent-text transition-colors"
                    >
                      {#if playing}
                        <Pause class="w-3.5 h-3.5" weight="fill" />
                      {:else}
                        <Play class="w-3.5 h-3.5" weight="fill" />
                      {/if}
                    </button>
                  </div>
                </div>
              </div>
            </div>
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
          {#if updatedAgoLabel && dynamicStationLists.length > 0}
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

      {#if dynamicStationLists.length === 0 && manualRadioPlaylists.length === 0}
        <div
          class="p-5 rounded-xl border border-brand-border/40 bg-brand-surface/40 text-center space-y-1.5"
        >
          <p class="text-sm text-brand-text-secondary">
            {i18n.t("radio.noStationLists")}
          </p>
        </div>
      {:else}
        <div class="flex-1 flex flex-col {rowListGap(appearance)}">
          {#each dynamicStationLists as pl (pl.id)}
            {@const summary = formatSpecSummary(pl)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              role="button"
              tabindex="0"
              data-testid="station-list-card"
              onclick={() => navigationStore.viewPlaylist(pl.id)}
              onkeydown={(e) => onRowKeydown(e, () => navigationStore.viewPlaylist(pl.id))}
              class={rowClasses(appearance)}
            >
              <div class="relative shrink-0 overflow-hidden">
                <PlaylistCoverThumb playlist={pl} sizeClass="w-11 h-11" />
              </div>

              <div class="min-w-0 flex-1 flex flex-col gap-0.5">
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">
                    {pl.name}
                  </p>
                  <span class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0">
                    {pl.track_count}
                    {i18n.t("radio.stationsCount")}
                  </span>
                </div>
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">
                    {summary}
                  </p>
                  <div
                    class="flex items-center gap-0.5 shrink-0"
                    onclick={(e) => e.stopPropagation()}
                  >
                    <button
                      type="button"
                      onclick={() => void playerStore.playPlaylistItem(pl.id, 0)}
                      aria-label={i18n.t("playlists.playPlaylistTooltip")}
                      title={i18n.t("playlists.playPlaylistTooltip")}
                      disabled={pl.track_count === 0}
                      class="p-1 rounded-md text-brand-text-secondary hover:text-brand-accent-text disabled:opacity-40 transition-colors"
                    >
                      <Play class="w-3.5 h-3.5" weight="fill" />
                    </button>
                    <button
                      type="button"
                      onclick={() => openEditStationList(pl)}
                      aria-label={i18n.t("radio.editStationList")}
                      title={i18n.t("radio.editStationList")}
                      class="p-1 rounded-md text-brand-text-secondary hover:text-brand-text-primary transition-colors"
                    >
                      <Pencil class="w-3.5 h-3.5" />
                    </button>
                    <button
                      type="button"
                      onclick={() => void radioStore.deleteStationList(pl.id)}
                      aria-label={i18n.t("playlists.delete")}
                      title={i18n.t("playlists.delete")}
                      class="p-1 rounded-md text-brand-text-secondary hover:text-brand-text-primary transition-colors"
                    >
                      <Trash class="w-3.5 h-3.5" />
                    </button>
                  </div>
                </div>
              </div>
            </div>
          {/each}

          {#each manualRadioPlaylists as pl (pl.id)}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <div
              role="button"
              tabindex="0"
              onclick={() => navigationStore.viewPlaylist(pl.id)}
              onkeydown={(e) => onRowKeydown(e, () => navigationStore.viewPlaylist(pl.id))}
              class={rowClasses(appearance)}
            >
              <div class="relative shrink-0 overflow-hidden">
                <PlaylistCoverThumb playlist={pl} sizeClass="w-11 h-11" />
              </div>
              <div class="min-w-0 flex-1 flex flex-col gap-0.5">
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">
                    {pl.name}
                  </p>
                  <span class="text-xs text-brand-text-secondary font-medium shrink-0">
                    {i18n.t("radio.openList")}
                  </span>
                </div>
                <p class="truncate text-xs text-brand-text-secondary font-medium">
                  {i18n.t("sidebar.playlistsCustom")}
                </p>
              </div>
            </div>
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
    editing={editingList}
    onClose={() => {
      showStationListModal = false;
      editingList = null;
    }}
  />
{/if}
