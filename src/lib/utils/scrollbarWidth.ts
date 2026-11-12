/**
 * Publishes the width of the webview's classic scrollbar as `--scrollbar-width` on `<html>`.
 *
 * `app.css` sets `scrollbar-width: thin`, which leaves the actual width to the engine (WebView2,
 * WebKitGTK and WKWebView all differ, and overlay scrollbars take no space at all), so it can't be
 * hard-coded. A probe that inherits the same global scrollbar rules reports the real value.
 * Anything that must line up with content inside a scroller (e.g. the sync drawer, which floats
 * over the scrollbar) reads the variable instead of guessing.
 */
export function publishScrollbarWidth(): void {
  const probe = document.createElement("div");
  probe.style.cssText =
    "position:absolute;visibility:hidden;top:-9999px;width:100px;height:100px;overflow-y:scroll";
  document.body.appendChild(probe);
  const width = probe.offsetWidth - probe.clientWidth;
  probe.remove();
  document.documentElement.style.setProperty("--scrollbar-width", `${width}px`);
}
