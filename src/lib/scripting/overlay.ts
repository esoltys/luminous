import type {
  ScriptCaptionOptions,
  ScriptCursorOptions,
  ScriptOverlayApi,
  ScriptWaitApi,
} from "./types";

export const OVERLAY_HOST_ID = "luminous-capture-overlays";
export const CURSOR_ELEMENT_ID = "luminous-capture-cursor";
export const CAPTION_ELEMENT_ID = "luminous-capture-caption";
export const KEYSTROKE_ELEMENT_ID = "luminous-capture-keystroke";

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function clampX(x: number): number {
  const max = typeof window !== "undefined" && window.innerWidth > 0 ? window.innerWidth : 1920;
  return Math.max(0, Math.min(x, max));
}

function clampY(y: number): number {
  const max = typeof window !== "undefined" && window.innerHeight > 0 ? window.innerHeight : 1080;
  return Math.max(0, Math.min(y, max));
}

/**
 * Creates the in-app overlay capture controller for tutorials and motion graphics.
 * Manages simulated cursor movement, target element tracking, click animations,
 * tutorial captions, and keystroke indicators.
 */
export function createOverlayController(wait: ScriptWaitApi): ScriptOverlayApi {
  let cursorX = 0;
  let cursorY = 0;

  function getOrCreateHost(): HTMLElement | null {
    if (typeof document === "undefined") return null;
    let host = document.getElementById(OVERLAY_HOST_ID);
    if (!host) {
      host = document.createElement("div");
      host.id = OVERLAY_HOST_ID;
      host.style.cssText =
        "position: fixed; inset: 0; pointer-events: none; z-index: 999999; overflow: hidden;";
      document.body.appendChild(host);
    }
    return host;
  }

  function getOrCreateCursor(host: HTMLElement): HTMLElement {
    let cursor = document.getElementById(CURSOR_ELEMENT_ID);
    if (!cursor) {
      cursor = document.createElement("div");
      cursor.id = CURSOR_ELEMENT_ID;
      cursor.style.cssText =
        "position: absolute; left: 0; top: 0; pointer-events: none; will-change: transform; transform-origin: 5px 3px; display: none; z-index: 20;";
      cursor.innerHTML = `
        <svg width="24" height="24" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg" style="filter: drop-shadow(0 2px 4px rgba(0,0,0,0.4));">
          <path d="M5.5 3.5L18.5 13.5L12.5 14.5L10 20.5L7.5 19.5L10 13.5L5.5 3.5Z" fill="#ffffff" stroke="#111111" stroke-width="1.5" stroke-linejoin="round" />
        </svg>
      `;
      host.appendChild(cursor);
    }
    return cursor;
  }

  function getOrCreateCaption(host: HTMLElement): HTMLElement {
    let caption = document.getElementById(CAPTION_ELEMENT_ID);
    if (!caption) {
      caption = document.createElement("div");
      caption.id = CAPTION_ELEMENT_ID;
      caption.style.cssText =
        "position: absolute; left: 50%; transform: translateX(-50%); pointer-events: none; max-width: 640px; padding: 12px 24px; border-radius: 12px; font-family: inherit; box-shadow: 0 10px 30px rgba(0,0,0,0.5); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px); background: rgba(18, 18, 22, 0.88); color: #f1f5f9; border: 1px solid rgba(255, 255, 255, 0.15); text-align: center; z-index: 10; display: none;";
      host.appendChild(caption);
    }
    return caption;
  }

  function getOrCreateKeystroke(host: HTMLElement): HTMLElement {
    let keystroke = document.getElementById(KEYSTROKE_ELEMENT_ID);
    if (!keystroke) {
      keystroke = document.createElement("div");
      keystroke.id = KEYSTROKE_ELEMENT_ID;
      keystroke.style.cssText =
        "position: absolute; bottom: 32px; right: 32px; pointer-events: none; padding: 8px 14px; border-radius: 8px; font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace; background: rgba(24, 24, 28, 0.9); border: 1px solid rgba(255, 255, 255, 0.2); box-shadow: 0 6px 20px rgba(0,0,0,0.4); display: none; align-items: center; gap: 6px; z-index: 10;";
      host.appendChild(keystroke);
    }
    return keystroke;
  }

  const cursor = {
    async show(position?: { x: number; y: number }): Promise<void> {
      const host = getOrCreateHost();
      if (!host) return;
      const el = getOrCreateCursor(host);

      if (position) {
        cursorX = clampX(position.x);
        cursorY = clampY(position.y);
      }
      el.style.transform = `translate3d(${cursorX - 5}px, ${cursorY - 3}px, 0)`;
      el.style.display = "block";
      await wait.settled();
    },

    async hide(): Promise<void> {
      if (typeof document === "undefined") return;
      const el = document.getElementById(CURSOR_ELEMENT_ID);
      if (el) {
        el.style.display = "none";
      }
      await wait.settled();
    },

    async moveTo(x: number, y: number, options?: ScriptCursorOptions): Promise<void> {
      const host = getOrCreateHost();
      if (!host) return;
      const el = getOrCreateCursor(host);

      cursorX = clampX(x);
      cursorY = clampY(y);
      const durationMs = options?.durationMs ?? 0;

      if (durationMs > 0) {
        el.style.transition = `transform ${durationMs}ms cubic-bezier(0.2, 0, 0, 1)`;
        el.style.transform = `translate3d(${cursorX - 5}px, ${cursorY - 3}px, 0)`;
        await new Promise((resolve) => setTimeout(resolve, durationMs));
        el.style.transition = "none";
      } else {
        el.style.transition = "none";
        el.style.transform = `translate3d(${cursorX - 5}px, ${cursorY - 3}px, 0)`;
      }

      await wait.settled();
    },

    async pointTo(selector: string, options?: ScriptCursorOptions): Promise<void> {
      if (typeof document === "undefined") return;
      const target = document.querySelector(selector);
      if (!target) {
        console.warn(`[script.overlay] Target element not found for selector: "${selector}"`);
        return;
      }

      const rect = target.getBoundingClientRect();
      const targetX = rect.left + rect.width / 2;
      const targetY = rect.top + rect.height / 2;
      await this.moveTo(targetX, targetY, options);
    },

    async click(position?: { x: number; y: number }): Promise<void> {
      const host = getOrCreateHost();
      if (!host) return;
      const el = getOrCreateCursor(host);

      if (position) {
        await this.moveTo(position.x, position.y);
      }

      // Visual press effect on cursor
      if (typeof el.animate === "function") {
        el.animate(
          [
            { transform: el.style.transform + " scale(1)" },
            { transform: el.style.transform + " scale(0.82)" },
            { transform: el.style.transform + " scale(1)" },
          ],
          { duration: 180, easing: "ease-out" }
        );
      }

      // Visual expanding ripple effect at the cursor pointer tip
      const ripple = document.createElement("div");
      ripple.className = "luminous-capture-ripple";
      ripple.style.cssText = `position: absolute; left: ${cursorX}px; top: ${cursorY}px; width: 0px; height: 0px; border-radius: 50%; pointer-events: none; border: 2px solid var(--color-accent, #6366f1); transform: translate(-50%, -50%); opacity: 0.9; z-index: 15;`;
      host.appendChild(ripple);

      if (typeof ripple.animate === "function") {
        const anim = ripple.animate(
          [
            { width: "0px", height: "0px", opacity: 0.9 },
            { width: "48px", height: "48px", opacity: 0 },
          ],
          { duration: 350, easing: "cubic-bezier(0, 0, 0.2, 1)" }
        );
        anim.onfinish = () => ripple.remove();
      } else {
        setTimeout(() => ripple.remove(), 350);
      }

      await wait.settled();
    },
  };

  const caption = {
    async show(text: string, options?: ScriptCaptionOptions): Promise<void> {
      const host = getOrCreateHost();
      if (!host) return;
      const el = getOrCreateCaption(host);

      if (options?.position === "top") {
        el.style.top = "32px";
        el.style.bottom = "auto";
      } else {
        el.style.bottom = "48px";
        el.style.top = "auto";
      }

      let html = "";
      if (options?.title) {
        html += `<div style="font-weight: 600; font-size: 13px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--color-accent, #818cf8); margin-bottom: 4px;">${escapeHtml(options.title)}</div>`;
      }
      html += `<div style="font-size: 15px; font-weight: 500; line-height: 1.4;">${escapeHtml(text)}</div>`;
      el.innerHTML = html;
      el.style.display = "block";

      await wait.settled();
    },

    async hide(): Promise<void> {
      if (typeof document === "undefined") return;
      const el = document.getElementById(CAPTION_ELEMENT_ID);
      if (el) {
        el.style.display = "none";
        el.innerHTML = "";
      }
      await wait.settled();
    },
  };

  const keystroke = {
    async show(keys: string): Promise<void> {
      const host = getOrCreateHost();
      if (!host) return;
      const el = getOrCreateKeystroke(host);

      const parts = keys.includes("+")
        ? keys.split("+").map((s) => s.trim()).filter(Boolean)
        : [keys.trim()];

      const badges = parts
        .map(
          (part) =>
            `<kbd style="background: rgba(255, 255, 255, 0.12); padding: 3px 8px; border-radius: 4px; font-size: 13px; font-weight: 600; color: #fff; box-shadow: inset 0 -2px 0 rgba(0, 0, 0, 0.3); border: 1px solid rgba(255, 255, 255, 0.2);">${escapeHtml(part)}</kbd>`
        )
        .join('<span style="color: rgba(255, 255, 255, 0.5); font-size: 12px; font-weight: 600;">+</span>');

      el.innerHTML = badges;
      el.style.display = "flex";

      await wait.settled();
    },

    async hide(): Promise<void> {
      if (typeof document === "undefined") return;
      const el = document.getElementById(KEYSTROKE_ELEMENT_ID);
      if (el) {
        el.style.display = "none";
        el.innerHTML = "";
      }
      await wait.settled();
    },
  };

  return {
    cursor,
    caption,
    keystroke,
    async clear(): Promise<void> {
      cursorX = 0;
      cursorY = 0;
      if (typeof document !== "undefined") {
        const host = document.getElementById(OVERLAY_HOST_ID);
        if (host) {
          host.remove();
        }
      }
      await wait.settled();
    },
  };
}
