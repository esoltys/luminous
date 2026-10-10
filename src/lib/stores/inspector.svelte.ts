import { invoke } from "@tauri-apps/api/core";
import { untrack } from "svelte";
import type { Song } from "../types";
import { navigationStore } from "./navigation.svelte";
import { playerStore } from "./player.svelte";
import { entitySubject, type ContextSubject } from "./context.svelte";

/**
 * What the info sidebar describes. `song` drives the panel's tags and context lookup (for an
 * album/artist, a representative song of it) and `key` changes if and only if the subject does.
 */
interface InspectorSubject extends ContextSubject {
  source: "selection" | "view" | "playing";
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
  private resolved = $state<ContextSubject | null>(null);
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
    let subject: ContextSubject | null = null;
    try {
      const songs =
        entity.kind === "album"
          ? await invoke<Song[]>("get_songs_by_album", { album: entity.name })
          : await invoke<Song[]>("get_songs_by_artist", { artist: entity.name });
      subject = entitySubject(entity.kind, entity.name, songs ?? []);
    } catch {
      // An entity we can't resolve is simply one with no context to show.
    }
    if (id !== this.requestId) return;
    this.resolved = subject;
  }

  get subject(): InspectorSubject | null {
    const selected = this.selections.at(-1);
    if (selected) return { kind: "song", song: selected.song, source: "selection", key: `song:${selected.song.id}`, name: selected.song.artist ?? "" };

    const entity = this.viewedEntity;
    if (entity && this.resolved?.key === `${entity.kind}:${entity.name}`) {
      return { ...this.resolved, source: "view" };
    }

    const playing = playerStore.currentSong;
    if (playing) return { kind: "song", song: playing, source: "playing", key: `song:${playing.id}`, name: playing.artist ?? "" };
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
