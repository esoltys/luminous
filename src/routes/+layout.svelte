<script lang="ts">
  import '../app.css';
  import TopNavigation from '../lib/components/TopNavigation.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import PlayerBar from '../lib/components/PlayerBar.svelte';
  
  let { children } = $props();

  // Dynamic layout states
  let sidebarWidth = $state(256);
  let rightPanelWidth = $state(288);
  let rightPanelOpen = $state(true);

  // Restore states from localStorage on mount
  $effect(() => {
    const savedSidebar = localStorage.getItem("sidebarWidth");
    if (savedSidebar) sidebarWidth = parseInt(savedSidebar, 10);

    const savedRight = localStorage.getItem("rightPanelWidth");
    if (savedRight) rightPanelWidth = parseInt(savedRight, 10);

    const savedOpen = localStorage.getItem("rightPanelOpen");
    if (savedOpen !== null) rightPanelOpen = savedOpen === "true";
  });

  // Save states to localStorage when changed
  $effect(() => {
    localStorage.setItem("sidebarWidth", sidebarWidth.toString());
  });

  $effect(() => {
    localStorage.setItem("rightPanelWidth", rightPanelWidth.toString());
  });

  $effect(() => {
    localStorage.setItem("rightPanelOpen", rightPanelOpen.toString());
  });

  // Pointer drag resizing for Sidebar (left-to-right increase)
  function startResizeSidebar(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = sidebarWidth;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      sidebarWidth = Math.max(180, Math.min(400, startWidth + deltaX));
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
    const startWidth = rightPanelWidth;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      rightPanelWidth = Math.max(220, Math.min(480, startWidth - deltaX));
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
      sidebarWidth = Math.max(180, sidebarWidth - 10);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      sidebarWidth = Math.min(400, sidebarWidth + 10);
    }
  }

  // Accessible keyboard resizing for RightPanel
  function handleRightPanelKeyDown(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      rightPanelWidth = Math.min(480, rightPanelWidth + 10);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      rightPanelWidth = Math.max(220, rightPanelWidth - 10);
    }
  }
</script>

<div class="flex flex-col h-screen overflow-hidden">
  <!-- Top Navigation Ribbon (fixed positioning handled in TopNavigation) -->
  <TopNavigation />

  <!-- Main Grid Layout (fills space between top nav and player bar) -->
  <div class="flex flex-1 overflow-hidden mt-20">
    <!-- Left Sidebar -->
    <Sidebar width={sidebarWidth} />

    <!-- Left Resize Handle -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div 
      role="separator"
      aria-valuenow={sidebarWidth}
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

    <!-- Central Content Area -->
    <main class="flex-1 bg-brand-main overflow-y-auto">
      {@render children()}
    </main>

    <!-- Right Contextual Panel -->
    {#if rightPanelOpen}
      <!-- Right Resize Handle -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <div 
        role="separator"
        aria-valuenow={rightPanelWidth}
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

      <RightPanel isOpen={rightPanelOpen} width={rightPanelWidth} onClose={() => rightPanelOpen = false} />
    {/if}
  </div>

  <!-- Full-Width Player Bar at Bottom -->
  <PlayerBar rightPanelOpen={rightPanelOpen} onToggleRightPanel={() => rightPanelOpen = !rightPanelOpen} />
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
