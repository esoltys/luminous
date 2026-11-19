<script lang="ts">
  import { onMount } from "svelte";
  import {
    BookmarkSimpleIcon as Bookmark,
    ClockCounterClockwiseIcon as History,
    PlaylistIcon as ListMusic,
    PlayIcon as Play,
    PencilSimpleIcon as Pencil,
    TrashIcon as Trash,
  } from "phosphor-svelte";
  import Plus from "../PlusIcon.svelte";
  import { formatUpdatedAgo, radioStore } from "../../stores/radio.svelte";
  import { playlistsStore } from "../../stores/playlists.svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { isStationList, parseStationListSpec } from "../../utils/playlist";
  import type { Playlist, RadioStationListSpec } from "../../types";
  import RadioStationRow from "./RadioStationRow.svelte";
  import RadioStationCard from "./RadioStationCard.svelte";
  import StationListModal from "./StationListModal.svelte";

  let scrollEl: HTMLElement | null = $state(null);
  let showStationListModal = $state(false);
  let editingList = $state<{ id: number; name: string; spec: RadioStationListSpec } | null>(null);

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
  <section class="space-y-3">
    <h2 class="flex items-center gap-2 text-xl font-semibold text-brand-text-primary">
      <Bookmark class="w-5 h-5 text-brand-accent-text" />
      <span>{i18n.t("radio.savedStationsTitle")}</span>
    </h2>

    {#if radioStore.savedStations.length === 0}
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
      <div
        class="grid grid-cols-2 @md:grid-cols-3 @2xl:grid-cols-4 @4xl:grid-cols-5 @6xl:grid-cols-6 gap-4"
      >
        {#each radioStore.savedStations as station (station.stationuuid)}
          <RadioStationCard
            {station}
            playing={radioStore.isPlayingStation(station.stationuuid)}
            onPlay={(s) => void radioStore.playStation(s)}
            onToggleSave={(s) => void radioStore.toggleSaveStation(s)}
          />
        {/each}
      </div>
    {/if}
  </section>

  <!-- Recently played stations -->
  <section class="space-y-3">
    <h2 class="flex items-center gap-2 text-xl font-semibold text-brand-text-primary">
      <History class="w-5 h-5 text-brand-text-secondary" />
      <span>{i18n.t("radio.recentStationsTitle")}</span>
    </h2>

    {#if radioStore.recentStations.length === 0}
      <p class="text-xs text-brand-text-secondary px-1">
        {i18n.t("radio.noRecentStations")}
      </p>
    {:else}
      <div class="space-y-1.5">
        {#each radioStore.recentStations as station (station.stationuuid)}
          <RadioStationRow
            {station}
            saved={radioStore.isSaved(station.stationuuid)}
            playing={radioStore.isPlayingStation(station.stationuuid)}
            onPlay={(s) => void radioStore.playStation(s)}
            onToggleSave={(s) => void radioStore.toggleSaveStation(s)}
          />
        {/each}
      </div>
    {/if}
  </section>

  <!-- Station lists (auto-updating genre lists + manual radio playlists) -->
  <section class="space-y-3" data-testid="radio-station-lists-section">
    <div class="flex items-center justify-between gap-3">
      <div class="flex items-center gap-2 min-w-0">
        <ListMusic class="w-5 h-5 text-brand-text-secondary shrink-0" />
        <h2 class="text-xl font-semibold text-brand-text-primary truncate">
          {i18n.t("radio.stationListsTitle")}
        </h2>
        {#if updatedAgoLabel && dynamicStationLists.length > 0}
          <span
            class="text-xs text-brand-text-secondary shrink-0"
            data-testid="station-lists-updated-ago"
          >
            · {updatedAgoLabel}
          </span>
        {/if}
      </div>
      {#if radioStore.status?.optedIn}
        <button
          type="button"
          data-testid="new-station-list-btn"
          onclick={openNewStationList}
          class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-brand-accent/15 text-brand-accent-text text-xs font-medium hover:bg-brand-accent/25 shrink-0"
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
      <div class="grid grid-cols-1 @md:grid-cols-2 @3xl:grid-cols-3 gap-2.5">
        {#each dynamicStationLists as pl (pl.id)}
          {@const summary = formatSpecSummary(pl)}
          <div
            data-testid="station-list-card"
            class="flex items-center justify-between gap-2 px-3.5 py-3 rounded-xl border border-brand-border/40 bg-brand-surface/40 hover:bg-brand-surface/80 transition-colors"
          >
            <button
              type="button"
              onclick={() => navigationStore.viewPlaylist(pl.id)}
              class="flex-1 min-w-0 text-left"
            >
              <div class="text-sm font-medium text-brand-text-primary truncate">{pl.name}</div>
              <div class="text-xs text-brand-text-secondary truncate">
                {pl.track_count}
                {i18n.t("radio.stationsCount")}{summary ? ` · ${summary}` : ""}
              </div>
            </button>
            <div class="flex items-center gap-1 shrink-0">
              <button
                type="button"
                onclick={() => void playerStore.playPlaylistItem(pl.id, 0)}
                aria-label={i18n.t("playlists.playPlaylistTooltip")}
                title={i18n.t("playlists.playPlaylistTooltip")}
                disabled={pl.track_count === 0}
                class="p-1.5 rounded-lg text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-surface-hover disabled:opacity-40"
              >
                <Play class="w-4 h-4" />
              </button>
              <button
                type="button"
                onclick={() => openEditStationList(pl)}
                aria-label={i18n.t("radio.editStationList")}
                title={i18n.t("radio.editStationList")}
                class="p-1.5 rounded-lg text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-surface-hover"
              >
                <Pencil class="w-4 h-4" />
              </button>
              <button
                type="button"
                onclick={() => void radioStore.deleteStationList(pl.id)}
                aria-label={i18n.t("playlists.delete")}
                title={i18n.t("playlists.delete")}
                class="p-1.5 rounded-lg text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-surface-hover"
              >
                <Trash class="w-4 h-4" />
              </button>
            </div>
          </div>
        {/each}

        {#each manualRadioPlaylists as pl (pl.id)}
          <button
            type="button"
            onclick={() => navigationStore.viewPlaylist(pl.id)}
            class="flex items-center justify-between px-3.5 py-3 rounded-xl border border-brand-border/40 bg-brand-surface/40 hover:bg-brand-surface/80 text-left transition-colors"
          >
            <span class="text-sm font-medium text-brand-text-primary truncate">{pl.name}</span>
            <span class="text-xs text-brand-text-secondary shrink-0 ml-2">
              {i18n.t("radio.openList")}
            </span>
          </button>
        {/each}
      </div>
    {/if}
  </section>
</div>

{#if showStationListModal}
  <StationListModal
    editing={editingList}
    onClose={() => {
      showStationListModal = false;
      editingList = null;
    }}
  />
{/if}
