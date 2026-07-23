<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Playlist, PlaylistItem } from "../types";
  import { songsToCoverStack } from "../utils/covers";
  import CoverStack from "./CoverStack.svelte";

  let { playlist, sizeClass = "w-11 h-11" }: { playlist: Playlist; sizeClass?: string } = $props();

  let tracks = $state<PlaylistItem[]>([]);

  $effect(() => {
    const id = playlist.id;
    invoke<PlaylistItem[]>("get_playlist_tracks", { playlistId: id })
      .then((res) => {
        if (playlist.id === id) tracks = res ?? [];
      })
      .catch((err) => {
        console.error("Failed to load playlist tracks for cover:", err);
      });
  });

  let covers = $derived(songsToCoverStack(tracks.filter((t) => !!t.song).map((t) => t.song!)));
</script>

<CoverStack {covers} fallbackName={playlist.name} {sizeClass} />
