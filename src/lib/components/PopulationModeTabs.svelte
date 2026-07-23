<script lang="ts">
  import { i18n } from "../stores/i18n.svelte";
  import type { QueuePopulationMode } from "../types";

  interface Props {
    mode: QueuePopulationMode;
    onChange: (mode: QueuePopulationMode) => void;
    disabled?: boolean;
  }

  let { mode, onChange, disabled = false }: Props = $props();

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
  class="flex items-center gap-0.5 p-0.5 rounded-full border border-brand-border bg-brand-main/40 shrink-0 select-none"
  role="tablist"
  aria-label={i18n.t("playlists.populationModeLabel")}
>
  {#each MODES as m (m.value)}
    <button
      type="button"
      role="tab"
      aria-selected={mode === m.value}
      {disabled}
      title={i18n.t(m.tooltipKey)}
      onclick={() => onChange(m.value)}
      class="px-2.5 py-1 rounded-full text-[11px] font-semibold whitespace-nowrap transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed
        {mode === m.value
        ? 'bg-brand-accent text-brand-accent-contrast shadow-sm'
        : 'text-brand-text-secondary/70 hover:text-brand-text-primary hover:bg-brand-sidebar'}"
    >
      {i18n.t(m.labelKey)}
    </button>
  {/each}
</div>
