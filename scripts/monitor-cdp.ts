#!/usr/bin/env bun
/**
 * Monitors a running Luminous remote devtools session (port 9222) via the Chrome
 * DevTools Protocol (CDP).
 *
 * Measures renderer event loop latency, evaluates expressions, tracks document
 * visibility, and can test window minimization / restoration responsiveness.
 *
 * Requires Luminous to be launched with LUMINOUS_REMOTE_DEVTOOLS=1:
 *   $env:LUMINOUS_REMOTE_DEVTOOLS = "1"; bun run tauri dev   (PowerShell)
 *   LUMINOUS_REMOTE_DEVTOOLS=1 bun run tauri dev            (Bash)
 *
 * Usage:
 *   bun run scripts/monitor-cdp.ts                          # Single latency & state check
 *   bun run scripts/monitor-cdp.ts --watch                  # Continuous heartbeat monitoring
 *   bun run scripts/monitor-cdp.ts --watch --interval 3     # Poll every 3 seconds
 *   bun run scripts/monitor-cdp.ts --eval "document.title"  # Evaluate arbitrary JS in webview
 *   bun run scripts/monitor-cdp.ts --minimize --duration 10 # Minimize, monitor for 10s, and restore
 */

interface CdpTarget {
  id: string;
  title: string;
  type: string;
  url: string;
  webSocketDebuggerUrl?: string;
}

interface EvalResult {
  value: any;
  latencyMs: number;
  error?: string;
}

class CdpClient {
  private ws: WebSocket | null = null;
  private nextId = 1;
  private pending = new Map<
    number,
    { resolve: (val: any) => void; reject: (err: any) => void }
  >();

  async connect(port = 9222): Promise<string> {
    const endpoint = `http://127.0.0.1:${port}/json`;
    let targets: CdpTarget[];
    try {
      const res = await fetch(endpoint);
      targets = (await res.json()) as CdpTarget[];
    } catch (e) {
      throw new Error(
        `Failed to connect to CDP endpoint at ${endpoint}.\n` +
          `Is Luminous running with LUMINOUS_REMOTE_DEVTOOLS=1?\n` +
          `  PowerShell: $env:LUMINOUS_REMOTE_DEVTOOLS = "1"; bun run tauri dev\n` +
          `  Bash:       LUMINOUS_REMOTE_DEVTOOLS=1 bun run tauri dev`
      );
    }

    const target =
      targets.find(
        (t) =>
          t.type === "page" &&
          (t.title.includes("Luminous") || t.url.includes("localhost:1420"))
      ) ?? targets.find((t) => t.type === "page");

    if (!target || !target.webSocketDebuggerUrl) {
      throw new Error(
        `Found CDP endpoint, but no active page target. Available targets:\n` +
          JSON.stringify(targets, null, 2)
      );
    }

    this.ws = new WebSocket(target.webSocketDebuggerUrl);

    this.ws.onmessage = (event) => {
      try {
        const msg = JSON.parse(String(event.data));
        if (msg.id && this.pending.has(msg.id)) {
          const handler = this.pending.get(msg.id)!;
          this.pending.delete(msg.id);
          if (msg.error) {
            handler.reject(msg.error);
          } else {
            handler.resolve(msg.result);
          }
        }
      } catch (err) {
        console.error("Failed to parse CDP message:", err);
      }
    };

    this.ws.onerror = (err) => {
      console.error("CDP WebSocket error:", err);
    };

    await new Promise<void>((resolve, reject) => {
      if (!this.ws) return reject(new Error("WebSocket not created"));
      this.ws.onopen = () => resolve();
      this.ws.onerror = (e) => reject(e);
    });

    return target.title;
  }

  send(method: string, params: Record<string, any> = {}): Promise<any> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error("CDP WebSocket is not connected"));
    }
    return new Promise((resolve, reject) => {
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      this.ws!.send(JSON.stringify({ id, method, params }));
    });
  }

  async eval(expression: string, awaitPromise = true): Promise<EvalResult> {
    const t0 = performance.now();
    try {
      const res = await this.send("Runtime.evaluate", {
        expression,
        returnByValue: true,
        awaitPromise,
      });
      const latencyMs = Number((performance.now() - t0).toFixed(2));
      if (res?.exceptionDetails) {
        return {
          value: undefined,
          latencyMs,
          error: res.exceptionDetails.text || "Evaluation exception",
        };
      }
      return {
        value: res?.result?.value,
        latencyMs,
      };
    } catch (e: any) {
      const latencyMs = Number((performance.now() - t0).toFixed(2));
      return {
        value: undefined,
        latencyMs,
        error: e.message || String(e),
      };
    }
  }

  close(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }
}

function parseArgs() {
  const args = process.argv.slice(2);
  const get = (flag: string) => {
    const i = args.indexOf(flag);
    return i !== -1 && i + 1 < args.length ? args[i + 1] : undefined;
  };

  return {
    port: Number(get("--port") ?? "9222"),
    watch: args.includes("--watch"),
    intervalSec: Number(get("--interval") ?? "2"),
    durationSec: get("--duration") ? Number(get("--duration")) : undefined,
    evalExpr: get("--eval"),
    minimize: args.includes("--minimize"),
    unminimize: args.includes("--unminimize") || args.includes("--restore"),
    help: args.includes("--help") || args.includes("-h"),
  };
}

