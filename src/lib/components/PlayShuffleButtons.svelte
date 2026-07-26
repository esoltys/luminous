<script lang="ts">
  import { Play, Shuffle } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";
  import Button from "./Button.svelte";

  interface Props {
    onPlayAll: () => void;
    onShufflePlay: () => void;
    disabled?: boolean;
    /** Extra classes appended to both buttons (e.g. shrink-0 in a flex-wrap layout). */
    class?: string;
  }

  let { onPlayAll, onShufflePlay, disabled = false, class: extraClass = "" }: Props = $props();
</script>

<Button onclick={onPlayAll} {disabled} variant="primary" class={extraClass}>
  <Play class="w-4 h-4 fill-current" /> {i18n.t('artistDetail.playAll')}
</Button>
<Button onclick={onShufflePlay} {disabled} variant="secondary" class={extraClass}>
  <Shuffle class="w-4 h-4" /> {i18n.t('artistDetail.shuffleAndPlay')}
</Button>
