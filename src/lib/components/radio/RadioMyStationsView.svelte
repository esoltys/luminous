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

  let scrollEl: HTMLElement | null = $state(null);

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
  class="@container flex-1 min-h-0 overflow-y-auto space-y-6 pr-1"
>
  <!-- Saved stations -->
  <section class="space-y-2.5">
    <div class="flex items-center justify-between">
      <h2 class="label-heading text-brand-text-primary flex items-center gap-2">
        <Bookmark class="w-4 h-4 text-brand-accent-text" />
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
      <div class="space-y-1.5">
        {#each radioStore.savedStations as station (station.stationuuid)}
          <RadioStationRow
            {station}
            saved={true}
            playing={radioStore.isPlayingStation(station.stationuuid)}
            onPlay={(s) => void radioStore.playStation(s)}
            onToggleSave={(s) => void radioStore.toggleSaveStation(s)}
          />
        {/each}
      </div>
    {/if}
  </section>

  <!-- Recently played stations -->
  <section class="space-y-2.5">
    <h2 class="label-heading text-brand-text-primary flex items-center gap-2">
      <History class="w-4 h-4 text-brand-text-secondary" />
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

  <!-- Station lists (custom playlists) -->
  {#if playlistsStore.customPlaylists.length > 0}
    <section class="space-y-2.5">
      <h2 class="label-heading text-brand-text-primary flex items-center gap-2">
        <ListMusic class="w-4 h-4 text-brand-text-secondary" />
        <span>{i18n.t("radio.stationListsTitle")}</span>
      </h2>
      <div class="grid grid-cols-1 @md:grid-cols-2 @3xl:grid-cols-3 gap-2">
        {#each playlistsStore.customPlaylists as pl (pl.id)}
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
