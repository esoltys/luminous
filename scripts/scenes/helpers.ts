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

/** Moves the pointer to a point (CSS pixels) so :hover styles apply there. */
export async function hoverAt(ctx: SceneContext, x: number, y: number): Promise<void> {
  await ctx.driver.send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
}
