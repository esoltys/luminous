import { getContext, setContext } from "svelte";

/**
 * How the shared cards and list rows draw themselves.
 *
 * - `boxed`: a filled, bordered, rounded box (collection views, artist pages, Stats).
 * - `plain`: no box. Tiles show a bare cover; rows sit on the page separated by a
 *   hairline. Home uses this so covers are not framed twice (#2044).
 *
 * A view opts a whole subtree in with `provideCardAppearance("plain")`; each card
 * reads it with `useCardAppearance()` during its own init. Context rather than a
 * prop so the choice does not have to be threaded through every intermediate
 * component (PinnedRow, TopTenList, ...) down to the card that draws the box.
 */
export type CardAppearance = "boxed" | "plain";

const APPEARANCE_KEY = Symbol("card-appearance");

/** Call once from a view's script; every card rendered beneath it follows. */
export function provideCardAppearance(appearance: CardAppearance): void {
  setContext(APPEARANCE_KEY, appearance);
}

/** Call during component init. Defaults to `boxed` outside any provider. */
export function useCardAppearance(): CardAppearance {
  return getContext<CardAppearance | undefined>(APPEARANCE_KEY) ?? "boxed";
}

/** Shared by every compact list row (cover, two text lines, optional trailing bits). */
const ROW_BASE = "group flex items-center gap-3 px-3 py-2.5 select-none";

const ROW_BOXED =
  "rounded-lg bg-brand-sidebar border border-brand-border/60 outline-2 -outline-offset-2 outline-transparent hover:outline-brand-accent transition-[outline-color,border-color] duration-200";

// Hover tint only; keyboard focus keeps a visible outline since the tint alone
// would leave focus invisible.
const ROW_PLAIN =
  "border-b border-brand-border/60 outline-2 -outline-offset-2 outline-transparent focus-visible:outline-brand-accent hover:bg-brand-sidebar transition-colors duration-200";

/** Classes for a compact list row, without any cursor/width/overflow extras. */
export function rowClasses(appearance: CardAppearance): string {
  return `${ROW_BASE} ${appearance === "plain" ? ROW_PLAIN : ROW_BOXED}`;
}

/** The cover square of a tile: gets the hover/focus outline when the box is gone. */
export const PLAIN_TILE_COVER =
  "rounded-lg outline-2 -outline-offset-2 outline-transparent group-hover:outline-brand-accent group-focus-within:outline-brand-accent transition-[outline-color] duration-200";
