import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  SIDEBAR_MIN_WIDTH_PX,
  SIDEBAR_COLLAPSED_WIDTH_PX,
  MEDIUM_BREAKPOINT_WIDTH_PX,
  SMALL_BREAKPOINT_WIDTH_PX,
  PLAYBAR_ONLY_HEIGHT_BREAKPOINT_PX,
  DETAIL_HEADER_COLLAPSE_HEIGHT_PX,
} from "../constants";

class WindowLayoutStore {
  // Layout panel states
  sidebarOpen = $state<boolean>(true);
  rightPanelOpen = $state<boolean>(true);
  sidebarWidth = $state<number>(256);
  lastExpandedSidebarWidth = $state<number>(256);
  rightPanelWidth = $state<number>(288);
  immersiveMode = $state<boolean>(false);
  isMiniplayer = $state<boolean>(
    typeof window !== "undefined" && localStorage.getItem("layout_isMiniplayer") === "true"
  );
  // Guards enter/exitMiniplayerMode against overlapping calls: isMiniplayer
  // flips synchronously, but the window resize/decoration IPC call it
  // guards is async, so a second toggle fired before that call resolves
  // could apply a stale target size on top of an in-progress one.
  private miniplayerTransitionInFlight = false;
  // Debounce for the settled-geometry capture below: only commit a size/
  // position once resize/move events stop firing for this long, so a mode
  // toggle's own programmatic resize/reposition never gets captured as "the
  // user changed it" for the wrong mode.
  private static readonly GEOMETRY_CAPTURE_DEBOUNCE_MS = 400;
  private geometryCaptureTimer: ReturnType<typeof setTimeout> | null = null;
  savedWindowWidth = $state<number>(1280);
  savedWindowHeight = $state<number>(800);
  savedWindowX = $state<number | null>(null);
  savedWindowY = $state<number | null>(null);
  miniplayerWidth = $state<number>(300);
  miniplayerHeight = $state<number>(360);
  miniplayerX = $state<number | null>(null);
  miniplayerY = $state<number | null>(null);

  // Live CSS viewport size, used only to derive responsive breakpoint flags
  // below — a plain in-memory mirror of window.innerWidth/innerHeight, kept
  // deliberately separate from the geometry-capture fields above (those
  // persist native OS window geometry via debounced Tauri onResized/onMoved
  // for cross-restart restore; this one drives synchronous layout decisions
  // and touches no IPC or localStorage).
  viewportWidth = $state<number>(typeof window !== "undefined" ? window.innerWidth : 1280);
  viewportHeight = $state<number>(typeof window !== "undefined" ? window.innerHeight : 800);

