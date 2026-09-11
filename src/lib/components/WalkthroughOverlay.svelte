<script lang="ts">
  import { walkthroughStore } from "../stores/walkthrough.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { portal } from "../utils/portal";
  import { XIcon as X, CaretLeftIcon as ChevronLeft, CaretRightIcon as ChevronRight } from "phosphor-svelte";

  const SPOTLIGHT_PADDING = 8;
  const POPOVER_GAP = 16;
  const VIEWPORT_MARGIN = 16;

  let targetRect = $state<DOMRect | null>(null);
  let popoverEl = $state<HTMLDivElement | undefined>();
  let popoverPos = $state<{ top: number; left: number }>({ top: 0, left: 0 });

  function resolveTarget(): HTMLElement | null {
    const step = walkthroughStore.currentStep;
    if (!step) return null;
    return document.querySelector<HTMLElement>(step.targetSelector);
  }

  // The target element for a step may not be in the DOM for the app's
  // current state (e.g. the player-bar step with nothing ever played, or
  // the right-panel step before its beforeStep hook has had a chance to
  // open the panel) — skip forward rather than spotlighting nothing. This
  // terminates: next() calls finish() once the last step is reached.
  function updateTargetRect() {
    const el = resolveTarget();
    if (!el) {
      targetRect = null;
      if (walkthroughStore.isActive) {
        walkthroughStore.next();
      }
      return;
    }
    targetRect = el.getBoundingClientRect();
  }

  function updatePopoverPosition() {
    if (!targetRect || !popoverEl) return;
    const popoverRect = popoverEl.getBoundingClientRect();
    const placement = walkthroughStore.currentStep?.placement ?? "bottom";
    let top: number;
    let left: number;
    switch (placement) {
      case "top":
        top = targetRect.top - popoverRect.height - POPOVER_GAP;
        left = targetRect.left + targetRect.width / 2 - popoverRect.width / 2;
        break;
      case "left":
        top = targetRect.top + targetRect.height / 2 - popoverRect.height / 2;
        left = targetRect.left - popoverRect.width - POPOVER_GAP;
        break;
      case "right":
        top = targetRect.top + targetRect.height / 2 - popoverRect.height / 2;
        left = targetRect.right + POPOVER_GAP;
        break;
      case "center":
        top = window.innerHeight / 2 - popoverRect.height / 2;
        left = window.innerWidth / 2 - popoverRect.width / 2;
        break;
      case "bottom":
      default:
        top = targetRect.bottom + POPOVER_GAP;
        left = targetRect.left + targetRect.width / 2 - popoverRect.width / 2;
        break;
    }
    top = Math.min(Math.max(top, VIEWPORT_MARGIN), window.innerHeight - popoverRect.height - VIEWPORT_MARGIN);
    left = Math.min(Math.max(left, VIEWPORT_MARGIN), window.innerWidth - popoverRect.width - VIEWPORT_MARGIN);
    popoverPos = { top, left };
  }

  $effect(() => {
    if (!walkthroughStore.isActive) {
      targetRect = null;
      return;
    }
    void walkthroughStore.currentStepIndex;
    updateTargetRect();
  });

  $effect(() => {
    if (!targetRect || !popoverEl) return;
    updatePopoverPosition();
  });

  $effect(() => {
    if (!walkthroughStore.isActive || !targetRect) return;
    const el = resolveTarget();
    if (!el) return;
    const observer = new ResizeObserver(() => updateTargetRect());
    observer.observe(el);
    return () => observer.disconnect();
  });

  function handleWindowChange() {
    if (walkthroughStore.isActive) updateTargetRect();
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!walkthroughStore.isActive) return;
    if (e.key === "Escape") {
      e.preventDefault();
      walkthroughStore.skip();
    } else if (e.key === "ArrowRight" || e.key === "Enter") {
      e.preventDefault();
      walkthroughStore.next();
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      walkthroughStore.prev();
    }
  }
</script>

<svelte:window onresize={handleWindowChange} onscroll={handleWindowChange} onkeydown={handleKeydown} />

