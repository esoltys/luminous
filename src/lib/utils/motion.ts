/**
 * Reduced-motion-aware drop-ins for `svelte/transition`.
 *
 * Svelte transitions run in JavaScript, so the `@media (prefers-reduced-motion)`
 * rules in app.css/animations.css can't reach them. Import `fly`, `slide`,
 * `scale` and `fade` from here instead of `svelte/transition`: with the OS
 * "reduce motion" preference on, the transform-based ones degrade to an
 * opacity fade of the same duration ("fewer and gentler, never zero"), and
 * `slide` — which animates layout size rather than a transform, so a fade
 * would still make neighbours jump — collapses to an instant change.
 *
 * The preference is read each time a transition starts, so a change in OS
 * settings applies to the next transition without a reload.
 * `prefersReducedMotion()` is also reactive when read from a template or
 * effect, for motion that isn't a Svelte transition (e.g. a CSS class that
 * swaps a 3D flip for a cross-fade).
 */
import {
  fade,
  fly as svelteFly,
  scale as svelteScale,
  slide as svelteSlide,
  type FlyParams,
  type ScaleParams,
  type SlideParams,
  type TransitionConfig,
} from "svelte/transition";
import { MediaQuery } from "svelte/reactivity";

export { fade };

// Created on first read, not at import: MediaQuery calls matchMedia in its
// constructor, which test environments (jsdom) don't provide.
let reducedMotionQuery: MediaQuery | undefined;

export function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") return false;
  reducedMotionQuery ??= new MediaQuery("(prefers-reduced-motion: reduce)");
  return reducedMotionQuery.current;
}

export function fly(node: Element, params: FlyParams = {}): TransitionConfig {
  if (!prefersReducedMotion()) return svelteFly(node, params);
  return fade(node, { delay: params.delay, duration: params.duration, easing: params.easing });
}

export function scale(node: Element, params: ScaleParams = {}): TransitionConfig {
  if (!prefersReducedMotion()) return svelteScale(node, params);
  return fade(node, { delay: params.delay, duration: params.duration, easing: params.easing });
}

export function slide(node: Element, params: SlideParams = {}): TransitionConfig {
  return svelteSlide(node, prefersReducedMotion() ? { ...params, duration: 0 } : params);
}
