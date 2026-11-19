<script lang="ts">
  import {
    BroadcastIcon as Broadcast,
    PlayIcon as Play,
    PauseIcon as Pause,
    BookmarkSimpleIcon as Bookmark,
  } from "phosphor-svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { resolveArtUrl, type RadioDirectoryStation } from "../../types";
  import { PLAIN_TILE_COVER, PLAIN_TILE_RING, useCardAppearance } from "../../utils/cardAppearance";

  const plain = useCardAppearance() === "plain";

  let {
    station,
    playing = false,
    onPlay,
    onToggleSave,
  }: {
    station: RadioDirectoryStation;
    playing?: boolean;
    onPlay: (station: RadioDirectoryStation) => void;
    onToggleSave: (station: RadioDirectoryStation) => void;
  } = $props();

  let artFailed = $state(false);
  const artworkUrl = $derived(resolveArtUrl(station.favicon));

  $effect(() => {
    void station.favicon;
    artFailed = false;
  });
</script>

<div
  data-testid="radio-station-card"
  data-station-uuid={station.stationuuid}
  class="w-full flex flex-col group relative select-none {plain
    ? ''
    : 'bg-brand-sidebar border border-brand-border/60 rounded-b-xl overflow-hidden'}"
>
  <button
    type="button"
    disabled={!station.playable}
    onclick={() => onPlay(station)}
    aria-label={playing
      ? i18n.t("radio.playingLabel", { name: station.name })
      : i18n.t("radio.playStation", { name: station.name })}
    class="aspect-square bg-brand-main flex items-center justify-center text-brand-accent-text relative overflow-hidden w-full {plain
      ? PLAIN_TILE_COVER
      : ''} {station.playable ? 'cursor-pointer' : 'cursor-not-allowed opacity-60'}"
  >
    {#if artworkUrl && !artFailed}
      <img
        src={artworkUrl}
        alt=""
        loading="lazy"
        onerror={() => {
          artFailed = true;
        }}
        class="w-full h-full object-cover transition-transform duration-300 group-hover:scale-105"
      />
    {:else}
      <Broadcast class="w-12 h-12 text-brand-text-secondary/50" />
    {/if}

    {#if station.playable}
      <span
        class="absolute inset-0 flex items-center justify-center bg-brand-sidebar/65 text-brand-text-primary transition-opacity duration-150 {playing
          ? 'opacity-100'
          : 'opacity-0 group-hover:opacity-100 group-focus-visible:opacity-100'}"
      >
        <span
          class="w-11 h-11 rounded-full bg-brand-accent text-brand-accent-contrast flex items-center justify-center shadow-lg"
        >
          {#if playing}
            <Pause class="w-5 h-5" weight="fill" />
          {:else}
            <Play class="w-5 h-5" weight="fill" />
          {/if}
        </span>
      </span>
    {/if}

    {#if plain}
      <div class={PLAIN_TILE_RING}></div>
    {/if}
  </button>

  <div class="flex flex-col flex-1 {plain ? 'pt-3' : 'p-3.5'}">
    <div class="flex items-start justify-between gap-2">
      <button
        type="button"
        disabled={!station.playable}
        onclick={() => onPlay(station)}
        class="font-semibold text-sm text-brand-text-primary group-hover:text-brand-accent-text transition-colors duration-150 truncate text-left flex-1 min-w-0"
      >
        {station.name || i18n.t("radio.unnamedStation")}
      </button>
      <button
        type="button"
        onclick={() => onToggleSave(station)}
        aria-pressed={true}
        aria-label={i18n.t("radio.unsaveStation")}
        class="text-brand-accent-text hover:opacity-80 shrink-0 p-0.5"
      >
        <Bookmark class="w-4 h-4" weight="fill" />
      </button>
    </div>

    <div
      class="flex items-center justify-between mt-0.5 gap-2 text-xs text-brand-text-secondary font-medium"
    >
      <span class="truncate min-w-0">
        {station.country || station.codec || ""}
      </span>
      {#if station.bitrate > 0}
        <span class="shrink-0">{station.bitrate} kbps</span>
      {/if}
    </div>
  </div>

  {#if !plain}
    <div
      class="absolute inset-0 rounded-b-xl ring-2 ring-inset ring-transparent group-hover:ring-brand-accent transition-[box-shadow] duration-200 pointer-events-none"
    ></div>
  {/if}
</div>
