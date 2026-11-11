// Paste into the Claude Browser pane's javascript_tool (any page on 127.0.0.1:9222,
// e.g. http://127.0.0.1:9222/json) to drive the running Luminous webview over CDP.
// The pane can't "attach" to a CDP target itself; page JS can open the websocket.
// Usage after pasting:
//   await cdp.eval("document.title")
//   await cdp.screenshot()            // -> data:image/jpeg;base64,... (render via an <img>)
//   await cdp.send("DOM.getDocument") // any raw CDP method
// Port: change PORT if LUMINOUS_REMOTE_DEVTOOLS_PORT / CDP_PORT is set.
(async () => {
  const PORT = 9222;
  const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
  const page = targets.find((t) => t.type === "page");
  if (!page) throw new Error("no CDP page target; is `bun run tauri dev` running?");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((ok, fail) => {
    ws.onopen = ok;
    ws.onerror = () => fail(new Error("CDP websocket refused"));
  });
  let id = 0;
  const pending = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    const p = pending.get(m.id);
    if (!p) return;
    pending.delete(m.id);
    m.error ? p.fail(new Error(m.error.message)) : p.ok(m.result);
  };
  const send = (method, params = {}) =>
    new Promise((ok, fail) => {
      pending.set(++id, { ok, fail });
      ws.send(JSON.stringify({ id, method, params }));
    });
  window.cdp = {
    send,
    close: () => ws.close(),
    async eval(expression) {
      const r = await send("Runtime.evaluate", {
        expression,
        awaitPromise: true,
        returnByValue: true,
      });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.text);
      return r.result.value;
    },
    async screenshot() {
      const r = await send("Page.captureScreenshot", { format: "jpeg", quality: 70 });
      return `data:image/jpeg;base64,${r.data}`;
    },
  };
  return `attached to ${page.title} (${page.url})`;
})();
