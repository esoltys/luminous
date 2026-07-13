<script lang="ts">
  import '../app.css';
  import TopNavigation from '../lib/components/TopNavigation.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import PlayerBar from '../lib/components/PlayerBar.svelte';
  import { slide } from 'svelte/transition';
  import { collectionStore } from '../lib/stores/collection.svelte';
  import { playerStore } from '../lib/stores/player.svelte';
  import CoverArt from '../lib/components/CoverArt.svelte';
  import { Music } from 'lucide-svelte';
  
  let { children } = $props();

  // Pointer drag resizing for Sidebar (left-to-right increase)
  function startResizeSidebar(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = collectionStore.sidebarWidth;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      let newWidth = startWidth + deltaX;
      if (newWidth < 120) {
        newWidth = 64;
      } else {
        newWidth = Math.max(180, Math.min(400, newWidth));
      }
      collectionStore.setSidebarWidth(newWidth);
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
      const currentWidth = collectionStore.sidebarWidth;
      if (currentWidth === 180) {
        collectionStore.setSidebarWidth(64);
      } else if (currentWidth > 180) {
        collectionStore.setSidebarWidth(Math.max(180, currentWidth - 10));
      }
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      const currentWidth = collectionStore.sidebarWidth;
      if (currentWidth === 64) {
        collectionStore.setSidebarWidth(180);
      } else {
        collectionStore.setSidebarWidth(Math.min(400, currentWidth + 10));
      }
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

<div class="flex flex-col h-screen overflow-hidden bg-brand-main">
  <!-- 3D Flip Container for everything above the PlayerBar -->
  <div class="flex-1 relative overflow-hidden flip-perspective">
    <!-- Inner Card Wrapper -->
    <div class="w-full h-full relative flip-card" class:flipped={collectionStore.immersiveMode}>
      
      <!-- FRONT FACE: Normal App Layout -->
      <div class="flip-face flex flex-col {collectionStore.immersiveMode ? 'pointer-events-none' : 'pointer-events-auto'}">
        <!-- Top Navigation Ribbon -->
        <div class="flex-shrink-0 z-40 overflow-hidden">
          <TopNavigation />
        </div>

        <!-- Main Grid Layout -->
        <div class="flex flex-1 overflow-hidden">
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
                aria-valuemin={64}
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
          <main class="flex-1 bg-brand-main overflow-hidden flex flex-col">
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
      </div>

      <!-- BACK FACE: Immersive Dedicated Album Artwork Screen -->
      <div class="flip-face flip-back overflow-hidden bg-brand-main flex flex-col items-center justify-center p-8 select-none {!collectionStore.immersiveMode ? 'pointer-events-none' : 'pointer-events-auto'}">
        <!-- Immersive Ambient Blurred Background -->
        {#if playerStore.currentSong}
          <div class="absolute inset-0 z-0 opacity-20 blur-3xl pointer-events-none scale-110">
            <CoverArt
              songId={playerStore.currentSong?.id}
              artEmbedded={playerStore.currentSong?.art_embedded}
              artAutomatic={playerStore.currentSong?.art_automatic}
              artManual={playerStore.currentSong?.art_manual}
              sizeClass="w-full h-full object-cover"
            />
          </div>
        {/if}

        <!-- Center Container: Card and Details -->
        <div class="relative z-10 flex flex-col md:flex-row items-center gap-12 max-w-4xl w-full justify-center">
          <!-- Floating Cover Art Frame -->
          <div class="w-72 h-72 md:w-[380px] md:h-[380px] rounded-2xl overflow-hidden shadow-[0_25px_50px_-12px_rgba(0,0,0,0.7)] border border-brand-border/40 hover:scale-[1.02] transition-transform duration-500 bg-brand-sidebar flex items-center justify-center relative select-none">
            <CoverArt
              songId={playerStore.currentSong?.id}
              artEmbedded={playerStore.currentSong?.art_embedded}
              artAutomatic={playerStore.currentSong?.art_automatic}
              artManual={playerStore.currentSong?.art_manual}
              sizeClass="w-full h-full object-cover"
            />
          </div>

          <!-- Song Details Info -->
          <div class="flex flex-col text-center md:text-left space-y-4 max-w-md">
            {#if playerStore.currentSong}
              <div>
                <span class="px-3 py-1 text-xs font-semibold uppercase tracking-wider bg-brand-accent/15 text-brand-accent border border-brand-accent/25 rounded-full select-none">
                  Now Playing
                </span>
              </div>
              <h1 class="text-3xl md:text-5xl font-black text-brand-text-primary leading-tight tracking-tight select-text">
                {playerStore.currentSong.title || "Unknown Title"}
              </h1>
              <p class="text-lg md:text-xl text-brand-text-secondary select-text font-medium">
                {playerStore.currentSong.artist || "Unknown Artist"}
              </p>
              <p class="text-sm text-brand-text-secondary/60 italic truncate select-text">
                {playerStore.currentSong.album || "Unknown Album"}
              </p>
            {:else}
              <div class="flex flex-col items-center justify-center text-center">
                <Music class="w-16 h-16 text-brand-text-secondary/20 mb-4 animate-pulse" />
                <h2 class="text-2xl font-bold text-brand-text-primary">No track playing</h2>
                <p class="text-sm text-brand-text-secondary/60 mt-1">Select a song from your collection to start playing.</p>
              </div>
            {/if}
          </div>
        </div>
      </div>

    </div>
  </div>

  <!-- Full-Width Player Bar at Bottom (Always Visible, mt-auto keeps bottom-aligned) -->
  <div class="flex-shrink-0 z-40 overflow-hidden mt-auto">
    <PlayerBar />
  </div>
</div>

<style>
  .flip-perspective {
    perspective: 1500px;
  }

  .flip-card {
    transform-style: preserve-3d;
    transition: transform 0.8s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .flip-card.flipped {
    transform: rotateY(180deg);
  }

  .flip-face {
    backface-visibility: hidden;
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
  }

  .flip-back {
    transform: rotateY(180deg);
  }

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
