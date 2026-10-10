import { invoke } from "@tauri-apps/api/core";
import { untrack } from "svelte";
import type { Song } from "../types";
import { navigationStore } from "./navigation.svelte";
import { playerStore } from "./player.svelte";

/** What the info sidebar describes. */
interface InspectorSubject {
  kind: "song" | "album" | "artist";
  /** Drives the panel's tags and context lookup; for an album/artist, a representative song of it. */
  song: Song;
  source: "selection" | "view" | "playing";
  /** Changes if and only if the subject changes — use it to key fetches and caches. */
  key: string;
}

interface Entity {
  kind: "album" | "artist";
  name: string;
}

/**
 * Answers one question: "what should the info sidebar describe right now?"
 *
 * Precedence: a song selected in a song table > the album/artist whose detail
 * view is open > the playing song. Callers never see this rule: song tables
 * report their single selection with `setSelection`, the viewed entity comes
 * from `navigationStore`, and the playing song from `playerStore`.
 */
class InspectorStore {
  /** Single-song selections by reporting view, oldest first; the newest wins. */
  private selections = $state<{ viewId: string; song: Song }[]>([]);
  private resolved = $state<{ key: string; song: Song } | null>(null);
  private requestId = 0;

  constructor() {
    // Resolving an album/artist to a representative song is async, so it runs
    // as a store-owned effect rather than leaking into every consumer.
    $effect.root(() => {
      $effect(() => {
        const entity = this.viewedEntity;
        if (!entity) {
          this.resolved = null;
          return;
        }
        void this.resolveEntity(entity);
      });
    });
  }

  private get viewedEntity(): Entity | null {
    if (navigationStore.activeTab !== "collection") return null;
    // CollectionView shows the album view in preference to the artist view.
    if (navigationStore.selectedAlbumName !== null) return { kind: "album", name: navigationStore.selectedAlbumName };
    if (navigationStore.selectedArtistName !== null) return { kind: "artist", name: navigationStore.selectedArtistName };
    return null;
  }

  private async resolveEntity(entity: Entity) {
    const id = ++this.requestId;
    const key = `${entity.kind}:${entity.name}`;
    let song: Song | undefined;
    try {
      const songs =
        entity.kind === "album"
          ? await invoke<Song[]>("get_songs_by_album", { album: entity.name })
          : await invoke<Song[]>("get_songs_by_artist", { artist: entity.name });
      song = songs?.[0];
    } catch {
      // An entity we can't resolve is simply one with no context to show.
    }
    if (id !== this.requestId) return;
    this.resolved = song ? { key, song } : null;
  }

  get subject(): InspectorSubject | null {
    const selected = this.selections.at(-1);
    if (selected) return { kind: "song", song: selected.song, source: "selection", key: `song:${selected.song.id}` };

    const entity = this.viewedEntity;
    if (entity && this.resolved?.key === `${entity.kind}:${entity.name}`) {
      return { kind: entity.kind, song: this.resolved.song, source: "view", key: this.resolved.key };
    }

    const playing = playerStore.currentSong;
    if (playing) return { kind: "song", song: playing, source: "playing", key: `song:${playing.id}` };
    return null;
  }

  /**
   * Reports the sole selected song in `viewId`, or null to clear it. Idempotent;
   * clearing a view that reported nothing is a no-op. A multi-selection has no
   * single subject and should be reported as null.
   */
  setSelection(viewId: string, song: Song | null): void {
    // Untracked: callers invoke this from effects, which must not subscribe to the list they write.
    untrack(() => {
      const rest = this.selections.filter((s) => s.viewId !== viewId);
      this.selections = song ? [...rest, { viewId, song }] : rest;
    });
  }

  /** Drops every reported selection (test and teardown helper). */
  clearAll(): void {
    this.selections = [];
    this.resolved = null;
  }
}

export const inspectorStore = new InspectorStore();
