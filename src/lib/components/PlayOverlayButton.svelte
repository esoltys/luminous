<script lang="ts">
  import { Play } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    onPlay: (e: MouseEvent) => void;
  }

  let { onPlay }: Props = $props();
</script>

<div class="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 flex items-center justify-center transition-opacity z-20">
  <button
    onclick={onPlay}
    class="w-12 h-12 rounded-full bg-brand-accent text-brand-accent-contrast flex items-center justify-center scale-75 group-hover:scale-100 transition-transform cursor-pointer"
    title={i18n.t('playerBar.play')}
  >
    <Play class="w-5 h-5 fill-current ml-0.5" />
  </button>
</div>
