<script lang="ts">
  import { Play, Shuffle } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    onPlayAll: () => void;
    onShufflePlay: () => void;
    disabled?: boolean;
    /** Extra classes appended to both buttons (e.g. shrink-0 in a flex-wrap layout). */
    class?: string;
  }

  let { onPlayAll, onShufflePlay, disabled = false, class: extraClass = "" }: Props = $props();
</script>

<button
  onclick={onPlayAll}
  {disabled}
  class="flex items-center gap-2 px-5 py-2 rounded-full bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed shadow-md shadow-brand-accent/20 {extraClass}"
>
  <Play class="w-4 h-4 fill-current" /> {i18n.t('artistDetail.playAll')}
</button>
<button
  onclick={onShufflePlay}
  {disabled}
  class="flex items-center gap-2 px-5 py-2 rounded-full border border-brand-border text-brand-text-primary hover:bg-brand-sidebar font-semibold text-sm transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed {extraClass}"
>
  <Shuffle class="w-4 h-4" /> {i18n.t('artistDetail.shuffleAndPlay')}
</button>
