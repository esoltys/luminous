import type { SceneContext } from "./types";

interface FoundSong {
  id: number;
  title?: string;
  artist?: string;
}

/** Finds the featured song in the real library, or says which one is missing. */
export async function featuredSongId(ctx: SceneContext): Promise<number> {
  const { song, artist } = ctx.featured;
  const results = await ctx.driver.invoke<FoundSong[]>("search_songs", { query: song ?? "", limit: 100 });
  const match = results.find((s) => s.title === song && (!artist || s.artist === artist));
  if (!match) throw new Error(`"${song}" by ${artist} isn't in the library.`);
  return match.id;
}

/**
 * The library has no hand-made playlists, so make one a listener would: a short
 * mix of the featured artist's songs. Does nothing when one already exists, so every
 * locale reuses it and a cloned profile keeps its own.
 */
export async function ensureCustomPlaylist(ctx: SceneContext, name: string): Promise<void> {
  const existing = await ctx.driver.invoke<Array<{ name: string; is_queue?: boolean; dynamic_enabled?: boolean }>>("get_playlists");
  // A profile with playlists of its own (a clone of a real one) shows those.
  if (existing.some((p) => p.name === name || (!p.is_queue && !p.dynamic_enabled))) return;
  const playlist = await ctx.driver.invoke<{ id: number }>("create_playlist", { name });
  const songs = await ctx.driver.invoke<FoundSong[]>("search_songs", { query: ctx.featured.artist ?? "", limit: 12 });
  await ctx.driver.invoke("add_to_playlist", { playlistId: playlist.id, songIds: songs.map((s) => s.id) });
}

/** Moves the pointer to a point (CSS pixels): :hover styles apply and pointer handlers fire. */
async function hoverAt(ctx: SceneContext, x: number, y: number): Promise<void> {
  await ctx.driver.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
}

/** Hovers the centre of the first element matching `selector`, as a real pointer would. */
export async function hoverOver(ctx: SceneContext, selector: string): Promise<void> {
  const rect = await ctx.driver.evaluate((sel: string) => {
    const r = document.querySelector(sel)?.getBoundingClientRect();
    return r ? { x: r.x + r.width / 2, y: r.y + r.height / 2 } : null;
  }, selector);
  if (!rect) throw new Error(`No element matches "${selector}" to hover.`);
  await hoverAt(ctx, rect.x, rect.y);
}

/** Moves the pointer to the window corner, which hover-aware components treat as "outside". */
export async function unhover(ctx: SceneContext): Promise<void> {
  await hoverAt(ctx, 0, 0);
}
