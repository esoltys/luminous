<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import type { AddonTheme } from "../stores/addons.svelte";
  import { playerStore } from "../stores/player.svelte";
  import { themeStore } from "../stores/theme.svelte";
  import { acquireSpectrum, releaseSpectrum } from "../utils/spectrumEnable";
  import { prefersReducedMotion } from "../utils/motion";
  import { addonFrameUrl, spectrumMessage, type OverlayMessage } from "../utils/addonBridge";

  // Mounts an add-on's overlay above the player bar (#1036). The overlay runs
  // in a sandboxed iframe with no Tauri IPC; this component is the only thing
  // that talks to it, one-way, through the message API in addonBridge.ts.
  let { addon }: { addon: AddonTheme } = $props();

  let frame = $state<HTMLIFrameElement | undefined>(undefined);
  let loaded = $state(false);

  let src = $derived(addon.overlayEntry ? addonFrameUrl(addon.id, addon.overlayEntry) : null);
  let wantsSpectrum = $derived(addon.capabilities?.includes("spectrum") ?? false);

  function post(message: OverlayMessage) {
    // The frame's origin is opaque, so "*" is the only usable target; messages
    // carry playback info only and nothing in them is secret.
    frame?.contentWindow?.postMessage(message, "*");
  }

  function postSize() {
    if (!frame) return;
    post({
      luminousAddon: 1,
      kind: "size",
      width: frame.clientWidth,
      height: frame.clientHeight
    });
  }

  $effect(() => {
    // Reading every field here subscribes the effect to each of them.
    const song = playerStore.currentSong;
    const message: OverlayMessage = {
      luminousAddon: 1,
      kind: "state",
      isPlaying: playerStore.state === "playing",
      positionMs: Math.round(playerStore.positionNanosec / 1_000_000),
      track: song
        ? { title: song.title ?? "", artist: song.artist ?? "", album: song.album ?? "" }
        : null,
      accent: themeStore.resolvedColors["color-accent"],
      reducedMotion: prefersReducedMotion()
    };
    if (loaded) post(message);
  });

  $effect(() => {
    if (!wantsSpectrum) return;
    acquireSpectrum();
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen<number[]>("spectrum-data", (event) => {
      if (loaded && !document.hidden && event.payload?.length) {
        post(spectrumMessage(event.payload));
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      releaseSpectrum();
    };
  });

  onMount(() => {
    if (!frame) return;
    const observer = new ResizeObserver(() => {
      if (loaded) postSize();
    });
    observer.observe(frame);
    return () => observer.disconnect();
  });

  // Debug builds only (#1426): the backend re-reads an unpacked add-on folder
  // when a file changes. The query string is ignored by the scheme handler and
  // forces a fresh load of the frame.
  onMount(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void listen<string>("addon-dev-reloaded", (event) => {
      if (event.payload === addon.id && src && frame) {
        loaded = false;
        frame.src = `${src}?r=${Date.now()}`;
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  function onLoad() {
    loaded = true;
    postSize();
  }
</script>

{#if src}
  <iframe
    bind:this={frame}
    {src}
    title={addon.name}
    sandbox="allow-scripts"
    referrerpolicy="no-referrer"
    tabindex="-1"
    aria-hidden="true"
    data-addon-id={addon.id}
    class="absolute bottom-full inset-x-0 h-60 w-full border-0 bg-transparent pointer-events-none"
    onload={onLoad}
  ></iframe>
{/if}