  constructor() {
    if (typeof window !== "undefined") {
      const savedSidebar = localStorage.getItem("layout_sidebarOpen");
      if (savedSidebar !== null) this.sidebarOpen = savedSidebar === "true";

      const savedRight = localStorage.getItem("layout_rightPanelOpen");
      if (savedRight !== null) this.rightPanelOpen = savedRight === "true";

      const savedSidebarWidth = localStorage.getItem("layout_sidebarWidth");
      if (savedSidebarWidth) {
        const w = parseInt(savedSidebarWidth, 10);
        this.sidebarWidth = w;
        if (w >= SIDEBAR_MIN_WIDTH_PX) {
          this.lastExpandedSidebarWidth = w;
        }
      }
      const savedLastExpanded = localStorage.getItem("layout_lastExpandedSidebarWidth");
      if (savedLastExpanded) {
        this.lastExpandedSidebarWidth = parseInt(savedLastExpanded, 10);
      }

      const savedRightWidth = localStorage.getItem("layout_rightPanelWidth");
      if (savedRightWidth) this.rightPanelWidth = parseInt(savedRightWidth, 10);

      const savedImmersive = localStorage.getItem("layout_immersiveMode");
      if (savedImmersive !== null) this.immersiveMode = savedImmersive === "true";

      const savedWindowWidthStr = localStorage.getItem("layout_savedWindowWidth");
      if (savedWindowWidthStr) this.savedWindowWidth = parseInt(savedWindowWidthStr, 10);

      const savedWindowHeightStr = localStorage.getItem("layout_savedWindowHeight");
      if (savedWindowHeightStr) this.savedWindowHeight = parseInt(savedWindowHeightStr, 10);

      const savedWindowXStr = localStorage.getItem("layout_savedWindowX");
      if (savedWindowXStr !== null) this.savedWindowX = parseFloat(savedWindowXStr);

      const savedWindowYStr = localStorage.getItem("layout_savedWindowY");
      if (savedWindowYStr !== null) this.savedWindowY = parseFloat(savedWindowYStr);

      const savedMiniWidthStr = localStorage.getItem("layout_miniplayerWidth");
      if (savedMiniWidthStr) this.miniplayerWidth = parseInt(savedMiniWidthStr, 10);

      const savedMiniHeightStr = localStorage.getItem("layout_miniplayerHeight");
      if (savedMiniHeightStr) this.miniplayerHeight = parseInt(savedMiniHeightStr, 10);

      const savedMiniXStr = localStorage.getItem("layout_miniplayerX");
      if (savedMiniXStr !== null) this.miniplayerX = parseFloat(savedMiniXStr);

      const savedMiniYStr = localStorage.getItem("layout_miniplayerY");
      if (savedMiniYStr !== null) this.miniplayerY = parseFloat(savedMiniYStr);

      // Listeners are attached unconditionally — cheap either way, and the
      // gate that actually matters is whether captured values get sent to
      // the backend at all (see enter/exitMiniplayerMode, which check
      // isGeometryCaptureSupported() fresh on each toggle).
      this.initWindowGeometryTracking();
      this.initViewportTracking();

      const savedIsMiniplayer = localStorage.getItem("layout_isMiniplayer");
      if (savedIsMiniplayer === "true") {
        void this.enterMiniplayerMode(true);
      }
    }
  }

