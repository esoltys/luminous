<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { picardStore } from "../stores/picard.svelte";
  import { openInPicard } from "../utils/picard";
  import { slide } from "../utils/motion";
  import { organizeStore, computeOrganizeConsequences, type OrganizeConsequences } from "../stores/organizer.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { TOAST_DURATION_MS } from "../constants";
  import { portal } from "../utils/portal";
  import {
    XIcon as X,
    FolderIcon as Folder,
    SparkleIcon as Sparkles,
    CheckIcon as Check,
    WarningIcon as AlertTriangle,
    ArrowsClockwiseIcon as RefreshCw,
    StackIcon as Layers,
    MusicNotesIcon as Music,
    CopySimpleIcon as Duplicate,
    XCircleIcon as ErrorIcon,
    PencilSimpleIcon as Edit3,
    ArrowSquareOutIcon as OpenInPicard
  } from "phosphor-svelte";
  import { VirtualList } from "svelte-virtual-list-ts";
  import Toggle from "./Toggle.svelte";
  import Button from "./Button.svelte";
  import HelpTip from "./HelpTip.svelte";
  import TemplateInput from "./TemplateInput.svelte";
  import ContextMenu from "./ContextMenu.svelte";
  import ContextMenuItem from "./ContextMenuItem.svelte";
  import TagEditor from "./TagEditor.svelte";
  import ViewInFilesItem from "./ViewInFilesItem.svelte";

  const PREVIEW_DEBOUNCE_MS = 300;
  const COL_MIN_WIDTH_PX = 150;
  const COL_MAX_WIDTH_PX = 1000;
  const AUTO_FIT_MIN_FROM_WIDTH_PX = 250;
  const AUTO_FIT_MIN_TO_WIDTH_PX = 300;
  const PX_PER_CHAR_ESTIMATE = 7.5;

  export interface OrganizePreviewItem {
    song_id: number;
    from_path: string;
    to_path: string;
    status: "ok" | "unchanged" | "collision" | "missing_tag" | "error";
    error_message: string | null;
    title?: string | null;
    artist?: string | null;
    album?: string | null;
    missing_fields?: string[];
  }

  let {
    isOpen = false,
    embedded = false,
    songIds = [],
    initialScope = "selection",
    refreshKey = 0,
    summaryReadyCount = $bindable(0),
    summaryPendingCount = $bindable(0),
    summaryCanApply = $bindable(false),
    summaryIsApplying = $bindable(false),
    applyRequestKey = 0,
    onClose,
    onSuccess,
  }: {
    isOpen?: boolean;
    /** Render inline in the page instead of as a modal dialog — no backdrop, no close button. */
    embedded?: boolean;
    songIds?: number[];
    initialScope?: "selection" | "library";
    /** Bump this to force the preview to refetch (e.g. after an external prune). */
    refreshKey?: number;
    /** Embedded mode only: mirrors readyCount/canApply/isApplying so the host page can render its own Apply control. */
    summaryReadyCount?: number;
    summaryPendingCount?: number;
    summaryCanApply?: boolean;
    summaryIsApplying?: boolean;
    /** Embedded mode only: bump this from the host page to trigger handleApply. */
    applyRequestKey?: number;
    onClose?: () => void;
    onSuccess?: () => void;
  } = $props();

  /** Embedded instances are always "open"; modal instances follow the isOpen prop. */
  let effectiveOpen = $derived(embedded || isOpen);

  const DEFAULT_TEMPLATE = "%albumartist/{%year - }{%album/}{%disc-}{%track }%title";
  function highlightPathHtml(path: string): string {
    if (!path) return "";
    const escaped = path
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");

    return escaped.replace(/([/\\])/g, '<span class="text-brand-text-secondary font-bold px-0.5">$1</span>');
  }

  // Template presets, mirroring how SettingsThemes.svelte offers predefined
  // presets plus a "Custom" option that reveals a free-text field. The
  // picker structure below makes adding more presets later a matter of
  // extending TEMPLATE_PRESETS.
  const TEMPLATE_PRESETS = [
    { id: "default", labelKey: "organizer.presetDefault", template: DEFAULT_TEMPLATE },
    { id: "alternative", labelKey: "organizer.presetAlternative", template: "%artist/%album (%year)/{CD %disc/}{%track-}%artist-%title" },
  ] as const;
  type TemplatePresetId = (typeof TEMPLATE_PRESETS)[number]["id"] | "custom";

  let templatePreset = $state<TemplatePresetId>("default");
  let customTemplate = $state(DEFAULT_TEMPLATE);
  let template = $derived(
    templatePreset === "custom"
      ? customTemplate
      : (TEMPLATE_PRESETS.find((p) => p.id === templatePreset)?.template ?? DEFAULT_TEMPLATE)
  );

  function selectPreset(id: TemplatePresetId) {
    if (id === "custom" && templatePreset !== "custom") {
      // Seed the free-text field with whatever pattern was active so
      // switching to Custom starts from something rather than blank.
      customTemplate = template;
    }
    templatePreset = id;
  }

  // Compact sample data for the live tree preview (artist → album → tracks).
  // Purely illustrative — never sent to the backend.
  interface PreviewSample {
    albumArtist: string;
    artist: string;
    album: string;
    year: number;
    genre: string;
    disc?: number;
    track?: number;
    title: string;
  }
  const PREVIEW_SAMPLES: PreviewSample[] = [
    { albumArtist: "Radiohead", artist: "Radiohead", album: "OK Computer", year: 1997, genre: "Alternative Rock", track: 1, title: "Airbag" },
    { albumArtist: "Radiohead", artist: "Radiohead", album: "OK Computer", year: 1997, genre: "Alternative Rock", track: 2, title: "Paranoid Android" },
    { albumArtist: "Daft Punk", artist: "Daft Punk", album: "Discovery", year: 2001, genre: "Electronic", track: 1, title: "One More Time" },
    // A track with no track number to demonstrate conditional track wrapping
    // ({%track }, {%track-}) omitting leading spaces or hyphens.
    { albumArtist: "Daft Punk", artist: "Daft Punk", album: "Aerodynamic", year: 2001, genre: "Electronic", title: "Aerodynamic (Remix)" },
    // A multi-disc album so the preview demonstrates the conditional
    // {CD %disc/} / {%disc-} blocks splitting into per-disc folders/prefixes.
    { albumArtist: "Pink Floyd", artist: "Pink Floyd", album: "The Wall", year: 1979, genre: "Rock", disc: 1, track: 1, title: "In The Flesh?" },
    { albumArtist: "Pink Floyd", artist: "Pink Floyd", album: "The Wall", year: 1979, genre: "Rock", disc: 2, track: 1, title: "Hey You" },
  ];

  /** A lightweight, display-only mirror of the backend's expand_template
   * (src-tauri/src/organizer.rs) — conditional {…} blocks plus %variable
   * substitution — used only to render the sample tree preview below. */
  function expandTemplatePreview(tpl: string, s: PreviewSample): string {
    let expanded = tpl;
    for (let guard = 0; guard < 20; guard++) {
      const start = expanded.indexOf("{");
      if (start === -1) break;
      const end = expanded.indexOf("}", start);
      if (end === -1) break;
      const block = expanded.slice(start + 1, end);

      const hasDisc = block.includes("%disc") && !!s.disc && s.disc > 0;
      const hasYear = block.includes("%year") && !!s.year;
      const hasGenre = block.includes("%genre") && !!s.genre;
      const hasAlbumArtist = block.includes("%albumartist") && !!s.albumArtist;
      const hasAlbum = block.includes("%album") && !block.includes("%albumartist") && !!s.album;
      const hasArtist = block.includes("%artist") && !block.includes("%albumartist") && !!s.artist;
      const hasTrack = (block.includes("%track") || block.includes("%rawtrack")) && !!s.track;
      const shouldRender = hasDisc || hasYear || hasGenre || hasAlbumArtist || hasAlbum || hasArtist || hasTrack;

      expanded = expanded.slice(0, start) + (shouldRender ? block : "") + expanded.slice(end + 1);
    }

    const track2 = s.track ? String(s.track).padStart(2, "0") : "00";
    const track3 = s.track ? String(s.track).padStart(3, "0") : "000";
    const rawtrack = s.track ? String(s.track) : "0";
    expanded = expanded
      .split("%albumartist").join(s.albumArtist)
      .split("%artist").join(s.artist)
      .split("%album").join(s.album)
      .split("%disc").join(String(s.disc ?? 1))
      .split("%track3").join(track3)
      .split("%rawtrack").join(rawtrack)
      .split("%track").join(track2)
      .split("%title").join(s.title)
      .split("%year").join(String(s.year))
      .split("%genre").join(s.genre);
    return expanded;
  }

  interface PreviewTreeNode {
    name: string;
    isFile: boolean;
    children: PreviewTreeNode[];
  }

  function buildPreviewTree(template: string): PreviewTreeNode[] {
    const roots: PreviewTreeNode[] = [];
    for (const sample of PREVIEW_SAMPLES) {
      const expanded = expandTemplatePreview(template, sample);
      const parts = expanded.split(/[/\\]/).filter((p) => p.trim() !== "");
      if (parts.length === 0) continue;

      let siblings = roots;
      parts.forEach((part, idx) => {
        const isFile = idx === parts.length - 1;
        const name = isFile ? `${part}.mp3` : part;
        let node = siblings.find((n) => n.name === name && n.isFile === isFile);
        if (!node) {
          node = { name, isFile, children: [] };
          siblings.push(node);
        }
        siblings = node.children;
      });
    }
    return roots;
  }

  let previewTree = $derived(buildPreviewTree(template));

  let scope = $state<"selection" | "library">("library");
  let replaceSpaces = $state(false);
  let asciiOnly = $state(false);
  let cleanEmptyDirs = $state(true);
  let moveExtraFiles = $state(true);

  // Destination is a two-way toggle: reorganize files in place under their
  // original library folder(s), or consolidate everything under one custom
  // folder. customDestinationDir is kept separate from the derived
  // destinationDir so switching back to "original" and back to "custom"
  // doesn't lose whatever path the user had typed/browsed to.
  let destinationMode = $state<"original" | "custom">("original");
  let customDestinationDir = $state<string>("");
  let destinationDir = $derived(destinationMode === "custom" ? customDestinationDir : "");

  let initializedFromStore = $state(false);
  $effect(() => {
    if (organizeStore.isLoaded && !initializedFromStore) {
      initializedFromStore = true;
      templatePreset = (organizeStore.preset as TemplatePresetId) || "default";
      if (organizeStore.preset === "custom") {
        customTemplate = organizeStore.template || DEFAULT_TEMPLATE;
      }
      replaceSpaces = organizeStore.replaceSpaces;
      asciiOnly = organizeStore.asciiOnly;
      cleanEmptyDirs = organizeStore.cleanEmptyDirs;
      moveExtraFiles = organizeStore.moveExtraFiles;
      destinationMode = organizeStore.destinationMode;
      customDestinationDir = organizeStore.customDestinationDir;
    }
  });

  let isDraftModified = $derived(
    initializedFromStore &&
      organizeStore.isLoaded &&
      (template !== organizeStore.template ||
        templatePreset !== organizeStore.preset ||
        destinationMode !== organizeStore.destinationMode ||
        customDestinationDir !== organizeStore.customDestinationDir ||
        replaceSpaces !== organizeStore.replaceSpaces ||
        asciiOnly !== organizeStore.asciiOnly ||
        cleanEmptyDirs !== organizeStore.cleanEmptyDirs ||
        moveExtraFiles !== organizeStore.moveExtraFiles)
  );

  $effect(() => {
    organizeStore.setDraftActive(isDraftModified);
  });

  function revertDraft() {
    templatePreset = (organizeStore.preset as TemplatePresetId) || "default";
    if (organizeStore.preset === "custom") {
      customTemplate = organizeStore.template || DEFAULT_TEMPLATE;
    }
    replaceSpaces = organizeStore.replaceSpaces;
    asciiOnly = organizeStore.asciiOnly;
    cleanEmptyDirs = organizeStore.cleanEmptyDirs;
    moveExtraFiles = organizeStore.moveExtraFiles;
    destinationMode = organizeStore.destinationMode;
    customDestinationDir = organizeStore.customDestinationDir;
  }

  $effect(() => {
    if (effectiveOpen) {
      scope = songIds.length > 0 ? initialScope : "library";
    }
  });

  let items = $state<OrganizePreviewItem[]>([]);
  let isLoading = $state(false);
  let isApplying = $state(false);
  let errorMessage = $state<string | null>(null);
  let successMessage = $state<string | null>(null);

  let activeSongIds = $derived(scope === "library" ? [] : songIds);

  function getItemObj(raw: any): OrganizePreviewItem {
    if (raw && typeof raw === "object" && "item" in raw && raw.item) {
      return raw.item as OrganizePreviewItem;
    }
    return raw as OrganizePreviewItem;
  }

  // The backend no longer skips colliding files — it auto-reroutes them into a
  // "Duplicates" subfolder and reports them as "ok" with an explanatory
  // error_message (see organizer.rs's "Routed to Duplicates" messages). The
  // legacy "collision" status is kept here for forward compatibility but is
  // no longer actually emitted.
  function getItemStatus(item: OrganizePreviewItem): "ok" | "unchanged" | "duplicate" | "missing_tag" | "error" {
    const target = getItemObj(item);
    const s = String(target?.status || "").toLowerCase();
    if (s === "collision") return "duplicate";
    if (s === "ok") return target?.error_message ? "duplicate" : "ok";
    if (s === "unchanged") return "unchanged";
    if (s === "missing_tag" || s === "missingtag") return "missing_tag";
    return "error";
  }

  // Only changing files are ever worth reviewing here — there's no real use
  // case for scrolling through thousands of unchanged rows just to confirm
  // they're unchanged, so this filtering is permanent rather than a toggle.
  let displayedItems = $derived(items.filter((i) => getItemStatus(i) !== "unchanged"));

  let commonPrefix = $derived.by(() => {
    const allPaths: string[] = [];
    for (const raw of displayedItems) {
      const i = getItemObj(raw);
      if (i.from_path) allPaths.push(i.from_path);
      if (i.to_path) allPaths.push(i.to_path);
    }
    if (allPaths.length < 2) return "";
    let prefix = allPaths[0];
    const lastSep = Math.max(prefix.lastIndexOf("/"), prefix.lastIndexOf("\\"));
    if (lastSep > 0) prefix = prefix.slice(0, lastSep + 1);

    for (const p of allPaths) {
      while (prefix.length > 0 && !p.startsWith(prefix)) {
        const sep = Math.max(prefix.slice(0, -1).lastIndexOf("/"), prefix.slice(0, -1).lastIndexOf("\\"));
        if (sep >= 0) {
          prefix = prefix.slice(0, sep + 1);
        } else {
          prefix = "";
          break;
        }
      }
    }
    return prefix;
  });

  function getDisplayPath(fullPath: string, prefix: string): string {
    if (!fullPath) return "";
    if (prefix && prefix.length > 3 && fullPath.startsWith(prefix)) {
      return "…" + fullPath.slice(prefix.length - 1);
    }
    return fullPath;
  }

  /** Shortens the backend's "Routed to Duplicates (collision with <full path>)"
   * message down to just the colliding file's name — the full path is still
   * available in the cell's title attribute on hover. */
  function shortenDuplicateMessage(message: string | null): string {
    if (!message) return "";
    return message.replace(/\(collision with (.+)\)$/, (_match, collidingPath) => {
      const name = collidingPath.split(/[/\\]/).pop() || collidingPath;
      return `(collision with ${name})`;
    });
  }

  let fromColWidth = $state(340);
  let toColWidth = $state(380);
  let isResizing = $state<"from" | "to" | null>(null);
  let resizeStartX = 0;
  let resizeStartWidth = 0;

  function startResize(col: "from" | "to", e: MouseEvent) {
    e.preventDefault();
    isResizing = col;
    resizeStartX = e.clientX;
    resizeStartWidth = col === "from" ? fromColWidth : toColWidth;
    window.addEventListener("mousemove", handleResizeMove);
    window.addEventListener("mouseup", handleResizeUp);
  }

  function handleResizeMove(e: MouseEvent) {
    if (!isResizing) return;
    const diff = e.clientX - resizeStartX;
    if (isResizing === "from") {
      fromColWidth = Math.max(COL_MIN_WIDTH_PX, resizeStartWidth + diff);
    } else {
      toColWidth = Math.max(COL_MIN_WIDTH_PX, resizeStartWidth + diff);
    }
  }

  function handleResizeUp() {
    isResizing = null;
    window.removeEventListener("mousemove", handleResizeMove);
    window.removeEventListener("mouseup", handleResizeUp);
  }

  function autoFitColumns() {
    let maxFrom = AUTO_FIT_MIN_FROM_WIDTH_PX;
    let maxTo = AUTO_FIT_MIN_TO_WIDTH_PX;
    for (const raw of items) {
      const i = getItemObj(raw);
      if (i.from_path) maxFrom = Math.max(maxFrom, i.from_path.length * PX_PER_CHAR_ESTIMATE);
      if (i.to_path) maxTo = Math.max(maxTo, i.to_path.length * PX_PER_CHAR_ESTIMATE);
    }
    fromColWidth = Math.min(COL_MAX_WIDTH_PX, Math.max(AUTO_FIT_MIN_FROM_WIDTH_PX, maxFrom));
    toColWidth = Math.min(COL_MAX_WIDTH_PX, Math.max(AUTO_FIT_MIN_TO_WIDTH_PX, maxTo));
  }

  let collisionCount = $derived(items.filter((i) => getItemStatus(i) === "duplicate").length);
  let errorCount = $derived(items.filter((i) => getItemStatus(i) === "error").length);
  let missingTagCount = $derived(items.filter((i) => getItemStatus(i) === "missing_tag").length);
  let readyCount = $derived(items.filter((i) => getItemStatus(i) === "ok").length);
  let pendingCount = $derived(missingTagCount + collisionCount + errorCount);

  // Untagged tracks (missing_tag) are held back by default; users can opt in via a toggle
  let includeMissingTags = $state(false);
  let eligibleCount = $derived(readyCount + collisionCount + (includeMissingTags ? missingTagCount : 0));
  let canApply = $derived(eligibleCount > 0 && !isLoading && !isApplying);

  $effect(() => {
    summaryReadyCount = readyCount;
    summaryPendingCount = pendingCount;
    summaryCanApply = canApply;
    summaryIsApplying = isApplying;
  });

  type FilterKey = "all" | "ok" | "missing_tag" | "duplicate" | "error";
  let activeFilter = $state<FilterKey>("all");

  $effect(() => {
    if (activeFilter === "missing_tag" && missingTagCount === 0) activeFilter = "all";
    if (activeFilter === "duplicate" && collisionCount === 0) activeFilter = "all";
    if (activeFilter === "error" && errorCount === 0) activeFilter = "all";
  });

  let filteredItems = $derived.by(() => {
    if (activeFilter === "all") return displayedItems;
    return displayedItems.filter((i) => getItemStatus(i) === activeFilter);
  });

  let isConfigExpanded = $state(false);
  let editingSongId = $state<number | null>(null);
  let contextMenu = $state<{ x: number; y: number; item: OrganizePreviewItem } | null>(null);

  function openTagEditor(songId: number) {
    editingSongId = songId;
  }

  async function handleTagEditorSaved() {
    await collectionStore.refreshLibrary();
    await fetchPreview();
  }

  async function handleOpenInPicard(songId: number) {
    await openInPicard([songId]);
  }

  function handleRowDblClick(item: OrganizePreviewItem) {
    if (getItemStatus(item) === "missing_tag") {
      openTagEditor(item.song_id);
    }
  }

  function handleRowContextMenu(item: OrganizePreviewItem, e: MouseEvent) {
    e.preventDefault();
    contextMenu = { x: e.clientX, y: e.clientY, item };
  }

  function getTrackTitle(item: OrganizePreviewItem): string {
    if (item.title && item.title.trim().length > 0) return item.title.trim();
    if (item.from_path) {
      const parts = item.from_path.split(/[/\\]/);
      const fileName = parts[parts.length - 1];
      const stem = fileName.replace(/\.[^/.]+$/, "");
      if (stem.trim().length > 0) return stem;
    }
    return i18n.t("organizer.unknownTrack");
  }

  function getTrackSubtitle(item: OrganizePreviewItem): string {
    const artist = item.artist?.trim() || "";
    const album = item.album?.trim() || "";
    if (artist && album) return `${artist} • ${album}`;
    if (artist) return artist;
    if (album) return album;
    return "";
  }

  function formatMissingTagTooltip(item: OrganizePreviewItem): string {
    const fields = item.missing_fields;
    if (fields && fields.length > 0) {
      const fieldNames = fields.map((f) => {
        switch (f) {
          case "title": return i18n.t("organizer.chipTitle");
          case "artist": return i18n.t("organizer.chipArtist");
          case "album": return i18n.t("organizer.chipAlbum");
          case "year": return i18n.t("organizer.chipYear");
          case "genre": return i18n.t("organizer.chipGenre");
          case "track": return i18n.t("tagEditor.fieldTrack");
          case "disc": return i18n.t("tagEditor.fieldDisc");
          default: return f;
        }
      });
      return `${i18n.t("organizer.statusMissingTag")}: ${fieldNames.join(", ")}`;
    }
    return item.error_message || i18n.t("organizer.statusMissingTag");
  }

  // -1 sentinel: skip the initial effect run so mounting doesn't fire an apply.
  let lastAppliedRequestKey = $state(-1);
  $effect(() => {
    if (!embedded) return;
    const key = applyRequestKey;
    if (lastAppliedRequestKey === -1) {
      lastAppliedRequestKey = key;
    } else if (key !== lastAppliedRequestKey) {
      lastAppliedRequestKey = key;
      requestApply();
    }
  });

  let debounceTimer: ReturnType<typeof setTimeout> | null = null;

  function handleKeydown(e: KeyboardEvent) {
    if (!effectiveOpen) return;
    if (e.key === "Escape") {
      if (!embedded) onClose?.();
    } else if (e.key === "Enter") {
      const target = e.target as HTMLElement;
      if (target.tagName === "BUTTON" || target.tagName === "TEXTAREA" || target.tagName === "INPUT") return;
      if (!canApply || showConfirmApply) return;
      e.preventDefault();
      requestApply();
    }
  }

  async function selectDestinationDir() {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: i18n.t("organizer.destinationDirLabel"),
      });
      if (selected && typeof selected === "string") {
        customDestinationDir = selected;
      }
    } catch (e) {
      console.error("Failed to select destination folder:", e);
    }
  }

  async function fetchPreview() {
    if (!effectiveOpen) return;
    isLoading = true;
    errorMessage = null;

    try {
      const res = await invoke<OrganizePreviewItem[]>("preview_organize", {
        songIds: activeSongIds,
        template,
        options: {
          destination_dir: destinationDir.trim() !== "" ? destinationDir : null,
          replace_spaces_with_underscores: replaceSpaces,
          ascii_only: asciiOnly,
          clean_empty_dirs: cleanEmptyDirs,
          move_extra_files: moveExtraFiles,
        },
      });
      items = res;
    } catch (err: any) {
      console.error("Preview failed:", err);
      errorMessage = typeof err === "string" ? err : err.message || i18n.t("organizer.previewFailed");
      items = [];
    } finally {
      isLoading = false;
    }
  }

  $effect(() => {
    // Read reactive variables synchronously so Svelte 5 tracks dependencies
    const _t = template;
    const _d = destinationDir;
    const _r = replaceSpaces;
    const _a = asciiOnly;
    const _c = cleanEmptyDirs;
    const _m = moveExtraFiles;
    const _s = scope;
    const _ids = activeSongIds;
    const _open = effectiveOpen;
    const _refresh = refreshKey;

    if (_open) {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = setTimeout(() => {
        fetchPreview();
      }, PREVIEW_DEBOUNCE_MS);
    }
    return () => {
      if (debounceTimer) clearTimeout(debounceTimer);
    };
  });

  let showConfirmApply = $state(false);
  let pendingConsequences = $state<OrganizeConsequences | null>(null);

  function requestApply() {
    if (!canApply || isApplying) return;
    const consequences = computeOrganizeConsequences(items, { cleanEmptyDirs, moveExtraFiles, includeMissingTags });
    if (consequences.fileCount === 0) return;
    pendingConsequences = consequences;
    showConfirmApply = true;
  }

  let confirmApplyMessage = $derived.by(() => {
    if (!pendingConsequences) return "";
    let msg = i18n.t("organizer.confirmApplyMessage", {
      files: i18n.plural("organizer.filesCount", pendingConsequences.fileCount),
      folders: i18n.plural("organizer.foldersCount", pendingConsequences.folderCount),
    });
    const bullets: string[] = [];
    if (pendingConsequences.cleanEmptyDirs) {
      bullets.push(i18n.t("organizer.confirmApplyCleanDirs"));
    }
    if (pendingConsequences.moveExtraFiles) {
      bullets.push(i18n.t("organizer.confirmApplyMoveExtra"));
    }
    if (organizeStore.autoOrganize) {
      bullets.push(i18n.t("organizer.confirmApplyAutoOrganize"));
    }
    if (destinationMode === "custom" && customDestinationDir) {
      bullets.push(i18n.t("organizer.confirmApplyCustomDest", { path: customDestinationDir }));
    }
    if (bullets.length > 0) {
      msg += "\n\n" + bullets.map((b) => `• ${b}`).join("\n");
    }
    return msg;
  });

  async function executeApply() {
    if (!canApply) return;
    isApplying = true;
    errorMessage = null;

    const itemsToApply = items
      .filter((i) => i.status === "ok" || (includeMissingTags && (i.status === "missing_tag" || (i.status as any) === "missingtag")))
      .map((i) => ({
        song_id: i.song_id,
        from_path: i.from_path,
        to_path: i.to_path,
      }));

    try {
      const result = await invoke<{ moved_count: number; skipped_count: number; errors: string[] }>(
        "apply_organize",
        {
          items: itemsToApply,
          cleanEmptyDirs: cleanEmptyDirs,
          moveExtraFiles: moveExtraFiles,
        }
      );

      // Persist confirmed template and options so future operations and Auto-Organize use them
      await organizeStore.updateConfig({
        template,
        preset: templatePreset,
        destination_mode: destinationMode,
        custom_destination_dir: customDestinationDir,
        replace_spaces: replaceSpaces,
        ascii_only: asciiOnly,
        clean_empty_dirs: cleanEmptyDirs,
        move_extra_files: moveExtraFiles,
      });

      if (result.errors && result.errors.length > 0) {
        errorMessage = result.errors.join("; ");
        toastStore.show(
          i18n.t("organizer.toastErrors", { count: result.errors.length }),
          "warning"
        );
      } else {
        successMessage = i18n.plural("organizer.applySuccess", result.moved_count);
        toastStore.show(
          i18n.plural("organizer.applySuccess", result.moved_count),
          "success",
          TOAST_DURATION_MS
        );
        onSuccess?.();
        // Leave the modal open so the user can see the result — refresh the
        // preview to reflect the just-applied moves instead of auto-closing.
        await fetchPreview();
      }
    } catch (err: any) {
      console.error("Failed to apply organize:", err);
      errorMessage = typeof err === "string" ? err : err.message || i18n.t("organizer.organizeFailed");
    } finally {
      isApplying = false;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeydown);
    return () => {
      window.removeEventListener("keydown", handleKeydown);
    };
  });
