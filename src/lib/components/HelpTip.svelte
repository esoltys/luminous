<!--
  A hint that isn't hover-only: the trigger is a real, focusable button, and
  the hint text opens on mouse hover *and* on keyboard focus (:focus-visible
  only, so clicking the trigger doesn't pin it open). It closes on pointer
  leave, blur, Escape, any press, or wheel. The text is also always in the DOM
  as the trigger's accessible description — and, via `describes`, the
  description of the form control it explains — so screen readers get it
  without opening anything.

  Use this instead of `title=` whenever the hint is the only place that
  information appears.
-->
<script lang="ts">
  import type { Snippet } from "svelte";
  import { InfoIcon as Info } from "phosphor-svelte";
  import { portal } from "../utils/portal";

  interface Props {
    text: string;
    /** Accessible name for the trigger; defaults to the hint text itself. */
    label?: string;
    /** id of a form control this hint explains — it gets `aria-describedby` too. */
    describes?: string;
    /** Trigger content; defaults to a small info icon. */
    children?: Snippet;
    class?: string;
  }

  let { text, label, describes, children, class: className = "" }: Props = $props();

  const descriptionId = `help-tip-${Math.random().toString(36).slice(2, 10)}`;
  let trigger = $state<HTMLButtonElement>();
  let open = $state(false);
  let position = $state({ left: 0, top: 0, placeAbove: true });

  const GAP = 6;
  const EDGE = 8;

  function show() {
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    // Above unless there's clearly no room; the popover is at most a few lines.
    const placeAbove = rect.top > 96;
    position = {
      left: rect.left + rect.width / 2,
      top: placeAbove ? rect.top - GAP : rect.bottom + GAP,
      placeAbove,
    };
    open = true;
  }

  function hide() {
    open = false;
  }

  function onFocus() {
    if (trigger?.matches(":focus-visible")) show();
  }

  // Keep the popover inside the viewport horizontally once its width is known.
  function clampToViewport(node: HTMLElement) {
    const rect = node.getBoundingClientRect();
    const overflowRight = rect.right - (window.innerWidth - EDGE);
    if (overflowRight > 0) node.style.translate = `${-overflowRight}px 0`;
    else if (rect.left < EDGE) node.style.translate = `${EDGE - rect.left}px 0`;
  }

  $effect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") hide();
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("pointerdown", hide, true);
    window.addEventListener("wheel", hide, { capture: true, passive: true });
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("pointerdown", hide, true);
      window.removeEventListener("wheel", hide, true);
    };
  });

  $effect(() => {
    if (!describes) return;
    const target = document.getElementById(describes);
    if (!target) return;
    const existing = target.getAttribute("aria-describedby")?.split(/\s+/).filter(Boolean) ?? [];
    target.setAttribute("aria-describedby", [...existing, descriptionId].join(" "));
    return () => {
      const remaining = (target.getAttribute("aria-describedby") ?? "").split(/\s+/).filter((id) => id && id !== descriptionId);
      if (remaining.length) target.setAttribute("aria-describedby", remaining.join(" "));
      else target.removeAttribute("aria-describedby");
    };
  });
</script>

<button
  bind:this={trigger}
  type="button"
  aria-label={label ?? text}
  aria-describedby={label ? descriptionId : undefined}
  class="inline-flex items-center justify-center cursor-help rounded-sm focus-visible:outline focus-visible:outline-1 focus-visible:outline-brand-accent {children ? '' : 'text-brand-text-secondary/50 hover:text-brand-text-secondary focus-visible:text-brand-text-secondary'} {className}"
  onpointerenter={(e) => { if (e.pointerType === "mouse") show(); }}
  onpointerleave={hide}
  onfocus={onFocus}
  onblur={hide}
>
  {#if children}
    {@render children()}
  {:else}
    <Info class="w-3 h-3" />
  {/if}
</button>
<!-- `hidden` keeps it out of the reading order; aria-describedby still reads it. -->
<span id={descriptionId} hidden>{text}</span>

{#if open}
  <div
    use:portal
    use:clampToViewport
    aria-hidden="true"
    class="fixed z-[130] max-w-xs -translate-x-1/2 px-2.5 py-1.5 rounded-md border border-brand-border bg-brand-sidebar text-brand-text-primary text-xs font-normal normal-case tracking-normal leading-snug shadow-xl pointer-events-none whitespace-normal {position.placeAbove ? '-translate-y-full' : ''}"
    style="left: {position.left}px; top: {position.top}px;"
  >
    {text}
  </div>
{/if}
