#!/usr/bin/env bun
/**
 * Lightweight DevTools protocol (CDP) driver for controlling real Luminous
 * application windows without Playwright.
 *
 * Connects directly to WebView2's remote debugging port (port 9222) and exposes:
 * - Backend IPC invocation (`invoke(cmd, args)`)
 * - In-page evaluation (`evaluate(expressionOrFn, ...args)`)
 * - Viewport screenshots with automatic file saving (`screenshot(pathOrOptions)`)
 * - Viewport dimensions override and guaranteed restoration (`setWindowSize`, `clearWindowSize`)
 * - Real Tauri event listening (`waitForEvent(eventName, options)`)
 * - State polling (`waitForCondition(predicate, options)`)
 * - Page reloading and webview lifecycle management
 *
 * Architecture Invariant: No selector-based UI driving in the driver itself.
 * Anything requiring user-interface automation is driven either via the in-app
 * scripting API (window.__LUMINOUS_SCRIPT__) or through page evaluation.
 */

import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

export const DEFAULT_CDP_PORT = 9222;

export interface CdpTarget {
  id: string;
  title: string;
  type: string;
  url: string;
  webSocketDebuggerUrl?: string;
}

export interface ConnectOptions {
  /** Remote debugging port (defaults to 9222). */
  port?: number;
  /** Remote debugging host (defaults to '127.0.0.1'). */
  host?: string;
  /** Max time to wait for the webview to become ready in milliseconds (defaults to 30,000). */
  timeoutMs?: number;
}

export interface ScreenshotOptions {
  /** Optional file path where the screenshot will be saved. Ensures directories exist. */
  path?: string;
  /** Format of the captured screenshot (defaults to 'png'). */
  format?: "png" | "jpeg" | "webp";
  /** Compression quality (0-100) for jpeg or webp. */
  quality?: number;
  /** Whether to capture the full page beyond the viewport. */
  captureBeyondViewport?: boolean;
}

export interface WaitForEventOptions {
  /** Maximum wait time in milliseconds (defaults to 10,000). */
  timeoutMs?: number;
}

export interface WaitForConditionOptions {
  /** Maximum wait time in milliseconds (defaults to 10,000). */
  timeoutMs?: number;
  /** Polling interval in milliseconds (defaults to 100). */
  intervalMs?: number;
  /** Optional custom failure message when the condition times out. */
  message?: string;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export class DevtoolsDriver implements AsyncDisposable {
  private ws: WebSocket | null = null;
  private nextId = 1;
  private pending = new Map<
    number,
    { resolve: (val: any) => void; reject: (err: any) => void; method: string }
  >();
  private hasWindowOverride = false;
  private isClosed = false;
  readonly port: number;
  readonly host: string;

  private constructor(port: number, host: string) {
    this.port = port;
    this.host = host;
  }

  /**
   * Connects to a running Luminous instance over CDP and awaits webview readiness.
   *
   * Automatically polls the endpoint and waits until the target webview is available,
   * the document is fully loaded, and window.__TAURI_INTERNALS__ is ready.
   */
  static async connect(options?: number | ConnectOptions): Promise<DevtoolsDriver> {
    const opts: ConnectOptions =
      typeof options === "number" ? { port: options } : (options ?? {});
    const port = opts.port ?? DEFAULT_CDP_PORT;
    const host = opts.host ?? "127.0.0.1";
    const timeoutMs = opts.timeoutMs ?? 30_000;
    const deadline = Date.now() + timeoutMs;

    const driver = new DevtoolsDriver(port, host);
    let lastError: Error | null = null;

    while (Date.now() < deadline) {
      try {
        const target = await driver.discoverTarget();
        if (target?.webSocketDebuggerUrl) {
          await driver.connectSocket(target.webSocketDebuggerUrl);
          await driver.enableDomains();
          const ready = await driver.isPageReady();
          if (ready) {
            return driver;
          }
        }
      } catch (err) {
        lastError = err instanceof Error ? err : new Error(String(err));
      }
      driver.disconnectSocket();
      await sleep(250);
    }

    throw new Error(
      `Failed to connect to Luminous DevTools at http://${host}:${port} within ${timeoutMs}ms.` +
        (lastError ? ` Last error: ${lastError.message}` : "")
    );
  }

  private async discoverTarget(): Promise<CdpTarget | null> {
    const endpoint = `http://${this.host}:${this.port}/json`;
    const res = await fetch(endpoint);
    if (!res.ok) return null;
    const targets = (await res.json()) as CdpTarget[];

    return (
      targets.find(
        (t) =>
          t.type === "page" &&
          (t.title.includes("Luminous") || t.url.includes("localhost:1420"))
      ) ??
      targets.find((t) => t.type === "page") ??
      null
    );
  }

  private async connectSocket(url: string): Promise<void> {
    this.ws = new WebSocket(url);

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(String(event.data));
        if (msg.id && this.pending.has(msg.id)) {
          const handler = this.pending.get(msg.id)!;
          this.pending.delete(msg.id);
          if (msg.error) {
            handler.reject(
              new Error(
                `CDP method ${handler.method} failed: ${msg.error.message ?? JSON.stringify(msg.error)}`
              )
            );
          } else {
            handler.resolve(msg.result);
          }
        }
      } catch (err) {
        console.error("Failed to parse incoming CDP message:", err);
      }
    };

