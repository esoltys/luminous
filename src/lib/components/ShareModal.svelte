<script lang="ts">
  import { onMount } from "svelte";
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
  import Toggle from "./Toggle.svelte";
  import { i18n } from "../stores/i18n.svelte";
  import { toastStore } from "../stores/toast.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { extractColorsFromImage } from "../stores/theme.svelte";
  import { getCoverArtUrl, resolveArtUrl, type Song, type StatsSummary, type StatsRange, type StatsTopItem } from "../types";
  import { getArtistAlbums, classifyRelease } from "../utils/artist";
  import { songsToCoverStack, getArtistCoverStack, type CoverStackItem } from "../utils/covers";
  import { bucketListeningClock } from "../utils/listeningClock";
  import type { DaypartBucket } from "../utils/daypart";
  import {
    SHARE_ASPECT_RATIOS,
    rasterizeShareCard,
    rasterizeStatsShareCard,
    toDataUri,
    blobToBase64,
    type ShareAspectRatio,
    type ShareCardTheme,
    type ShareCardTrack,
    type StatsShareCardSection,
    type StatsShareCardClockBucket,
  } from "../utils/shareCard";

  export type ShareEntity =
    | { kind: "album"; albumName: string }
    | { kind: "playlist"; title: string; songs: Song[] }
    | { kind: "artist"; artistName: string }
    | { kind: "stats"; summary: StatsSummary; range: StatsRange; rangeLabel: string };

  let { entity, onClose }: { entity: ShareEntity; onClose: () => void } = $props();

  // Remembered across cards/sessions as app settings — a user who picks
  // 9:16 + track list once almost always wants the same setup next time.
  const SETTING_ASPECT_RATIO = "share_card_aspect_ratio";
  const SETTING_THEME = "share_card_theme";
  const SETTING_INCLUDE_TRACK_LIST = "share_card_include_track_list";
  const SETTING_INCLUDE_LIBRARY_INFO = "share_card_include_library_info";

  let aspectRatio = $state<ShareAspectRatio>("1:1");
  let theme = $state<ShareCardTheme>("dark");
  let includeTrackList = $state(true);
  let includeLibraryInfo = $state(true);
  let settingsLoaded = $state(false);
  let previewUrl = $state<string | null>(null);
  let rendering = $state(false);
  let exporting = $state(false);
  let lastBlob: Blob | null = null;

  onMount(async () => {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      const savedRatio = settings[SETTING_ASPECT_RATIO];
      if (savedRatio && SHARE_ASPECT_RATIOS.some((r) => r.id === savedRatio)) {
        aspectRatio = savedRatio as ShareAspectRatio;
      }
      const savedTheme = settings[SETTING_THEME];
      if (savedTheme === "light" || savedTheme === "dark") {
        theme = savedTheme;
      }
      const savedTrackList = settings[SETTING_INCLUDE_TRACK_LIST];
      if (savedTrackList === "true" || savedTrackList === "false") {
        includeTrackList = savedTrackList === "true";
      }
      const savedLibraryInfo = settings[SETTING_INCLUDE_LIBRARY_INFO];
      if (savedLibraryInfo === "true" || savedLibraryInfo === "false") {
        includeLibraryInfo = savedLibraryInfo === "true";
      }
    } catch (err) {
      console.error("Failed to load share card settings:", err);
    } finally {
      settingsLoaded = true;
    }
  });

  $effect(() => {
    if (!settingsLoaded) return;
    void invoke("set_app_setting", { key: SETTING_ASPECT_RATIO, value: aspectRatio });
  });

  $effect(() => {
    if (!settingsLoaded) return;
    void invoke("set_app_setting", { key: SETTING_THEME, value: theme });
  });

  $effect(() => {
    if (!settingsLoaded) return;
    void invoke("set_app_setting", { key: SETTING_INCLUDE_TRACK_LIST, value: String(includeTrackList) });
  });

  $effect(() => {
    if (!settingsLoaded) return;
    void invoke("set_app_setting", { key: SETTING_INCLUDE_LIBRARY_INFO, value: String(includeLibraryInfo) });
  });

  // Only the album/playlist entity cards have a track list to toggle —
  // artist and stats cards never show one.
  let showTrackListToggle = $derived(entity.kind === "album" || entity.kind === "playlist");
  // The artist/playlist cards' metadata line (album/track counts, duration)
  // is "library data" that can be toggled off for users who just want a
  // clean name-and-image card — independent of the playlist card's own
  // track-list toggle.
  let showLibraryToggle = $derived(entity.kind === "artist" || entity.kind === "playlist");

  let albumItem = $derived(
    entity.kind === "album" ? collectionStore.albums.find((a) => a.album === entity.albumName) || null : null
  );
  let albumSongs = $state<Song[]>([]);

  $effect(() => {
    if (entity.kind !== "album") {
      albumSongs = [];
      return;
    }
    const name = entity.albumName;
    let cancelled = false;
    invoke<Song[]>("get_songs_by_album", { album: name })
      .then((fetched) => {
        if (!cancelled) albumSongs = fetched;
      })
      .catch((err) => console.error("Failed to load songs for share card:", err));
    return () => {
      cancelled = true;
    };
  });

  let artistSongs = $state<Song[]>([]);
  let artistPortraitUrl = $state<string | null>(null);

  $effect(() => {
    if (entity.kind !== "artist") {
      artistSongs = [];
      artistPortraitUrl = null;
      return;
    }
    const name = entity.artistName;
    let cancelled = false;
    invoke<Song[]>("get_songs_by_artist", { artist: name })
      .then((fetched) => {
        if (!cancelled) artistSongs = fetched;
      })
      .catch((err) => console.error("Failed to load songs for share card:", err));
    collectionStore.getExtendedArtworkForArtist(name).then((res) => {
      if (!cancelled) artistPortraitUrl = getCoverArtUrl(res?.artist_portrait_uri) ?? null;
    });
    return () => {
      cancelled = true;
    };
  });

  let artistAlbums = $derived(entity.kind === "artist" ? getArtistAlbums(collectionStore.albums, entity.artistName) : []);
  // "Album count" means full albums/box-sets, matching ArtistDetailView's
  // fullAlbums+sets split — EPs and singles are real releases but users
  // reported the raw artistAlbums.length (which includes them) reading as
  // an inflated, confusing "album count" on the card.
  let artistFullAlbumCount = $derived(
    artistAlbums.filter((a) => {
      const category = classifyRelease(a.track_count, a.disc_count, a.total_duration_nanosec);
      return category === "album" || category === "set";
    }).length
  );

  // Songs backing the entity-card path (album/playlist/artist) — the stats
  // card has no song list of its own.
  let songs = $derived.by((): Song[] => {
    if (entity.kind === "album") return albumSongs;
    if (entity.kind === "playlist") return entity.songs;
    if (entity.kind === "artist") return artistSongs;
    return [];
  });

  let sortedTracks = $derived.by(() => {
    if (entity.kind !== "album") return songs;
    return [...songs].sort((a, b) => {
      if ((a.disc ?? 1) !== (b.disc ?? 1)) return (a.disc ?? 1) - (b.disc ?? 1);
      return (a.track ?? 0) - (b.track ?? 0);
    });
  });

  let trackCards = $derived<ShareCardTrack[]>(
    entity.kind === "playlist"
      // A playlist spans multiple artists, unlike an album, so each row
      // needs its own artist to be legible on its own.
      ? sortedTracks.map((s, i) => ({ number: i + 1, title: s.title || "", secondary: s.artist || s.album_artist || "" }))
      : sortedTracks.map((s) => ({ number: s.track ?? null, title: s.title || "" }))
  );

  let totalDurationLabel = $derived.by(() => {
    const totalNs = songs.reduce((sum, s) => sum + (s.length_nanosec ?? 0), 0);
    const totalMinutes = Math.round(totalNs / 1_000_000_000 / 60);
    const h = Math.floor(totalMinutes / 60);
    const m = totalMinutes % 60;
    return h > 0 ? `${h}h ${m}m` : `${m}m`;
  });

  let cardTitle = $derived.by(() => {
    switch (entity.kind) {
      case "album":
        return entity.albumName || i18n.t("collection.unknownAlbum");
      case "playlist":
        return entity.title || i18n.t("playlists.untitledPlaylistName");
      case "artist":
        return entity.artistName || i18n.t("collection.unknownArtist");
      case "stats":
        return entity.rangeLabel;
    }
  });

  let cardSubtitle = $derived.by(() => {
    if (entity.kind !== "album") return "";
    if (albumItem?.artist) return albumItem.artist;
    if (songs.length > 0) return songs[0].album_artist || songs[0].artist || "";
    return "";
  });

  let cardMetadataLine = $derived.by(() => {
    if (entity.kind === "stats") return "";
    if ((entity.kind === "artist" || entity.kind === "playlist") && !includeLibraryInfo) return "";
    const parts: string[] = [];
    if (entity.kind === "album" && albumItem?.year) parts.push(String(albumItem.year));
    if (entity.kind === "artist") {
      parts.push(
        artistFullAlbumCount === 1
          ? i18n.t("collection.oneAlbum")
          : i18n.t("collection.albumsCount", { count: artistFullAlbumCount })
      );
    }
    parts.push(
      songs.length === 1 ? i18n.t("playlists.oneSong") : i18n.t("playlists.songsCount", { count: songs.length })
    );
    if (totalDurationLabel) parts.push(totalDurationLabel);
    return parts.join(" • ");
  });

  let cardSeed = $derived.by(() => {
    switch (entity.kind) {
      case "album":
        return entity.albumName;
      case "playlist":
        return entity.title;
      case "artist":
        return entity.artistName;
      case "stats":
        return `stats-${entity.range}`;
    }
  });

  /** Resolves a CoverStackItem (manual/automatic art, or embedded art needing a lookup) to a displayable URL. */
  async function resolveCoverUrl(item: CoverStackItem): Promise<string | null> {
    if (item.artManual) return resolveArtUrl(item.artManual);
    if (item.artAutomatic) return resolveArtUrl(item.artAutomatic);
    if (item.artEmbedded && item.songId !== undefined) {
      try {
        const uri = await invoke<string | null>("get_cover_art_uri", { songId: item.songId });
        return uri ? getCoverArtUrl(uri) : null;
      } catch (e) {
        console.error("Failed to load cover art for share card:", e);
        return null;
      }
    }
    return null;
  }

  let coverUrl = $state<string | null>(null);
  let coverStackUrls = $state<string[]>([]);

  $effect(() => {
    let cancelled = false;

    async function resolve() {
      if (entity.kind === "album") {
        const item = albumItem;
        const fallbackSongId = item?.sample_song_id ?? songs[0]?.id;
        const url = await resolveCoverUrl({
          songId: fallbackSongId,
          artManual: item?.art_manual,
          artAutomatic: item?.art_automatic,
          artEmbedded: item?.art_embedded,
        });
        if (!cancelled) {
          coverUrl = url;
          coverStackUrls = [];
        }
      } else if (entity.kind === "artist") {
        // Matches ArtistDetailView's own hero header: a portrait photo wins
        // as a single image when one exists, otherwise fall back to a
        // fanned stack of the artist's own album covers (getArtistCoverStack)
        // rather than a single cover.
        if (artistPortraitUrl) {
          if (!cancelled) {
            coverUrl = artistPortraitUrl;
            coverStackUrls = [];
          }
        } else {
          const stackItems = getArtistCoverStack(artistAlbums, artistSongs, 4);
          const urls = (await Promise.all(stackItems.map(resolveCoverUrl))).filter((u): u is string => !!u);
          if (!cancelled) {
            coverUrl = urls[0] ?? null;
            coverStackUrls = urls;
          }
        }
      } else if (entity.kind === "playlist") {
        const stackItems = songsToCoverStack(entity.songs, 4);
        const urls = (await Promise.all(stackItems.map(resolveCoverUrl))).filter((u): u is string => !!u);
        if (!cancelled) {
          coverUrl = urls[0] ?? null;
          coverStackUrls = urls;
        }
      } else if (!cancelled) {
        coverUrl = null;
        coverStackUrls = [];
      }
    }

    resolve();
    return () => {
      cancelled = true;
    };
  });

  let coverDataUri = $state<string | null>(null);
  let coverStackDataUris = $state<string[]>([]);
  let backgroundColors = $state<string[] | undefined>(undefined);

  $effect(() => {
    const url = coverUrl;
    const stackUrls = coverStackUrls;
    let cancelled = false;
    if (!url && stackUrls.length === 0) {
      coverDataUri = null;
      coverStackDataUris = [];
      backgroundColors = undefined;
      return;
    }
    Promise.all([
      url ? toDataUri(url) : Promise.resolve(null),
      Promise.all(stackUrls.map((u) => toDataUri(u))),
      extractColorsFromImage((url ?? stackUrls[0])!),
    ]).then(([dataUri, stackDataUris, colors]) => {
      if (cancelled) return;
      coverDataUri = dataUri;
      coverStackDataUris = stackDataUris.filter((u): u is string => !!u);
      backgroundColors = [colors.vibrant, colors.darkVibrant, colors.lightVibrant, colors.muted].filter(
        (c): c is string => !!c
      );
    });
    return () => {
      cancelled = true;
    };
  });

  // ---- Stats card ----

  let statsTotalMinutesLabel = $derived.by(() => {
    if (entity.kind !== "stats") return "";
    const n = entity.summary.total_minutes;
    return n === 1
      ? i18n.t("stats.totalMinutesOne", {}, "1 minute listened")
      : i18n.t("stats.totalMinutes", { count: n }, `${n} minutes listened`);
  });

  let statsSections = $derived.by((): StatsShareCardSection[] => {
    if (entity.kind !== "stats") return [];
    const s = entity.summary;
    const toItems = (items: StatsTopItem[]) =>
      items.slice(0, 5).map((it) => ({ label: it.label, secondary: it.secondary }));
    return [
      { title: i18n.t("stats.topArtists", {}, "Top Artists"), items: toItems(s.top_artists) },
      { title: i18n.t("stats.topAlbums", {}, "Top Albums"), items: toItems(s.top_albums) },
      { title: i18n.t("stats.topSongs", {}, "Top Songs"), items: toItems(s.top_songs) },
      { title: i18n.t("stats.topGenres", {}, "Top Genres"), items: toItems(s.top_genres) },
    ];
  });

  let statsClockBuckets = $derived.by((): StatsShareCardClockBucket[] => {
    if (entity.kind !== "stats") return [];
    const counts = bucketListeningClock(entity.summary.play_timestamps);
    const labels: Record<DaypartBucket, string> = {
      morning: i18n.t("stats.clockMorning", {}, "Morning"),
      afternoon: i18n.t("stats.clockAfternoon", {}, "Afternoon"),
      evening: i18n.t("stats.clockEvening", {}, "Evening"),
      latenight: i18n.t("stats.clockLateNight", {}, "Late Night"),
    };
    const order: DaypartBucket[] = ["morning", "afternoon", "evening", "latenight"];
    return order.map((k) => ({ label: labels[k], count: counts[k] ?? 0 }));
  });

  // ---- Rendering ----

  async function renderCard(scale: number): Promise<Blob | null> {
    if (entity.kind === "stats") {
      return rasterizeStatsShareCard(
        {
          aspectRatio,
          theme,
          seed: cardSeed,
          backgroundColors,
          rangeLabel: entity.rangeLabel,
          totalMinutesLabel: statsTotalMinutesLabel,
          sections: statsSections,
          clockBuckets: statsClockBuckets,
        },
        scale
      );
    }
    return rasterizeShareCard(
      {
        aspectRatio,
        theme,
        seed: cardSeed,
        backgroundColors,
        coverDataUri,
        coverStackDataUris,
        title: cardTitle,
        subtitle: cardSubtitle,
        metadataLine: cardMetadataLine,
        tracks: trackCards,
        includeTrackList: showTrackListToggle && includeTrackList,
      },
      scale
    );
  }

  $effect(() => {
    // Track every option the rendered card depends on so this re-runs when any changes.
    void aspectRatio;
    void theme;
    void includeTrackList;
    void includeLibraryInfo;
    void coverDataUri;
    void coverStackDataUris;
    void backgroundColors;
    void trackCards;
    void cardTitle;
    void cardSubtitle;
    void cardMetadataLine;
    void statsSections;
    void statsClockBuckets;
    void statsTotalMinutesLabel;

    let cancelled = false;
    rendering = true;
    renderCard(1.5)
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
    return renderCard(3);
  }

  let exportFilename = $derived.by(() => {
    switch (entity.kind) {
      case "album":
        return entity.albumName || "album";
      case "playlist":
        return entity.title || "playlist";
      case "artist":
        return entity.artistName || "artist";
      case "stats":
        return `stats-${entity.range}`;
    }
  });

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
        defaultPath: `${exportFilename}-share.png`,
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

        {#if showTrackListToggle}
          <div class="flex items-center gap-2">
            <span class="text-xs font-medium text-brand-text-secondary text-right whitespace-nowrap">{i18n.t("shareModal.trackListToggle")}</span>
            <Toggle
              checked={includeTrackList}
              onchange={(v) => (includeTrackList = v)}
              label={i18n.t("shareModal.trackListToggle")}
              showOnOffLabel={false}
            />
          </div>
        {/if}
        {#if showLibraryToggle}
          <div class="flex items-center gap-2">
            <span class="text-xs font-medium text-brand-text-secondary text-right whitespace-nowrap">{i18n.t("shareModal.libraryToggle")}</span>
            <Toggle
              checked={includeLibraryInfo}
              onchange={(v) => (includeLibraryInfo = v)}
              label={i18n.t("shareModal.libraryToggle")}
              showOnOffLabel={false}
            />
          </div>
        {/if}
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
