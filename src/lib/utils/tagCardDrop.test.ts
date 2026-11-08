import { describe, it, expect } from "vitest";
import { cardDropApplies, chipDropAction, type TagCardDropTarget } from "./tagCardDrop";

describe("tag card drop rules", () => {
  const chip = { name: "Doom Metal", fromGroup: "Metal" };

  describe("chipDropAction", () => {
    // Every target kind on the chip's own card and on another card, plus nothing.
    const cases: Array<[string, TagCardDropTarget | null, ReturnType<typeof chipDropAction>]> = [
      ["nothing", null, null],
      ["own card's header", { kind: "header", group: "Metal" }, "promote"],
      ["own card's body", { kind: "card", group: "Metal" }, null],
      ["itself", { kind: "chip", group: "Metal", chip: "Doom Metal" }, null],
      ["another chip on its card", { kind: "chip", group: "Metal", chip: "Black Metal" }, "reorder"],
      ["another card's header", { kind: "header", group: "Rock" }, null],
      ["another card's body", { kind: "card", group: "Rock" }, "reparent"],
      ["a chip on another card", { kind: "chip", group: "Rock", chip: "Indie Rock" }, "reparent"],
    ];
    it.each(cases)("dropped on %s", (_, target, expected) => {
      expect(chipDropAction(chip, target)).toBe(expected);
    });
  });

  describe("cardDropApplies", () => {
    it("applies anywhere on another card", () => {
      expect(cardDropApplies("Metal", { kind: "card", group: "Rock" })).toBe(true);
      expect(cardDropApplies("Metal", { kind: "header", group: "Rock" })).toBe(true);
      expect(cardDropApplies("Metal", { kind: "chip", group: "Rock", chip: "Indie Rock" })).toBe(true);
    });

    it("does nothing on its own card or away from every card", () => {
      expect(cardDropApplies("Metal", { kind: "header", group: "Metal" })).toBe(false);
      expect(cardDropApplies("Metal", { kind: "chip", group: "Metal", chip: "Doom Metal" })).toBe(false);
      expect(cardDropApplies("Metal", null)).toBe(false);
    });
  });
});
