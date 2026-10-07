<script lang="ts">
  import { onMount } from "svelte";
  import {
    XIcon as X,
    ArrowSquareOutIcon as ExternalLink,
    PencilSimpleIcon as Pencil,
    EyeIcon as Eye,
    WarningCircleIcon as Warning,
    ArrowClockwiseIcon as Reload,
  } from "phosphor-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Button from "./Button.svelte";
  import MarkdownBio from "./MarkdownBio.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { portal } from "../utils/portal";

  let {
    isOpen = $bindable(false),
    title,
    initialValue = "",
    targetType,
    targetKey,
    onApply,
    onClose,
  }: {
    isOpen?: boolean;
    title: string;
    initialValue?: string;
    targetType: "artist" | "album";
    targetKey: string;
    onApply: (value: string) => void;
    onClose: () => void;
  } = $props();

  let mode = $state<"edit" | "preview">("edit");
  let draft = $state("");
  let lastSyncedValue = $state("");
  let isOpeningExternal = $state(false);

  // External edit conflict tracking
  let hasConflict = $state(false);
  let pendingDiskValue = $state("");

  $effect(() => {
    if (isOpen) {
      draft = initialValue ?? "";
      lastSyncedValue = initialValue ?? "";
      mode = "edit";
      hasConflict = false;
      pendingDiskValue = "";
    }
  });

  async function checkDiskContent() {
    if (!isOpen || !targetKey) return;
    try {
      const command = targetType === "artist" ? "read_artist_bio_file" : "read_album_bio_file";
      const argKey = targetType === "artist" ? "artist" : "album";
      const diskContent = await invoke<string | null>(command, { [argKey]: targetKey });

      const normalizedDisk = diskContent ?? "";
      if (normalizedDisk !== lastSyncedValue) {
        // If user hasn't made dirty edits inside Luminous: auto-refresh cleanly
        if (draft === lastSyncedValue) {
          draft = normalizedDisk;
          lastSyncedValue = normalizedDisk;
          hasConflict = false;
        } else {
          // User has unsaved edits in the editor: prompt without overwriting
          pendingDiskValue = normalizedDisk;
          hasConflict = true;
        }
      }
    } catch (e) {
      console.warn("Failed to check disk content:", e);
    }
  }

  function handleReloadFromDisk() {
    draft = pendingDiskValue;
    lastSyncedValue = pendingDiskValue;
    hasConflict = false;
  }

  function handleKeepLocalEdits() {
    lastSyncedValue = pendingDiskValue;
    hasConflict = false;
  }

  async function handleOpenExternal() {
    if (isOpeningExternal) return;
    isOpeningExternal = true;
    try {
      const command = targetType === "artist" ? "open_artist_bio_file" : "open_album_bio_file";
      const argKey = targetType === "artist" ? "artist" : "album";
      await invoke(command, {
        [argKey]: targetKey,
        currentContent: draft,
      });
      lastSyncedValue = draft;
      toastStore.show(i18n.t("markdownEditor.openedToast", {}, "Opened in external editor"), "info");
    } catch (e) {
      console.error("Failed to open external editor:", e);
      toastStore.show(
        typeof e === "string" ? e : i18n.t("markdownEditor.openErrorToast", {}, "Failed to open external editor"),
        "error"
      );
    } finally {
      isOpeningExternal = false;
    }
  }

  function handleApply() {
    onApply(draft);
    handleClose();
  }

  function handleClose() {
    isOpen = false;
    onClose();
  }

  function handleBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget) {
      handleClose();
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if (!isOpen) return;
    if (e.key === "Escape") {
      e.stopPropagation();
      handleClose();
    }
  }

  onMount(() => {
    function onFocus() {
      if (isOpen) {
        checkDiskContent();
      }
    }
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
    };
  });
</script>

<svelte:window onkeydown={handleKeydown} />

