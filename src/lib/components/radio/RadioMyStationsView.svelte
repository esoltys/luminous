<script lang="ts">
  import { onMount } from "svelte";
  import {
    BookmarkSimpleIcon as Bookmark,
    ClockCounterClockwiseIcon as History,
    PlaylistIcon as ListMusic,
  } from "phosphor-svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import { playlistsStore } from "../../stores/playlists.svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import RadioStationRow from "./RadioStationRow.svelte";
  import RadioStationCard from "./RadioStationCard.svelte";

  let scrollEl: HTMLElement | null = $state(null);

  const radioPlaylists = $derived.by(() => {
    const ids = new Set(radioStore.radioPlaylistIds);
    return playlistsStore.customPlaylists.filter((pl) => ids.has(pl.id));
  });

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
    <div class="flex items-center justify-between">
      <h2 class="flex items-center gap-2 text-xl font-semibold text-brand-text-primary">
        <Bookmark class="w-5 h-5 text-brand-accent-text" />
        <span>{i18n.t("radio.savedStationsTitle")}</span>
      </h2>
      <span class="text-xs text-brand-text-secondary">
        {radioStore.savedStations.length}
      </span>
    </div>

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

  <!-- Station lists (custom playlists containing radio stations) -->
  {#if radioPlaylists.length > 0}
    <section class="space-y-3">
      <h2 class="flex items-center gap-2 text-xl font-semibold text-brand-text-primary">
        <ListMusic class="w-5 h-5 text-brand-text-secondary" />
        <span>{i18n.t("radio.stationListsTitle")}</span>
      </h2>
      <div class="grid grid-cols-1 @md:grid-cols-2 @3xl:grid-cols-3 gap-2">
        {#each radioPlaylists as pl (pl.id)}
          <button
            type="button"
            onclick={() => navigationStore.viewPlaylist(pl.id)}
            class="flex items-center justify-between px-3 py-2.5 rounded-xl border border-brand-border/40 bg-brand-surface/40 hover:bg-brand-surface/80 text-left transition-colors"
          >
            <span class="text-sm font-medium text-brand-text-primary truncate">{pl.name}</span>
            <span class="text-xs text-brand-text-secondary shrink-0 ml-2">
              {i18n.t("radio.openList")}
            </span>
          </button>
        {/each}
      </div>
    </section>
  {/if}
</div>
