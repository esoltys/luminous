import { describe, it, expect } from "vitest";
import { boxBlurRGBA } from "./boxBlur";

function image(width: number, height: number, fill: (x: number, y: number) => number): Uint8ClampedArray {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      data[i] = data[i + 1] = data[i + 2] = fill(x, y);
      data[i + 3] = 255;
    }
  }
  return data;
}

describe("boxBlurRGBA", () => {
  it("leaves a flat image unchanged", () => {
    const data = image(8, 8, () => 120);
    boxBlurRGBA(data, 8, 8, 2);
    expect([...data].every((v, i) => v === (i % 4 === 3 ? 255 : 120))).toBe(true);
  });

  it("spreads a bright point into its neighbours, symmetrically", () => {
    const data = image(9, 9, (x, y) => (x === 4 && y === 4 ? 255 : 0));
    boxBlurRGBA(data, 9, 9, 1);
    const at = (x: number, y: number) => data[(y * 9 + x) * 4];
    expect(at(4, 4)).toBeLessThan(255);
    expect(at(3, 4)).toBeGreaterThan(0);
    expect(at(3, 4)).toBe(at(5, 4));
    expect(at(4, 3)).toBe(at(4, 5));
    expect(at(0, 0)).toBe(0);
  });

  it("is a no-op for radius 0", () => {
    const data = image(4, 4, (x) => x * 60);
    const before = [...data];
    boxBlurRGBA(data, 4, 4, 0);
    expect([...data]).toEqual(before);
  });
});
