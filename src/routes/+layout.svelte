<script lang="ts">
  import '../app.css';
  import TopNavigation from '../lib/components/TopNavigation.svelte';
  import Sidebar from '../lib/components/Sidebar.svelte';
  import RightPanel from '../lib/components/RightPanel.svelte';
  import PlayerBar from '../lib/components/PlayerBar.svelte';
  import { slide, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { collectionStore } from '../lib/stores/collection.svelte';
  import { playerStore } from '../lib/stores/player.svelte';
  import CoverArt from '../lib/components/CoverArt.svelte';
  import Miniplayer from '../lib/components/Miniplayer.svelte';
  import KeyboardShortcutsModal from '../lib/components/KeyboardShortcutsModal.svelte';
  import Toast from '../lib/components/Toast.svelte';
  import { Music, UploadCloud } from 'lucide-svelte';

  import { i18n } from '../lib/stores/i18n.svelte';
  import { prefs } from '../lib/stores/prefs.svelte';
  import { updaterStore } from '../lib/stores/updater.svelte';
  import { toastStore } from '../lib/stores/toast.svelte';
  import { onMount } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { invoke } from '@tauri-apps/api/core';
  import {
    SIDEBAR_MIN_WIDTH_PX,
    SIDEBAR_MAX_WIDTH_PX,
    SIDEBAR_COLLAPSED_WIDTH_PX,
    RIGHT_PANEL_MIN_WIDTH_PX,
    RIGHT_PANEL_MAX_WIDTH_PX,
    PANEL_RESIZE_STEP_PX,
  } from '../lib/constants';

  let { children } = $props();
  let isLinux = $state(false);
  let isShortcutsModalOpen = $state(false);
  let isDragActive = $state(false);
  let isShiftHeld = $state(false);
  let isResizingSidebar = $state(false);

  // Visual-only override: never touches the stored sidebarWidth preference,
  // so widening the window back out restores exactly the width the user had.
  let effectiveSidebarWidth = $derived(
    collectionStore.isSidebarAutoCollapsed ? SIDEBAR_COLLAPSED_WIDTH_PX : collectionStore.sidebarWidth
  );

  onMount(() => {
    isLinux = typeof navigator !== 'undefined' && navigator.userAgent.includes('Linux');
    i18n.init();
    prefs.init();
    updaterStore.init();
    void getCurrentWindow().show().catch(() => {});

    function handleGlobalHotkeys(e: KeyboardEvent) {
      if (!(e.ctrlKey || e.metaKey)) return;

      switch (e.key.toLowerCase()) {
        case 'm':
          e.preventDefault();
          collectionStore.toggleMiniplayerMode();
          break;
        case ',':
          e.preventDefault();
          collectionStore.activeTab = 'settings';
          break;
        case '[':
          e.preventDefault();
          collectionStore.goBack();
          break;
        case ']':
          e.preventDefault();
          collectionStore.goForward();
          break;
        case '1':
          e.preventDefault();
          collectionStore.toggleSidebar();
          break;
        case '2':
          e.preventDefault();
          collectionStore.toggleImmersiveMode();
          break;
        case '3':
          e.preventDefault();
          collectionStore.toggleRightPanel();
          break;
        case '/':
          e.preventDefault();
          isShortcutsModalOpen = !isShortcutsModalOpen;
          break;
        case 'o':
          e.preventDefault();
          playerStore.openFileDialog();
          break;
      }
    }
    // Tracks whether Shift is currently held so a drop can be told apart from
    // a plain drop (replace Queue & play) vs. a Shift-drop (append to Queue).
    // The native DragDropEvent payload carries no modifier state, and an OS
    // file drag never gives the target window keyboard focus while hovering
    // — so keydown/keyup here would never fire. Instead this polls the
    // physical key state via a backend command (is_shift_key_held, queried
    // from the OS rather than the DOM) while a drag is active, and re-checks
    // it definitively at drop time.
    let shiftPollInterval: ReturnType<typeof setInterval> | undefined;
    async function refreshShiftHeld() {
      try {
        isShiftHeld = await invoke<boolean>('is_shift_key_held');
      } catch (e) {
        console.warn('Failed to query Shift key state:', e);
      }
    }
    function stopShiftPolling() {
      if (shiftPollInterval) {
        clearInterval(shiftPollInterval);
        shiftPollInterval = undefined;
      }
    }

    async function handleFilesDropped(paths: string[]) {
      const append = await invoke<boolean>('is_shift_key_held').catch(() => false);
      if (append) {
        const outcome = await playerStore.addPathsToQueue(paths);
        if (outcome.added > 0) {
          const text = outcome.added === 1
            ? i18n.t('dragDrop.addedSong', {}, 'Added 1 song to queue')
            : i18n.t('dragDrop.addedSongs', { count: outcome.added }, `Added ${outcome.added} songs to queue`);
          toastStore.show(text, 'success');
        }
      } else {
        const outcome = await playerStore.openAndPlay(paths);
        if (outcome.played > 0) {
          const text = outcome.played === 1
            ? i18n.t('dragDrop.playingSong', {}, 'Playing 1 song')
            : i18n.t('dragDrop.playingSongs', { count: outcome.played }, `Playing ${outcome.played} songs`);
          toastStore.show(text, 'success');
        }
      }
    }

    // Must be getCurrentWebview(), not getCurrentWindow(): the Rust side
    // (manager/webview.rs's emit_to_webview) emits drag-drop events scoped to
    // an EventTarget::Webview, but Window.onDragDropEvent() subscribes with
    // an EventTarget::Window target — a kind mismatch the backend's target
    // filter never matches, so a Window-level listener silently never fires.
    // Webview.onDragDropEvent() (and only that one) targets EventTarget::Webview
    // and actually receives the events.
    let dragDropUnlisten: (() => void) | undefined;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === 'drop') {
          isDragActive = false;
          stopShiftPolling();
          void handleFilesDropped(event.payload.paths);
        } else if (event.payload.type === 'leave') {
          isDragActive = false;
          stopShiftPolling();
        } else if (!isDragActive) {
          // 'enter' or 'over', first time this hover — start live-polling the
          // Shift state so the overlay hint tracks it while dragging.
          isDragActive = true;
          void refreshShiftHeld();
          shiftPollInterval = setInterval(refreshShiftHeld, 150);
        }
      })
      .then((unlisten) => {
        dragDropUnlisten = unlisten;
      })
      .catch((e) => {
        console.warn('Failed to attach drag-and-drop listener:', e);
      });

    window.addEventListener('keydown', handleGlobalHotkeys);
    return () => {
      window.removeEventListener('keydown', handleGlobalHotkeys);
      stopShiftPolling();
      dragDropUnlisten?.();
    };
  });



  // There's no way to exit immersive mode when nothing is playing — the only
  // toggle lives on the PlayerBar, which is itself hidden whenever there's no
  // current song. Force immersive mode off whenever there's nothing to show,
  // so a stale "immersive" flag from a previous session (or playback
  // stopping while immersive) never leaves the user stranded.
  $effect(() => {
    if (!playerStore.currentSong) {
      collectionStore.exitImmersiveMode();
    }
  });

  // Collection/Playlists/Lyrics are hidden from the sidebar until the library
  // has songs (see Sidebar.svelte). If the last watched folder is removed
  // while one of those tabs is active, its nav button vanishes — bounce back
  // to Home rather than stranding the user on a tab they can no longer reach.
  $effect(() => {
    if (
      collectionStore.statsLoaded &&
      collectionStore.stats.total_songs === 0 &&
      (collectionStore.activeTab === "collection" ||
        collectionStore.activeTab === "playlists" ||
        collectionStore.activeTab === "lyrics")
    ) {
      collectionStore.activeTab = "home";
    }
  });

  // Pointer drag resizing for Sidebar (left-to-right increase)
  function startResizeSidebar(e: PointerEvent) {
    e.preventDefault();
    isResizingSidebar = true;
    const startX = e.clientX;
    const startWidth = collectionStore.sidebarWidth;

    function onPointerMove(moveEvent: PointerEvent) {
      const deltaX = moveEvent.clientX - startX;
      let newWidth = startWidth + deltaX;
      if (newWidth < 120) {
        newWidth = SIDEBAR_COLLAPSED_WIDTH_PX;
      } else {
        newWidth = Math.max(SIDEBAR_MIN_WIDTH_PX, Math.min(SIDEBAR_MAX_WIDTH_PX, newWidth));
      }
      collectionStore.setSidebarWidth(newWidth);
    }

    function onPointerUp() {
      isResizingSidebar = false;
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
      collectionStore.setRightPanelWidth(Math.max(RIGHT_PANEL_MIN_WIDTH_PX, Math.min(RIGHT_PANEL_MAX_WIDTH_PX, startWidth - deltaX)));
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
      if (currentWidth === SIDEBAR_MIN_WIDTH_PX) {
        collectionStore.setSidebarWidth(SIDEBAR_COLLAPSED_WIDTH_PX);
      } else if (currentWidth > SIDEBAR_MIN_WIDTH_PX) {
        collectionStore.setSidebarWidth(Math.max(SIDEBAR_MIN_WIDTH_PX, currentWidth - PANEL_RESIZE_STEP_PX));
      }
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      const currentWidth = collectionStore.sidebarWidth;
      if (currentWidth === SIDEBAR_COLLAPSED_WIDTH_PX) {
        collectionStore.setSidebarWidth(SIDEBAR_MIN_WIDTH_PX);
      } else {
        collectionStore.setSidebarWidth(Math.min(SIDEBAR_MAX_WIDTH_PX, currentWidth + PANEL_RESIZE_STEP_PX));
      }
    }
  }

  // Accessible keyboard resizing for RightPanel
  function handleRightPanelKeyDown(e: KeyboardEvent) {
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      collectionStore.setRightPanelWidth(Math.min(RIGHT_PANEL_MAX_WIDTH_PX, collectionStore.rightPanelWidth + PANEL_RESIZE_STEP_PX));
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      collectionStore.setRightPanelWidth(Math.max(RIGHT_PANEL_MIN_WIDTH_PX, collectionStore.rightPanelWidth - PANEL_RESIZE_STEP_PX));
    }
  }
