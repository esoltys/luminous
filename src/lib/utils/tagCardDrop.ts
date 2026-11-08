/**
 * Drop rules shared by the genre cards and the artist-tag cards, which work the same way: each
 * card is a top-level tag holding sub-tag chips. Both the drop itself and the drag ghost's
 * "over a valid target" highlight come from here, so the highlight never promises a no-op.
 */

/** What is under the pointer: a card's body, its header, or one of its chips. */
export type TagCardDropTarget = { kind: "card" | "header" | "chip"; group: string; chip?: string };

export type ChipDropAction = "promote" | "reparent" | "reorder";

/** Whether dropping a whole card on `target` does anything: anywhere on another card makes the
 * dragged card a sub-tag of that card; its own card does nothing. */
export function cardDropApplies(card: string, target: TagCardDropTarget | null): boolean {
  return target !== null && target.group !== card;
}

/**
 * What dropping a chip on `target` does:
 * - its own card's header: promote it to a top-level card;
 * - anywhere on another card: move it under that card;
 * - another chip on its own card: move it to that chip's position;
 * - anything else (its own card's body, itself, nothing): null.
 */
export function chipDropAction(
  chip: { name: string; fromGroup: string },
  target: TagCardDropTarget | null
): ChipDropAction | null {
  if (!target) return null;
  if (target.kind === "header") return target.group === chip.fromGroup ? "promote" : null;
  if (target.group !== chip.fromGroup) return "reparent";
  if (target.kind === "chip" && target.chip !== chip.name) return "reorder";
  return null;
}
