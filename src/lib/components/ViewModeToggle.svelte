<script lang="ts">
  import { SquaresFourIcon as LayoutGrid, RowsIcon as Rows3 } from "phosphor-svelte";
  import type { CollectionViewMode } from "../stores/prefs.svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    mode: CollectionViewMode;
    onChange: (mode: CollectionViewMode) => void;
    /** Marks the pill as a guided-tour anchor (`data-walkthrough-target`). */
    walkthroughTarget?: string;
  }

  let { mode, onChange, walkthroughTarget }: Props = $props();
</script>

<!-- Cards / rows pill with a sliding indicator, shared by every view that
     offers both layouts. -->
<div
  data-walkthrough-target={walkthroughTarget}
  class="relative inline-flex items-center gap-0.5 bg-brand-sidebar border border-brand-border rounded-full p-1"
>
  <span
    class="absolute top-1 bottom-1 left-1 w-7 h-7 rounded-full bg-brand-accent shadow-sm pointer-events-none transition-transform duration-200 ease-out {mode === 'rows' ? 'translate-x-[30px]' : 'translate-x-0'}"
    aria-hidden="true"
  ></span>
  <button
    type="button"
    onclick={() => onChange("cards")}
    class="relative z-10 flex items-center justify-center w-7 h-7 rounded-full transition-colors duration-200 {mode === 'cards' ? 'text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
    title={i18n.t('collection.viewCards')}
    aria-label={i18n.t('collection.viewCards')}
    aria-pressed={mode === "cards"}
  >
    <LayoutGrid class="w-4 h-4" />
  </button>
  <button
    type="button"
    onclick={() => onChange("rows")}
    class="relative z-10 flex items-center justify-center w-7 h-7 rounded-full transition-colors duration-200 {mode === 'rows' ? 'text-white' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
    title={i18n.t('collection.viewRows')}
    aria-label={i18n.t('collection.viewRows')}
    aria-pressed={mode === "rows"}
  >
    <Rows3 class="w-4 h-4" />
  </button>
</div>
