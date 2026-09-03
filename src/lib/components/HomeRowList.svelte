<script lang="ts">
  import type { HomeItem, Song, Playlist, AlbumItem, TopAlbumChartInfo } from "../types";
  import { playerStore } from "../stores/player.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { navigationStore } from "../stores/navigation.svelte";
  import { playlistsStore } from "../stores/playlists.svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { isSmartPlaylistSpec } from "../utils/filterParser";
  import CoverArt from "./CoverArt.svelte";
  import PlaylistCoverThumb from "./PlaylistCoverThumb.svelte";
  import SongRating from "./SongRating.svelte";
  import FavouriteCornerFlag from "./FavouriteCornerFlag.svelte";
  import SongContextMenu from "./SongContextMenu.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { getPlaylistDisplayName } from "../utils/playlist";
  import { formatChartWeekRange } from "../utils/date";
  import {
    CaretRightIcon as ChevronRight,
    TrendUpIcon as TrendingUp,
    TrendDownIcon as TrendingDown,
    MinusIcon as Minus
  } from "phosphor-svelte";

  interface Props {
    title?: string;
    items: HomeItem[];
    /** "rank" shows a 01-05 numeral + track duration; "added" shows a relative
     * added date; "chart" shows each album's weekly chart rank, a movement
     * indicator, and a peak-rank/weeks-on-chart stat (Home "Top Albums", #662). */
    variant: "rank" | "added" | "chart";
    /** When provided, the title becomes a clickable button that navigates to
     * the category's full expanded view (see #169). */
    onHeaderClick?: () => void;
  }

  let { title, items, variant, onHeaderClick }: Props = $props();

  let contextMenuState = $state<{ x: number; y: number; song: Song } | null>(null);

  function keyFor(item: HomeItem): string {
    if (item.type === "song") return "s_" + item.song.id;
    if (item.type === "playlist") return "p_" + item.playlist.id;
    return "a_" + (item.album.album || "") + "_" + (item.album.artist || "");
  }

  function titleFor(item: HomeItem): string {
    if (item.type === "song") return item.song.title || i18n.t("collection.unknownSong");
    if (item.type === "album") return item.album.album || i18n.t("collection.unknownAlbum");
    return getPlaylistDisplayName(item.playlist);
  }

  /** "Genre" / "Decade" / "Smart" / "Custom" — mirrors PlaylistCard's autoKind derivation. */
  function playlistCategoryFor(playlist: Playlist): string {
    if (!playlist.dynamic_enabled) return i18n.t("sidebar.playlistsCustom");
    if (isSmartPlaylistSpec(playlist.dynamic_spec)) return i18n.t("playlists.smartAutoPlaylist");
    return playlist.dynamic_spec?.startsWith("decade:")
      ? i18n.t("playlists.decadeAutoPlaylist")
      : i18n.t("playlists.genreAutoPlaylist");
  }

  function subtitleFor(item: HomeItem): string {
    if (item.type === "song") return item.song.artist || i18n.t("collection.unknownArtist");
    if (item.type === "album") return item.album.artist || i18n.t("collection.variousArtists");
    return playlistCategoryFor(item.playlist);
  }

  function yearFor(item: HomeItem): string {
    if (item.type === "song") return item.song.year ? String(item.song.year) : "";
    if (item.type === "album") return item.album.year ? String(item.album.year) : "";
    return "";
  }

  function trailingLabel(item: HomeItem): string {
    if (item.type === "playlist") return i18n.t("playlists.playlistTypeLabel");
    return "";
  }

  function trackCountFor(item: HomeItem): string {
    if (item.type === "playlist") {
      return item.playlist.track_count === 1
        ? i18n.t("playlists.oneSong")
        : i18n.t("playlists.songsCount", { count: item.playlist.track_count });
    }
    return "";
  }

  function rankFor(item: HomeItem, index: number): number {
    if (variant === "chart" && item.type === "album" && item.chart) return item.chart.rank;
    return index + 1;
  }

  function movementLabel(movement: "new" | "rising" | "falling" | "steady"): string {
    if (movement === "new") return i18n.t("home.chartNew");
    if (movement === "rising") return i18n.t("home.chartRising");
    if (movement === "falling") return i18n.t("home.chartFalling");
    return i18n.t("home.chartSteady");
  }

  function peakWeeksLabel(chart: TopAlbumChartInfo): string {
    return chart.weeks_on_chart === 1
      ? i18n.t("home.chartPeakWeek", { peak: chart.peak_rank })
      : i18n.t("home.chartPeakWeeks", { peak: chart.peak_rank, weeks: chart.weeks_on_chart });
  }

  // Mirrors ArtistDetailView's openPlaylist: genre/decade auto-playlists open
  // in AutoPlaylistDetailView, custom playlists (including Smart Playlists)
  // in the regular PlaylistView.
  function openPlaylist(playlist: Playlist) {
    if (playlist.dynamic_enabled && !isSmartPlaylistSpec(playlist.dynamic_spec)) {
      const isDecade = playlist.dynamic_spec?.startsWith("decade:") ?? false;
      navigationStore.viewAutoPlaylist(
        isDecade
          ? { kind: "decade", decade: playlist.dynamic_spec?.replace(/^decade:/, "") ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
          : { kind: "genre", genre: playlist.dynamic_spec?.replace(/^tag:/, "") ?? playlist.name, playlistId: playlist.id, updated: playlist.updated }
      );
      return;
    }
    playlistsStore.selectPlaylist(playlist.id);
    navigationStore.viewPlaylist(playlist.id);
  }

  function openItem(item: HomeItem) {
    if (item.type === "song" && item.song.album) {
      navigationStore.viewAlbum(item.song.album);
    } else if (item.type === "song") {
      playerStore.playSong(item.song.id);
    } else if (item.type === "album") {
      navigationStore.viewAlbum(item.album.album || "");
    } else if (item.type === "playlist") {
      openPlaylist(item.playlist);
    }
  }

  function handleContextMenu(e: MouseEvent, item: HomeItem) {
    if (item.type !== "song") return;
    e.preventDefault();
    contextMenuState = { x: e.clientX, y: e.clientY, song: item.song };
  }

  async function rateSong(song: Song, rating: number) {
    song.rating = await invoke<number>("set_song_rating", { songId: song.id, rating });
  }

  async function rateAlbum(album: AlbumItem, rating: number) {
    if (!album.album) return;
    album.rating = await invoke<number>("set_album_rating", { album: album.album, rating });
  }
</script>

<div class="space-y-4">
  {#if title && onHeaderClick}
    <button
      type="button"
      onclick={onHeaderClick}
      class="group flex items-center gap-1 text-xl font-semibold text-brand-text-primary hover:text-brand-accent-text transition-colors"
    >
      {title}
      {#if variant === "chart"}
        <span class="text-sm font-normal text-brand-text-secondary">{formatChartWeekRange()}</span>
      {/if}
      <ChevronRight class="w-5 h-5 opacity-0 group-hover:opacity-100 transition-opacity" />
    </button>
  {:else if title}
    <h2 class="flex items-center gap-2 text-xl font-semibold text-brand-text-primary">
      {title}
      {#if variant === "chart"}
        <span class="text-sm font-normal text-brand-text-secondary">{formatChartWeekRange()}</span>
      {/if}
    </h2>
  {/if}

  <div class="flex flex-col gap-2">
    {#each items as item, i (keyFor(item))}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        role="button"
        tabindex="0"
        onclick={() => openItem(item)}
        oncontextmenu={(e) => handleContextMenu(e, item)}
        onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openItem(item); } }}
        class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none"
      >
        {#if variant === "rank" || variant === "chart"}
          <span class="w-5 shrink-0 text-center text-sm font-bold text-brand-text-secondary tabular-nums">
            {String(rankFor(item, i)).padStart(2, "0")}
          </span>
        {/if}

        <div class="relative shrink-0 overflow-hidden">
          {#if item.type === "song"}
            <CoverArt
              songId={item.song.id}
              artEmbedded={item.song.art_embedded}
              artAutomatic={item.song.art_automatic}
              artManual={item.song.art_manual}
              sizeClass="w-11 h-11"
            />
          {:else if item.type === "album"}
            <CoverArt
              songId={item.album.sample_song_id ?? undefined}
              artEmbedded={item.album.art_embedded}
              artAutomatic={item.album.art_automatic}
              artManual={item.album.art_manual}
              sizeClass="w-11 h-11"
            />
            {#if item.album.rating === 5}
              <FavouriteCornerFlag size="sm" />
            {/if}
          {:else}
            <PlaylistCoverThumb playlist={item.playlist} sizeClass="w-11 h-11" />
          {/if}
        </div>

        {#if item.type === "album" || item.type === "song"}
          <div class="min-w-0 flex-1 flex flex-col gap-0.5">
            <div class="flex items-center justify-between gap-2">
              <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">{titleFor(item)}</p>
              <span class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0">{yearFor(item)}</span>
            </div>
            <div class="flex items-center justify-between gap-2">
              <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">{subtitleFor(item)}</p>
              <span class="shrink-0">
                {#if item.type === "song"}
                  <SongRating rating={item.song.rating} onRate={(r) => rateSong(item.song, r)} size="sm" />
                {:else}
                  <SongRating rating={item.album.rating} onRate={(r) => rateAlbum(item.album, r)} size="sm" />
                {/if}
              </span>
            </div>
            {#if variant === "chart" && item.type === "album" && item.chart}
              {@const chart = item.chart}
              <div class="flex items-center justify-between gap-2">
                <span
                  class="flex items-center gap-1 text-xs font-semibold {chart.movement === 'rising' ? 'text-green-400' : chart.movement === 'falling' ? 'text-red-400' : 'text-brand-text-secondary'}"
                  aria-label={movementLabel(chart.movement)}
                  title={movementLabel(chart.movement)}
                >
                  {#if chart.movement === "new"}
                    <span class="uppercase tracking-wide">{i18n.t('home.chartNew')}</span>
                  {:else if chart.movement === "rising"}
                    <TrendingUp class="w-3.5 h-3.5" />
                  {:else if chart.movement === "falling"}
                    <TrendingDown class="w-3.5 h-3.5" />
                  {:else}
                    <Minus class="w-3.5 h-3.5" />
                  {/if}
                </span>
                <span class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0">
                  {peakWeeksLabel(chart)}
                </span>
              </div>
            {/if}
          </div>
        {:else}
          <div class="min-w-0 flex-1 flex flex-col gap-0.5">
            <div class="flex items-center justify-between gap-2">
              <p class="truncate text-sm font-semibold text-brand-text-primary min-w-0">{titleFor(item)}</p>
              <span class="text-xs text-brand-text-secondary font-medium tabular-nums shrink-0">{trailingLabel(item)}</span>
            </div>
            <div class="flex items-center justify-between gap-2">
              <p class="truncate text-xs text-brand-text-secondary font-medium min-w-0">{subtitleFor(item)}</p>
              <span class="text-xs text-brand-text-secondary truncate shrink-0">{trackCountFor(item)}</span>
            </div>
          </div>
        {/if}
      </div>
    {/each}

    {#if items.length === 0}
      <p class="text-sm text-brand-text-secondary px-3 py-6 text-center">{i18n.t('home.emptyState')}</p>
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
