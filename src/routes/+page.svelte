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

  let activeTab = $state<"collection" | "playlists" | "settings" | "equalizer" | "lyrics">("collection");
  let activeSubTab = $state<"songs" | "albums" | "artists">("songs");

  let isInitialized = $state(false);

  onMount(async () => {
    try {
      const settings = await invoke<Record<string, string>>("get_all_app_settings");
      if (settings) {
        if (settings.active_tab) {
          activeTab = settings.active_tab as any;
        }
        if (settings.active_sub_tab) {
          activeSubTab = settings.active_sub_tab as any;
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
      invoke("set_app_setting", { key: "active_tab", value: activeTab }).catch((err) => {
        console.error("Failed to save active_tab:", err);
      });
    }
  });

  $effect(() => {
    if (isInitialized) {
      invoke("set_app_setting", { key: "active_sub_tab", value: activeSubTab }).catch((err) => {
        console.error("Failed to save active_sub_tab:", err);
      });
    }
  });
</script>

<div class="flex flex-col h-screen overflow-hidden bg-gray-950 font-sans">
  <div class="flex flex-1 overflow-hidden">
    <!-- Sidebar navigation -->
    <Sidebar bind:activeTab bind:activeSubTab />

    <!-- Main View Content Area -->
    <main class="flex-1 flex flex-col min-w-0">
      {#if activeTab === "collection"}
        <CollectionView {activeSubTab} />
      {:else if activeTab === "playlists"}
        <PlaylistView />
      {:else if activeTab === "settings"}
        <FoldersView />
      {:else if activeTab === "equalizer"}
        <Equalizer />
      {:else if activeTab === "lyrics"}
        <LyricsView />
      {/if}
    </main>
  </div>

  <!-- Player controls panel -->
  <PlayerBar />
</div>
