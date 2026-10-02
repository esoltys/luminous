<script lang="ts">
  import { HeartIcon as Heart, HeartBreakIcon as HeartBreak } from "phosphor-svelte";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    /** Tri-state loved: 1 = loved, 0 = neutral, -1 = hated. */
    loved?: number;
    /** Backwards-compatible boolean favourite flag. */
    favorite?: boolean;
    onToggle?: () => void;
    onSetLoved?: (loved: number) => void;
    sizeClass?: string;
  }

  let { loved, favorite, onToggle, onSetLoved, sizeClass = "w-4 h-4" }: Props = $props();

  let buttonEl: HTMLButtonElement | undefined = $state();

  // Tri-state: 1 = loved, 0 = neutral, -1 = hated
  let lovedState = $derived(
    loved !== undefined ? loved : (favorite ? 1 : 0)
  );

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

  function emitChange(next: number) {
    if (onSetLoved) {
      onSetLoved(next);
    } else if (onToggle) {
      onToggle();
    }
  }

  function handleClick(e: MouseEvent) {
    e.stopPropagation();
    let next: number;
    if (lovedState === 1) {
      next = 0;
    } else if (lovedState === -1) {
      // clicking broken heart clears back to 0
      next = 0;
    } else {
      next = 1;
      justFavorited = true;
      burstRing();
      setTimeout(() => { justFavorited = false; }, 320);
    }
    emitChange(next);
  }

  function handleContextMenu(e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    // Secondary/menu action sets -1 (Hate). If already -1, clears back to 0.
    const next = lovedState === -1 ? 0 : -1;
    emitChange(next);
  }
</script>

<button
  bind:this={buttonEl}
  type="button"
  onclick={handleClick}
  oncontextmenu={handleContextMenu}
  class="inline-flex items-center justify-center transition-colors {lovedState === 1
    ? 'text-brand-accent-text'
    : lovedState === -1
      ? 'text-brand-accent-text/80 hover:text-brand-text-primary'
      : 'text-brand-text-primary/60 hover:text-brand-accent-text'}"
  title={lovedState === 1
    ? i18n.t('rating.unfavoriteTooltip', {}, 'Remove from favourites')
    : lovedState === -1
      ? i18n.t('rating.clearHateTooltip', {}, 'Clear dislike')
      : i18n.t('rating.favoriteTooltip', {}, 'Add to favourites')}
  aria-label={lovedState === 1
    ? i18n.t('rating.unfavoriteTooltip', {}, 'Remove from favourites')
    : lovedState === -1
      ? i18n.t('rating.clearHateTooltip', {}, 'Clear dislike')
      : i18n.t('rating.favoriteTooltip', {}, 'Add to favourites')}
  aria-pressed={lovedState === 1}
>
  {#if lovedState === -1}
    <HeartBreak weight="fill" class={sizeClass} />
  {:else if lovedState === 1}
    <Heart weight="fill" class="{sizeClass} {justFavorited ? 'anim-heart-pulse' : ''}" />
  {:else}
    <Heart weight="duotone" class={sizeClass} />
  {/if}
</button>
