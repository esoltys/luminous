<script lang="ts">
  import type { HomeItem } from "../types";
  import CarouselCard from "./CarouselCard.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";

  interface Props {
    items: HomeItem[];
  }

  let { items }: Props = $props();
</script>

<HorizontalScrollRow>
  {#each items as item (item.type === 'song' ? 's_' + item.song.id : 'a_' + (item.album.album || '') + '_' + (item.album.artist || ''))}
    <CarouselCard {item} />
  {/each}
</HorizontalScrollRow>
