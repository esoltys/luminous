<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Music, Disc, LoaderCircle } from "lucide-svelte";
  import { getCoverArtUrl } from "../types";

  interface Props {
    songId: number | undefined;
    artEmbedded?: boolean;
    artAutomatic?: string | null;
    artManual?: string | null;
    sizeClass?: string; // e.g. "w-12 h-12" or "w-full h-full"
    animateSpin?: boolean;
  }

  let {
    songId,
    artEmbedded = false,
    artAutomatic = null,
    artManual = null,
    sizeClass = "w-12 h-12",
    animateSpin = false
  }: Props = $props();

  let imgSrc = $state<string | null>(null);
  let isLoading = $state(false);
  let hasFailed = $state(false);

  // Function to load the cover art URI
  async function loadCoverArt() {
    if (artManual) {
      if (artManual.startsWith("http://") || artManual.startsWith("https://") || artManual.startsWith("/")) {
        imgSrc = artManual;
      } else {
        imgSrc = getCoverArtUrl(`luminous-art://${artManual}`);
      }
      hasFailed = false;
      return;
    }
    if (artAutomatic) {
      if (artAutomatic.startsWith("http://") || artAutomatic.startsWith("https://") || artAutomatic.startsWith("/")) {
        imgSrc = artAutomatic;
      } else if (artAutomatic.startsWith("album-")) {
        imgSrc = getCoverArtUrl(`luminous-art://${artAutomatic}`);
      } else {
        imgSrc = getCoverArtUrl(`luminous-art://local/${artAutomatic}`);
      }
      hasFailed = false;
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
      const uri = await invoke<string | null>("get_cover_art_uri", { songId });
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
    if (songId === undefined) return;
    try {
      const uri = await invoke<string | null>("fetch_remote_cover", { songId });
      if (uri) {
        imgSrc = getCoverArtUrl(`luminous-art://${uri}`);
      }
    } catch (e) {
      console.error("Failed to fetch remote cover:", e);
    }
  }

  // React to changes in songId, artAutomatic, etc. using Svelte 5 $effect
  $effect(() => {
    // Svelte 5 will re-run this function if any of these referenced variables change
    const _id = songId;
    const _auto = artAutomatic;
    const _manual = artManual;
    const _embed = artEmbedded;
    loadCoverArt();
  });
</script>

<div class="{sizeClass} relative rounded overflow-hidden bg-gray-900 border border-gray-800 flex items-center justify-center text-gray-400 group shrink-0">
  {#if imgSrc && !hasFailed}
    <img
      src={imgSrc}
      alt="Album Art"
      class="w-full h-full object-cover transition-opacity duration-300 {isLoading ? 'opacity-0' : 'opacity-100'} {animateSpin ? 'animate-spin' : ''}"
      style={animateSpin ? "animation-duration: 6s;" : ""}
      onerror={() => {
        hasFailed = true;
        triggerRemoteFetch();
      }}
    />
  {:else if isLoading}
    <LoaderCircle class="w-1/2 h-1/2 animate-spin text-violet-400" />
  {:else}
    <div class="flex items-center justify-center w-full h-full bg-linear-to-b from-gray-800 to-gray-950">
      {#if animateSpin}
        <Disc class="w-1/2 h-1/2 animate-spin text-violet-400" style="animation-duration: 4s;" />
      {:else}
        <Music class="w-1/2 h-1/2 text-gray-600" />
      {/if}
    </div>
  {/if}
</div>
