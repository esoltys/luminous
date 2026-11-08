import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { createEventScope } from "../utils/eventScope";
import type { SongProvenance } from "../types";
import { tasksStore } from "./tasks.svelte";
import { i18n } from "./i18n.svelte";
import { TOAST_DURATION_MS } from "../constants";

export class ProvenanceStore {
  verdicts = $state<Map<number, string>>(new Map());
  private events = createEventScope(import.meta.hot);
  private initPromise: Promise<void> | null = null;

  constructor() {
    this.init();
  }

  init(): Promise<void> {
    if (this.initPromise) return this.initPromise;
    this.initPromise = (async () => {
      try {
        const records = await invoke<Record<string, string>>("get_provenance_verdicts");
        const map = new Map<number, string>();
        if (records) {
          for (const [idStr, verdict] of Object.entries(records)) {
            map.set(Number(idStr), verdict);
          }
        }
        this.verdicts = map;
      } catch (err) {
        console.warn("Failed to load provenance verdicts:", err);
      }

      try {
        await this.events.listen<SongProvenance>("song-provenance-changed", (event) => {
          if (event?.payload?.song_id) {
            const next = new Map(this.verdicts);
            next.set(event.payload.song_id, event.payload.verdict);
            this.verdicts = next;
          }
        });
      } catch (err) {
        console.warn("Failed to listen for song-provenance-changed:", err);
      }
    })();
    return this.initPromise;
  }

  async verifySongs(songIds: number[], contextName?: string): Promise<number> {
    const validIds = songIds.filter((id) => Number.isFinite(id) && id > 0);
    if (validIds.length === 0) return 0;

    const taskId = `provenance-${Date.now()}`;
    const total = validIds.length;

    const startLabel = i18n.t(
      "collection.verifyingAuthenticity",
      {},
      "Verifying audio authenticity..."
    );

    tasksStore.startTask({
      id: taskId,
      label: startLabel,
      total,
      contextName,
    });

    let unlisten: (() => void) | undefined;
    try {
      unlisten = await listen<{ current: number; total: number }>(
        "provenance-progress",
        (event) => {
          if (event?.payload && typeof event.payload.current === "number") {
            tasksStore.updateTask(taskId, {
              current: event.payload.current,
              total: event.payload.total,
            });
          }
        }
      );

      const count = await invoke<number>("analyze_songs_provenance", { songIds: validIds });

      const completeLabel = i18n.t(
        "collection.verifyAuthenticityComplete",
        {},
        "Audio authenticity verification complete"
      );

      tasksStore.completeTask(taskId, completeLabel, TOAST_DURATION_MS);
      return count;
    } catch (err: any) {
      console.error("Provenance verification failed:", err);
      tasksStore.failTask(taskId, err?.toString() || "Audio authenticity verification failed");
      throw err;
    } finally {
      unlisten?.();
    }
  }

  getVerdict(songId: number): string | undefined {
    return this.verdicts.get(songId);
  }

  isVerified(songId: number): boolean {
    return this.verdicts.has(songId);
  }

  dispose() {
    this.events.dispose();
    this.initPromise = null;
  }
}

export const provenanceStore = new ProvenanceStore();
