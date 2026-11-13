import { describe, it, expect } from "vitest";
import { ORGANIZE_PRESETS } from "./organizePresets";
import { en } from "../locales/en";

/**
 * Folder separators in a pattern. Whether a level is `optional` isn't derivable
 * from the pattern alone: Default's `{%album/}` is a conditional block, yet
 * Organize holds back songs without an album, so that folder always exists.
 */
function countSeparators(template: string): number {
  return [...template].filter((ch) => ch === "/").length;
}

function lookup(key: string): unknown {
  return key.split(".").reduce<unknown>((node, part) => (node as any)?.[part], en);
}

describe("ORGANIZE_PRESETS outlines", () => {
  for (const preset of ORGANIZE_PRESETS) {
    describe(preset.id, () => {
      it("has one folder row per separator in its pattern, then a single file row", () => {
        const folders = preset.outline.filter((r) => r.kind === "folder");
        expect(folders).toHaveLength(countSeparators(preset.template));
        expect(preset.outline.filter((r) => r.kind === "file")).toHaveLength(1);
        expect(preset.outline.at(-1)?.kind).toBe("file");
      });

      it("only uses label keys that exist", () => {
        for (const key of [preset.labelKey, ...preset.outline.map((r) => r.labelKey)]) {
          expect(typeof lookup(key), key).toBe("string");
        }
      });
    });
  }
});