    this.ws.onerror = (err) => {
      // WebSocket errors will reject any inflight pending messages
      for (const [id, handler] of this.pending.entries()) {
        handler.reject(new Error(`WebSocket error on ${handler.method}`));
        this.pending.delete(id);
      }
    };

    await new Promise<void>((resolve, reject) => {
      if (!this.ws) return reject(new Error("WebSocket instance missing"));
      if (this.ws.readyState === WebSocket.OPEN) return resolve();
      this.ws.onopen = () => resolve();
      this.ws.onerror = (e) => reject(e);
    });
  }

  private disconnectSocket(): void {
    if (this.ws) {
      try {
        this.ws.close();
      } catch {}
      this.ws = null;
    }
    for (const [id, handler] of this.pending.entries()) {
      handler.reject(new Error("Connection reset"));
      this.pending.delete(id);
    }
  }

  private async enableDomains(): Promise<void> {
    await Promise.all([
      this.send("Runtime.enable"),
      this.send("Page.enable"),
    ]);
  }

  private async isPageReady(): Promise<boolean> {
    try {
      const res = await this.send("Runtime.evaluate", {
        expression: "document.readyState === 'complete' && !!window.__TAURI_INTERNALS__",
        returnByValue: true,
      });
      return res?.result?.value === true;
    } catch {
      return false;
    }
  }

  /**
   * Sends a raw JSON-RPC command over CDP and returns the result.
   */
  send(method: string, params: Record<string, unknown> = {}): Promise<any> {
    if (this.isClosed || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error(`CDP WebSocket is not connected (method: ${method})`));
    }
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject, method });
      this.ws!.send(JSON.stringify({ id, method, params }));
    });
  }

  /**
   * Invokes a Tauri IPC command directly in the host application.
   *
   * @param cmd The command name (e.g. 'play_song', 'get_playback_state').
   * @param args Command arguments.
   * @returns Deserialized result from the backend.
   */
  async invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
    try {
      return await this.evaluate<T>(
        (c: string, a: Record<string, unknown>) => {
          const internals = (window as any).__TAURI_INTERNALS__;
          if (!internals?.invoke) {
            throw new Error("window.__TAURI_INTERNALS__.invoke is unavailable");
          }
          return internals.invoke(c, a);
        },
        cmd,
        args
      );
    } catch (err) {
      throw new Error(`invoke('${cmd}') failed: ${err instanceof Error ? err.message : String(err)}`);
    }
  }

  /**
   * Evaluates an expression or executes a function within the webview page context.
   *
   * Seamlessly handles both string expressions and parameterized functions.
   */
  async evaluate<T = unknown>(
    fnOrExpression: string | ((...args: any[]) => T | Promise<T>),
    ...args: unknown[]
  ): Promise<T> {
    if (typeof fnOrExpression === "string" && args.length === 0) {
      const res = await this.send("Runtime.evaluate", {
        expression: fnOrExpression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (res?.exceptionDetails) {
        throw new Error(
          res.exceptionDetails.exception?.description ??
            res.exceptionDetails.text ??
            "Evaluation failed"
        );
      }
      return res?.result?.value as T;
    }

    const globalObj = await this.send("Runtime.evaluate", { expression: "globalThis" });
    const fnDeclaration =
      typeof fnOrExpression === "function"
        ? fnOrExpression.toString()
        : `function() { return (${fnOrExpression}); }`;

    const res = await this.send("Runtime.callFunctionOn", {
      objectId: globalObj.result.objectId,
      functionDeclaration: fnDeclaration,
      arguments: args.map((value) => ({ value })),
      awaitPromise: true,
      returnByValue: true,
    });

    if (res?.exceptionDetails) {
      throw new Error(
        res.exceptionDetails.exception?.description ??
          res.exceptionDetails.text ??
          "Evaluation failed"
      );
    }
    return res?.result?.value as T;
  }

  /**
   * Captures a screenshot of the webview as a PNG/JPEG/WEBP buffer.
   *
   * If a file path is provided (as a string or `options.path`), creates any
   * missing parent directories and writes the file to disk. Always returns the
   * raw image Buffer.
   */
  async screenshot(pathOrOptions?: string | ScreenshotOptions): Promise<Buffer> {
    const opts: ScreenshotOptions =
      typeof pathOrOptions === "string" ? { path: pathOrOptions } : (pathOrOptions ?? {});
    const format = opts.format ?? "png";
    const params: Record<string, unknown> = {
      format,
      captureBeyondViewport: opts.captureBeyondViewport ?? false,
    };
    if (opts.quality !== undefined && (format === "jpeg" || format === "webp")) {
      params.quality = opts.quality;
    }

    const res = await this.send("Page.captureScreenshot", params);
    const buffer = Buffer.from(res.data, "base64");

    if (opts.path) {
      const targetDir = path.dirname(opts.path);
      if (targetDir && !existsSync(targetDir)) {
        mkdirSync(targetDir, { recursive: true });
      }
      writeFileSync(opts.path, buffer);
    }

    return buffer;
  }

  /**
   * Overrides the viewport dimensions in physical pixels using CDP device metrics.
   *
   * @param width Viewport width in pixels.
   * @param height Viewport height in pixels.
   * @param deviceScaleFactor Display scale factor (defaults to 1).
   */
  async setWindowSize(width: number, height: number, deviceScaleFactor = 1): Promise<void> {
    await this.send("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor,
      mobile: false,
    });
    this.hasWindowOverride = true;
  }

  /**
   * Clears any active device metrics override, restoring the window's natural dimensions.
   * Safe no-op if no override is active.
   */
  async clearWindowSize(): Promise<void> {
    if (this.hasWindowOverride) {
      await this.send("Emulation.clearDeviceMetricsOverride");
      this.hasWindowOverride = false;
    }
  }

  /**
   * Waits for a real Tauri backend event to be received by the webview.
   *
   * Registers a one-shot event listener via Tauri's internal event plugin,
   * cleanly unregisters on receipt or timeout, and resolves with the event's payload.
   */
  async waitForEvent<T = unknown>(eventName: string, options?: WaitForEventOptions): Promise<T> {
    const timeoutMs = options?.timeoutMs ?? 10_000;

    return await this.evaluate<T>(
      (event: string, timeout: number) => {
        return new Promise<T>((resolve, reject) => {
          let unlistenFn: (() => void) | null = null;
          const timer = setTimeout(() => {
            if (unlistenFn) unlistenFn();
            reject(new Error(`Timed out waiting ${timeout}ms for Tauri event '${event}'`));
          }, timeout);

          const internals = (window as any).__TAURI_INTERNALS__;
          if (!internals?.invoke || !internals?.transformCallback) {
            clearTimeout(timer);
            return reject(new Error("Tauri internals unavailable for event listening"));
          }

          const handlerId = internals.transformCallback((e: any) => {
            clearTimeout(timer);
            if (unlistenFn) unlistenFn();
            resolve(e ? e.payload : undefined);
          }, true);

          internals
            .invoke("plugin:event|listen", {
              event,
              target: { kind: "Any" },
              handler: handlerId,
            })
            .then((eventId: number) => {
              unlistenFn = () => {
                internals.invoke("plugin:event|unlisten", { event, eventId }).catch(() => {});
              };
            })
            .catch((err: unknown) => {
              clearTimeout(timer);
              reject(err);
            });
        });
      },
      eventName,
      timeoutMs
    );
  }

  /**
   * Repeatedly polls a predicate function until it returns truthy, or times out.
   */
  async waitForCondition(
    predicate: () => boolean | Promise<boolean>,
    options?: WaitForConditionOptions
  ): Promise<void> {
    const timeoutMs = options?.timeoutMs ?? 10_000;
    const intervalMs = options?.intervalMs ?? 100;
    const deadline = Date.now() + timeoutMs;

    while (Date.now() < deadline) {
      try {
        const result = await predicate();
        if (result) return;
      } catch {}
      await sleep(intervalMs);
    }

    throw new Error(
      options?.message ?? `Timed out waiting for condition after ${timeoutMs}ms`
    );
  }

  /**
   * Reloads the page in the webview and waits for document and IPC readiness.
   */
  async reload(): Promise<void> {
    await this.send("Page.reload");
    const deadline = Date.now() + 30_000;
    while (Date.now() < deadline) {
      if (await this.isPageReady()) return;
      await sleep(100);
    }
    throw new Error("Page failed to become ready after reload within 30s");
  }

  /**
   * Closes the driver connection and reverts any active viewport size override.
   * Idempotent.
   */
  async close(): Promise<void> {
    if (this.isClosed) return;

    try {
      await this.clearWindowSize().catch(() => {});
    } finally {
      this.isClosed = true;
      this.disconnectSocket();
    }
  }

  /**
   * Implements AsyncDisposable for `await using driver = ...` syntax.
   */
  async [Symbol.asyncDispose](): Promise<void> {
    await this.close();
  }
}
