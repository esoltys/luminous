import "@testing-library/jest-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render } from "@testing-library/svelte";
import SettingsGeneral from "./SettingsGeneral.svelte";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
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

describe("SettingsGeneral.svelte", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the version number and build commit hash", async () => {
    const { findByText } = render(SettingsGeneral);
    expect(await findByText(/v0\.75\.0/)).toBeInTheDocument();
    expect(await findByText(/build 048f421/)).toBeInTheDocument();
  });
});
