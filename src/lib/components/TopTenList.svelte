<script lang="ts">
  import type { StatsTopItem, Song, AlbumItem } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import AlbumRowCard from "./AlbumRowCard.svelte";
  import ArtistRowCard from "./ArtistRowCard.svelte";
  import { getArtistAlbums, getArtistSongs } from "../utils/artist";
  import { i18n } from "../stores/i18n.svelte";
  import {
    CaretRightIcon as ChevronRight,
    TrendUpIcon,
    TrendDownIcon,
    AsteriskIcon,
    MinusIcon,
    ShareNetworkIcon as Share,
  } from "phosphor-svelte";

  interface Props {
    title?: string;
    items: StatsTopItem[];
    kind: "album" | "song" | "artist" | "genre";
    emptyText?: string;
    secondaryFallback?: string;
    /** Hides the plays/minutes trailing column, for compact contexts (e.g. Home). */
    showDuration?: boolean;
    /** When provided, the title becomes a clickable button that navigates to
     * the full expanded view. */
    onHeaderClick?: () => void;
    /** When provided, shows a Share Card button next to the title (used by
     * StatsView to share a single Top N category on its own — not shown
     * elsewhere TopTenList appears, e.g. Home). */
    onShareClick?: () => void;
  }

  let { title, items, kind, emptyText, secondaryFallback, showDuration = true, onHeaderClick, onShareClick }: Props = $props();

  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  function openItem(item: StatsTopItem) {
    if (kind === "artist") {
      navigationStore.viewArtist(item.label);
    } else if (kind === "album") {
      navigationStore.viewAlbum(item.label);
    } else if (kind === "song") {
      if (item.album) {
        navigationStore.viewAlbum(item.album);
      } else if (item.song_id) {
        playerStore.playSong(item.song_id);
      }
    } else if (kind === "genre") {
      navigationStore.viewGenreTag(item.label);
    }
  }

  function handleContextMenu(e: MouseEvent, item: StatsTopItem) {
    if (kind !== "song" || !item.song_id) return;
    e.preventDefault();
    const song: Song = {
      id: item.song_id,
      source: "local_file",
      filetype: "UNKNOWN",
      title: item.label,
      artist: item.secondary ?? undefined,
      album: item.album ?? undefined,
      art_embedded: item.art_embedded ?? false,
      art_automatic: item.art_automatic ?? undefined,
      art_manual: item.art_manual ?? undefined,
      art_unset: false,
      compilation: false,
      beginning_nanosec: 0,
      end_nanosec: 0,
      rating: item.rating ?? -1,
      playcount: item.play_count,
      skipcount: 0,
      year: item.year ?? undefined,
      unavailable: false,
    };
    contextMenuState = { x: e.clientX, y: e.clientY, song };
  }

  async function rateSong(item: StatsTopItem, rating: number) {
    if (!item.song_id) return;
    item.rating = await invoke<number>("set_song_rating", { songId: item.song_id, rating });
  }

  /** Tooltip for the weekly chart movement icon (#662): trend and weeks on chart
   * (peak rank is shown directly under the rank number, so it's left out here). */
  function movementTooltip(item: StatsTopItem): string {
    const movementLabels: Record<NonNullable<StatsTopItem["movement"]>, string> = {
      new: i18n.t("home.chartNew", {}, "New"),
      rising: i18n.t("home.chartRising", {}, "Rising"),
      falling: i18n.t("home.chartFalling", {}, "Falling"),
      steady: i18n.t("home.chartSteady", {}, "Steady"),
    };
    const weeks = item.weeks_on_chart ?? 1;
    const weeksLabel = weeks === 1
      ? i18n.t("home.chartWeek", {}, "1 week")
      : i18n.t("home.chartWeeksCount", { weeks }, `${weeks} weeks`);
    return `${movementLabels[item.movement ?? "steady"]} · ${weeksLabel}`;
  }
</script>

