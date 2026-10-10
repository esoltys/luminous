<script lang="ts">
  import { InfoIcon as Info } from "phosphor-svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";

  /** What the sidebar shows for this view, e.g. "Artist Info". */
  let { label }: { label: string } = $props();
</script>

<!-- The info sidebar is the only home for an artist's or album's details; this opens and closes it,
     and disappears when the window is too narrow to show it at all. -->
{#if !windowLayoutStore.isRightPanelAutoHidden}
  <button
    type="button"
    aria-pressed={windowLayoutStore.rightPanelOpen}
    onclick={() => windowLayoutStore.toggleRightPanel()}
    class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border bg-brand-sidebar text-xs font-medium transition-colors cursor-pointer shrink-0 hover:border-brand-accent/40 {windowLayoutStore.rightPanelOpen ? 'border-brand-accent/40 text-brand-accent-text' : 'border-brand-border text-brand-text-secondary hover:text-brand-text-primary'}"
  >
    <Info class="w-3.5 h-3.5" />
    <span>{label}</span>
  </button>
{/if}