  toggleSidebar() {
    this.sidebarOpen = !this.sidebarOpen;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_sidebarOpen", this.sidebarOpen.toString());
    }
  }

  toggleSidebarCompact() {
    if (!this.sidebarOpen) {
      this.sidebarOpen = true;
      if (typeof window !== "undefined") {
        localStorage.setItem("layout_sidebarOpen", "true");
      }
    }

    if (this.sidebarWidth < SIDEBAR_MIN_WIDTH_PX) {
      const targetWidth = Math.max(SIDEBAR_MIN_WIDTH_PX, this.lastExpandedSidebarWidth || 256);
      this.setSidebarWidth(targetWidth);
    } else {
      this.setSidebarWidth(SIDEBAR_COLLAPSED_WIDTH_PX);
    }
  }

  toggleImmersiveMode() {
    this.immersiveMode = !this.immersiveMode;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_immersiveMode", this.immersiveMode.toString());
    }
  }

  // Force immersive mode off — used when there's nothing playing, since the
  // only way back out is a toggle on the PlayerBar, which is itself hidden
  // until a track has ever played this session (issue #71).
  exitImmersiveMode() {
    if (!this.immersiveMode) return;
    this.immersiveMode = false;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_immersiveMode", "false");
    }
  }

  setSavedWindowGeometry(width: number, height: number, x?: number | null, y?: number | null) {
    this.savedWindowWidth = Math.max(900, Math.round(width));
    this.savedWindowHeight = Math.max(600, Math.round(height));
    if (x !== undefined && x !== null) this.savedWindowX = Math.round(x);
    if (y !== undefined && y !== null) this.savedWindowY = Math.round(y);
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_savedWindowWidth", this.savedWindowWidth.toString());
      localStorage.setItem("layout_savedWindowHeight", this.savedWindowHeight.toString());
      if (this.savedWindowX !== null) localStorage.setItem("layout_savedWindowX", this.savedWindowX.toString());
      if (this.savedWindowY !== null) localStorage.setItem("layout_savedWindowY", this.savedWindowY.toString());
    }
  }

  setMiniplayerGeometry(width: number, height: number, x?: number | null, y?: number | null) {
    this.miniplayerWidth = Math.max(300, Math.round(width));
    this.miniplayerHeight = Math.max(360, Math.round(height));
    if (x !== undefined && x !== null) this.miniplayerX = Math.round(x);
    if (y !== undefined && y !== null) this.miniplayerY = Math.round(y);
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_miniplayerWidth", this.miniplayerWidth.toString());
      localStorage.setItem("layout_miniplayerHeight", this.miniplayerHeight.toString());
      if (this.miniplayerX !== null) localStorage.setItem("layout_miniplayerX", this.miniplayerX.toString());
      if (this.miniplayerY !== null) localStorage.setItem("layout_miniplayerY", this.miniplayerY.toString());
    }
  }

  // Whether reading a window's geometry back is reliable on this platform —
  // false only under Linux/Wayland, where the compositor never reports a
  // window's absolute screen position to the client at all (confirmed via
  // tao's GTK backend; the official tauri-plugin-window-state hits the same
  // wall). Checked fresh via the backend each time rather than cached, since
  // it's a cheap, synchronous, session-constant fact on the Rust side.
  private async isGeometryCaptureSupported(): Promise<boolean> {
    try {
      return await invoke<boolean>("geometry_capture_supported");
    } catch (e) {
      console.warn("Failed to check geometry_capture_supported, assuming unsupported:", e);
      return false;
    }
  }

  async enterMiniplayerMode(force = false) {
    if ((this.isMiniplayer && !force) || this.miniplayerTransitionInFlight) return;
    this.miniplayerTransitionInFlight = true;
    this.isMiniplayer = true;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_isMiniplayer", "true");
    }
    try {
      // Remembered width/height/x/y are only trustworthy where geometry
      // capture is supported — elsewhere (Linux/Wayland) the backend falls
      // back to its fixed default when these are omitted, since a
      // remembered value there could never have been captured correctly in
      // the first place.
      const payload: { width?: number; height?: number; x?: number; y?: number } = {};
      if (await this.isGeometryCaptureSupported()) {
        payload.width = this.miniplayerWidth;
        payload.height = this.miniplayerHeight;
        if (this.miniplayerX !== null) payload.x = this.miniplayerX;
        if (this.miniplayerY !== null) payload.y = this.miniplayerY;
      }
      await invoke("enter_miniplayer_mode", payload);
    } catch (e) {
      console.warn("Failed to enter miniplayer backend window mode:", e);
    } finally {
      this.miniplayerTransitionInFlight = false;
    }
  }

  async exitMiniplayerMode(force = false) {
    if ((!this.isMiniplayer && !force) || this.miniplayerTransitionInFlight) return;
    this.miniplayerTransitionInFlight = true;
    this.isMiniplayer = false;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_isMiniplayer", "false");
    }
    try {
      const payload: { width?: number; height?: number; x?: number; y?: number } = {};
      if (await this.isGeometryCaptureSupported()) {
        payload.width = this.savedWindowWidth;
        payload.height = this.savedWindowHeight;
        if (this.savedWindowX !== null) payload.x = this.savedWindowX;
        if (this.savedWindowY !== null) payload.y = this.savedWindowY;
      }
      await invoke("exit_miniplayer_mode", payload);
    } catch (e) {
      console.warn("Failed to exit miniplayer backend window mode:", e);
    } finally {
      this.miniplayerTransitionInFlight = false;
    }
  }

  // Captures the window's real OS-reported geometry into whichever mode is
  // currently active (full player vs. miniplayer), but only from settled
  // `resized`/`moved` events — never from a synchronous read taken around a
  // mode-switch command, since that's unreliable immediately after
  // `set_decorations()` (see window.rs). Only wired up when
  // isGeometryCaptureSupported() resolves true (see init()). Debouncing
  // until events stop, and skipping while a toggle is in flight, ensures
  // only a genuine user
  // resize/move (native title-bar drag/resize on the full player, or the
  // drag/edge handles on the miniplayer) gets persisted.
  private initWindowGeometryTracking() {
    if (typeof window === "undefined") return;
    const scheduleCapture = () => {
      if (this.geometryCaptureTimer) clearTimeout(this.geometryCaptureTimer);
      this.geometryCaptureTimer = setTimeout(() => {
        void this.captureCurrentWindowGeometry();
      }, WindowLayoutStore.GEOMETRY_CAPTURE_DEBOUNCE_MS);
    };
    try {
      const win = getCurrentWindow();
      void win.onResized(scheduleCapture);
      void win.onMoved(scheduleCapture);
    } catch (e) {
      console.warn("Failed to attach window geometry listeners:", e);
    }
  }

  // Mirrors window.innerWidth/innerHeight into viewportWidth/viewportHeight
  // for the responsive breakpoint getters below. Deliberately separate from
  // initWindowGeometryTracking above: that one debounces and persists native
  // OS geometry across restarts via IPC; this one is a synchronous, in-memory
  // reflection of the CSS viewport with no persistence, so layout can react
  // to every resize immediately.
  private initViewportTracking() {
    if (typeof window === "undefined") return;
    const update = () => {
      this.viewportWidth = window.innerWidth;
      this.viewportHeight = window.innerHeight;
    };
    update();
    window.addEventListener("resize", update);
  }

  private async captureCurrentWindowGeometry() {
    if (this.miniplayerTransitionInFlight) return;
    try {
      const geo = await invoke<{ width: number; height: number; x: number | null; y: number | null }>(
        "get_window_geometry"
      );
      if (!geo) return;
      if (this.isMiniplayer) {
        this.setMiniplayerGeometry(geo.width, geo.height, geo.x, geo.y);
      } else {
        this.setSavedWindowGeometry(geo.width, geo.height, geo.x, geo.y);
      }
    } catch (e) {
      console.warn("Failed to capture window geometry:", e);
    }
  }

  async moveWindowToPreset(position: string) {
    try {
      await invoke("move_window_to_preset", { position });
    } catch (e) {
      console.warn("Failed to move window to preset:", e);
    }
  }

  async toggleMiniplayerMode() {
    if (this.isMiniplayer) {
      await this.exitMiniplayerMode();
    } else {
      await this.enterMiniplayerMode();
    }
  }

  toggleRightPanel() {
    this.rightPanelOpen = !this.rightPanelOpen;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_rightPanelOpen", this.rightPanelOpen.toString());
    }
  }

  // Responsive breakpoint flags (issue #413) — pure functions of
  // viewportWidth/viewportHeight. None of these read or write sidebarOpen,
  // sidebarWidth, rightPanelOpen, or immersiveMode: they're purely visual
  // overrides so widening/heightening the window back out always restores
  // exactly whatever the user had set manually.
  get isSidebarAutoCollapsed(): boolean {
    return this.viewportWidth < MEDIUM_BREAKPOINT_WIDTH_PX;
  }

  get isRightPanelAutoHidden(): boolean {
    return this.viewportWidth < MEDIUM_BREAKPOINT_WIDTH_PX;
  }

  get isImmersiveForced(): boolean {
    return this.viewportWidth < SMALL_BREAKPOINT_WIDTH_PX;
  }

  get effectiveImmersiveMode(): boolean {
    return this.immersiveMode || this.isImmersiveForced;
  }

  get isPlaybarOnlyMode(): boolean {
    return this.viewportHeight < PLAYBAR_ONLY_HEIGHT_BREAKPOINT_PX;
  }

  get isDetailHeaderCollapsed(): boolean {
    return this.viewportHeight < DETAIL_HEADER_COLLAPSE_HEIGHT_PX;
  }

  setSidebarWidth(width: number) {
    this.sidebarWidth = width;
    if (width >= SIDEBAR_MIN_WIDTH_PX) {
      this.lastExpandedSidebarWidth = width;
      if (typeof window !== "undefined") {
        localStorage.setItem("layout_lastExpandedSidebarWidth", width.toString());
      }
    }
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_sidebarWidth", width.toString());
    }
  }

  setRightPanelWidth(width: number) {
    this.rightPanelWidth = width;
    if (typeof window !== "undefined") {
      localStorage.setItem("layout_rightPanelWidth", width.toString());
    }
  }
}

export const windowLayoutStore = new WindowLayoutStore();
