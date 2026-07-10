<script lang="ts">
  import '../app.css';
  import TopNavigation from '../lib/components/TopNavigation.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import PlayerBar from '../lib/components/PlayerBar.svelte';
  
  let { children } = $props();
  let rightPanelOpen = $state(true);
</script>

<div class="flex flex-col h-screen overflow-hidden">
  <!-- Top Navigation Ribbon (fixed positioning handled in TopNavigation) -->
  <TopNavigation />

  <!-- Main Grid Layout (fills space between top nav and player bar) -->
  <div class="flex flex-1 overflow-hidden mt-20">
    <!-- Left Sidebar -->
    <Sidebar />

    <!-- Central Content Area -->
    <main class="flex-1 bg-brand-main overflow-y-auto">
      {@render children()}
    </main>

    <!-- Right Contextual Panel -->
    {#if rightPanelOpen}
      <RightPanel isOpen={rightPanelOpen} />
    {/if}
  </div>

  <!-- Full-Width Player Bar at Bottom -->
  <PlayerBar />
</div>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
    background-color: var(--bg-main);
  }

  :global(html) {
    height: 100%;
  }
</style>
