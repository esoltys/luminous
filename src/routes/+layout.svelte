<script lang="ts">
  import '../app.css';
  import TopNavigation from '../lib/components/TopNavigation.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  
  let { children } = $props();
  let rightPanelOpen = $state(true);
</script>

<!-- Top Navigation Ribbon -->
<TopNavigation />

<!-- Main Grid Layout -->
<div class="flex w-full h-screen pt-20 pb-24">
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
