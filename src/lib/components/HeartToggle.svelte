<script lang="ts">
  import { Heart } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    favorite: boolean;
    onToggle: () => void;
    sizeClass?: string;
  }

  let { favorite, onToggle, sizeClass = "w-4 h-4" }: Props = $props();
</script>

<button
  type="button"
  onclick={(e) => {
    e.stopPropagation();
    onToggle();
  }}
  class="transition-colors cursor-pointer {favorite
    ? 'text-brand-accent-text'
    : 'text-brand-text-secondary/60 hover:text-brand-accent-text'}"
  title={favorite ? i18n.t('rating.unfavoriteTooltip') : i18n.t('rating.favoriteTooltip')}
  aria-pressed={favorite}
>
  <Heart class="{sizeClass} {favorite ? 'fill-current' : ''}" />
</button>
