<script lang="ts" generics="T">
  import { onDestroy } from "svelte";
  import { buildAlphabetIndex } from "../utils/alphabetIndex";
  import { i18n } from "../stores/i18n.svelte";

  interface Props {
    items: T[];
    getKey: (item: T) => string | null | undefined;
    ascending?: boolean;
    onjump: (index: number, letter: string) => void;
    /** Minimum item count required before the scrubber is rendered. Defaults to 15. */
    minItems?: number;
    class?: string;
    /** Active locale to adapt alphabet sections. Defaults to i18n.currentLocale. */
    locale?: string;
  }

  let {
    items,
    getKey,
    ascending = true,
    onjump,
    minItems = 15,
    class: className = "",
    locale,
  }: Props = $props();

  let scrubberEl = $state<HTMLDivElement>();
  let isScrubbing = $state(false);
  let activeLetter = $state<string | null>(null);
  let bubbleY = $state<number | null>(null);
  let dismissTimer: ReturnType<typeof setTimeout> | null = null;

  onDestroy(() => {
    if (dismissTimer) clearTimeout(dismissTimer);
  });

  let activeLocale = $derived(locale ?? i18n.currentLocale);
  let indexData = $derived(buildAlphabetIndex(items, getKey, ascending, activeLocale));

  function updateFromPointer(clientY: number) {
    if (!scrubberEl || indexData.sections.length === 0) return;
    const rect = scrubberEl.getBoundingClientRect();
    const clampedY = Math.max(0, Math.min(clientY - rect.top, rect.height - 1));
    const secIndex = Math.min(
      indexData.sections.length - 1,
      Math.floor((clampedY / rect.height) * indexData.sections.length)
    );
    const section = indexData.sections[secIndex];
    if (section) {
      activeLetter = section.letter;
      bubbleY = (secIndex + 0.5) * (rect.height / indexData.sections.length);
      const targetIndex = indexData.getJumpIndex(section.letter);
      if (targetIndex !== -1) {
        onjump(targetIndex, section.letter);
      }
    }
  }

  function handlePointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    if (dismissTimer) clearTimeout(dismissTimer);
    isScrubbing = true;
    try {
      scrubberEl?.setPointerCapture(e.pointerId);
    } catch {
      // Ignore in environments where setPointerCapture might not be supported
    }
    updateFromPointer(e.clientY);
  }

  function handlePointerMove(e: PointerEvent) {
    if (!isScrubbing) return;
    updateFromPointer(e.clientY);
  }

  function handlePointerUp(e: PointerEvent) {
    if (!isScrubbing) return;
    isScrubbing = false;
    try {
      scrubberEl?.releasePointerCapture(e.pointerId);
    } catch {
      // Ignore
    }
    dismissTimer = setTimeout(() => {
      if (!isScrubbing) activeLetter = null;
    }, 350);
  }

  function handleLetterClick(letter: string, secIndex: number) {
    if (dismissTimer) clearTimeout(dismissTimer);
    activeLetter = letter;
    if (scrubberEl) {
      const rect = scrubberEl.getBoundingClientRect();
      bubbleY = (secIndex + 0.5) * (rect.height / indexData.sections.length);
    }
    const targetIndex = indexData.getJumpIndex(letter);
    if (targetIndex !== -1) {
      onjump(targetIndex, letter);
    }
    dismissTimer = setTimeout(() => {
      if (!isScrubbing) activeLetter = null;
    }, 350);
  }
</script>

{#if items.length >= minItems}
  <div
    class="relative h-full flex flex-col items-center select-none {className}"
    data-testid="alphabet-scrubber-container"
  >
    <!-- Floating active letter bubble -->
    {#if activeLetter && bubbleY !== null}
      <div
        class="absolute right-7 -translate-y-1/2 w-9 h-9 rounded-full bg-brand-accent text-brand-surface font-bold text-sm flex items-center justify-center shadow-lg pointer-events-none z-30 transition-all duration-75"
        style="top: {bubbleY}px;"
        data-testid="alphabet-scrubber-bubble"
      >
        {activeLetter}
      </div>
    {/if}

    <!-- Scrubber strip -->
    <div
      bind:this={scrubberEl}
      role="navigation"
      aria-label={i18n.t("collection.alphabetScrubberLabel")}
      data-testid="alphabet-scrubber"
      class="flex flex-col items-center justify-between py-1.5 px-1 rounded-full bg-brand-sidebar/85 hover:bg-brand-sidebar/95 border border-brand-border/40 backdrop-blur-xs transition-colors shadow-xs touch-none h-full w-5"
      onpointerdown={handlePointerDown}
      onpointermove={handlePointerMove}
      onpointerup={handlePointerUp}
      onpointercancel={handlePointerUp}
    >
      {#each indexData.sections as section, i (section.letter)}
        <button
          type="button"
          tabindex="-1"
          data-scrubber-letter={section.letter}
          aria-label={i18n.t("collection.jumpToLetter", { letter: section.letter })}
          class="w-full flex-1 min-h-[10px] sm:min-h-[12px] max-h-[28px] flex items-center justify-center text-[10px] sm:text-[11px] leading-none font-semibold transition-all rounded-xs focus:outline-none text-brand-text-secondary hover:text-brand-accent-text hover:font-bold hover:scale-130
            {activeLetter === section.letter ? 'text-brand-accent-text font-bold scale-140' : ''}"
          onclick={(e) => {
            e.stopPropagation();
            handleLetterClick(section.letter, i);
          }}
        >
          {section.letter}
        </button>
      {/each}
    </div>
  </div>
{/if}
