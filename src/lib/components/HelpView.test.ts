import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent } from "@testing-library/svelte";
import HelpView from "./HelpView.svelte";
import { openExternalUrl } from "../utils/openExternalUrl";

vi.mock("../utils/openExternalUrl", () => ({
  openExternalUrl: vi.fn(),
}));

describe("HelpView.svelte", () => {
  beforeEach(() => {
    vi.mocked(openExternalUrl).mockClear();
  });

  /** Render the view and stand in a guide page for the iframe's content. */
  async function renderGuide(body: string) {
    const { container } = render(HelpView);
    const frame = container.querySelector("iframe")!;
    const doc = frame.contentDocument!;
    doc.open();
    doc.write(`<!doctype html><html><body>${body}</body></html>`);
    doc.close();
    await fireEvent.load(frame);
    return doc;
  }

  it("opens a web link in the system browser instead of the iframe", async () => {
    const doc = await renderGuide(
      '<p>Look up your model on <a href="https://autoeq.app" target="_blank" rel="noopener"><b>AutoEq</b></a></p>'
    );
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    doc.querySelector("b")!.dispatchEvent(click);
    expect(openExternalUrl).toHaveBeenCalledWith("https://autoeq.app/");
    expect(click.defaultPrevented).toBe(true);
  });

  it("leaves in-page anchors to the guide", async () => {
    const doc = await renderGuide('<a href="#equalizer">Equalizer</a><h2 id="equalizer">Equalizer</h2>');
    const click = new MouseEvent("click", { bubbles: true, cancelable: true });
    doc.querySelector("a")!.dispatchEvent(click);
    expect(openExternalUrl).not.toHaveBeenCalled();
    expect(click.defaultPrevented).toBe(false);
  });
});
