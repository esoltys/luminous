<script lang="ts">
  import { sameAlbum } from "$lib/utils/albumKey";
  import type { StatsTopItem, Song, AlbumItem } from "../types";
  import { statsBarPercents } from "../utils/statsBars";
  import { playerStore } from "../stores/player.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { AlbumStatsPayload, SongStatsPayload } from "../utils/stats";
  import CoverArt from "./CoverArt.svelte";
  import SongRating from "./SongRating.svelte";
  import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import AlbumRowCard from "./AlbumRowCard.svelte";
  import ArtistRowCard from "./ArtistRowCard.svelte";
  import { getArtistAlbums, getArtistSongs } from "../utils/artist";
  import { i18n, formatNumber } from "../stores/i18n.svelte";
  import { CaretRightIcon as ChevronRight, ShareNetworkIcon as Share } from "phosphor-svelte";
  import { rowClasses, useCardAppearance } from "../utils/cardAppearance";

  const plain = useCardAppearance() === "plain";

  interface Props {
    title?: string;
    /** Quieter text after the title (e.g. the week's date range on Home). */
    titleSuffix?: string;
    items: StatsTopItem[];
    kind: "album" | "song" | "artist" | "genre";
    emptyText?: string;
    secondaryFallback?: string;
    /** Hides the plays/minutes trailing column, for compact contexts (e.g. Home). */
    showDuration?: boolean;
    /** Whether to show proportional accent colour bars behind ranked items (#1475). */
    showAccentBars?: boolean;
    /** When provided, the title becomes a clickable button that navigates to
     * the full expanded view. */
    onHeaderClick?: () => void;
    /** When provided, shows a Share Card button next to the title (used by
     * StatsView to share a single Top N category on its own — not shown
     * elsewhere TopTenList appears, e.g. Home). */
    onShareClick?: () => void;
  }

  let {
    title,
    titleSuffix,
    items,
    kind,
    emptyText,
    secondaryFallback,
    showDuration = true,
    showAccentBars = false,
    onHeaderClick,
    onShareClick,
  }: Props = $props();

  // Plain album rows carry the rank and the movement word inside the row, so the one
  // hairline under each row spans the whole line, rank included. Every other layout keeps
  // the rank beside the row.
  const rankInsideRow = $derived(plain && kind === "album");

  // Relative scaling for proportional accent bars (#1475)
  const barPercents = $derived(statsBarPercents(items));

  function getItemPercent(item: StatsTopItem): number {
    return showAccentBars ? (barPercents[items.indexOf(item)] ?? 0) : 0;
  }

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
      loved: item.loved,
      playcount: item.play_count,
      skipcount: 0,
      year: item.year ?? undefined,
      unavailable: false,
    };
    contextMenuState = { x: e.clientX, y: e.clientY, song };
  }

  let localLoved = $state<Record<string, number>>({});
  let localRatings = $state<Record<string, number>>({});

  function getItemLoved(item: StatsTopItem): number | undefined {
    return localLoved[item.key] ?? item.loved;
  }

  function getItemRating(item: StatsTopItem): number {
    return localRatings[item.key] ?? item.rating ?? -1;
  }

  async function rateSong(item: StatsTopItem, rating: number) {
    if (!item.song_id) return;
    const normalized = await invoke<number>("set_song_rating", { songId: item.song_id, rating });
    item.rating = normalized;
    localRatings[item.key] = normalized;
  }

  async function setLoved(item: StatsTopItem, loved: number) {
    if (!item.song_id) return;
    const normalized = await invoke<number>("set_song_loved", { songId: item.song_id, loved });
    item.loved = normalized;
    localLoved[item.key] = normalized;
  }

  $effect(() => {
    let unlistenAlbum: (() => void) | undefined;
    let unlistenSong: (() => void) | undefined;
    let disposed = false;

    if (kind === "album") {
      listen<AlbumStatsPayload>("album-stats-changed", (event) => {
        const match = items.find((it) => sameAlbum(it.label, event.payload.album));
        if (match && typeof event.payload.rating === "number") {
          match.rating = event.payload.rating;
          localRatings[match.key] = event.payload.rating;
        }
      }).then((fn) => {
        if (disposed) fn();
        else unlistenAlbum = fn;
      });
    } else if (kind === "song") {
      listen<SongStatsPayload>("song-stats-changed", (event) => {
        const match = items.find((it) => it.song_id === event.payload.song_id);
        if (match) {
          if (typeof event.payload.rating === "number") {
            match.rating = event.payload.rating;
            localRatings[match.key] = event.payload.rating;
          }
          if (typeof event.payload.loved === "number") {
            match.loved = event.payload.loved;
            localLoved[match.key] = event.payload.loved;
          }
        }
      }).then((fn) => {
        if (disposed) fn();
        else unlistenSong = fn;
      });
    }

    return () => {
      disposed = true;
      unlistenAlbum?.();
      unlistenSong?.();
    };
  });

  /** How an album moved on the weekly chart (#662), in words so no meaning rests on a
   * tooltip or an icon. Steady albums (and lists with no chart data) say nothing. */
  function movementLabel(item: StatsTopItem, rank: number): string | null {
    switch (item.movement) {
      case "new":
        return i18n.t("home.chartNewThisWeek", {}, "New this week");
      case "reentry":
        return i18n.t("home.chartReentry", {}, "Re-entry");
      case "rising":
      case "falling": {
        const rising = item.movement === "rising";
        const places = item.previous_rank == null ? 0 : Math.abs(item.previous_rank - rank);
        if (places === 0) {
          return rising
            ? i18n.t("home.chartRising", {}, "Rising")
            : i18n.t("home.chartFalling", {}, "Falling");
        }
        return rising
          ? i18n.t("home.chartUp", { count: places }, `Up ${places}`)
          : i18n.t("home.chartDown", { count: places }, `Down ${places}`);
      }
      default:
        return null;
    }
  }