{#snippet rankSnippet(item: StatsTopItem, rank: number)}
  <div class="w-8 shrink-0 flex flex-col items-center">
    <span class="text-center text-sm font-bold text-brand-text-secondary tabular-nums">
      {String(rank).padStart(2, "0")}
    </span>
    {#if item.movement}
      <span class="text-center text-[10px] font-normal text-brand-text-secondary/60 tabular-nums">
        {item.movement === "new" ? "—" : i18n.t("home.chartPeak", { peak: item.peak_rank ?? rank }, `Peak #${item.peak_rank ?? rank}`)}
      </span>
    {/if}
  </div>
{/snippet}

{#snippet movementSnippet(item: StatsTopItem)}
  <div class="w-5 shrink-0 flex items-center justify-center" title={movementTooltip(item)}>
    {#if item.movement === "rising"}
      <TrendUpIcon class="w-4 h-4 text-green-400" weight="bold" />
    {:else if item.movement === "falling"}
      <TrendDownIcon class="w-4 h-4 text-red-400" weight="bold" />
    {:else if item.movement === "new"}
      <AsteriskIcon class="w-4 h-4 text-brand-accent-text" weight="bold" />
    {:else}
      <MinusIcon class="w-4 h-4 text-brand-text-secondary" />
    {/if}
  </div>
{/snippet}

{#snippet durationSnippet(item: StatsTopItem)}
  <div class="shrink-0 flex flex-col items-end justify-center text-right">
    <span
      class="text-xs font-medium text-brand-text-secondary tabular-nums"
      title={item.play_count === 1
        ? i18n.t("stats.playsCountOne", {}, "1 play")
        : i18n.t("stats.playsCount", { count: item.play_count }, `${item.play_count} plays`)}
    >
      {item.minutes === 0 && item.play_count > 0
        ? i18n.t("stats.minuteUnderOne", {}, "< 1 min")
        : i18n.t("stats.minuteCount", { count: item.minutes.toLocaleString() }, `${item.minutes.toLocaleString()} min`)}
    </span>
  </div>
{/snippet}

<div class="h-full flex flex-col gap-4">
  {#if title}
    <div class="flex items-center justify-between gap-2">
      {#if onHeaderClick}
        <button
          type="button"
          onclick={onHeaderClick}
          class="group flex items-center gap-1 text-xl font-semibold text-brand-text-primary hover:text-brand-accent-text transition-colors"
        >
          {title}
          <ChevronRight class="w-5 h-5 opacity-0 group-hover:opacity-100 transition-opacity" />
        </button>
      {:else}
        <h2 class="text-xl font-semibold text-brand-text-primary">{title}</h2>
      {/if}
      {#if onShareClick}
        <button
          type="button"
          onclick={onShareClick}
          title={i18n.t("shareModal.menuItem")}
          class="flex items-center justify-center w-8 h-8 rounded-full text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-main transition-colors cursor-pointer shrink-0"
        >
          <Share class="w-4 h-4" />
        </button>
      {/if}
    </div>
  {/if}

  <div class="flex-1 flex flex-col gap-2">
    {#each items as item, i (item.key)}
      <div class="flex items-center gap-3">
        {#if item.movement}
          {@render movementSnippet(item)}
        {/if}
        {@render rankSnippet(item, i + 1)}

        <div class="min-w-0 flex-1">
          {#if kind === "album"}
            {@const albumItem: AlbumItem = {
              album: item.label,
              artist: item.secondary || secondaryFallback || null,
              year: item.year ?? null,
              sample_song_id: item.sample_song_id,
              art_embedded: item.art_embedded ?? false,
              art_automatic: item.art_automatic ?? null,
              art_manual: item.art_manual ?? null,
              rating: item.rating ?? -1,
              track_count: 0,
              disc_count: 0,
              total_duration_nanosec: 0,
            }}
            <AlbumRowCard
              album={albumItem}
              onclick={() => openItem(item)}
            />
          {:else if kind === "artist"}
            {@const artist = collectionStore.artists.find((a) => a.name === item.label) ?? {
              name: item.label,
              album_count: 0,
              song_count: 0,
              genre: item.secondary || undefined,
            }}
            {@const artistAlbums = getArtistAlbums(collectionStore.albums, artist.name)}
            {@const artistSongs = getArtistSongs(collectionStore.songs, artist.name)}
            <ArtistRowCard
              {artist}
              {artistAlbums}
              {artistSongs}
              onclick={() => openItem(item)}
            />
          {:else if kind === "song"}
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <div
              role="button"
              tabindex="0"
              onclick={() => openItem(item)}
              oncontextmenu={(e) => handleContextMenu(e, item)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  openItem(item);
                }
              }}
              class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none cursor-pointer w-full"
            >
              <div class="relative shrink-0 overflow-hidden">
                <CoverArt
                  songId={item.song_id ?? undefined}
                  artEmbedded={item.art_embedded}
                  artAutomatic={item.art_automatic}
                  artManual={item.art_manual}
                  sizeClass="w-11 h-11"
                />
                {#if item.rating === 5}
                  <FavouriteCornerFlag size="sm" />
                {/if}
              </div>

              <div class="min-w-0 flex-1 flex flex-col gap-0.5">
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">{item.label}</p>
                  {#if item.year}
                    <span class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0">{item.year}</span>
                  {/if}
                </div>
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">{item.secondary || secondaryFallback || ""}</p>
                  <span class="shrink-0" onclick={(e) => e.stopPropagation()}>
                    <SongRating rating={item.rating ?? -1} onRate={(r) => rateSong(item, r)} size="sm" />
                  </span>
                </div>
              </div>
            </div>
          {:else}
            <!-- genre -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <!-- svelte-ignore a11y_no_static_element_interactions -->
            <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
            <div
              role="button"
              tabindex="0"
              onclick={() => openItem(item)}
              onkeydown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  openItem(item);
                }
              }}
              class="group flex items-center min-h-[66px] px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none cursor-pointer w-full"
            >
              <div class="min-w-0 flex-1 flex flex-col gap-0.5">
                <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">{item.label}</p>
                {#if item.secondary || secondaryFallback}
                  <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">{item.secondary || secondaryFallback}</p>
                {/if}
              </div>
            </div>
          {/if}
        </div>

        {#if showDuration}
          {@render durationSnippet(item)}
        {/if}
      </div>
    {/each}

    {#if items.length === 0}
      <p class="text-sm text-brand-text-secondary px-3 py-6 text-center">{emptyText ?? i18n.t('stats.noData', {}, 'No data for this range.')}</p>
    {/if}
  </div>
</div>

{#if contextMenuState}
  {@const song = contextMenuState.song}
  <SongContextMenu
    x={contextMenuState.x}
    y={contextMenuState.y}
    {song}
    onPlay={() => playerStore.playSong(song.id)}
    onGoToArtist={() => navigationStore.viewArtist(song.album_artist?.trim() || song.artist || "")}
    onGoToAlbum={() => navigationStore.viewAlbum(song.album || "")}
    onClose={() => { contextMenuState = null; }}
  />
{/if}
