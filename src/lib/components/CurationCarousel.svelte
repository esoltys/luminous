<script lang="ts">
  import type { HomeItem } from "../types";
  import CarouselCard from "./CarouselCard.svelte";
  import HorizontalScrollRow from "./HorizontalScrollRow.svelte";

  interface Props {
    title?: string;
    items: HomeItem[];
  }

  let { title, items }: Props = $props();

  function keyFor(item: HomeItem): string {
    if (item.type === "song") return "s_" + item.song.id;
    if (item.type === "playlist") return "p_" + item.playlist.id;
    return "a_" + (item.album.album || "") + "_" + (item.album.artist || "");
  }
</script>

<HorizontalScrollRow {title}>
  {#each items as item (keyFor(item))}
    <CarouselCard {item} />
  {/each}
</HorizontalScrollRow>
