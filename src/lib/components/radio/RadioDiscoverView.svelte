<script lang="ts">
  import { onMount } from "svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import {
    MagnifyingGlassIcon as Search,
    XIcon as X,
    BroadcastIcon as Broadcast,
  } from "phosphor-svelte";
  import { radioStore } from "../../stores/radio.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import type { RadioDirectoryOrder, RadioDirectoryStation } from "../../types";
  import Select from "../Select.svelte";
  import RadioStationRow from "./RadioStationRow.svelte";

  let scrollContainerEl: HTMLElement | null = $state(null);

  function unwrapRow(raw: unknown): RadioDirectoryStation {
    if (raw && typeof raw === "object" && "item" in raw) {
      return (raw as { item: RadioDirectoryStation }).item;
    }
    return raw as RadioDirectoryStation;
  }

  const hasActiveFilters = $derived(
    Boolean(
      radioStore.searchQuery.trim() ||
      radioStore.selectedCountry ||
      radioStore.selectedLanguage ||
      radioStore.selectedCodec,
    ),
  );

  onMount(() => {
    const savedScroll = radioStore.getScrollTop("discover");
    if (scrollContainerEl && savedScroll > 0) {
      scrollContainerEl.scrollTop = savedScroll;
    }
  });
</script>

<div class="@container flex flex-col h-full min-h-0 gap-3" data-testid="radio-discover-view">
  <!-- Search & filter bar -->
  <div class="flex flex-wrap items-center gap-2 shrink-0">
    <div class="relative flex-1 min-w-[12rem]">
      <Search
        class="w-4 h-4 text-brand-text-secondary absolute left-3 top-1/2 -translate-y-1/2 pointer-events-none"
      />
      <input
        type="text"
        data-testid="radio-search-input"
        value={radioStore.searchQuery}
        oninput={(e) => radioStore.setSearchQuery(e.currentTarget.value)}
        placeholder={i18n.t("radio.searchPlaceholder")}
        class="w-full pl-9 pr-8 py-1.5 rounded-lg bg-brand-surface border border-brand-border/60 text-sm text-brand-text-primary placeholder:text-brand-text-secondary/70 focus:outline-none focus:border-brand-accent"
      />
      {#if radioStore.searchQuery}
        <button
          type="button"
          onclick={() => radioStore.setSearchQuery("", true)}
          aria-label={i18n.t("radio.clearSearch")}
          class="absolute right-2 top-1/2 -translate-y-1/2 text-brand-text-secondary hover:text-brand-text-primary"
        >
          <X class="w-3.5 h-3.5" />
        </button>
      {/if}
    </div>

    <Select
      ariaLabel={i18n.t("radio.sortBy")}
      value={radioStore.orderBy}
      onchange={(e) => {
        radioStore.orderBy = e.currentTarget.value as RadioDirectoryOrder;
      }}
      class="px-2.5 pr-8 py-1.5 rounded-lg bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-xs text-brand-text-primary focus:outline-none focus:border-brand-accent"
    >
      <option value="clickcount">{i18n.t("radio.sortPopularity")}</option>
      <option value="votes">{i18n.t("radio.sortVotes")}</option>
      <option value="name">{i18n.t("radio.sortName")}</option>
      <option value="bitrate">{i18n.t("radio.sortBitrate")}</option>
    </Select>

    <Select
      ariaLabel={i18n.t("radio.filterCountry")}
      value={radioStore.selectedCountry ?? ""}
      onchange={(e) => radioStore.setFilter("country", e.currentTarget.value || null)}
      class="px-2.5 pr-8 py-1.5 rounded-lg bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-xs text-brand-text-primary max-w-[11rem] focus:outline-none focus:border-brand-accent"
    >
      <option value="">{i18n.t("radio.allCountries")}</option>
      {#each radioStore.facets.countries as c (c.code ?? c.name)}
        <option value={c.code ?? c.name}>
          {c.name} ({c.stationcount})
        </option>
      {/each}
    </Select>

    <Select
      ariaLabel={i18n.t("radio.filterLanguage")}
      value={radioStore.selectedLanguage ?? ""}
      onchange={(e) => radioStore.setFilter("language", e.currentTarget.value || null)}
      class="hidden @md:block px-2.5 pr-8 py-1.5 rounded-lg bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-xs text-brand-text-primary max-w-[10rem] focus:outline-none focus:border-brand-accent"
    >
      <option value="">{i18n.t("radio.allLanguages")}</option>
      {#each radioStore.facets.languages as lang (lang.name)}
        <option value={lang.name}>
          {lang.name} ({lang.stationcount})
        </option>
      {/each}
    </Select>

    <Select
      ariaLabel={i18n.t("radio.filterCodec")}
      value={radioStore.selectedCodec ?? (radioStore.hideUnsupported ? "" : "__all__")}
      onchange={(e) => {
        const val = e.currentTarget.value;
        if (val === "__all__") {
          radioStore.setCodecFilter(null, false);
        } else if (val === "") {
          radioStore.setCodecFilter(null, true);
        } else {
          radioStore.setCodecFilter(val, radioStore.hideUnsupported);
        }
      }}
      class="px-2.5 pr-8 py-1.5 rounded-lg bg-brand-sidebar border border-brand-border hover:border-brand-accent/60 text-xs text-brand-text-primary max-w-[12rem] focus:outline-none focus:border-brand-accent"
    >
      <option value="">{i18n.t("radio.allCodecs")}</option>
      <option value="__all__">{i18n.t("radio.includeUnsupportedCodecs")}</option>
      {#each radioStore.facets.codecs as codec (codec.name)}
        <option value={codec.name}>
          {codec.name} ({codec.stationcount})
        </option>
      {/each}
    </Select>

    {#if hasActiveFilters}
      <button
        type="button"
        onclick={() => radioStore.clearFilters()}
        class="px-2.5 py-1.5 rounded-lg text-xs text-brand-accent-text hover:bg-brand-accent/10 transition-colors"
      >
        {i18n.t("radio.clearFilters")}
      </button>
    {/if}
  </div>

  <!-- Station results -->
  <div
    bind:this={scrollContainerEl}
    onscroll={(e) => radioStore.setScrollTop("discover", e.currentTarget.scrollTop)}
    class="flex-1 min-h-0 overflow-y-auto"
  >
    {#if radioStore.discoverLoading && radioStore.discoverStations.length === 0}
      <div class="py-12 text-center text-sm text-brand-text-secondary">
        {i18n.t("radio.loadingStations")}
      </div>
    {:else if radioStore.discoverStations.length === 0}
      <div
        data-testid="radio-discover-empty"
        class="py-12 text-center space-y-3 text-brand-text-secondary"
      >
        <Broadcast class="w-8 h-8 mx-auto opacity-50" />
        <p class="text-sm">{i18n.t("radio.noStationsMatch")}</p>
        {#if hasActiveFilters}
          <button
            type="button"
            onclick={() => radioStore.clearFilters()}
            class="px-3.5 py-1.5 rounded-lg bg-brand-accent/15 text-brand-accent-text text-xs font-medium hover:bg-brand-accent/25"
          >
            {i18n.t("radio.clearFilters")}
          </button>
        {/if}
      </div>
    {:else}
      <VirtualList items={radioStore.discoverStations} height="100%" itemHeight={68}>
        {#snippet children(rawRow: unknown)}
          {@const station = unwrapRow(rawRow)}
          <div class="pb-1.5 pr-1">
            <RadioStationRow
              {station}
              saved={radioStore.isSaved(station.stationuuid)}
              playing={radioStore.isPlayingStation(station.stationuuid)}
              onPlay={(s) => void radioStore.playStation(s)}
              onToggleSave={(s) => void radioStore.toggleSaveStation(s)}
              onTagClick={(tag) => radioStore.setSearchQuery(tag, true)}
            />
          </div>
        {/snippet}
      </VirtualList>
    {/if}
  </div>
</div>
