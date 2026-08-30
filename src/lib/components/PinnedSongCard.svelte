<script lang="ts">
  import type { Song } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import CoverArt from "./CoverArt.svelte";

  interface Props {
    song: Song;
    widthClass?: string;
    onclick?: (e: MouseEvent) => void;
  }

  let { song, widthClass = "w-full", onclick }: Props = $props();
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<div
  {onclick}
  class="{widthClass} bg-brand-sidebar border border-brand-border/60 rounded-b-xl overflow-hidden flex flex-col group hover:border-brand-accent/40 transition-all duration-200 select-none cursor-pointer"
>
  <div class="aspect-square bg-brand-main flex items-center justify-center text-brand-accent-text relative overflow-hidden w-full">
    <CoverArt
      songId={song.id}
      artEmbedded={song.art_embedded}
      artAutomatic={song.art_automatic}
      artManual={song.art_manual}
      sizeClass="w-full h-full"
    />
  </div>
  <div class="p-3.5 flex flex-col flex-1">
    <p class="font-semibold text-sm text-brand-text-primary truncate w-full">
      {song.title || i18n.t('collection.unknownSong')}
    </p>
    <p class="text-xs text-brand-text-secondary truncate mt-0.5 font-medium">
      {song.artist || i18n.t('collection.unknownArtist')}
    </p>
  </div>
</div>
