import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import FoldersView from "./FoldersView.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_all_app_settings") {
      return Promise.resolve({ active_settings_tab: "general" });
    }
    if (cmd === "get_commit_hash") {
      return Promise.resolve("048f421");
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

  it("renders the version number and build commit hash in the General Settings view", async () => {
    const { findByText } = render(FoldersView);
    expect(await findByText(/v0\.75\.0/)).toBeInTheDocument();
    expect(await findByText(/build 048f421/)).toBeInTheDocument();
  });

  it("renders Dynamic Themes row and footnotes for Luminous and System in Themes view", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_all_app_settings") {
        return Promise.resolve({ active_settings_tab: "themes" });
      }
      return Promise.resolve([]);
    });

    const { findByText } = render(FoldersView);
    expect(await findByText("Dynamic Themes")).toBeInTheDocument();
    expect(await findByText("Colors shift to match whatever album art is playing now")).toBeInTheDocument();
    expect(await findByText("Switches between light and dark to match your OS")).toBeInTheDocument();
  });
});

