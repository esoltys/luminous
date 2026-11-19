<script lang="ts">
  import {
    BroadcastIcon as Broadcast,
    PlayIcon as Play,
    PauseIcon as Pause,
  } from "phosphor-svelte";
  import { i18n } from "../../stores/i18n.svelte";
  import { resolveArtUrl, type RadioDirectoryStation } from "../../types";
  import { rowClasses, useCardAppearance } from "../../utils/cardAppearance";
  import RadioStationContextMenu from "./RadioStationContextMenu.svelte";

  const appearance = useCardAppearance();

  let {
    station,
    playing = false,
    onPlay,
  }: {
    station: RadioDirectoryStation;
    playing?: boolean;
    onPlay: (station: RadioDirectoryStation) => void;
  } = $props();

  let artFailed = $state(false);
  let menu = $state<{ x: number; y: number } | null>(null);
  const artworkUrl = $derived(resolveArtUrl(station.favicon));

  $effect(() => {
    void station.favicon;
    artFailed = false;
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  data-testid="pinned-station-row-card"
  data-station-uuid={station.stationuuid}
  onclick={() => {
    if (station.playable) onPlay(station);
  }}
  onkeydown={(e) => {
    if ((e.key === "Enter" || e.key === " ") && station.playable) {
      e.preventDefault();
      onPlay(station);
    }
  }}
  oncontextmenu={(e) => {
    e.preventDefault();
    e.stopPropagation();
    menu = { x: e.clientX, y: e.clientY };
  }}
  class="{rowClasses(appearance)} group w-full {station.playable
    ? 'cursor-pointer'
    : 'cursor-not-allowed opacity-60'}"
>
  <div
    class="relative shrink-0 w-11 h-11 bg-brand-main border border-brand-border/60 flex items-center justify-center overflow-hidden"
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
    {:else}
      <Broadcast class="w-5 h-5 text-brand-text-secondary/60" />
    {/if}

    {#if station.playable}
      <span
        class="absolute inset-0 flex items-center justify-center bg-brand-sidebar/70 text-brand-text-primary transition-opacity duration-150 {playing
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
  </div>

  <div class="min-w-0 flex-1">
    <p
      class="truncate text-sm font-semibold text-brand-text-primary group-hover:text-brand-accent-text transition-colors"
    >
      {station.name || i18n.t("radio.unnamedStation")}
    </p>
  </div>

  {#if station.bitrate > 0}
    <span class="shrink-0 text-xs text-brand-text-secondary font-medium tabular-nums">
      {station.bitrate} kbps
    </span>
  {/if}
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
