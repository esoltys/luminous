import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { i18n, formatNumber } from "./i18n.svelte";
import { navigationStore } from "./navigation.svelte";
import { tasksStore } from "./tasks.svelte";
import { toastStore } from "./toast.svelte";
import type { TagCleanupPreview, TagCleanupResult, TagCleanupSelection } from "../types";

const TASK_ID = "tag-cleanup-check";
const REMOVE_TASK_ID = "tag-cleanup-remove";

/**
 * The library-wide tag clean-up (#1939). Reading or rewriting every file can
 * take minutes on a network share, so both steps belong to this store rather
 * than to the review dialog. Closing the dialog sends a check to the
 * background (shown as a task toast), and when it finishes a toast offers to
 * open the review; removal always runs as a task toast.
 */
class TagCleanupStore {
  checking = $state(false);
  progress = $state<{ checked: number; total: number } | null>(null);
  /** The finished check's findings, until the review is closed or a new check starts. */
  preview = $state<TagCleanupPreview | null>(null);
  error = $state<string | null>(null);
  reviewOpen = $state(false);
  removing = $state(false);

  /** Opens the review and starts a fresh check, or shows the one already running. */
  start() {
    this.reviewOpen = true;
    if (this.checking) {
      tasksStore.clearTask(TASK_ID);
      return;
    }
    this.preview = null;
    this.error = null;
    this.progress = null;
    void this.check();
  }

  /** Closes the review; a check still running carries on in the background. */
  close() {
    this.reviewOpen = false;
    this.preview = null;
    if (this.checking) {
      tasksStore.startTask({
        id: TASK_ID,
        label: i18n.t("tagCleanup.checking"),
        taskName: i18n.t("tagCleanup.title"),
        total: this.progress?.total,
      });
      if (this.progress) tasksStore.updateTask(TASK_ID, { current: this.progress.checked });
    }
  }

  /** Stops the running check and closes the review. */
  cancel() {
    invoke("cancel_tag_cleanup").catch(() => {});
    this.reviewOpen = false;
  }

  /** Closes the review and removes `selections` (from any songs) as a background task. */
  async remove(selections: TagCleanupSelection[]) {
    this.close();
    this.removing = true;
    tasksStore.startTask({
      id: REMOVE_TASK_ID,
      label: i18n.t("tagCleanup.removing"),
      taskName: i18n.t("tagCleanup.title"),
      total: selections.length,
    });
    const unlisten = await listen<{ done: number; total: number }>("tag-cleanup-apply-progress", (event) => {
      tasksStore.updateTask(REMOVE_TASK_ID, { current: event.payload.done, total: event.payload.total });
    });
    try {
      const result = await invoke<TagCleanupResult>("apply_tag_cleanup", { selections });
      tasksStore.completeTask(
        REMOVE_TASK_ID,
        i18n.t("tagCleanup.done", {
          count: formatNumber(result.removed_fields),
          songs: formatNumber(result.cleaned_songs),
        }),
      );
      if (result.errors.length > 0) {
        console.warn("Tag clean-up errors:", result.errors);
        toastStore.show(i18n.t("tagCleanup.partialErrors", { count: formatNumber(result.errors.length) }), "error");
      }
    } catch (e) {
      tasksStore.failTask(REMOVE_TASK_ID, i18n.t("tagCleanup.failedPrefix") + String(e));
    } finally {
      unlisten();
      this.removing = false;
    }
  }

  private openReview() {
    navigationStore.activeTab = "organize";
    this.reviewOpen = true;
  }

  private async check() {
    this.checking = true;
    const unlisten = await listen<{ checked: number; total: number }>("tag-cleanup-progress", (event) => {
      this.progress = event.payload;
      if (tasksStore.isTaskActive(TASK_ID)) {
        tasksStore.updateTask(TASK_ID, { current: event.payload.checked, total: event.payload.total });
      }
    });
    try {
      const preview = await invoke<TagCleanupPreview>("preview_tag_cleanup", { songIds: [] });
      if (this.reviewOpen) {
        this.preview = preview;
      } else if (preview.songs.length === 0) {
        tasksStore.completeTask(TASK_ID, i18n.t("tagCleanup.nothingFound"));
      } else {
        tasksStore.clearTask(TASK_ID);
        this.preview = preview;
        const toastId = toastStore.show(
          i18n.t("tagCleanup.readyToReview", { count: formatNumber(preview.songs.length) }),
          "info",
          undefined,
          undefined,
          {
            label: i18n.t("tagCleanup.reviewBtn"),
            onClick: () => {
              toastStore.dismiss(toastId);
              this.openReview();
            },
          },
        );
      }
    } catch (e) {
      // A cancelled check closed its review already; anything else is a real failure.
      if (this.reviewOpen) this.error = String(e);
      else if (tasksStore.isTaskActive(TASK_ID)) tasksStore.failTask(TASK_ID, String(e));
    } finally {
      unlisten();
      this.checking = false;
    }
  }
}

export const tagCleanupStore = new TagCleanupStore();
