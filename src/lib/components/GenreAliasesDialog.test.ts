import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/svelte";
import { invoke } from "@tauri-apps/api/core";
import GenreAliasesDialog from "./GenreAliasesDialog.svelte";
import { tagsStore } from "../stores/tags.svelte";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("GenreAliasesDialog", () => {
  let aliases: { alias: string; canonical: string; song_count: number }[];

  beforeEach(() => {
    vi.clearAllMocks();
    aliases = [{ alias: "Prog Rock", canonical: "Progressive Rock", song_count: 2 }];
    tagsStore.genreAliases = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: any) => {
      if (cmd === "get_genre_aliases") return aliases;
      if (cmd === "get_tags_overview") return { tags: [], graph: [], no_genre_count: 0 };
      if (cmd === "set_genre_alias") {
        aliases = [...aliases, { alias: args.alias, canonical: args.canonical, song_count: 0 }];
      }
      if (cmd === "remove_genre_alias") {
        aliases = aliases.filter((a) => a.alias !== args.alias);
      }
      return undefined;
    });
  });

  it("lists existing aliases with their song counts", async () => {
    render(GenreAliasesDialog, { props: { onClose: vi.fn() } });
    expect(await screen.findByText("Prog Rock")).toBeTruthy();
    expect(screen.getByText("Progressive Rock")).toBeTruthy();
    expect(screen.getByText("2 songs")).toBeTruthy();
  });

  it("prefills the alias field when opened from the context menu", () => {
    render(GenreAliasesDialog, { props: { initialAlias: "prog-rock", onClose: vi.fn() } });
    expect((screen.getByLabelText("Alias") as HTMLInputElement).value).toBe("prog-rock");
  });

  it("adds a trimmed alias and clears the form", async () => {
    render(GenreAliasesDialog, { props: { onClose: vi.fn() } });
    await fireEvent.input(screen.getByLabelText("Alias"), { target: { value: " prog-rock " } });
    await fireEvent.input(screen.getByLabelText("Treat as"), {
      target: { value: "Progressive Rock" },
    });
    await fireEvent.click(screen.getByText("Add"));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("set_genre_alias", {
        alias: "prog-rock",
        canonical: "Progressive Rock",
      }),
    );
    await waitFor(() =>
      expect((screen.getByLabelText("Alias") as HTMLInputElement).value).toBe(""),
    );
  });

  it("removes an alias", async () => {
    render(GenreAliasesDialog, { props: { onClose: vi.fn() } });
    await fireEvent.click(await screen.findByLabelText("Remove alias"));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("remove_genre_alias", { alias: "Prog Rock" }),
    );
  });
});