</script>

<div class="relative flex flex-col h-screen overflow-hidden bg-brand-main">
  {#if collectionStore.isMiniplayer}
    <div class="w-full h-full">
      <Miniplayer />
    </div>
  {:else if collectionStore.isPlaybarOnlyMode}
    <!-- Playbar-only mode: the window is too short to show anything else,
         including the immersive cover view. Renders PlayerBar unconditionally
         (it already has a graceful "not playing" state) so the user never
         sees a blank window when it's squashed short. Reuses the same
         inset-x-4 horizontal insets as the normal floating dock below so
         PlayerBar's rendered width is identical between the two render sites. -->
    <div class="relative w-full h-full">
      <div class="absolute inset-x-4 top-1/2 -translate-y-1/2 z-40">
        <PlayerBar />
      </div>
    </div>
  {:else}
    <!-- 3D Flip Container fills the full window height; the PlayerBar floats
         on top of it (absolute, below) so scrolled content passes underneath
         the glass footer instead of stopping above it. -->
    <div class="flex-1 relative overflow-hidden flip-perspective" class:no-3d={isLinux}>
      <!-- Inner Card Wrapper -->
      <div class="w-full h-full relative flip-card" class:flipped={collectionStore.effectiveImmersiveMode}>

        <!-- FRONT FACE: Normal App Layout -->
        <div class="flip-face flip-front flex flex-col {collectionStore.effectiveImmersiveMode ? 'pointer-events-none' : 'pointer-events-auto'}">
          <!-- Top Navigation Ribbon -->
          <div class="flex-shrink-0 z-50 overflow-visible">
            <TopNavigation />
          </div>

          <!-- Main Grid Layout -->
          <div class="flex flex-1 overflow-hidden">
            <!-- Left Sidebar -->
            {#if collectionStore.sidebarOpen}
              <div transition:slide={{ axis: 'x', duration: 350 }} class="h-full flex-shrink-0 flex overflow-hidden">
                <Sidebar width={effectiveSidebarWidth} resizing={isResizingSidebar} />

                <!-- Left Resize Handle: hidden while auto-collapsed — dragging
                     from a visually-64px rail would otherwise jump using the
                     real (larger) stored sidebarWidth as the drag's starting
                     point. Widening the window is the way out. -->
                {#if !collectionStore.isSidebarAutoCollapsed}
                  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                  <div
                    role="separator"
                    aria-valuenow={collectionStore.sidebarWidth}
                    aria-valuemin={SIDEBAR_COLLAPSED_WIDTH_PX}
                    aria-valuemax={SIDEBAR_MAX_WIDTH_PX}
                    aria-label={i18n.t('topNav.resizeSidebar')}
                    tabindex="0"
                    class="relative w-1 bg-brand-border hover:bg-brand-accent/50 active:bg-brand-accent cursor-col-resize transition-colors self-stretch flex-shrink-0 z-30 touch-none focus:outline-none focus:bg-brand-accent"
                    onpointerdown={startResizeSidebar}
                    onkeydown={handleSidebarKeyDown}
                  >
                    <!-- Expanded hover/touch area wrapper -->
                    <div class="absolute -inset-x-2 top-0 bottom-0 cursor-col-resize"></div>
                  </div>
                {/if}
              </div>
            {/if}

            <!-- Central Content Area -->
            <main class="flex-1 bg-brand-main overflow-hidden flex flex-col">
              {@render children()}
            </main>

            <!-- Right Contextual Panel -->
            {#if collectionStore.rightPanelOpen && !collectionStore.isRightPanelAutoHidden}
              <div transition:slide={{ axis: 'x', duration: 350 }} class="h-full flex-shrink-0 flex overflow-hidden">
                <!-- Right Resize Handle -->
                <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
                <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
                <div 
                  role="separator"
                  aria-valuenow={collectionStore.rightPanelWidth}
                  aria-valuemin={RIGHT_PANEL_MIN_WIDTH_PX}
                  aria-valuemax={RIGHT_PANEL_MAX_WIDTH_PX}
                  aria-label={i18n.t('topNav.resizeRightPanel')}
                  tabindex="0"
                  class="relative w-1 bg-brand-border hover:bg-brand-accent/50 active:bg-brand-accent cursor-col-resize transition-colors self-stretch flex-shrink-0 z-30 touch-none focus:outline-none focus:bg-brand-accent"
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
        <!-- pb is larger than pt to offset the floating PlayerBar dock (h-20 + bottom-4 inset
             ≈ 96px) that overlays the bottom of this face, so the content centers within the
             visible area above the dock rather than the full face height. -->
        <div class="flip-face flip-back overflow-hidden bg-brand-main flex flex-col items-center justify-center pt-8 px-8 pb-32 select-none {!collectionStore.effectiveImmersiveMode ? 'pointer-events-none' : 'pointer-events-auto'}">
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
            <div class="w-72 h-72 md:w-[380px] md:h-[380px] overflow-hidden shadow-[0_25px_50px_-12px_rgba(0,0,0,0.7)] border border-brand-border/40 hover:scale-[1.02] transition-transform duration-500 bg-brand-sidebar flex items-center justify-center relative select-none">
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
                  <span class="px-3 py-1 text-xs font-semibold uppercase tracking-wider bg-brand-accent/15 text-brand-accent-text border border-brand-border rounded-full select-none">
                    {i18n.t('playerBar.nowPlaying')}
                  </span>
                </div>
                <h1 class="text-3xl md:text-5xl font-black text-brand-text-primary leading-tight tracking-tight select-text">
                  {playerStore.currentSong.title || i18n.t('collection.unknownSong')}
                </h1>
                <p class="text-lg md:text-xl text-brand-text-secondary select-text font-medium">
                  {playerStore.currentSong.artist || i18n.t('collection.unknownArtist')}
                </p>
                <p class="text-sm text-brand-text-secondary/60 italic truncate select-text">
                  {playerStore.currentSong.album || i18n.t('collection.unknownAlbum')}
                </p>
              {:else}
                <div class="flex flex-col items-center justify-center text-center">
                  <Music class="w-16 h-16 text-brand-text-secondary/20 mb-4 animate-pulse" />
                  <h2 class="text-2xl font-bold text-brand-text-primary">{i18n.t('playerBar.notPlaying')}</h2>
                  <p class="text-sm text-brand-text-secondary/60 mt-1">{i18n.t('immersive.emptyStateText')}</p>
                </div>
              {/if}
            </div>
          </div>
        </div>

      </div>
    </div>

    <!-- Floating PlayDock: inset from all edges (not flush) so it reads as a
         floating glass dock, and the content behind it can still scroll
         underneath the gap for the blur to have something to blur. Hidden
         whenever there's no current song, so it doesn't linger showing
         "Nothing playing" after the queue ends. -->
    {#if playerStore.currentSong && collectionStore.activeTab !== 'help'}
      <div class="absolute inset-x-4 bottom-4 z-40">
        <PlayerBar />
      </div>
    {/if}
  {/if}

  <!-- Drag-and-drop overlay: shown while the OS reports a file/folder drag
       hovering the window (native `dragDropEnabled` events, not HTML5 DnD).
       Purely visual — pointer-events-none so it never intercepts the drop
       itself, which the window-level listener in onMount handles. -->
  {#if isDragActive}
    <div
      class="absolute inset-0 z-[100] flex items-center justify-center bg-brand-main/85 backdrop-blur-sm border-4 border-dashed border-brand-accent pointer-events-none select-none"
      transition:fly={{ duration: 150 }}
    >
      <div class="flex flex-col items-center gap-3 text-center px-8">
        <UploadCloud class="w-14 h-14 text-brand-accent" />
        <p class="text-2xl font-bold text-brand-text-primary">
          {isShiftHeld
            ? i18n.t('dragDrop.overlayAppendTitle', {}, 'Drop to append to queue')
            : i18n.t('dragDrop.overlayReplaceTitle', {}, 'Drop to replace queue & play')}
        </p>
        <p class="text-sm text-brand-text-secondary">
          {isShiftHeld
            ? i18n.t('dragDrop.overlayAppendHint', {}, "Playback won't be interrupted")
            : i18n.t('dragDrop.overlayReplaceHint', {}, 'Hold Shift to append instead')}
        </p>
      </div>
    </div>
  {/if}
</div>

{#if isShortcutsModalOpen}
  <KeyboardShortcutsModal onClose={() => (isShortcutsModalOpen = false)} />
{/if}

<Toast />

<style>
  .flip-perspective {
    perspective: none;
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
    -webkit-backface-visibility: hidden;
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    transform: rotateY(0deg);
    transform-style: preserve-3d;
  }

  .flip-back {
    transform: rotateY(180deg);
  }

  /* Disable 3D flip on Linux/WebKit and use simple, clean opacity cross-fade.
     `perspective` alone (with no rotation left to apply) still forces its
     subtree into a separate 3D compositing layer, which is enough to break
     WebKitGTK's backdrop-filter sampling for elements outside that subtree
     (e.g. the glass PlayerBar) — so drop it here too, not just the transform. */
  .flip-perspective.no-3d {
    perspective: none;
  }

  .no-3d .flip-card {
    transform-style: flat;
    transform: none !important;
    transition: none;
  }

  .no-3d .flip-face {
    backface-visibility: visible;
    -webkit-backface-visibility: visible;
    transform: none !important;
    transition: opacity 0.4s ease-in-out;
  }

  .no-3d .flip-front {
    opacity: 1;
  }

  .no-3d .flip-card.flipped .flip-front {
    opacity: 0;
  }

  .no-3d .flip-back {
    opacity: 0;
  }

  .no-3d .flip-card.flipped .flip-back {
    opacity: 1;
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