</script>

{#snippet scopeToggle()}
  {#if songIds.length > 0}
    <div class="flex items-center justify-between bg-brand-sidebar/40 p-3 rounded-xl border border-brand-border/30">
      <span class="font-semibold text-brand-text-primary">{i18n.t("organizer.scopeLabel")}</span>
      <div class="flex items-center gap-1.5 bg-brand-sidebar p-1 rounded-lg border border-brand-border/50">
        <button
          onclick={() => { scope = "selection"; }}
          class="px-3 py-1 rounded-md transition-colors {scope === 'selection' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t("organizer.scopeSelection", { count: songIds.length })}
        </button>
        <button
          onclick={() => { scope = "library"; }}
          class="px-3 py-1 rounded-md transition-colors {scope === 'library' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t("organizer.scopeLibrary", { count: items.length })}
        </button>
      </div>
    </div>
  {/if}
{/snippet}

{#snippet previewTreeNode(node: PreviewTreeNode)}
  <div class="flex items-center gap-1.5 py-0.5 {node.isFile ? 'text-brand-text-secondary' : 'text-brand-text-primary font-medium'}">
    {#if node.isFile}
      <Music class="w-3 h-3 shrink-0 text-brand-text-secondary/70" />
    {:else}
      <Folder class="w-3 h-3 shrink-0 text-brand-accent-text/80" />
    {/if}
    <span class="truncate font-mono text-[11px]">{node.name}</span>
  </div>
  {#if node.children.length > 0}
    <div class="pl-4 border-l border-brand-border/40 ml-1.5">
      {#each node.children as child (child.name + child.isFile)}
        {@render previewTreeNode(child)}
      {/each}
    </div>
  {/if}
{/snippet}

{#snippet templateSection()}
  <div class="space-y-4">
    {#if isDraftModified && organizeStore.autoOrganize}
      <div class="flex items-center justify-between p-2.5 rounded-xl bg-amber-500/10 border border-amber-500/30 text-amber-200">
        <div class="flex items-center gap-2 min-w-0">
          <AlertTriangle class="w-4 h-4 shrink-0 text-amber-400" />
          <span class="text-xs font-medium truncate">
            {i18n.t("organizer.draftModifiedBadge")}
          </span>
        </div>
        <button
          type="button"
          onclick={revertDraft}
          class="text-xs underline hover:text-white shrink-0 ml-2 font-medium"
        >
          {i18n.t("organizer.revertDraftBtn")}
        </button>
      </div>
    {:else if isDraftModified}
      <div class="flex items-center justify-end">
        <button
          type="button"
          onclick={revertDraft}
          class="text-xs text-brand-text-secondary hover:text-brand-text-primary underline font-medium"
        >
          {i18n.t("organizer.revertDraftBtn")}
        </button>
      </div>
    {/if}

    <div class="grid grid-cols-1 @xl:grid-cols-3 gap-3">
      {#each TEMPLATE_PRESETS as preset (preset.id)}
        <button
          type="button"
          onclick={() => selectPreset(preset.id)}
          aria-label={i18n.t(preset.labelKey)}
          class="text-left bg-brand-main/50 border-2 rounded-xl p-3 transition-colors duration-200 hover:border-brand-accent/40 {templatePreset === preset.id ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60'}"
        >
          <span class="font-semibold text-sm text-brand-text-primary block">{i18n.t(preset.labelKey)}</span>
          <code class="text-[10px] text-brand-text-secondary font-mono truncate block mt-1">{preset.template}</code>
        </button>
      {/each}
      <button
        type="button"
        onclick={() => selectPreset("custom")}
        aria-label={i18n.t("organizer.presetCustom")}
        class="text-left bg-brand-main/50 border-2 rounded-xl p-3 transition-colors duration-200 hover:border-brand-accent/40 {templatePreset === 'custom' ? 'border-brand-accent shadow-md shadow-brand-accent/5' : 'border-brand-border/60'}"
      >
        <span class="font-semibold text-sm text-brand-text-primary block">{i18n.t("organizer.presetCustom")}</span>
        <span class="text-[10px] text-brand-text-secondary block mt-1">{i18n.t("organizer.presetCustomHint")}</span>
      </button>
    </div>

    {#if templatePreset === "custom"}
      <div>
        <label for="template-input" class="font-semibold text-brand-text-primary block mb-1.5">
          {i18n.t("organizer.templateLabel")}
        </label>

        <TemplateInput id="template-input" bind:value={customTemplate} />
      </div>
    {/if}

    <div>
      <span class="text-[11px] text-brand-text-secondary font-semibold uppercase tracking-wider">{i18n.t("organizer.templateSampleOutputLabel")}</span>
      <div class="mt-1.5 bg-brand-main/50 border border-brand-border/60 rounded-xl p-3 max-h-40 overflow-y-auto">
        {#each previewTree as node (node.name + node.isFile)}
          {@render previewTreeNode(node)}
        {/each}
      </div>
    </div>
  </div>
{/snippet}

{#snippet destinationSection()}
  <div class="space-y-4">
    <div>
      <span class="block font-semibold text-brand-text-primary mb-1.5">{i18n.t("organizer.destinationDirLabel")}</span>
      <div class="flex items-center gap-1.5 bg-brand-sidebar p-1 rounded-lg border border-brand-border/50 w-full">
        <button
          type="button"
          onclick={() => { destinationMode = "original"; }}
          class="flex-1 px-3 py-1.5 rounded-md transition-colors text-xs {destinationMode === 'original' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t("organizer.destinationDefault")}
        </button>
        <button
          type="button"
          onclick={() => { destinationMode = "custom"; }}
          class="flex-1 px-3 py-1.5 rounded-md transition-colors text-xs {destinationMode === 'custom' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'text-brand-text-secondary hover:text-brand-text-primary'}"
        >
          {i18n.t("organizer.destinationCustom")}
        </button>
      </div>

      {#if destinationMode === "custom"}
        <div class="flex items-center gap-2 mt-2">
          <input
            id="dest-dir-input"
            type="text"
            bind:value={customDestinationDir}
            placeholder={i18n.t("organizer.destinationDirLabel")}
            class="flex-1 px-3 py-1.5 bg-brand-sidebar/80 border border-brand-border/80 rounded-xl text-brand-text-primary text-xs focus:outline-none focus:border-brand-accent transition-colors truncate"
          />
          <button
            onclick={selectDestinationDir}
            class="px-3 py-1.5 bg-brand-sidebar border border-brand-border/80 hover:bg-brand-accent/15 hover:text-brand-accent-text text-brand-text-primary rounded-xl transition-colors font-medium"
          >
            {i18n.t("organizer.browse")}
          </button>
        </div>
      {/if}
    </div>

    <div class="flex flex-col gap-3">
      <div class="flex items-center justify-between gap-2 text-brand-text-secondary">
        <span>{i18n.t("organizer.moveExtraFiles")}</span>
        <Toggle
          checked={moveExtraFiles}
          onchange={(v) => { moveExtraFiles = v; }}
          label={i18n.t("organizer.moveExtraFiles")}
        />
      </div>

      <div class="flex items-center justify-between gap-2 text-brand-text-secondary">
        <span>{i18n.t("organizer.replaceSpaces")}</span>
        <Toggle
          checked={replaceSpaces}
          onchange={(v) => { replaceSpaces = v; }}
          label={i18n.t("organizer.replaceSpaces")}
        />
      </div>

      <div class="flex items-center justify-between gap-2 text-brand-text-secondary">
        <span>{i18n.t("organizer.asciiOnly")}</span>
        <Toggle
          checked={asciiOnly}
          onchange={(v) => { asciiOnly = v; }}
          label={i18n.t("organizer.asciiOnly")}
        />
      </div>
    </div>
  </div>
{/snippet}

{#snippet previewSection()}
  <div class="space-y-2">
    <div class="flex flex-col gap-1.5">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex flex-wrap items-center gap-1.5" role="group" aria-label="Status filters">
          <button
            type="button"
            onclick={() => { activeFilter = "all"; }}
            class="px-2.5 py-1 rounded-lg text-xs transition-colors {activeFilter === 'all' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border/40'}"
          >
            {i18n.t("organizer.filterAll", { count: displayedItems.length })}
          </button>
          <button
            type="button"
            onclick={() => { activeFilter = "ok"; }}
            class="px-2.5 py-1 rounded-lg text-xs transition-colors {activeFilter === 'ok' ? 'bg-brand-accent text-brand-accent-contrast font-bold shadow-sm' : 'bg-brand-sidebar text-brand-text-secondary hover:text-brand-text-primary border border-brand-border/40'}"
          >
            {i18n.t("organizer.filterReady", { count: readyCount })}
          </button>
          {#if missingTagCount > 0}
            <button
              type="button"
              onclick={() => { activeFilter = "missing_tag"; }}
              class="px-2.5 py-1 rounded-lg text-xs transition-colors {activeFilter === 'missing_tag' ? 'bg-amber-500 text-black font-bold shadow-sm' : 'bg-brand-sidebar text-amber-400 hover:text-amber-300 border border-amber-500/30'}"
            >
              {i18n.t("organizer.filterNeedsTags", { count: missingTagCount })}
            </button>
          {/if}
          {#if collisionCount > 0}
            <button
              type="button"
              onclick={() => { activeFilter = "duplicate"; }}
              class="px-2.5 py-1 rounded-lg text-xs transition-colors {activeFilter === 'duplicate' ? 'bg-amber-500 text-black font-bold shadow-sm' : 'bg-brand-sidebar text-amber-400 hover:text-amber-300 border border-amber-500/30'}"
            >
              {i18n.t("organizer.filterDuplicates", { count: collisionCount })}
            </button>
          {/if}
          {#if errorCount > 0}
            <button
              type="button"
              onclick={() => { activeFilter = "error"; }}
              class="px-2.5 py-1 rounded-lg text-xs transition-colors {activeFilter === 'error' ? 'bg-rose-500 text-white font-bold shadow-sm' : 'bg-brand-sidebar text-rose-400 hover:text-rose-300 border border-rose-500/30'}"
            >
              {i18n.t("organizer.filterErrors", { count: errorCount })}
            </button>
          {/if}
        </div>

        <div class="flex items-center gap-3">
          {#if missingTagCount > 0}
            <div class="flex items-center gap-2">
              <Toggle
                checked={includeMissingTags}
                onchange={(v) => { includeMissingTags = v; }}
                label={i18n.t("organizer.includeMissingTagsToggle")}
                showOnOffLabel={false}
              />
              <span class="text-xs text-brand-text-secondary select-none">
                {i18n.t("organizer.includeMissingTagsToggle")}
              </span>
            </div>
          {/if}

          <Button
            onclick={fetchPreview}
            variant="secondary"
            size="sm"
            disabled={isLoading}
            title={i18n.t("organizer.refreshPreviewTooltip")}
            class="gap-1.5"
          >
            <RefreshCw class="w-3.5 h-3.5 {isLoading ? 'animate-spin text-brand-accent-text' : ''}" />
            {i18n.t("organizer.refreshPreviewTooltip")}
          </Button>
        </div>
      </div>
    </div>

    <div class="h-64 border border-brand-border/60 rounded-xl overflow-hidden bg-brand-sidebar/40 flex flex-col">
      {#if items.length === 0}
        <div class="h-full flex items-center justify-center text-brand-text-secondary">
          {#if isLoading}
            <RefreshCw class="w-5 h-5 text-brand-accent-text animate-spin" />
          {:else}
            <span>{i18n.t("organizer.noTracksToOrganize")}</span>
          {/if}
        </div>
      {:else if filteredItems.length === 0}
        <div class="h-full flex items-center justify-center text-brand-text-secondary">
          <span>{i18n.t("organizer.noChangingFilesMatch")}</span>
        </div>
      {:else}
        <div class="flex-1 min-h-0 overflow-x-auto overflow-y-hidden">
          <div style="min-width: {48 + 224 + fromColWidth + 24 + toColWidth + 140}px;" class="h-full flex flex-col">
            <div class="h-7 px-3 flex items-center bg-brand-sidebar/95 border-b border-brand-border/60 text-[10px] font-semibold text-brand-text-secondary uppercase tracking-wider select-none shrink-0">
              <div class="w-12 shrink-0">{i18n.t("organizer.colStatus")}</div>
              <div class="w-56 shrink-0 pr-2">{i18n.t("organizer.chipTitle")}</div>

              <div class="flex items-center shrink-0 pr-1" style="width: {fromColWidth}px;">
                <span class="truncate flex-1">{i18n.t("organizer.colSource")}</span>
                <button
                  type="button"
                  aria-label={i18n.t("organizer.resizeOriginalColumn")}
                  onmousedown={(e) => startResize("from", e)}
                  ondblclick={autoFitColumns}
                  class="w-3 h-5 hover:bg-brand-accent/30 cursor-col-resize flex items-center justify-center group shrink-0 bg-transparent border-0 p-0"
                  title={i18n.t("organizer.resizeColumnHint")}
                >
                  <div class="w-0.5 h-3 bg-brand-border group-hover:bg-brand-accent"></div>
                </button>
              </div>

              <div class="w-6 text-center shrink-0 text-brand-text-secondary">→</div>

              <div class="flex items-center shrink-0 pl-1" style="width: {toColWidth}px;">
                <span class="truncate flex-1">{i18n.t("organizer.colDestination")}</span>
                <button
                  type="button"
                  aria-label={i18n.t("organizer.resizeTargetColumn")}
                  onmousedown={(e) => startResize("to", e)}
                  ondblclick={autoFitColumns}
                  class="w-3 h-5 hover:bg-brand-accent/30 cursor-col-resize flex items-center justify-center group shrink-0 bg-transparent border-0 p-0"
                  title={i18n.t("organizer.resizeColumnHint")}
                >
                  <div class="w-0.5 h-3 bg-brand-border group-hover:bg-brand-accent"></div>
                </button>
              </div>
            </div>

            <div class="flex-1 min-h-0">
              <VirtualList items={filteredItems} height="100%" itemHeight={46}>
                {#snippet children(rawRow: any)}
                  {@const item = getItemObj(rawRow)}
                  {@const st = getItemStatus(item)}
                  {@const displayTitle = getTrackTitle(item)}
                  {@const displaySubtitle = getTrackSubtitle(item)}
                  {@const displayFrom = getDisplayPath(item.from_path, commonPrefix)}
                  {@const displayTo = getDisplayPath(item.to_path, commonPrefix)}
                  <div
                    class="h-[46px] px-3 flex items-center border-b border-brand-border/20 text-[11px] hover:bg-brand-accent/10 transition-colors whitespace-nowrap group relative cursor-default"
                    ondblclick={() => handleRowDblClick(item)}
                    oncontextmenu={(e) => handleRowContextMenu(item, e)}
                    role="row"
                    tabindex="0"
                  >
                    <div class="w-12 shrink-0">
                      {#if st === "ok"}
                        <span class="inline-flex items-center justify-center w-6 h-6 rounded bg-brand-accent/15 text-brand-accent-text border border-brand-accent/30" title={i18n.t("organizer.statusOk")}>
                          <Check class="w-3.5 h-3.5" />
                          <span class="sr-only">{i18n.t("organizer.statusOk")}</span>
                        </span>
                      {:else if st === "unchanged"}
                        <span class="px-2 py-0.5 rounded bg-brand-sidebar border border-brand-border/60 text-brand-text-secondary">
                          {i18n.t("organizer.statusUnchanged")}
                        </span>
                      {:else if st === "duplicate"}
                        <HelpTip
                          text={item.error_message || i18n.t("organizer.statusCollision")}
                          label={item.error_message ? i18n.t("organizer.statusCollision") : undefined}
                          class="w-6 h-6 rounded bg-amber-500/20 text-amber-400 border border-amber-500/40"
                        >
                          <Duplicate class="w-3.5 h-3.5" />
                        </HelpTip>
                      {:else if st === "missing_tag"}
                        <HelpTip
                          text={formatMissingTagTooltip(item)}
                          label={i18n.t("organizer.statusMissingTag")}
                          class="w-6 h-6 rounded bg-amber-500/20 text-amber-400 border border-amber-500/40"
                        >
                          <AlertTriangle class="w-3.5 h-3.5" />
                        </HelpTip>
                      {:else}
                        <HelpTip
                          text={item.error_message || i18n.t("organizer.statusError")}
                          label={item.error_message ? i18n.t("organizer.statusError") : undefined}
                          class="w-6 h-6 rounded bg-rose-500/20 text-rose-400 border border-rose-500/40"
                        >
                          <ErrorIcon class="w-3.5 h-3.5" />
                        </HelpTip>
                      {/if}
                    </div>

                    <!-- Track identity -->
                    <div class="w-56 shrink-0 pr-3 min-w-0 flex flex-col justify-center">
                      <span class="truncate font-medium text-brand-text-primary text-xs" title={displayTitle}>
                        {displayTitle}
                      </span>
                      {#if displaySubtitle}
                        <span class="truncate text-[10px] text-brand-text-secondary" title={displaySubtitle}>
                          {displaySubtitle}
                        </span>
                      {/if}
                    </div>

                    <div
                      class="px-2 overflow-x-auto scrollbar-none shrink-0 {item.from_path ? 'text-brand-text-secondary' : 'text-rose-400 font-medium'}"
                      style="width: {fromColWidth}px;"
                      title={item.from_path}
                    >
                      {@html highlightPathHtml(displayFrom || i18n.t("organizer.noPathRecorded"))}
                    </div>

                    <div class="w-6 text-center text-brand-text-secondary shrink-0">→</div>

                    <div
                      class="px-2 overflow-x-auto scrollbar-none shrink-0 {st === 'ok' ? 'text-brand-text-primary font-medium' : st === 'error' ? 'text-rose-400 font-semibold' : st === 'duplicate' ? 'text-amber-400 font-medium' : 'text-brand-text-primary'}"
                      style="width: {toColWidth}px;"
                      title={item.error_message ? `${item.to_path ? item.to_path + ' — ' : ''}${item.error_message}` : item.to_path}
                    >
                      {#if st === 'error'}
                        <span class="text-rose-400 font-medium">
                          {item.error_message ? item.error_message : (displayTo || i18n.t("organizer.unknownError"))}
                        </span>
                      {:else if st === 'duplicate'}
                        <span class="text-amber-400 font-medium">
                          {item.error_message ? shortenDuplicateMessage(item.error_message) : displayTo}
                        </span>
                      {:else}
                        {@html highlightPathHtml(displayTo)}
                      {/if}
                    </div>

                    <!-- Hover quick actions -->
                    <div class="ml-auto shrink-0 flex items-center gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity pl-2">
                      <button
                        type="button"
                        onclick={(e) => { e.stopPropagation(); openTagEditor(item.song_id); }}
                        class="px-2 py-1 rounded bg-brand-sidebar hover:bg-brand-accent hover:text-brand-accent-contrast text-brand-text-secondary text-[10px] font-medium border border-brand-border/60 transition-colors flex items-center gap-1 shadow-sm"
                        title={i18n.t("organizer.editTags")}
                      >
                        <Edit3 class="w-3 h-3" />
                        <span>{i18n.t("organizer.editTags")}</span>
                      </button>
                      {#if picardStore.available}
                        <button
                          type="button"
                          onclick={(e) => { e.stopPropagation(); handleOpenInPicard(item.song_id); }}
                          class="px-2 py-1 rounded bg-brand-sidebar hover:bg-brand-accent hover:text-brand-accent-contrast text-brand-text-secondary text-[10px] font-medium border border-brand-border/60 transition-colors flex items-center gap-1 shadow-sm"
                          title={i18n.t("picard.openInPicard")}
                        >
                          <OpenInPicard class="w-3 h-3" />
                          <span>{i18n.t("picard.openInPicard")}</span>
                        </button>
                      {/if}
                    </div>
                  </div>
                {/snippet}
              </VirtualList>
            </div>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/snippet}

{#snippet applyButton()}
  <Button
    variant="primary"
    onclick={requestApply}
    disabled={!canApply}
  >
    {#if isApplying}
      <RefreshCw class="w-4 h-4 animate-spin" />
      <span>{i18n.t("organizer.applying")}</span>
    {:else}
      <span>{i18n.t("organizer.applyButton")}</span>
    {/if}
  </Button>
{/snippet}

{#snippet actionMessages()}
  {#if errorMessage}
    <div class="text-rose-400 flex items-center gap-2 font-medium text-xs">
      <AlertTriangle class="w-4 h-4 shrink-0" />
      <span>{errorMessage}</span>
    </div>
  {/if}
{/snippet}

{#if effectiveOpen}
  {#if embedded}
    <!-- Apply action lives in the host page header (top-right); this renders Preview, Template Pattern, Destination & Options -->
    <div class="space-y-4 text-xs">
      {#if songIds.length > 0}
        <div class="max-w-3xl mx-auto">
          {@render scopeToggle()}
        </div>
      {/if}

      {#if errorMessage}
        <div class="max-w-3xl mx-auto space-y-1.5">
          {@render actionMessages()}
        </div>
      {/if}

      <!-- Unlike the sections below, this one isn't capped to max-w-3xl —
           the file list benefits from the extra width on wide windows. -->
      <div class="bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4 text-brand-text-primary">
        {@render previewSection()}
      </div>

      <!-- Collapsible Template & Destination Summary Bar -->
      <div class="max-w-3xl mx-auto bg-brand-sidebar border border-brand-border rounded-xl p-4 flex items-center justify-between gap-4">
        <div class="flex flex-wrap items-center gap-x-6 gap-y-2 min-w-0 flex-1">
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider shrink-0">
              {i18n.t("organizer.activeTemplateLabel")}:
            </span>
            <code class="px-2 py-0.5 rounded bg-brand-main/60 border border-brand-border/60 text-xs font-mono text-brand-text-primary truncate">
              {template}
            </code>
          </div>
          <div class="flex items-center gap-2 min-w-0">
            <span class="text-xs font-semibold text-brand-text-secondary uppercase tracking-wider shrink-0">
              {i18n.t("organizer.activeDestinationLabel")}:
            </span>
            <span class="text-xs font-medium text-brand-text-primary truncate">
              {destinationMode === "custom" && customDestinationDir ? customDestinationDir : i18n.t("organizer.destinationDefault")}
            </span>
          </div>
        </div>
        <Button
          variant="secondary"
          size="sm"
          onclick={() => { isConfigExpanded = !isConfigExpanded; }}
        >
          {isConfigExpanded ? i18n.t("organizer.collapseSummaryBtn") : i18n.t("organizer.customizeSummaryBtn")}
        </Button>
      </div>

      {#if isConfigExpanded}
        <div transition:slide class="space-y-4">
          <div class="max-w-3xl mx-auto bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4 text-brand-text-primary">
            <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t("organizer.sectionTemplatePattern")}</h3>
            {@render templateSection()}
          </div>

          <div class="max-w-3xl mx-auto bg-brand-sidebar border border-brand-border rounded-xl p-6 space-y-4 text-brand-text-primary">
            <h3 class="font-bold text-sm text-brand-text-primary">{i18n.t("organizer.sectionDestinationOptions")}</h3>
            {@render destinationSection()}
          </div>
        </div>
      {/if}
    </div>
  {:else}
    <div
      use:portal
      class="fixed inset-0 z-50 flex items-center justify-center p-4 acrylic-scrim"
      role="dialog"
      aria-modal="true"
    >
      <div
        class="w-full max-w-4xl max-h-[90vh] bg-brand-sidebar border border-brand-border/80 rounded-2xl shadow-2xl flex flex-col overflow-hidden text-brand-text-primary"
      >
        <div class="px-6 py-4 border-b border-brand-border/40 flex items-center justify-between shrink-0">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-xl bg-brand-accent/15 text-brand-accent-text">
              <Folder class="w-5 h-5" />
            </div>
            <div>
              <h2 class="text-base font-bold text-brand-text-primary">
                {i18n.t("organizer.title")}
              </h2>
              <p class="text-xs text-brand-text-secondary">
                {i18n.t("organizer.subtitle")}
              </p>
            </div>
          </div>

          <button
            onclick={() => onClose?.()}
            class="p-1.5 rounded-lg text-brand-text-secondary hover:text-brand-text-primary hover:bg-brand-sidebar/80 transition-colors"
            title={i18n.t("organizer.close")}
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <div class="p-6 space-y-5 overflow-y-auto flex-1 text-xs">
          {@render scopeToggle()}

          <div>
            <h3 class="font-bold text-sm text-brand-text-primary mb-3">{i18n.t("organizer.sectionTemplatePattern")}</h3>
            {@render templateSection()}
          </div>

          <div class="pt-2 border-t border-brand-border/40">
            <h3 class="font-bold text-sm text-brand-text-primary mb-3 pt-3">{i18n.t("organizer.sectionDestinationOptions")}</h3>
            {@render destinationSection()}
          </div>

          <div class="pt-2 border-t border-brand-border/40 pt-3">
            {@render previewSection()}
          </div>
        </div>

        <div class="px-6 py-4 border-t border-brand-border/40 flex items-center justify-end shrink-0 bg-brand-sidebar/50">
          <div class="flex items-center gap-3">
            {@render actionMessages()}
            <button
              type="button"
              onclick={() => onClose?.()}
              class="px-4 py-2 rounded-xl border border-brand-border/80 text-brand-text-primary hover:bg-brand-sidebar transition-colors"
            >
              {i18n.t("organizer.cancel")}
            </button>

            {@render applyButton()}
          </div>
        </div>
      </div>
    </div>
  {/if}

  {#if showConfirmApply}
    <ConfirmDialog
      title={i18n.t("organizer.confirmApplyTitle")}
      message={confirmApplyMessage}
      confirmLabel={i18n.t("organizer.confirmApplyBtn")}
      cancelLabel={i18n.t("organizer.cancel")}
      danger={false}
      onConfirm={() => {
        showConfirmApply = false;
        executeApply();
      }}
      onCancel={() => {
        showConfirmApply = false;
      }}
    />
  {/if}

  {#if contextMenu}
    <ContextMenu x={contextMenu.x} y={contextMenu.y} onClose={() => { contextMenu = null; }}>
      <ContextMenuItem
        icon={Edit3}
        label={i18n.t("organizer.editTags")}
        onclick={() => {
          const id = contextMenu!.item.song_id;
          contextMenu = null;
          openTagEditor(id);
        }}
      />
      {#if picardStore.available}
        <ContextMenuItem
          icon={OpenInPicard}
          label={i18n.t("picard.openInPicard")}
          onclick={() => {
            const id = contextMenu!.item.song_id;
            contextMenu = null;
            handleOpenInPicard(id);
          }}
        />
      {/if}
      <ViewInFilesItem songId={contextMenu.item.song_id} onClose={() => { contextMenu = null; }} />
    </ContextMenu>
  {/if}

  {#if editingSongId !== null}
    <TagEditor
      songId={editingSongId}
      onClose={() => { editingSongId = null; }}
      onSave={handleTagEditorSaved}
    />
  {/if}
{/if}