function printHelp() {
  console.log(`
Luminous CDP Monitor & Diagnostics

Usage:
  bun run scripts/monitor-cdp.ts [options]

Options:
  --port <port>       CDP debugging port (default: 9222)
  --watch             Continuously monitor latency, title, and visibility
  --interval <sec>    Interval between checks in seconds (default: 2)
  --duration <sec>    Total duration to run monitoring before exiting
  --eval <expr>       Evaluate an arbitrary JavaScript expression in the webview
  --minimize          Minimize the window, monitor for duration (default 10s), then restore
  --unminimize        Restore / unminimize the window
  --help, -h          Show this help message
`);
}

async function main() {
  const opts = parseArgs();

  if (opts.help) {
    printHelp();
    return;
  }

  const client = new CdpClient();
  const pageTitle = await client.connect(opts.port);
  console.log(`Connected to Luminous WebView (Target: "${pageTitle}")`);

  if (opts.evalExpr) {
    const res = await client.eval(opts.evalExpr);
    console.log(`Latency: ${res.latencyMs}ms`);
    if (res.error) {
      console.error(`Error: ${res.error}`);
      process.exitCode = 1;
    } else {
      console.log(`Result:`, res.value);
    }
    client.close();
    return;
  }

  if (opts.unminimize) {
    console.log("Restoring / unminimizing window...");
    const res = await client.eval(
      `window.__TAURI_INTERNALS__ ? window.__TAURI_INTERNALS__.invoke('plugin:window|unminimize') : null`
    );
    console.log(`Unminimize command executed in ${res.latencyMs}ms`);
    client.close();
    return;
  }

  if (opts.minimize) {
    const duration = opts.durationSec ?? 10;
    console.log(
      `Minimizing window and monitoring responsiveness for ${duration}s...`
    );
    const minRes = await client.eval(
      `window.__TAURI_INTERNALS__ ? window.__TAURI_INTERNALS__.invoke('plugin:window|minimize') : null`
    );
    console.log(`Minimize invocation latency: ${minRes.latencyMs}ms`);

    const startTime = Date.now();
    let tick = 0;
    while (Date.now() - startTime < duration * 1000) {
      await new Promise((r) => setTimeout(r, opts.intervalSec * 1000));
      tick++;
      const [titleRes, hiddenRes] = await Promise.all([
        client.eval("document.title"),
        client.eval("document.hidden"),
      ]);
      console.log(
        `  [+${tick * opts.intervalSec}s] Title: '${titleRes.value}' | hidden: ${hiddenRes.value} | Latency: ${titleRes.latencyMs}ms`
      );
    }

    console.log("Restoring window...");
    const restoreRes = await client.eval(
      `window.__TAURI_INTERNALS__ ? window.__TAURI_INTERNALS__.invoke('plugin:window|unminimize') : null`
    );
    const postTitle = await client.eval("document.title");
    console.log(
      `Restored window in ${restoreRes.latencyMs}ms. Current title: '${postTitle.value}' (heartbeat: ${postTitle.latencyMs}ms)`
    );
    client.close();
    return;
  }

  if (opts.watch) {
    console.log(
      `Monitoring renderer responsiveness every ${opts.intervalSec}s (Ctrl+C to stop)...`
    );
    const startTime = Date.now();
    while (true) {
      if (opts.durationSec && (Date.now() - startTime) >= opts.durationSec * 1000) {
        break;
      }
      const [titleRes, hiddenRes] = await Promise.all([
        client.eval("document.title"),
        client.eval("document.hidden"),
      ]);
      const timestamp = new Date().toLocaleTimeString();
      console.log(
        `[${timestamp}] Latency: ${titleRes.latencyMs.toFixed(1).padStart(5)}ms | Title: '${titleRes.value}' | hidden: ${hiddenRes.value}`
      );
      await new Promise((r) => setTimeout(r, opts.intervalSec * 1000));
    }
    client.close();
    return;
  }

  // Single check
  const [titleRes, hiddenRes, readyRes] = await Promise.all([
    client.eval("document.title"),
    client.eval("document.hidden"),
    client.eval("document.readyState"),
  ]);

  console.log("\n--- Luminous Webview Status ---");
  console.log(`Document Title: '${titleRes.value}'`);
  console.log(`Ready State:    ${readyRes.value}`);
  console.log(`Hidden:         ${hiddenRes.value}`);
  console.log(`CDP Latency:    ${titleRes.latencyMs}ms`);

  if (titleRes.latencyMs > 100) {
    console.warn(
      `\nWARNING: High latency (${titleRes.latencyMs}ms). The renderer event loop may be under heavy load.`
    );
  } else {
    console.log(`Health:         EXCELLENT (sub-100ms response)`);
  }

  client.close();
}

main().catch((err) => {
  console.error(err.message || err);
  process.exit(1);
});
