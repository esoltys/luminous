<script lang="ts">
  import { MagnifyingGlassIcon as Search } from "phosphor-svelte";
  import { playerStore } from "../../stores/player.svelte";
  import { navigationStore } from "../../stores/navigation.svelte";
  import { windowLayoutStore } from "../../stores/windowLayout.svelte";
  import { i18n } from "../../stores/i18n.svelte";

  /** Searches the library for the track the playing station announces
   * (#2008). Renders nothing while no station is announcing one. */
  let { size = "sm" }: { size?: "sm" | "md" } = $props();

  const query = $derived(playerStore.stationNowPlayingQuery);
  const label = $derived(i18n.t("playerBar.findInLibrary", undefined, "Find in library"));
</script>

{#if query}
  <button
    type="button"
    data-testid="station-find-in-library"
    onclick={(e) => {
      e.stopPropagation();
      windowLayoutStore.exitImmersiveMode();
      navigationStore.searchLibrary(query);
    }}
    aria-label={label}
    title={label}
    class="inline-flex items-center justify-center rounded-lg text-brand-text-secondary hover:text-brand-text-primary transition-colors duration-150 {size ===
    'md'
      ? 'p-1.5'
      : 'p-1'}"
  >
    <Search class={size === "md" ? "w-5 h-5" : "w-4 h-4"} />
  </button>
{/if}
