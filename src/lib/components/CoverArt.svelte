<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { untrack } from "svelte";
  import {
    MusicNotesIcon as Music,
    DiscIcon as Disc,
    CircleNotchIcon as LoaderCircle
  } from "phosphor-svelte";
  import { getCoverArtUrl, resolveArtUrl } from "../types";
  import { i18n } from "../stores/i18n.svelte";
  import { prefs } from "../stores/prefs.svelte";
  import { collectionStore } from "../stores/collection.svelte";
  import { sizedCoverUrl } from "../utils/covers";

  interface Props {
    songId: number | undefined;
    artEmbedded?: boolean;
    artAutomatic?: string | null;
    artManual?: string | null;
    sizeClass?: string; // e.g. "w-12 h-12" or "w-full h-full"
    animateSpin?: boolean;
    /** Show the original embedded picture rather than the downscaled cache copy (#1272). */
    fullResolution?: boolean;
  }

  let {
    songId,
    artEmbedded = false,
    artAutomatic = null,
    artManual = null,
    sizeClass = "w-12 h-12",
    animateSpin = false,
    fullResolution = false
  }: Props = $props();

  let imgSrc = $state<string | null>(null);
  let isLoading = $state(false);
  let hasFailed = $state(false);
  let loadToken = 0;
  // The box's widest measured width in device pixels, so a card loads a
  // cover copy near its drawn size (#1528). Never shrinks, so a narrowing
  // window doesn't refetch; null until measured, and the <img> waits for it
  // so it never starts loading a copy of the wrong size.
  let box = $state<HTMLDivElement>();
  let boxPixels = $state<number | null>(null);
  let displaySrc = $derived(
    imgSrc && boxPixels !== null && !fullResolution ? sizedCoverUrl(imgSrc, boxPixels) : imgSrc
  );

  $effect(() => {
    const node = box;
    if (!node) return;
    const measure = () => {
      const pixels = node.clientWidth * window.devicePixelRatio;
      if (boxPixels === null || pixels > boxPixels) boxPixels = pixels;
    };
    untrack(measure);
    const ro = new ResizeObserver(measure);
    ro.observe(node);
    return () => ro.disconnect();
  });

  async function loadCoverArt() {
    const token = ++loadToken;
    if (artManual) {
      imgSrc = resolveArtUrl(artManual);
      hasFailed = false;
      return;
    }
    if (artAutomatic) {
      imgSrc = resolveArtUrl(artAutomatic, fullResolution);
      hasFailed = false;
      // The cache holds a downscaled copy; show it at once, then swap in the
      // original embedded picture for large views.
      if (fullResolution && artEmbedded && songId !== undefined && artAutomatic.startsWith("album-")) {
        try {
          const uri = await invoke<string | null>("get_cover_art_uri", { songId, fullResolution: true });
          if (uri && token === loadToken) imgSrc = getCoverArtUrl(uri);
        } catch (e) {
          console.error("Failed to load full-resolution cover art URI:", e);
        }
      }
      return;
    }

    if (songId === undefined) {
      imgSrc = null;
      hasFailed = false;
      return;
    }

    isLoading = true;
    hasFailed = false;
    try {
      const uri = await invoke<string | null>("get_cover_art_uri", { songId, fullResolution });
      if (token !== loadToken) return;
      if (uri) {
        imgSrc = getCoverArtUrl(uri);
      } else {
        imgSrc = null;
        triggerRemoteFetch();
      }
    } catch (e) {
      console.error("Failed to load cover art URI:", e);
      hasFailed = true;
    } finally {
      isLoading = false;
    }
  }

  async function triggerRemoteFetch() {
    if (songId === undefined || !prefs.onlineEnabled) return;
    try {
      const uri = await invoke<string | null>("fetch_remote_cover", { songId });
      if (uri) {
        imgSrc = getCoverArtUrl(`luminous-art://${uri}`);
        hasFailed = false;
        return;
      }
      // iTunes has now missed too, which makes the fanart.tv cover the
      // fallback (#1277): ask once more rather than re-running
      // loadCoverArt, which would loop back here.
      const fallback = await invoke<string | null>("get_cover_art_uri", { songId });
      if (fallback) {
        imgSrc = getCoverArtUrl(fallback);
        hasFailed = false;
      }
    } catch (e) {
      console.error("Failed to fetch remote cover:", e);
    }
  }

  $effect(() => {
    const _id = songId;
    const _auto = artAutomatic;
    const _manual = artManual;
    const _embed = artEmbedded;
    const _full = fullResolution;
    // A fanart.tv cover arriving or its toggle changing re-resolves (#1277).
    const _fanart = prefs.fanartFetchAlbumCover;
    // Going Offline hides fanart.tv covers; going Online retries missing art (#1398).
    const _online = prefs.onlineEnabled;
    const _version = collectionStore.coverArtVersion;
    loadCoverArt();
  });
</script>

<div bind:this={box} class="{sizeClass} relative overflow-hidden bg-brand-sidebar border border-brand-border flex items-center justify-center text-brand-text-secondary group shrink-0">
  {#if displaySrc && boxPixels !== null && !hasFailed}
    <img
      src={displaySrc}
      alt={i18n.t('common.albumArtAlt')}
      loading="lazy"
      class="w-full h-full object-cover transition-opacity duration-300 {isLoading ? 'opacity-0' : 'opacity-100'} {animateSpin ? 'animate-spin' : ''}"
      style={animateSpin ? "animation-duration: 6s;" : ""}
      onerror={() => {
        hasFailed = true;
        triggerRemoteFetch();
      }}
    />
  {:else if isLoading}
    <LoaderCircle class="w-1/2 h-1/2 animate-spin text-brand-accent-text" />
  {:else}
    <div class="flex items-center justify-center w-full h-full bg-linear-to-b from-brand-sidebar to-brand-main">
      {#if animateSpin}
        <Disc class="w-1/2 h-1/2 animate-spin text-brand-accent-text" style="animation-duration: 4s;" />
      {:else}
        <Music class="w-1/2 h-1/2 text-brand-text-secondary/60" />
      {/if}
    </div>
  {/if}
</div>
