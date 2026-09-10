<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { save } from "@tauri-apps/plugin-dialog";
  import {
    XIcon as X,
    CopySimpleIcon as Copy,
    DownloadSimpleIcon as Download,
    SunIcon as Sun,
    MoonIcon as Moon,
  } from "phosphor-svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { extractColorsFromImage } from "../stores/theme.svelte";
  import { getCoverArtUrl, resolveArtUrl, type Song } from "../types";
  import {
    SHARE_ASPECT_RATIOS,
    rasterizeShareCard,
    toDataUri,
    blobToBase64,
    type ShareAspectRatio,
    type ShareCardTheme,
    type ShareCardTrack,
  } from "../utils/shareCard";

  let { albumName, onClose }: { albumName: string; onClose: () => void } = $props();

  let aspectRatio = $state<ShareAspectRatio>("1:1");
  let theme = $state<ShareCardTheme>("dark");
  let includeTrackList = $state(true);
  let previewUrl = $state<string | null>(null);
  let rendering = $state(false);
  let exporting = $state(false);
  let lastBlob: Blob | null = null;

  let albumItem = $derived(collectionStore.albums.find((a) => a.album === albumName) || null);
  let songs = $state<Song[]>([]);

  $effect(() => {
    let cancelled = false;
    invoke<Song[]>("get_songs_by_album", { album: albumName })
      .then((fetched) => {
        if (!cancelled) songs = fetched;
      })
      .catch((err) => console.error("Failed to load songs for share card:", err));
    return () => {
      cancelled = true;
    };
  });

  let artistName = $derived.by(() => {
    if (albumItem?.artist) return albumItem.artist;
    if (songs.length > 0) return songs[0].album_artist || songs[0].artist || "";
    return "";
  });

  let sortedTracks = $derived.by(() => {
    return [...songs].sort((a, b) => {
      if ((a.disc ?? 1) !== (b.disc ?? 1)) return (a.disc ?? 1) - (b.disc ?? 1);
      return (a.track ?? 0) - (b.track ?? 0);
    });
  });

  let trackCards = $derived<ShareCardTrack[]>(
    sortedTracks.map((s) => ({ number: s.track ?? null, title: s.title || "" }))
  );

  let totalDurationLabel = $derived.by(() => {
    const totalNs = songs.reduce((sum, s) => sum + (s.length_nanosec ?? 0), 0);
    const totalMinutes = Math.round(totalNs / 1_000_000_000 / 60);
    const h = Math.floor(totalMinutes / 60);
    const m = totalMinutes % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  });

  let metadataLine = $derived.by(() => {
    const parts: string[] = [];
    if (albumItem?.year) parts.push(String(albumItem.year));
    parts.push(
      songs.length === 1
        ? i18n.t("playlists.oneSong")
        : i18n.t("playlists.songsCount", { count: songs.length })
    );
    if (totalDurationLabel) parts.push(totalDurationLabel);
    return parts.join(" • ");
  });

  let coverUrl = $state<string | null>(null);

  $effect(() => {
    const item = albumItem;
    const fallbackSongId = item?.sample_song_id ?? songs[0]?.id;
    let cancelled = false;

    async function resolve() {
      let url: string | null = null;
      if (item?.art_manual) {
        url = resolveArtUrl(item.art_manual);
      } else if (item?.art_automatic) {
        url = resolveArtUrl(item.art_automatic);
      } else if (item?.art_embedded && fallbackSongId !== undefined) {
        try {
          const uri = await invoke<string | null>("get_cover_art_uri", { songId: fallbackSongId });
          if (uri) url = getCoverArtUrl(uri);
        } catch (e) {
          console.error("Failed to load album cover for share card:", e);
        }
      }
      if (!cancelled) coverUrl = url;
    }

    resolve();
    return () => {
      cancelled = true;
    };
  });

  let coverDataUri = $state<string | null>(null);
  let backgroundColors = $state<string[] | undefined>(undefined);

  $effect(() => {
    const url = coverUrl;
    let cancelled = false;
    if (!url) {
      coverDataUri = null;
      backgroundColors = undefined;
      return;
    }
    Promise.all([toDataUri(url), extractColorsFromImage(url)]).then(([dataUri, colors]) => {
      if (cancelled) return;
      coverDataUri = dataUri;
      backgroundColors = [colors.vibrant, colors.darkVibrant, colors.lightVibrant, colors.muted].filter(
        (c): c is string => !!c
      );
    });
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    // Track every option the rendered card depends on so this re-runs when any changes.
    void aspectRatio;
    void theme;
    void includeTrackList;
    void coverDataUri;
    void backgroundColors;
    void trackCards;
    void metadataLine;

    let cancelled = false;
    rendering = true;
    rasterizeShareCard(
      {
        aspectRatio,
        theme,
        seed: albumName,
        backgroundColors,
        coverDataUri,
        title: albumName || i18n.t("collection.unknownAlbum"),
        subtitle: artistName || i18n.t("collection.unknownArtist"),
        metadataLine,
        tracks: trackCards,
        includeTrackList,
      },
      1.5
    )
      .then((blob) => {
        if (cancelled || !blob) return;
        lastBlob = blob;
        if (previewUrl) URL.revokeObjectURL(previewUrl);
        previewUrl = URL.createObjectURL(blob);
      })
      .finally(() => {
        if (!cancelled) rendering = false;
      });

    return () => {
      cancelled = true;
    };
  });

  async function getExportBlob(): Promise<Blob | null> {
    if (lastBlob) return lastBlob;
    return rasterizeShareCard(
      {
        aspectRatio,
        theme,
        seed: albumName,
        backgroundColors,
        coverDataUri,
        title: albumName || i18n.t("collection.unknownAlbum"),
        subtitle: artistName || i18n.t("collection.unknownArtist"),
        metadataLine,
        tracks: trackCards,
        includeTrackList,
      },
      3
    );
  }

  async function handleCopy() {
    exporting = true;
    try {
      const blob = await getExportBlob();
      if (!blob) throw new Error("render failed");
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      toastStore.show(i18n.t("shareModal.copySuccess"));
    } catch (err) {
      console.error("Failed to copy share card:", err);
      toastStore.show(i18n.t("shareModal.copyError"));
    } finally {
      exporting = false;
    }
  }

  async function handleSave() {
    exporting = true;
    try {
      const blob = await getExportBlob();
      if (!blob) throw new Error("render failed");
      const savePath = await save({
        title: i18n.t("shareModal.saveDialogTitle"),
        defaultPath: `${albumName || "album"}-share.png`,
        filters: [{ name: "PNG Image (*.png)", extensions: ["png"] }],
      });
      if (savePath && typeof savePath === "string") {
        const base64 = await blobToBase64(blob);
        await invoke("save_share_card_image", { path: savePath, dataBase64: base64 });
        toastStore.show(i18n.t("shareModal.saveSuccess"));
      }
    } catch (err) {
      console.error("Failed to save share card:", err);
      toastStore.show(i18n.t("shareModal.saveError"));
    } finally {
      exporting = false;
    }
  }
