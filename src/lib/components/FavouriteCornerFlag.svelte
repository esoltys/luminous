<script lang="ts">
  import { HeartIcon as Heart } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    size?: "xs" | "sm" | "md" | "lg";
    class?: string;
  }

  let { size = "md", class: customClass = "" }: Props = $props();

  type SizeKey = "xs" | "sm" | "md" | "lg";

  const sizeClasses: Record<SizeKey, { wrapper: string; icon: string }> = {
    xs: { wrapper: "w-5 h-5", icon: "w-2 h-2 top-0.5 right-0.5" },
    sm: { wrapper: "w-6 h-6", icon: "w-2.5 h-2.5 top-0.5 right-0.5" },
    md: { wrapper: "w-10 h-10", icon: "w-4 h-4 top-1 right-1" },
    lg: { wrapper: "w-12 h-12", icon: "w-4.5 h-4.5 top-1.5 right-1.5" },
  };

  let currentSize = $derived(sizeClasses[size ?? "md"]);
</script>

<div
  class="absolute top-0 right-0 z-10 pointer-events-none drop-shadow-sm {currentSize.wrapper} {customClass}"
  title={i18n.t('rating.favoriteTooltip')}
  aria-label={i18n.t('rating.favoriteTooltip')}
  data-testid="favourite-corner-flag"
>
  <svg class="w-full h-full" viewBox="0 0 100 100" preserveAspectRatio="none" aria-hidden="true">
    <polygon points="0,0 100,0 100,100" class="fill-brand-accent" />
  </svg>
  <Heart weight="fill" class="absolute {currentSize.icon} text-brand-accent-contrast" />
</div>
