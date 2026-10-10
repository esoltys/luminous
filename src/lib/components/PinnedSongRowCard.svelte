<script lang="ts">
  import type { Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import CoverArt from "./CoverArt.svelte";

  interface Props {
    song: Song;
    onclick?: (e: MouseEvent) => void;
    oncontextmenu?: (e: MouseEvent) => void;
  }

  let { song, onclick, oncontextmenu }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  role="button"
  tabindex="0"
  {onclick}
  oncontextmenu={(e) => oncontextmenu?.(e)}
  onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onclick?.(e as unknown as MouseEvent); } }}
  class="group flex items-center gap-3 px-3 py-2.5 rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200 select-none cursor-pointer w-full relative overflow-hidden"
>
  <div class="shrink-0 overflow-hidden">
    <CoverArt
      songId={song.id}
      artEmbedded={song.art_embedded}
      artAutomatic={song.art_automatic}
      artManual={song.art_manual}
      sizeClass="w-11 h-11"
    />
  </div>
  <div class="min-w-0 flex-1 flex flex-col gap-0.5">
    <p class="truncate text-sm font-semibold text-brand-text-primary">{song.title || i18n.t('collection.unknownSong')}</p>
    <p class="truncate text-xs text-brand-text-secondary font-medium">{song.artist || i18n.t('collection.unknownArtist')}</p>
  </div>
</div>
