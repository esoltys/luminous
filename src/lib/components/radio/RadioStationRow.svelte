<script lang="ts">
  import {
    PlayIcon as Play,
    PauseIcon as Pause,
    BookmarkSimpleIcon as Bookmark,
    WarningCircleIcon as AlertCircle,
    PushPinIcon as Pin,
  } from "phosphor-svelte";
  import RadioStationContextMenu from "./RadioStationContextMenu.svelte";
  import { pinnedStore } from "../../stores/pinned.svelte";
  import HelpTip from "../HelpTip.svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { resolveArtUrl, type RadioDirectoryStation } from "../../types";

  let {
    station,
    saved = false,
    playing = false,
    onPlay,
    onToggleSave,
    onTagClick,
  }: {
    station: RadioDirectoryStation;
    saved?: boolean;
    playing?: boolean;
    onPlay: (station: RadioDirectoryStation) => void;
    onToggleSave: (station: RadioDirectoryStation) => void;
    onTagClick?: (tag: string) => void;
  } = $props();

  let artFailed = $state(false);
  let menu = $state<{ x: number; y: number } | null>(null);
  const pinned = $derived(pinnedStore.isPinned("station", station.stationuuid));
  const pinLabel = $derived(
    pinned ? i18n.t("playlists.contextMenuUnpinHome") : i18n.t("playlists.contextMenuPinHome"),
  );
  const artworkUrl = $derived(resolveArtUrl(station.favicon));

  $effect(() => {
    void station.favicon;
    artFailed = false;
  });

  const tagList = $derived(
    station.tags
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean)
      .slice(0, 3),
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  data-testid="radio-station-row"
  data-station-uuid={station.stationuuid}
  oncontextmenu={(e) => {
    // Stop here so a wrapper that has its own menu (Home's pinned row) doesn't open a second one.
    e.preventDefault();
    e.stopPropagation();
    menu = { x: e.clientX, y: e.clientY };
  }}
  class="@container flex items-center gap-3 px-3 py-2.5 rounded-xl border border-brand-border/40 bg-brand-surface/40 hover:bg-brand-surface/80 transition-colors duration-150"
>
  <button
    type="button"
    disabled={!station.playable}
    onclick={() => onPlay(station)}
    aria-label={playing
      ? i18n.t("radio.playingLabel", { name: station.name })
      : i18n.t("radio.playStation", { name: station.name })}
    class="group relative w-9 h-9 rounded-lg overflow-hidden flex items-center justify-center shrink-0 transition-colors duration-150 {station.playable
      ? playing
        ? 'bg-brand-accent text-brand-accent-contrast'
        : 'bg-brand-accent/15 text-brand-accent-text hover:bg-brand-accent hover:text-brand-accent-contrast'
      : 'bg-brand-surface text-brand-text-secondary/40 cursor-not-allowed'}"
  >
    {#if artworkUrl && !artFailed}
      <img
        src={artworkUrl}
        alt=""
        loading="lazy"
        onerror={() => {
          artFailed = true;
        }}
        class="w-full h-full object-cover"
      />
      {#if station.playable}
        <span
          class="absolute inset-0 flex items-center justify-center bg-brand-sidebar/75 text-brand-text-primary transition-opacity duration-150 {playing
            ? 'opacity-100'
            : 'opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100'}"
        >
          {#if playing}
            <Pause class="w-4 h-4" weight="fill" />
          {:else}
            <Play class="w-4 h-4" weight="fill" />
          {/if}
        </span>
      {/if}
    {:else if playing}
      <Pause class="w-4 h-4" weight="fill" />
    {:else}
      <Play class="w-4 h-4" weight="fill" />
    {/if}
  </button>

  <div class="flex-1 min-w-0">
    <div class="flex items-center gap-2 min-w-0">
      <span class="font-medium text-sm text-brand-text-primary truncate">
        {station.name || i18n.t("radio.unnamedStation")}
      </span>
      {#if !station.playable}
        <span
          class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-brand-surface border border-brand-border/60 label-small text-brand-text-secondary shrink-0"
        >
          <AlertCircle class="w-3.5 h-3.5" />
          {i18n.t("radio.unsupportedBadge")}
        </span>
        <HelpTip text={i18n.t("radio.unsupportedHint")} label={i18n.t("radio.unsupportedBadge")} />
      {:else if station.reducedQuality}
        <span
          class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-brand-surface border border-brand-border/60 label-small text-brand-text-secondary shrink-0"
        >
          {i18n.t("radio.reducedQualityBadge")}
        </span>
        <HelpTip
          text={i18n.t("radio.reducedQualityHint")}
          label={i18n.t("radio.reducedQualityBadge")}
        />
      {/if}
    </div>

    <div
      class="flex flex-wrap items-center gap-x-2.5 gap-y-1 mt-0.5 text-xs text-brand-text-secondary"
    >
      {#if station.country}
        <span class="truncate max-w-[14rem]">{station.country}</span>
      {/if}
      {#if station.codec}
        <span
          class="code-caps px-1.5 py-0.2 rounded bg-brand-surface/80 border border-brand-border/40"
        >
          {station.codec}
        </span>
      {/if}
      {#if station.bitrate > 0}
        <span>{station.bitrate} kbps</span>
      {/if}
      {#if tagList.length > 0}
        <div class="hidden @lg:flex items-center gap-1 flex-wrap">
          {#each tagList as tag}
            {#if onTagClick}
              <button
                type="button"
                onclick={() => onTagClick(tag)}
                class="label-small px-1.5 py-0.5 rounded-md bg-brand-accent/10 text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-accent/20 transition-colors"
              >
                {tag}
              </button>
            {:else}
              <span
                class="label-small px-1.5 py-0.5 rounded-md bg-brand-surface text-brand-text-secondary"
              >
                {tag}
              </span>
            {/if}
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="flex items-center gap-1.5 shrink-0">
    <button
      type="button"
      data-testid="radio-station-pin-btn"
      onclick={() => void pinnedStore.toggle("station", station.stationuuid)}
      aria-pressed={pinned}
      aria-label={pinLabel}
      title={pinLabel}
      class="inline-flex items-center justify-center p-1.5 rounded-lg transition-colors duration-150 {pinned
        ? 'text-brand-accent-text hover:opacity-80'
        : 'text-brand-text-secondary hover:bg-brand-surface hover:text-brand-text-primary'}"
    >
      <Pin class="w-4 h-4" weight={pinned ? "fill" : "regular"} />
    </button>
    <button
      type="button"
      onclick={() => onToggleSave(station)}
      aria-pressed={saved}
      aria-label={saved ? i18n.t("radio.unsaveStation") : i18n.t("radio.saveStation")}
      class="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-xs font-medium transition-colors duration-150 {saved
        ? 'bg-brand-accent/20 text-brand-accent-text hover:bg-brand-accent/30'
        : 'text-brand-text-secondary hover:bg-brand-surface hover:text-brand-text-primary'}"
    >
      <Bookmark class="w-4 h-4" weight={saved ? "fill" : "regular"} />
      <span class="hidden @md:inline">
        {saved ? i18n.t("radio.saved") : i18n.t("radio.save")}
      </span>
    </button>
  </div>
</div>

{#if menu}
  <RadioStationContextMenu
    x={menu.x}
    y={menu.y}
    {station}
    onClose={() => {
      menu = null;
    }}
  />
{/if}
