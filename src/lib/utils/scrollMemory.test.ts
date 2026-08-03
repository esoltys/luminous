import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { rememberScroll, watchScrollMemory } from "./scrollMemory";

function makeScrollableDiv(): HTMLDivElement {
  const div = document.createElement("div");
  // jsdom doesn't implement layout, so scrollTop is a plain writable property.
  Object.defineProperty(div, "scrollTop", {
    value: 0,
    writable: true,
  });
  return div;
}

describe("scrollMemory", () => {
  let rafSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    rafSpy = vi.spyOn(window, "requestAnimationFrame").mockImplementation((cb) => {
      cb(0);
      return 0;
    });
  });

  afterEach(() => {
    rafSpy.mockRestore();
  });

  it("restores a previously saved scrollTop for the same key", () => {
    const first = makeScrollableDiv();
    const action1 = rememberScroll(first, "collection:albums");
    first.scrollTop = 250;
    first.dispatchEvent(new Event("scroll"));
    action1.destroy();

    const second = makeScrollableDiv();
    rememberScroll(second, "collection:albums");
    expect(second.scrollTop).toBe(250);
  });

  it("defaults to 0 for a key that was never scrolled", () => {
    const div = makeScrollableDiv();
    rememberScroll(div, "artist-detail:Radiohead");
    expect(div.scrollTop).toBe(0);
  });

  it("re-attaches and restores under the new key when the action updates", () => {
    const div = makeScrollableDiv();
    const action = rememberScroll(div, "collection:albums");
    div.scrollTop = 100;
    div.dispatchEvent(new Event("scroll"));

    action.update("collection:artists");
    expect(div.scrollTop).toBe(0);

    div.scrollTop = 40;
    div.dispatchEvent(new Event("scroll"));

    action.update("collection:albums");
    expect(div.scrollTop).toBe(100);

    action.update("collection:artists");
    expect(div.scrollTop).toBe(40);
  });

  it("ignores scroll events after a falsy key opts out", () => {
    const div = makeScrollableDiv();
    const action = rememberScroll(div, "home");
    div.scrollTop = 300;
    div.dispatchEvent(new Event("scroll"));
    action.update(null);
    div.scrollTop = 999;
    div.dispatchEvent(new Event("scroll"));

    action.update("home");
    expect(div.scrollTop).toBe(300);
  });

  it("supports the non-action watchScrollMemory helper for imperatively-reached elements", () => {
    const div = makeScrollableDiv();
    const detach = watchScrollMemory(div, "songs-viewport");
    div.scrollTop = 75;
    div.dispatchEvent(new Event("scroll"));
    detach();

    const div2 = makeScrollableDiv();
    watchScrollMemory(div2, "songs-viewport");
    expect(div2.scrollTop).toBe(75);
  });
});
