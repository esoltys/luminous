import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import FoldersView from "./FoldersView.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_all_app_settings") {
      return Promise.resolve({ active_settings_tab: "about" });
    }
    return Promise.resolve([]);
  }),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.75.0"),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

describe("FoldersView.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the version number from getVersion in the About view", async () => {
    const { findByText } = render(FoldersView);
    expect(await findByText("v0.75.0")).toBeInTheDocument();
  });
});
