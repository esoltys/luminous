<script lang="ts">
  import { onMount } from "svelte";
  import { i18n } from "../stores/i18n.svelte";
  import type { QueuePopulationMode } from "../types";

  interface Props {
    mode: QueuePopulationMode;
    onChange: (mode: QueuePopulationMode) => void;
    disabled?: boolean;
  }

  let { mode, onChange, disabled = false }: Props = $props();

  let tabElements = $state<Record<string, HTMLButtonElement>>({});
  let indicatorStyle = $state({ left: 2, width: 0, opacity: 0 });
  let mounted = $state(false);

  function updateIndicator() {
    const el = tabElements[mode];
    if (el) {
      indicatorStyle = {
        left: el.offsetLeft,
        width: el.offsetWidth,
        opacity: 1
      };
    }
  }

  onMount(() => {
    const handleResize = () => updateIndicator();
    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
    };
  });

  $effect(() => {
    if (tabElements[mode]) {
      updateIndicator();
      if (!mounted) {
        requestAnimationFrame(() => {
          mounted = true;
        });
      }
    }
  });

  // Tab order per #120 feedback: All, Favourites, Familiar, Discover, Deep Cuts.
  const MODES: { value: QueuePopulationMode; labelKey: string; tooltipKey: string }[] = [
    { value: "all", labelKey: "playlists.populationModeAll", tooltipKey: "playlists.populationModeTooltipAll" },
    {
      value: "favourites",
      labelKey: "playlists.populationModeFavourites",
      tooltipKey: "playlists.populationModeTooltipFavourites",
    },
    {
      value: "familiar",
      labelKey: "playlists.populationModeFamiliar",
      tooltipKey: "playlists.populationModeTooltipFamiliar",
    },
    {
      value: "discover",
      labelKey: "playlists.populationModeDiscover",
      tooltipKey: "playlists.populationModeTooltipDiscover",
    },
    {
      value: "deep_cuts",
      labelKey: "playlists.populationModeDeepCuts",
      tooltipKey: "playlists.populationModeTooltipDeepCuts",
    },
  ];
</script>

<div
  class="relative flex items-center gap-0.5 p-0.5 rounded-full border border-brand-border bg-brand-main/40 shrink-0 select-none"
  role="tablist"
  aria-label={i18n.t("playlists.populationModeLabel")}
>
  <!-- Sliding active indicator -->
  <div
    class="absolute top-0.5 bottom-0.5 bg-brand-accent rounded-full shadow-sm pointer-events-none {mounted ? 'transition-[left,width] duration-200 ease-out' : 'transition-none'}"
    style="left: {indicatorStyle.left}px; width: {indicatorStyle.width}px; opacity: {indicatorStyle.opacity};"
    aria-hidden="true"
  ></div>

  {#each MODES as m (m.value)}
    <button
      bind:this={tabElements[m.value]}
      type="button"
      role="tab"
      aria-selected={mode === m.value}
      {disabled}
      title={i18n.t(m.tooltipKey)}
      onclick={() => onChange(m.value)}
      class="relative z-10 px-2.5 py-1 rounded-full text-[11px] font-semibold whitespace-nowrap transition-colors duration-200 disabled:opacity-50 disabled:cursor-not-allowed
        {mode === m.value
        ? 'text-brand-accent-contrast'
        : 'text-brand-text-secondary/70 hover:text-brand-text-primary'}"
    >
      {i18n.t(m.labelKey)}
    </button>
  {/each}
</div>
