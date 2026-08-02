<script lang="ts">
  import { Heart } from "lucide-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    favorite: boolean;
    onToggle: () => void;
    sizeClass?: string;
  }

  let { favorite, onToggle, sizeClass = "w-4 h-4" }: Props = $props();

  let buttonEl: HTMLButtonElement | undefined = $state();

  // Only the moment the user actually favourites a song should pulse — triggered
  // directly on user click when favouriting.
  let justFavorited = $state(false);

  // HeartToggle renders inside all sorts of clipped containers (virtualized
  // table rows, the PlayerBar's `truncate` title column), which clip the
  // ring's box-shadow same as any other overflowing content. Rather than
  // shrinking the ring until it fits (still barely visible), portal it to
  // document.body — same escape hatch Toast.svelte already uses — positioned
  // over the heart's own screen coordinates, so it renders the full spec'd
  // ring regardless of what it's embedded in.
  function burstRing() {
    if (!buttonEl) return;
    const rect = buttonEl.getBoundingClientRect();
    const ring = document.createElement("span");
    ring.className = "anim-heart-ring";
    ring.style.position = "fixed";
    ring.style.left = `${rect.left + rect.width / 2}px`;
    ring.style.top = `${rect.top + rect.height / 2}px`;
    ring.style.width = `${rect.width}px`;
    ring.style.height = `${rect.height}px`;
    ring.style.borderRadius = "9999px";
    ring.style.transform = "translate(-50%, -50%)";
    ring.style.pointerEvents = "none";
    ring.style.zIndex = "200";
    document.body.appendChild(ring);
    ring.addEventListener("animationend", () => ring.remove());
  }

  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    if (!favorite) {
      justFavorited = true;
      burstRing();
      setTimeout(() => { justFavorited = false; }, 320);
    }
    onToggle();
  }
</script>

<button
  bind:this={buttonEl}
  type="button"
  onclick={handleClick}
  class="inline-flex transition-colors cursor-pointer {favorite
    ? 'text-brand-accent-text'
    : 'text-brand-text-secondary/60 hover:text-brand-accent-text'}"
  title={favorite ? i18n.t('rating.unfavoriteTooltip') : i18n.t('rating.favoriteTooltip')}
  aria-pressed={favorite}
>
  <Heart class="{sizeClass} {favorite ? 'fill-current' : ''} {justFavorited ? 'anim-heart-pulse' : ''}" />
</button>
