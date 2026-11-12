import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  POINTER_DRAG_THRESHOLD_PX,
  startPointerDrag,
  type PointerDragOptions,
} from "./pointerDrag";

let host: HTMLElement;
let child: HTMLButtonElement;

/** Builds a pointerdown whose `currentTarget` is `host`, as it is inside a component handler. */
function press(opts: PointerDragOptions, init: PointerEventInit = {}, target: HTMLElement = host) {
  let handle: ReturnType<typeof startPointerDrag> = null;
  const onDown = (e: PointerEvent) => {
    handle = startPointerDrag(e, opts);
  };
  host.addEventListener("pointerdown", onDown, { once: true });
  target.dispatchEvent(
    new MouseEvent("pointerdown", { bubbles: true, clientX: 10, clientY: 10, button: 0, ...init }),
  );
  return handle as ReturnType<typeof startPointerDrag>;
}

function fire(type: string, init: MouseEventInit = {}) {
  window.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true, ...init }));
}

describe("startPointerDrag", () => {
  beforeEach(() => {
    host = document.createElement("div");
    child = document.createElement("button");
    host.appendChild(child);
    document.body.appendChild(host);
    host.setPointerCapture = vi.fn();
    host.releasePointerCapture = vi.fn();
  });

  afterEach(() => {
    // Retract any lingering click guard from the previous test.
    fire("keydown");
    host.remove();
  });

  it("treats release below the threshold as a tap, never arming", () => {
    const o = { onArm: vi.fn(), onMove: vi.fn(), onDrop: vi.fn(), onTap: vi.fn() };
    press(o);
    fire("pointermove", { clientX: 10 + POINTER_DRAG_THRESHOLD_PX - 1, clientY: 10 });
    fire("pointerup");
    expect(o.onArm).not.toHaveBeenCalled();
    expect(o.onMove).not.toHaveBeenCalled();
    expect(o.onDrop).not.toHaveBeenCalled();
    expect(o.onTap).toHaveBeenCalledTimes(1);
  });

  it("arms exactly at the threshold and reports the arming move to onMove too", () => {
    const o = { onArm: vi.fn(), onMove: vi.fn() };
    press(o);
    fire("pointermove", { clientX: 10 + POINTER_DRAG_THRESHOLD_PX, clientY: 10 });
    expect(o.onArm).toHaveBeenCalledWith({ x: 14, y: 10 }, expect.anything());
    expect(o.onMove).toHaveBeenCalledTimes(1);
    fire("pointermove", { clientX: 30, clientY: 10 });
    expect(o.onArm).toHaveBeenCalledTimes(1);
    expect(o.onMove).toHaveBeenLastCalledWith({ x: 30, y: 10 }, expect.anything());
  });

  it("calls onDrop with the release position after arming", () => {
    const o = { onDrop: vi.fn(), onTap: vi.fn() };
    press(o);
    fire("pointermove", { clientX: 40, clientY: 10 });
    fire("pointerup", { clientX: 42, clientY: 11 });
    expect(o.onDrop).toHaveBeenCalledWith({ x: 42, y: 11 }, expect.anything());
    expect(o.onTap).not.toHaveBeenCalled();
  });

  it("stops listening after the release", () => {
    const o = { onMove: vi.fn() };
    press(o);
    fire("pointerup");
    fire("pointermove", { clientX: 90, clientY: 90 });
    expect(o.onMove).not.toHaveBeenCalled();
  });

  it("reports pointercancel with whether a live drag was aborted", () => {
    const early = { onCancel: vi.fn(), onDrop: vi.fn() };
    press(early);
    fire("pointercancel");
    expect(early.onCancel).toHaveBeenCalledWith(false);

    const live = { onCancel: vi.fn(), onDrop: vi.fn() };
    press(live);
    fire("pointermove", { clientX: 40, clientY: 10 });
    fire("pointercancel");
    expect(live.onCancel).toHaveBeenCalledWith(true);
    expect(live.onDrop).not.toHaveBeenCalled();
  });

  it("cancel() on the handle aborts once and is a no-op afterwards", () => {
    const o = { onCancel: vi.fn(), onMove: vi.fn() };
    const handle = press(o);
    handle!.cancel();
    handle!.cancel();
    fire("pointermove", { clientX: 90, clientY: 90 });
    expect(o.onCancel).toHaveBeenCalledTimes(1);
    expect(o.onMove).not.toHaveBeenCalled();
  });

  it("does not call onCancel for a gesture that already ended", () => {
    const o = { onCancel: vi.fn() };
    const handle = press(o);
    fire("pointerup");
    handle!.cancel();
    expect(o.onCancel).not.toHaveBeenCalled();
  });

  it("captures on press by default and releases at the end", () => {
    press({});
    expect(host.setPointerCapture).toHaveBeenCalledTimes(1);
    fire("pointerup");
    expect(host.releasePointerCapture).toHaveBeenCalledTimes(1);
  });

  it("with captureOn 'arm', captures only once the drag arms", () => {
    press({ captureOn: "arm" });
    expect(host.setPointerCapture).not.toHaveBeenCalled();
    fire("pointermove", { clientX: 12, clientY: 10 });
    expect(host.setPointerCapture).not.toHaveBeenCalled();
    fire("pointermove", { clientX: 40, clientY: 10 });
    expect(host.setPointerCapture).toHaveBeenCalledTimes(1);
  });

  it("still works when setPointerCapture throws or is missing", () => {
    host.setPointerCapture = vi.fn(() => {
      throw new Error("InvalidPointerId");
    });
    const o = { onDrop: vi.fn() };
    press(o);
    fire("pointermove", { clientX: 40, clientY: 10 });
    fire("pointerup");
    expect(o.onDrop).toHaveBeenCalledTimes(1);

    (host as { setPointerCapture?: unknown }).setPointerCapture = undefined;
    const o2 = { onDrop: vi.fn() };
    press(o2);
    fire("pointermove", { clientX: 40, clientY: 10 });
    fire("pointerup");
    expect(o2.onDrop).toHaveBeenCalledTimes(1);
  });

  it("ignores non-primary buttons and presses inside interactive descendants", () => {
    expect(press({}, { button: 2 })).toBeNull();
    expect(press({}, {}, child)).toBeNull();
    expect(host.setPointerCapture).not.toHaveBeenCalled();
  });

  it("honours a custom ignoreSelector in place of the default", () => {
    expect(press({ ignoreSelector: "input" }, {}, child)).not.toBeNull();
    fire("pointerup");
  });

  describe("trailing click", () => {
    function dragAndRelease() {
      press({});
      fire("pointermove", { clientX: 40, clientY: 10 });
      fire("pointerup");
    }

    it("swallows exactly the one click that follows a drag", () => {
      const seen = vi.fn();
      host.addEventListener("click", seen);
      dragAndRelease();
      host.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      host.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      expect(seen).toHaveBeenCalledTimes(1);
    });

    it("does not swallow the click after a plain tap", () => {
      const seen = vi.fn();
      host.addEventListener("click", seen);
      press({});
      fire("pointerup");
      host.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      expect(seen).toHaveBeenCalledTimes(1);
    });

    it("lets a click through after a new press when the drag's own click never came", () => {
      const seen = vi.fn();
      host.addEventListener("click", seen);
      dragAndRelease();
      press({});
      fire("pointerup");
      host.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      expect(seen).toHaveBeenCalledTimes(1);
    });

    it("lets a click through after a key press", () => {
      const seen = vi.fn();
      host.addEventListener("click", seen);
      dragAndRelease();
      fire("keydown");
      host.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true }));
      expect(seen).toHaveBeenCalledTimes(1);
    });
  });
});
