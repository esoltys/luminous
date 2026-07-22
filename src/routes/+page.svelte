<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import HomeView from "../lib/components/HomeView.svelte";
  import CollectionView from "../lib/components/CollectionView.svelte";
  import PlaylistsCollectionView from "../lib/components/PlaylistsCollectionView.svelte";
  import FoldersView from "../lib/components/FoldersView.svelte";
  import LyricsView from "../lib/components/LyricsView.svelte";
  import { themeStore } from "../lib/stores/theme.svelte";
  import { collectionStore, type ActiveTab, type ActiveSubTab } from "../lib/stores/collection.svelte";
  import { playerStore } from "../lib/stores/player.svelte";

  let isInitialized = $state(false);

  const SEEK_STEP_NS = 10_000_000_000;
  const VOLUME_STEP = 0.05;

  function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false;

    const editable = target.closest("input, textarea, select, [contenteditable]");
    return editable !== null;
  }

  function handleKeyboardShortcut(event: KeyboardEvent) {
    if (event.repeat || isEditableTarget(event.target)) return;

    switch (event.code) {
      case "Space":
        event.preventDefault();
        playerStore.togglePlayPause().catch((err) => console.error("Failed to toggle playback:", err));
        break;
      case "ArrowLeft":
        event.preventDefault();
        playerStore.seekRelative(-SEEK_STEP_NS).catch((err) => console.error("Failed to seek backward:", err));
        break;
      case "ArrowRight":
        event.preventDefault();
        playerStore.seekRelative(SEEK_STEP_NS).catch((err) => console.error("Failed to seek forward:", err));
        break;
      case "ArrowUp":
        event.preventDefault();
        playerStore.adjustVolume(VOLUME_STEP).catch((err) => console.error("Failed to increase volume:", err));
        break;
      case "ArrowDown":
        event.preventDefault();
        playerStore.adjustVolume(-VOLUME_STEP).catch((err) => console.error("Failed to decrease volume:", err));
        break;
      case "PageUp":
        event.preventDefault();
        playerStore.previous().catch((err) => console.error("Failed to play previous track:", err));
        break;
      case "PageDown":
        event.preventDefault();
        playerStore.next().catch((err) => console.error("Failed to play next track:", err));
        break;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", handleKeyboardShortcut);

    (async () => {
      // Initialize theme store first to prevent flash of default theme
      await themeStore.init();

      try {
        const settings = await invoke<Record<string, string>>("get_all_app_settings");
        if (settings) {
          if (settings.active_tab) {
            if (settings.active_tab === "equalizer") {
              collectionStore.activeTab = "settings";
              invoke("set_app_setting", { key: "active_tab", value: "settings" }).catch((err) => {
                console.error("Failed to save migrated active_tab:", err);
              });
              invoke("set_app_setting", { key: "active_settings_tab", value: "equalizer" }).catch((err) => {
                console.error("Failed to save migrated active_settings_tab:", err);
              });
            } else {
              collectionStore.activeTab = settings.active_tab as ActiveTab;
            }
          }
          if (settings.active_sub_tab) {
            collectionStore.activeSubTab = settings.active_sub_tab as ActiveSubTab;
          }
        }
      } catch (e) {
        console.error("Failed to restore app settings:", e);
      } finally {
        isInitialized = true;
      }
    })();

    return () => {
      window.removeEventListener("keydown", handleKeyboardShortcut);
    };
  });

  $effect(() => {
    if (isInitialized) {
      invoke("set_app_setting", { key: "active_tab", value: collectionStore.activeTab }).catch((err) => {
        console.error("Failed to save active_tab:", err);
      });
    }
  });

  $effect(() => {
    if (isInitialized) {
      invoke("set_app_setting", { key: "active_sub_tab", value: collectionStore.activeSubTab }).catch((err) => {
        console.error("Failed to save active_sub_tab:", err);
      });
    }
  });
  import SmartPlaylistBuilderModal from "../lib/components/SmartPlaylistBuilderModal.svelte";
</script>

<div class="flex flex-col h-full overflow-hidden bg-brand-main font-sans">
  <!-- Main View Content Area -->
  <div class="flex-1 min-w-0 overflow-hidden flex flex-col">
    {#if collectionStore.activeTab === "home"}
      <HomeView />
    {:else if collectionStore.activeTab === "collection"}
      <CollectionView />
    {:else if collectionStore.activeTab === "playlists"}
      <PlaylistsCollectionView />
    {:else if collectionStore.activeTab === "settings"}
      <FoldersView />
    {:else if collectionStore.activeTab === "lyrics"}
      <LyricsView />
    {/if}
  </div>
</div>

{#if collectionStore.isSmartBuilderOpen}
  <SmartPlaylistBuilderModal
    initialRules={collectionStore.smartBuilderRules}
    onClose={() => collectionStore.closeSmartBuilder()}
  />
{/if}