</script>

<Modal {onClose} maxWidth="max-w-2xl" ariaLabelledby="share-modal-title">
  <div class="flex items-center justify-between px-5 py-4 border-b border-brand-border">
    <h2 id="share-modal-title" class="text-lg font-heading font-bold text-brand-text-primary">
      {i18n.t("shareModal.title")}
    </h2>
    <button
      onclick={onClose}
      title={i18n.t("shareModal.closeTooltip")}
      class="flex items-center justify-center w-8 h-8 rounded-full text-brand-text-secondary hover:text-brand-accent-text hover:bg-brand-main transition-colors cursor-pointer"
    >
      <X class="w-4 h-4" />
    </button>
  </div>

  <div class="p-5 flex flex-col gap-4">
    <div class="flex items-center justify-center bg-brand-main rounded-lg border border-brand-border p-4 min-h-[280px]">
      {#if previewUrl}
        <img
          src={previewUrl}
          alt={i18n.t("shareModal.previewAlt")}
          class="max-w-full max-h-[50vh] rounded-lg shadow-2xl {rendering ? 'opacity-70' : ''} transition-opacity"
        />
      {:else}
        <div class="text-sm text-brand-text-secondary">{i18n.t("shareModal.rendering")}</div>
      {/if}
    </div>

    <div class="flex flex-col gap-3">
      <div class="flex flex-wrap items-center gap-2">
        <span class="text-xs font-semibold text-brand-text-secondary mr-1">{i18n.t("shareModal.aspectRatioLabel")}</span>
        {#each SHARE_ASPECT_RATIOS as ratio (ratio.id)}
          <button
            onclick={() => (aspectRatio = ratio.id)}
            class="px-3 py-1.5 rounded-full text-xs font-semibold border transition-colors cursor-pointer {aspectRatio === ratio.id
              ? 'bg-brand-accent text-brand-accent-contrast border-brand-accent'
              : 'border-brand-border text-brand-text-secondary hover:bg-brand-main'}"
          >
            {ratio.id}
          </button>
        {/each}
      </div>

      <div class="flex flex-wrap items-center justify-between gap-3">
        <div class="flex items-center gap-2">
          <span class="text-xs font-semibold text-brand-text-secondary mr-1">{i18n.t("shareModal.themeLabel")}</span>
          <button
            onclick={() => (theme = "dark")}
            title={i18n.t("shareModal.themeDark")}
            class="flex items-center justify-center w-8 h-8 rounded-full border transition-colors cursor-pointer {theme === 'dark'
              ? 'bg-brand-accent text-brand-accent-contrast border-brand-accent'
              : 'border-brand-border text-brand-text-secondary hover:bg-brand-main'}"
          >
            <Moon class="w-4 h-4" />
          </button>
          <button
            onclick={() => (theme = "light")}
            title={i18n.t("shareModal.themeLight")}
            class="flex items-center justify-center w-8 h-8 rounded-full border transition-colors cursor-pointer {theme === 'light'
              ? 'bg-brand-accent text-brand-accent-contrast border-brand-accent'
              : 'border-brand-border text-brand-text-secondary hover:bg-brand-main'}"
          >
            <Sun class="w-4 h-4" />
          </button>
        </div>

        <label class="flex items-center gap-2 text-xs font-semibold text-brand-text-secondary cursor-pointer select-none">
          <input type="checkbox" bind:checked={includeTrackList} class="accent-brand-accent" />
          {i18n.t("shareModal.trackListToggle")}
        </label>
      </div>
    </div>

    <div class="flex items-center justify-end gap-2 pt-2 border-t border-brand-border">
      <Button variant="secondary" onclick={handleCopy} disabled={exporting || rendering}>
        <Copy class="w-4 h-4" />
        {i18n.t("shareModal.copyButton")}
      </Button>
      <Button variant="primary" onclick={handleSave} disabled={exporting || rendering}>
        <Download class="w-4 h-4" />
        {i18n.t("shareModal.saveButton")}
      </Button>
    </div>
  </div>
</Modal>
