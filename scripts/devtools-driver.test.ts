import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { existsSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import { DEFAULT_CDP_PORT, DevtoolsDriver } from "./devtools-driver";

describe("devtools-driver", () => {
  it("exports standard constants", () => {
    expect(DEFAULT_CDP_PORT).toBe(9222);
  });

  describe("mock CDP server interaction", () => {
    let server: any;
    let serverPort: number;
    let receivedMethods: { method: string; params: any }[] = [];

    beforeEach(() => {
      receivedMethods = [];

      server = Bun.serve({
        port: 0,
        fetch(req, s) {
          const url = new URL(req.url);
          if (url.pathname === "/json") {
            return Response.json([
              {
                id: "test-page-1",
                type: "page",
                title: "Luminous Music Player",
                url: "http://localhost:1420",
                webSocketDebuggerUrl: `ws://127.0.0.1:${s.port}/devtools/page/1`,
              },
            ]);
          }
          if (s.upgrade(req)) {
            return;
          }
          return new Response("Not Found", { status: 404 });
        },
        websocket: {
          message(ws, raw) {
            const msg = JSON.parse(String(raw));
            receivedMethods.push({ method: msg.method, params: msg.params });

            if (msg.method === "Runtime.enable" || msg.method === "Page.enable") {
              ws.send(JSON.stringify({ id: msg.id, result: {} }));
            } else if (msg.method === "Runtime.evaluate") {
              if (msg.params.expression?.includes("document.readyState")) {
                ws.send(
                  JSON.stringify({
                    id: msg.id,
                    result: { result: { type: "boolean", value: true } },
                  })
                );
              } else if (msg.params.expression === "globalThis") {
                ws.send(
                  JSON.stringify({
                    id: msg.id,
                    result: { result: { objectId: "global-obj-1" } },
                  })
                );
              } else if (msg.params.expression === "1 + 1") {
                ws.send(
                  JSON.stringify({
                    id: msg.id,
                    result: { result: { type: "number", value: 2 } },
                  })
                );
              } else {
                ws.send(
                  JSON.stringify({
                    id: msg.id,
                    result: { result: { type: "string", value: "ok" } },
                  })
                );
              }
            } else if (msg.method === "Runtime.callFunctionOn") {
              if (msg.params.functionDeclaration?.includes("plugin:event|listen")) {
                const eventName = msg.params.arguments?.[0]?.value;
                ws.send(
                  JSON.stringify({
                    id: msg.id,
                    result: { result: { value: { event: eventName, payload: { track: "Midnight City" } } } },
                  })
                );
              } else if (msg.params.functionDeclaration?.includes("invoke")) {
                const cmd = msg.params.arguments?.[0]?.value;
                ws.send(
                  JSON.stringify({
                    id: msg.id,
                    result: { result: { value: { status: "invoked", cmd } } },
                  })
                );
              } else {
                const sum =
                  (msg.params.arguments?.[0]?.value ?? 0) +
                  (msg.params.arguments?.[1]?.value ?? 0);
                ws.send(
                  JSON.stringify({
                    id: msg.id,
                    result: { result: { value: sum } },
                  })
                );
              }
            } else if (msg.method === "Page.captureScreenshot") {
              // 1x1 transparent PNG base64
              const pngBase64 =
                "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=";
              ws.send(
                JSON.stringify({
                  id: msg.id,
                  result: { data: pngBase64 },
                })
              );
            } else if (msg.method === "Emulation.setDeviceMetricsOverride") {
              ws.send(JSON.stringify({ id: msg.id, result: {} }));
            } else if (msg.method === "Emulation.clearDeviceMetricsOverride") {
              ws.send(JSON.stringify({ id: msg.id, result: {} }));
            } else if (msg.method === "Page.reload") {
              ws.send(JSON.stringify({ id: msg.id, result: {} }));
            } else {
              ws.send(JSON.stringify({ id: msg.id, result: {} }));
            }
          },
        },
      });

      serverPort = server.port;
    });

    afterEach(() => {
      if (server) {
        server.stop(true);
      }
    });

    it("connects and initializes Page and Runtime domains", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      try {
        expect(driver.port).toBe(serverPort);
        expect(receivedMethods.some((m) => m.method === "Runtime.enable")).toBe(true);
        expect(receivedMethods.some((m) => m.method === "Page.enable")).toBe(true);
      } finally {
        await driver.close();
      }
    });

    it("evaluates string expressions and functions with arguments", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      try {
        const simpleResult = await driver.evaluate("1 + 1");
        expect(simpleResult).toBe(2);

        const sumResult = await driver.evaluate((a: number, b: number) => a + b, 15, 25);
        expect(sumResult).toBe(40);
      } finally {
        await driver.close();
      }
    });

    it("invokes backend IPC commands", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      try {
        const result = await driver.invoke<{ status: string; cmd: string }>("play_song", {
          songId: 42,
        });
        expect(result).toEqual({ status: "invoked", cmd: "play_song" });
      } finally {
        await driver.close();
      }
    });

    it("captures screenshot to Buffer and saves to disk", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      const tempPath = path.join(import.meta.dir, `test-screenshot-${Date.now()}.png`);
      try {
        const buf = await driver.screenshot(tempPath);
        expect(Buffer.isBuffer(buf)).toBe(true);
        expect(buf.length).toBeGreaterThan(0);
        expect(existsSync(tempPath)).toBe(true);
        expect(readFileSync(tempPath).length).toBe(buf.length);
      } finally {
        if (existsSync(tempPath)) rmSync(tempPath);
        await driver.close();
      }
    });

    it("sets window size and automatically clears on dispose", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      await driver.setWindowSize(1920, 1080);

      expect(
        receivedMethods.some(
          (m) =>
            m.method === "Emulation.setDeviceMetricsOverride" &&
            m.params.width === 1920 &&
            m.params.height === 1080
        )
      ).toBe(true);

      // Closing should automatically clear device metrics
      await driver.close();

      expect(
        receivedMethods.some((m) => m.method === "Emulation.clearDeviceMetricsOverride")
      ).toBe(true);
    });

    it("waits for Tauri events", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      try {
        const payload = await driver.waitForEvent<{ track: string }>("track-changed", { timeoutMs: 2000 });
        expect(payload).toEqual({ event: "track-changed", payload: { track: "Midnight City" } } as any);
      } finally {
        await driver.close();
      }
    });

    it("polls conditions until truthy", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      try {
        let count = 0;
        await driver.waitForCondition(() => ++count >= 3, { intervalMs: 20 });
        expect(count).toBeGreaterThanOrEqual(3);
      } finally {
        await driver.close();
      }
    });

    it("supports await using explicit resource management", async () => {
      {
        await using driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
        await driver.setWindowSize(800, 600);
      }
      expect(
        receivedMethods.some((m) => m.method === "Emulation.clearDeviceMetricsOverride")
      ).toBe(true);
    });

    it("reloads page and awaits readiness", async () => {
      const driver = await DevtoolsDriver.connect({ port: serverPort, timeoutMs: 5000 });
      try {
        await driver.reload();
        expect(receivedMethods.some((m) => m.method === "Page.reload")).toBe(true);
      } finally {
        await driver.close();
      }
    });
  });
});