{#if isOpen}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    use:portal
    class="fixed inset-0 z-[60] flex items-center justify-center bg-black/75 backdrop-blur-sm p-3 sm:p-6 overflow-hidden"
    onclick={handleBackdropClick}
  >
    <div
      class="w-full max-w-4xl h-[85vh] max-h-[850px] bg-brand-sidebar border border-brand-border rounded-xl shadow-2xl overflow-hidden flex flex-col"
      role="dialog"
      aria-modal="true"
      aria-label={title}
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-4 sm:px-6 py-3 border-b border-brand-border bg-brand-sidebar shrink-0 gap-3">
        <div class="flex items-center gap-2.5 min-w-0 flex-1">
          <Pencil class="w-4 h-4 text-brand-accent shrink-0" />
          <h2 class="text-sm sm:text-base font-bold text-brand-text-primary truncate">
            {title}
          </h2>
        </div>

        <!-- Controls: Tabs, External Open, Close -->
        <div class="flex items-center gap-2 shrink-0">
          <!-- Edit / Preview Mode Switcher -->
          <div class="flex items-center rounded-lg bg-brand-main/60 p-0.5 border border-brand-border text-xs">
            <button
              type="button"
              onclick={() => { mode = "edit"; }}
              class="flex items-center gap-1 px-2.5 py-1 rounded-md font-medium transition-colors cursor-pointer {mode === 'edit'
                ? 'bg-brand-accent text-brand-accent-contrast shadow-sm'
                : 'text-brand-text-secondary hover:text-brand-text-primary'}"
            >
              <Pencil class="w-3.5 h-3.5" />
              <span>{i18n.t("markdownEditor.edit", {}, "Edit")}</span>
            </button>
            <button
              type="button"
              onclick={() => { mode = "preview"; }}
              class="flex items-center gap-1 px-2.5 py-1 rounded-md font-medium transition-colors cursor-pointer {mode === 'preview'
                ? 'bg-brand-accent text-brand-accent-contrast shadow-sm'
                : 'text-brand-text-secondary hover:text-brand-text-primary'}"
            >
              <Eye class="w-3.5 h-3.5" />
              <span>{i18n.t("markdownEditor.preview", {}, "Preview")}</span>
            </button>
          </div>

          <!-- Open in External Editor -->
          <button
            type="button"
            onclick={handleOpenExternal}
            disabled={isOpeningExternal}
            class="flex items-center gap-1.5 px-2.5 py-1 text-xs rounded-lg border border-brand-border bg-brand-main/40 hover:bg-brand-main text-brand-text-primary hover:border-brand-accent/50 transition-colors disabled:opacity-50 cursor-pointer"
            title={i18n.t("markdownEditor.openExternalTooltip", {}, "Open this Markdown file in your default system editor")}
          >
            <ExternalLink class="w-3.5 h-3.5 text-brand-accent" />
            <span class="hidden sm:inline">{i18n.t("markdownEditor.openExternal", {}, "Open in external editor")}</span>
          </button>

          <!-- Close Button -->
          <button
            type="button"
            onclick={handleClose}
            class="p-1.5 text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-accent/10 rounded-md transition-colors shrink-0 cursor-pointer"
            aria-label={i18n.t("common.closeDialog")}
          >
            <X class="w-4 h-4" />
          </button>
        </div>
      </div>

      <!-- Conflict Banner if File Changed on Disk -->
      {#if hasConflict}
        <div class="px-4 py-2.5 bg-amber-500/15 border-b border-amber-500/30 flex items-center justify-between gap-3 text-xs text-amber-300 shrink-0">
          <div class="flex items-center gap-2 min-w-0">
            <Warning class="w-4 h-4 text-amber-400 shrink-0" />
            <span class="truncate">{i18n.t("markdownEditor.conflictDetected", {}, "This file was modified on disk by an external editor.")}</span>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <button
              type="button"
              onclick={handleReloadFromDisk}
              class="flex items-center gap-1 px-2 py-1 rounded bg-amber-500/20 hover:bg-amber-500/30 text-amber-200 border border-amber-500/40 font-medium cursor-pointer transition-colors"
            >
              <Reload class="w-3 h-3" />
              <span>{i18n.t("markdownEditor.reloadFromDisk", {}, "Reload from disk")}</span>
            </button>
            <button
              type="button"
              onclick={handleKeepLocalEdits}
              class="px-2 py-1 rounded bg-brand-main/60 hover:bg-brand-main text-brand-text-secondary hover:text-brand-text-primary font-medium cursor-pointer transition-colors"
            >
              <span>{i18n.t("markdownEditor.keepLocalEdits", {}, "Keep in-app edits")}</span>
            </button>
          </div>
        </div>
      {/if}

      <!-- Main Body: Edit Textarea or Preview -->
      <div class="flex-1 min-h-0 flex flex-col p-4 overflow-hidden bg-brand-main/20">
        {#if mode === "edit"}
          <textarea
            bind:value={draft}
            placeholder={i18n.t("markdownEditor.placeholder", {}, "Write markdown bio or description here...")}
            class="w-full h-full p-4 rounded-lg bg-brand-main/60 border border-brand-border text-brand-text-primary text-sm font-mono leading-relaxed placeholder:text-brand-text-secondary/50 focus:outline-none focus:border-brand-accent focus:ring-1 focus:ring-brand-accent resize-none overflow-y-auto transition-colors"
          ></textarea>
        {:else}
          <div class="w-full h-full p-6 rounded-lg bg-brand-main/40 border border-brand-border overflow-y-auto">
            {#if draft.trim()}
              <MarkdownBio text={draft} disableClamp={true} />
            {:else}
              <p class="text-xs text-brand-text-secondary italic">
                {i18n.t("markdownEditor.emptyPreview", {}, "No content to preview.")}
              </p>
            {/if}
          </div>
        {/if}
      </div>

      <!-- Formatting Cheatsheet -->
      <div class="px-4 sm:px-6 py-2 border-t border-brand-border/60 bg-brand-sidebar/40 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] text-brand-text-secondary select-none shrink-0">
        <span class="font-medium text-brand-text-secondary/70">{i18n.t("markdownEditor.syntaxTitle", {}, "Formatting:")}</span>
        <code class="px-1.5 py-0.5 rounded bg-brand-main/60 border border-brand-border/50 text-brand-text-primary">**{i18n.t("markdownEditor.bold", {}, "bold")}**</code>
        <code class="px-1.5 py-0.5 rounded bg-brand-main/60 border border-brand-border/50 text-brand-text-primary">*{i18n.t("markdownEditor.italic", {}, "italic")}*</code>
        <code class="px-1.5 py-0.5 rounded bg-brand-main/60 border border-brand-border/50 text-brand-text-primary">[{i18n.t("markdownEditor.linkText", {}, "link")}](url)</code>
        <code class="px-1.5 py-0.5 rounded bg-brand-main/60 border border-brand-border/50 text-brand-text-primary">https://...</code>
      </div>

      <!-- Footer -->
      <div class="flex items-center justify-between px-4 sm:px-6 py-3 border-t border-brand-border bg-brand-sidebar shrink-0">
        <div class="text-xs text-brand-text-secondary font-mono">
          {draft.length} {draft.length === 1 ? "char" : "chars"}
        </div>
        <div class="flex items-center gap-2">
          <Button variant="secondary" size="sm" onclick={handleClose}>
            {i18n.t("markdownEditor.cancel", {}, "Cancel")}
          </Button>
          <Button variant="primary" size="sm" onclick={handleApply}>
            {i18n.t("markdownEditor.apply", {}, "Apply Changes")}
          </Button>
        </div>
      </div>
    </div>
  </div>
{/if}