{#if walkthroughStore.isActive && targetRect && walkthroughStore.currentStep}
  <div use:portal class="fixed inset-0 z-[110] select-none" role="presentation">
    <svg class="absolute inset-0 w-full h-full pointer-events-none" aria-hidden="true">
      <defs>
        <mask id="walkthrough-spotlight-mask">
          <rect x="0" y="0" width="100%" height="100%" fill="white" />
          <rect
            x={targetRect.left - SPOTLIGHT_PADDING}
            y={targetRect.top - SPOTLIGHT_PADDING}
            width={targetRect.width + SPOTLIGHT_PADDING * 2}
            height={targetRect.height + SPOTLIGHT_PADDING * 2}
            rx="12"
            fill="black"
          />
        </mask>
      </defs>
      <rect x="0" y="0" width="100%" height="100%" fill="black" fill-opacity="0.75" mask="url(#walkthrough-spotlight-mask)" />
    </svg>

    <div
      class="absolute border-2 border-brand-accent rounded-xl pointer-events-none transition-[top,left,width,height] duration-200"
      style="top: {targetRect.top - SPOTLIGHT_PADDING}px; left: {targetRect.left - SPOTLIGHT_PADDING}px; width: {targetRect.width + SPOTLIGHT_PADDING * 2}px; height: {targetRect.height + SPOTLIGHT_PADDING * 2}px;"
    ></div>

    <div
      bind:this={popoverEl}
      role="dialog"
      aria-modal="true"
      aria-labelledby="walkthrough-step-title"
      class="absolute w-80 max-w-[calc(100vw-2rem)] bg-brand-sidebar border border-brand-border rounded-2xl shadow-2xl p-5 pointer-events-auto"
      style="top: {popoverPos.top}px; left: {popoverPos.left}px;"
    >
      <div class="flex items-start justify-between gap-3 mb-2">
        <h3 id="walkthrough-step-title" class="text-sm font-bold text-brand-text-primary">
          {i18n.t(walkthroughStore.currentStep.titleKey)}
        </h3>
        <button
          onclick={() => walkthroughStore.skip()}
          class="text-brand-text-secondary hover:text-brand-text-primary p-1 -m-1 rounded-lg hover:bg-brand-main/80 transition-colors shrink-0"
          title={i18n.t('walkthrough.skip', {}, 'Skip')}
          aria-label={i18n.t('walkthrough.skip', {}, 'Skip')}
        >
          <X class="w-4 h-4" />
        </button>
      </div>

      <p class="text-xs text-brand-text-secondary leading-relaxed mb-4">
        {i18n.t(walkthroughStore.currentStep.descriptionKey)}
      </p>

      <div class="flex items-center justify-between gap-3">
        <div class="flex items-center gap-1.5" role="tablist" aria-label={i18n.t('walkthrough.progress', {}, 'Walkthrough progress')}>
          {#each walkthroughStore.steps as step, i (step.id)}
            <span
              class="h-1.5 rounded-full transition-all {i === walkthroughStore.currentStepIndex ? 'w-4 bg-brand-accent' : 'w-1.5 bg-brand-border'}"
            ></span>
          {/each}
        </div>
        <div class="flex items-center gap-2">
          {#if walkthroughStore.currentStepIndex > 0}
            <button
              onclick={() => walkthroughStore.prev()}
              class="flex items-center gap-1 px-3 py-1.5 rounded-full text-xs font-semibold text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-main/80 transition-colors"
            >
              <ChevronLeft class="w-3.5 h-3.5" />
              {i18n.t('walkthrough.back', {}, 'Back')}
            </button>
          {/if}
          <button
            onclick={() => walkthroughStore.next()}
            class="flex items-center gap-1 px-4 py-1.5 rounded-full text-xs font-semibold bg-brand-accent hover:bg-brand-accent-hover text-brand-accent-contrast transition-colors"
          >
            {walkthroughStore.currentStepIndex === walkthroughStore.totalSteps - 1
              ? i18n.t('walkthrough.finish', {}, 'Finish')
              : i18n.t('walkthrough.next', {}, 'Next')}
            {#if walkthroughStore.currentStepIndex < walkthroughStore.totalSteps - 1}
              <ChevronRight class="w-3.5 h-3.5" />
            {/if}
          </button>
        </div>
      </div>
    </div>
  </div>
{/if}
