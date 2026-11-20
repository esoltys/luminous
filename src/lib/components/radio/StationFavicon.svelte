<script lang="ts">
  import { BroadcastIcon as Broadcast } from "phosphor-svelte";
  import { resolveArtUrl } from "../../types";

  let { favicon, sizeClass = "w-8 h-8" }: { favicon?: string | null; sizeClass?: string } =
    $props();

  let failed = $state(false);
  const url = $derived(resolveArtUrl(favicon ?? ""));

  $effect(() => {
    void favicon;
    failed = false;
  });
</script>

<div
  class="{sizeClass} flex-shrink-0 flex items-center justify-center bg-brand-main/60 border border-brand-border/40 overflow-hidden"
>
  {#if url && !failed}
    <img
      src={url}
      alt=""
      loading="lazy"
      onerror={() => {
        failed = true;
      }}
      class="w-full h-full object-contain"
    />
  {:else}
    <Broadcast class="w-4 h-4 text-brand-text-secondary" />
  {/if}
</div>
