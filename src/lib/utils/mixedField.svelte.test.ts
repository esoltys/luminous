import { describe, it, expect } from "vitest";
import { MixedField } from "./mixedField.svelte";

describe("MixedField", () => {
  it("shows the value every track shares and keeps it unless edited", () => {
    const field = new MixedField<number>();
    field.reset([2, 2]);
    expect(field.value).toBe(2);
    expect(field.showsMixed).toBe(false);
    expect(field.edit).toEqual({ mode: "keep" });
  });

  it("reads as mixed when tracks differ, counting blank as no value", () => {
    const field = new MixedField<string>();
    field.reset(["BBC Sessions", ""]);
    expect(field.value).toBeNull();
    expect(field.showsMixed).toBe(true);

    field.reset([null, undefined, ""]);
    expect(field.showsMixed).toBe(false);
  });

  it("sends the user's edit, including clearing it, for every track", () => {
    const field = new MixedField<number>();
    field.reset([1, 2]);
    field.set(null);
    expect(field.showsMixed).toBe(false);
    expect(field.edit).toEqual({ mode: "set", value: null });

    field.reset([1, 2]);
    expect(field.edit).toEqual({ mode: "keep" });
  });

  it("snapshot puts back the value and whether it was edited", () => {
    const field = new MixedField<string>();
    field.reset(["A", "B"]);
    const restore = field.snapshot();
    field.set("Various Artists");
    restore();
    expect(field.value).toBeNull();
    expect(field.showsMixed).toBe(true);
    expect(field.edit).toEqual({ mode: "keep" });
  });
});