</script>

{#snippet rankSnippet(rank: number)}
  <span
    class="block shrink-0 text-center font-bold text-brand-text-secondary tabular-nums {plain
      ? 'w-10 text-lg'
      : 'w-8 text-sm'}"
  >
    {plain ? rank : String(rank).padStart(2, "0")}
  </span>
{/snippet}

{#snippet movementSnippet(item: StatsTopItem, rank: number, alignedColumn: boolean = false)}
  {@const label = movementLabel(item, rank)}
  <!-- alignedColumn reserves one fixed width for every row, empty for steady albums, so the
       years beside it line up whatever the message says. The width fits the longest message
       in any locale at this size; longer text is cut short and still readable in the tooltip. -->
  {#if alignedColumn}
    <span
      class="block w-36 shrink-0 truncate text-right text-xs leading-5 font-medium text-brand-text-secondary"
      title={label
        ? `${label} · ${i18n.plural("home.chartWeeksCount", item.weeks_on_chart ?? 1)}`
        : undefined}
    >
      {label ?? ""}
    </span>
  {:else if label}
    <span
      class="shrink-0 text-xs leading-5 font-medium text-brand-text-secondary whitespace-nowrap"
      title={i18n.plural("home.chartWeeksCount", item.weeks_on_chart ?? 1)}
    >
      {label}
    </span>
  {/if}
{/snippet}

{#snippet durationSnippet(item: StatsTopItem)}
  <div class="shrink-0 flex flex-col items-end justify-center text-right">
    <span
      class="text-xs font-medium text-brand-text-secondary tabular-nums"
      title={i18n.plural("stats.playsCount", item.play_count)}
    >
      {item.minutes === 0 && item.play_count > 0
        ? i18n.t("stats.minuteUnderOne", {}, "< 1 min")
        : i18n.t(
            "stats.minuteCount",
            { count: formatNumber(item.minutes) },
            `${formatNumber(item.minutes)} min`,
          )}
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
          {#if titleSuffix}
            <span class="text-sm font-normal text-brand-text-secondary ml-1">{titleSuffix}</span>
          {/if}
          <ChevronRight class="w-5 h-5 opacity-0 group-hover:opacity-100 transition-opacity" />
        </button>
      {:else}
        <h2 class="text-xl font-semibold text-brand-text-primary">
          {title}
          {#if titleSuffix}
            <span class="text-sm font-normal text-brand-text-secondary ml-1">{titleSuffix}</span>
          {/if}
        </h2>
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

  <div class="flex-1 flex flex-col {plain ? '' : 'gap-2'}">
    {#each items as item, i (item.key)}
      {@const itemPercent = getItemPercent(item)}
      <div class="flex items-center gap-3">
        {#if !rankInsideRow}
          {@render rankSnippet(i + 1)}
        {/if}

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
            {#snippet rankPrefix()}
              {@render rankSnippet(i + 1)}
            {/snippet}
            <!-- Rendered on the row's year line, so the word shares the year's baseline whatever
                 height the row's rating slot gives the text column. -->
            {#snippet movementNote()}
              {@render movementSnippet(item, i + 1, true)}
            {/snippet}
            <AlbumRowCard
              album={albumItem}
              prefix={rankInsideRow ? rankPrefix : undefined}
              yearNote={rankInsideRow ? movementNote : undefined}
              progressPercent={showAccentBars ? itemPercent : undefined}
              onclick={() => openItem(item)}
              onRate={(r) => {
                item.rating = r;
              }}
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
              progressPercent={showAccentBars ? itemPercent : undefined}
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
              class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none cursor-pointer w-full relative overflow-hidden"
            >
              {#if showAccentBars && itemPercent > 0}
                <div
                  class="accent-bar absolute inset-y-0 left-0 rounded-lg bg-brand-accent/15 pointer-events-none transition-[width] duration-300 ease-out motion-reduce:transition-none"
                  style="width: {itemPercent}%;"
                  data-testid="stats-accent-bar"
                ></div>
              {/if}

              <div class="relative z-10 shrink-0 overflow-hidden">
                <CoverArt
                  songId={item.song_id ?? undefined}
                  artEmbedded={item.art_embedded}
                  artAutomatic={item.art_automatic}
                  artManual={item.art_manual}
                  sizeClass="w-11 h-11"
                />
                {#if getItemLoved(item) === 1 || (getItemLoved(item) === undefined && getItemRating(item) === 5)}
                  <FavouriteCornerFlag size="sm" />
                {/if}
              </div>

              <div class="relative z-10 min-w-0 flex-1 flex flex-col gap-0.5">
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">
                    {item.label}
                  </p>
                  {#if item.year}
                    <span
                      class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0"
                      >{item.year}</span
                    >
                  {/if}
                </div>
                <div class="flex items-center justify-between gap-2">
                  <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">
                    {item.secondary || secondaryFallback || ""}
                  </p>
                  <span class="shrink-0" onclick={(e) => e.stopPropagation()}>
                    <SongRating
                      rating={getItemRating(item)}
                      loved={getItemLoved(item)}
                      onRate={(r) => rateSong(item, r)}
                      onSetLoved={(l) => setLoved(item, l)}
                      size="sm"
                    />
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
              class="group flex items-center min-h-[66px] px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none cursor-pointer w-full relative overflow-hidden"
            >
              {#if showAccentBars && itemPercent > 0}
                <div
                  class="accent-bar absolute inset-y-0 left-0 rounded-lg bg-brand-accent/15 pointer-events-none transition-[width] duration-300 ease-out motion-reduce:transition-none"
                  style="width: {itemPercent}%;"
                  data-testid="stats-accent-bar"
                ></div>
              {/if}

              <div class="relative z-10 min-w-0 flex-1 flex flex-col gap-0.5">
                <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">
                  {item.label}
                </p>
                {#if item.secondary || secondaryFallback}
                  <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">
                    {item.secondary || secondaryFallback}
                  </p>
                {/if}
              </div>
            </div>
          {/if}
        </div>

        {#if !rankInsideRow}
          {@render movementSnippet(item, i + 1)}
        {/if}

        {#if showDuration}
          {@render durationSnippet(item)}
        {/if}
      </div>
    {/each}

    {#if items.length === 0}
      <p class="text-sm text-brand-text-secondary px-3 py-6 text-center">
        {emptyText ?? i18n.t("stats.noData", {}, "No data for this range.")}
      </p>
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
    onClose={() => {
      contextMenuState = null;
    }}
  />
{/if}
