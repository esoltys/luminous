<script lang="ts">
  import { InfoIcon as Info } from "phosphor-svelte";
  import { windowLayoutStore } from "../stores/windowLayout.svelte";
  import { inspectorStore } from "../stores/inspector.svelte";

  /** What the sidebar shows for this view, e.g. "Artist Info". */
  let { label }: { label: string } = $props();

  let showing = $derived(windowLayoutStore.rightPanelOpen && inspectorStore.isShowingViewed);

  function toggle() {
    if (showing) {
      windowLayoutStore.toggleRightPanel();
      return;
    }
    if (!windowLayoutStore.rightPanelOpen) windowLayoutStore.toggleRightPanel();
    inspectorStore.showViewed();
  }
</script>

<!-- The info sidebar is the only home for an artist's or album's details. This shows them there, or
     hides the sidebar when they are already showing; it disappears when the window is too narrow. -->
{#if !windowLayoutStore.isRightPanelAutoHidden}
  <button
    type="button"
    aria-pressed={showing}
    onclick={toggle}
    class="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border bg-brand-sidebar text-xs font-medium transition-colors cursor-pointer shrink-0 hover:border-brand-accent/40 {showing ? 'border-brand-accent/40 text-brand-accent-text' : 'border-brand-border text-brand-text-secondary hover:text-brand-text-primary'}"
  >
    <Info class="w-3.5 h-3.5" />
    <span>{label}</span>
  </button>
{/if}
