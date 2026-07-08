<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import Sidebar from "../lib/components/Sidebar.svelte";
  import PlayerBar from "../lib/components/PlayerBar.svelte";
  import CollectionView from "../lib/components/CollectionView.svelte";
  import PlaylistView from "../lib/components/PlaylistView.svelte";
  import FoldersView from "../lib/components/FoldersView.svelte";
  import Equalizer from "../lib/components/Equalizer.svelte";
  import LyricsView from "../lib/components/LyricsView.svelte";
  import { themeStore } from "../lib/stores/theme.svelte";
  import { collectionStore } from "../lib/stores/collection.svelte";

  let isInitialized = $state(false);

  onMount(async () => {
    // Initialize theme store first to prevent flash of default theme
    await themeStore.init();

    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings) {
        if (settings.active_tab) {
          collectionStore.activeTab = settings.active_tab as any;
        }
        if (settings.active_sub_tab) {
          collectionStore.activeSubTab = settings.active_sub_tab as any;
        }
      }
    } catch (e) {
      console.error("Failed to restore app settings:", e);
    } finally {
      isInitialized = true;
    }
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
</script>

<div class="flex flex-col h-screen overflow-hidden bg-brand-main font-sans">
  <div class="flex flex-1 overflow-hidden">
    <!-- Sidebar navigation -->
    <Sidebar />

    <!-- Main View Content Area -->
    <main class="flex-1 flex flex-col min-w-0">
      {#if collectionStore.activeTab === "collection"}
        <CollectionView />
      {:else if collectionStore.activeTab === "playlists"}
        <PlaylistView />
      {:else if collectionStore.activeTab === "settings"}
        <FoldersView />
      {:else if collectionStore.activeTab === "equalizer"}
        <Equalizer />
      {:else if collectionStore.activeTab === "lyrics"}
        <LyricsView />
      {/if}
    </main>
  </div>

  <!-- Player controls panel -->
  <PlayerBar />
</div>
