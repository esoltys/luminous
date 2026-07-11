<script lang="ts">
  import '../app.css';
  import TopNavigation from '../lib/components/TopNavigation.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import PlayerBar from '../lib/components/PlayerBar.svelte';
  import { slide } from 'svelte/transition';
  import { collectionStore } from '../lib/stores/collection.svelte';
  
  let { children } = $props();

  // Pointer drag resizing for Sidebar (left-to-right increase)
  function startResizeSidebar(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = collectionStore.sidebarWidth;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      collectionStore.setSidebarWidth(Math.max(180, Math.min(400, startWidth + deltaX)));
    }

    function onPointerUp() {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    }

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // Pointer drag resizing for RightPanel (right-to-left increase)
  function startResizeRightPanel(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = collectionStore.rightPanelWidth;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      collectionStore.setRightPanelWidth(Math.max(220, Math.min(480, startWidth - deltaX)));
    }

    function onPointerUp() {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    }

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  // Accessible keyboard resizing for Sidebar
  function handleSidebarKeyDown(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      collectionStore.setSidebarWidth(Math.max(180, collectionStore.sidebarWidth - 10));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      collectionStore.setSidebarWidth(Math.min(400, collectionStore.sidebarWidth + 10));
    }
  }

  // Accessible keyboard resizing for RightPanel
  function handleRightPanelKeyDown(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      collectionStore.setRightPanelWidth(Math.min(480, collectionStore.rightPanelWidth + 10));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      collectionStore.setRightPanelWidth(Math.max(220, collectionStore.rightPanelWidth - 10));
    }
  }
</script>

<div class="flex flex-col h-screen overflow-hidden">
  {#if collectionStore.playerBarOpen}
    <!-- Top Navigation Ribbon -->
    <div transition:slide={{ axis: 'y', duration: 250 }} class="flex-shrink-0 z-40 overflow-hidden">
      <TopNavigation />
    </div>

    <!-- Main Grid Layout (fills space between top nav and player bar) -->
    <div transition:slide={{ axis: 'y', duration: 350 }} class="flex flex-1 overflow-hidden">
      <!-- Left Sidebar -->
      {#if collectionStore.sidebarOpen}
        <div transition:slide={{ axis: 'x', duration: 350 }} class="h-full flex-shrink-0 flex overflow-hidden">
          <Sidebar width={collectionStore.sidebarWidth} />

          <!-- Left Resize Handle -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div 
            role="separator"
            aria-valuenow={collectionStore.sidebarWidth}
            aria-valuemin={180}
            aria-valuemax={400}
            aria-label="Resize Left Sidebar"
            tabindex="0"
            class="relative w-1 hover:w-1.5 active:w-1.5 bg-brand-border hover:bg-brand-accent/50 active:bg-brand-accent cursor-col-resize transition-all self-stretch flex-shrink-0 z-30 touch-none focus:outline-none focus:bg-brand-accent focus:w-1.5"
            onpointerdown={startResizeSidebar}
            onkeydown={handleSidebarKeyDown}
          >
            <!-- Expanded hover/touch area wrapper -->
            <div class="absolute -inset-x-2 top-0 bottom-0 cursor-col-resize"></div>
          </div>
        </div>
      {/if}

      <!-- Central Content Area -->
      <main class="flex-1 bg-brand-main overflow-y-auto">
        {@render children()}
      </main>

      <!-- Right Contextual Panel -->
      {#if collectionStore.rightPanelOpen}
        <div transition:slide={{ axis: 'x', duration: 350 }} class="h-full flex-shrink-0 flex overflow-hidden">
          <!-- Right Resize Handle -->
          <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
          <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
          <div 
            role="separator"
            aria-valuenow={collectionStore.rightPanelWidth}
            aria-valuemin={220}
            aria-valuemax={480}
            aria-label="Resize Right Panel"
            tabindex="0"
            class="relative w-1 hover:w-1.5 active:w-1.5 bg-brand-border hover:bg-brand-accent/50 active:bg-brand-accent cursor-col-resize transition-all self-stretch flex-shrink-0 z-30 touch-none focus:outline-none focus:bg-brand-accent focus:w-1.5"
            onpointerdown={startResizeRightPanel}
            onkeydown={handleRightPanelKeyDown}
          >
            <!-- Expanded hover/touch area wrapper -->
            <div class="absolute -inset-x-2 top-0 bottom-0 cursor-col-resize"></div>
          </div>

          <RightPanel isOpen={collectionStore.rightPanelOpen} width={collectionStore.rightPanelWidth} onClose={() => collectionStore.toggleRightPanel()} />
        </div>
      {/if}
    </div>
  {/if}

  <!-- Full-Width Player Bar at Bottom (Always Visible, mt-auto keeps bottom-aligned) -->
  <div class="flex-shrink-0 z-40 overflow-hidden mt-auto">
    <PlayerBar />
  </div>
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

  :global(.animate-spin-slow) {
    animation: spin-slow 12s linear infinite;
  }

  @keyframes spin-slow {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
</style>
