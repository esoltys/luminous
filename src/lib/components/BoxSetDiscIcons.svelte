<script lang="ts">
  import { DiscIcon as Disc } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    discCount: number;
    size?: "xs" | "sm" | "md" | "lg";
    class?: string;
  }

  let { discCount, size = "md", class: customClass = "" }: Props = $props();

  type SizeKey = "xs" | "sm" | "md" | "lg";

  const sizeClasses: Record<SizeKey, { icon: string; overlap: string }> = {
    xs: { icon: "w-3.5 h-3.5", overlap: "-ml-1.5" },
    sm: { icon: "w-4 h-4", overlap: "-ml-2" },
    md: { icon: "w-6 h-6", overlap: "-ml-3" },
    lg: { icon: "w-8 h-8", overlap: "-ml-4" },
  };

  // Cap the stack visually — beyond this it just reads as clutter.
  const MAX_VISIBLE_DISCS = 6;

  let currentSize = $derived(sizeClasses[size ?? "md"]);
  let visibleDiscs = $derived(Math.min(Math.max(discCount, 1), MAX_VISIBLE_DISCS));
</script>

{#if discCount > 1}
  <div
    class="absolute bottom-1.5 left-1.5 z-10 pointer-events-none flex items-center {customClass}"
    title={i18n.t('artistDetail.discSet', { count: discCount })}
    aria-label={i18n.t('artistDetail.discSet', { count: discCount })}
    data-testid="box-set-disc-icons"
  >
    {#each Array(visibleDiscs) as _, i (i)}
      <Disc
        weight="fill"
        class="{currentSize.icon} text-white drop-shadow-[0_1px_2px_rgba(0,0,0,0.8)] {i > 0 ? currentSize.overlap : ''}"
        style="z-index: {visibleDiscs - i}"
      />
    {/each}
  </div>
{/if}
