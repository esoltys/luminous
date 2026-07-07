<script lang="ts">
  import Sidebar from "../lib/components/Sidebar.svelte";
  import PlayerBar from "../lib/components/PlayerBar.svelte";
  import CollectionView from "../lib/components/CollectionView.svelte";
  import PlaylistView from "../lib/components/PlaylistView.svelte";
  import FoldersView from "../lib/components/FoldersView.svelte";

  let activeTab = $state<"collection" | "playlists" | "settings">("collection");
  let activeSubTab = $state<"songs" | "albums" | "artists">("songs");
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
      {/if}
    </main>
  </div>

  <!-- Player controls panel -->
  <PlayerBar />
</div>
