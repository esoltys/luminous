<script lang="ts">
  import type { Song } from "../types";
  import CarouselCard from "./CarouselCard.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";

  let { songs }: { songs: Song[] } = $props();
</script>

<HorizontalScrollRow>
  {#each songs as song (song.id)}
    <CarouselCard {song} />
  {/each}
</HorizontalScrollRow>
