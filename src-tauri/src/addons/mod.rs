//! Add-on theme runtime (#1036): the in-memory asset store and the
//! `luminous-addon` URI scheme that serves a decrypted add-on's overlay to its
//! sandboxed iframe (#1413).
//!
//! Decrypted assets are held only in memory and are never written to disk. The
//! loader (#1415) calls [`register`] after verifying and decrypting a bundle;
//! the scheme handler serves nothing that was not registered.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

pub mod bundle;
pub mod verifier;

/// Version of the host→overlay message API (see `docs/ADDONS.md`). Bumped when
/// a message shape changes incompatibly; manifests declare `minApiVersion`.
pub const OVERLAY_API_VERSION: u32 = 1;

/// Served with every add-on response. The overlay runs in an iframe with
/// `sandbox="allow-scripts"` (opaque origin, no Tauri IPC); this is the second
/// layer: no network, no eval, no framing of anything else.
const CONTENT_SECURITY_POLICY: &str = "default-src 'none'; script-src 'self'; \
style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; \
media-src 'self'; connect-src 'none'; frame-src 'none'; object-src 'none'; \
base-uri 'none'; form-action 'none'";

/// One decrypted asset: its declared content type and bytes.
#[derive(Debug, Clone)]
pub struct Asset {
    pub content_type: String,
    pub data: Arc<Vec<u8>>,
}

/// Decrypted assets of one add-on, keyed by bundle-relative path.
pub type AddonAssets = HashMap<String, Asset>;

type Registry = HashMap<String, Arc<AddonAssets>>;

static REGISTRY: LazyLock<RwLock<Registry>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Make an add-on's assets servable, replacing any earlier registration.
pub fn register(id: &str, assets: AddonAssets) {
    REGISTRY.write().insert(id.to_string(), Arc::new(assets));
}

/// Drop an add-on's assets (e.g. entitlement lost); further requests 404.
pub fn unregister(id: &str) {
    REGISTRY.write().remove(id);
}

/// Handle a `luminous-addon` request against the process-wide registry.
pub fn serve_request(uri: &str) -> tauri::http::Response<Vec<u8>> {
    serve_request_from(&REGISTRY.read(), uri)
}

fn serve_request_from(registry: &Registry, uri: &str) -> tauri::http::Response<Vec<u8>> {
    let Some((id, path)) = parse_request(uri) else {
        return empty_response(404);
    };
    let Some(asset) = registry.get(&id).and_then(|assets| assets.get(&path)) else {
        return empty_response(404);
    };
    tauri::http::Response::builder()
        .status(200)
        .header("content-type", asset.content_type.as_str())
        .header("content-security-policy", CONTENT_SECURITY_POLICY)
        .header("x-content-type-options", "nosniff")
        .header("cache-control", "no-store")
        // The sandboxed frame has an opaque origin, so module scripts and
        // fonts are cross-origin fetches and need CORS to load.
        .header("access-control-allow-origin", "*")
        .body(asset.data.as_ref().clone())
        .unwrap()
}

/// Split `<id>/<path>` out of the request URI, rejecting anything that is not a
/// plain bundle-relative path. The registry lookup is exact-match, so this is
/// belt and braces against traversal rather than the only guard.
fn parse_request(uri: &str) -> Option<(String, String)> {
    // Windows WebView2 requests `http://luminous-addon.localhost/...`; wry
    // rewrites that to `luminous-addon://localhost/...` before the handler
    // runs (same as `luminous-art`, #715). Accept either form.
    let rest = uri
        .strip_prefix("http://luminous-addon.localhost/")
        .or_else(|| uri.strip_prefix("luminous-addon://"))?;
    let rest = rest.strip_prefix("localhost/").unwrap_or(rest);
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let decoded = percent_encoding::percent_decode_str(rest)
        .decode_utf8()
        .ok()?
        .into_owned();
    let (id, path) = decoded.split_once('/')?;
    if id.is_empty() || path.is_empty() || path.contains('\\') || path.contains('\0') {
        return None;
    }
    if path
        .split('/')
        .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return None;
    }
    Some((id.to_string(), path.to_string()))
}

fn empty_response(status: u16) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("content-security-policy", CONTENT_SECURITY_POLICY)
        .body(Vec::new())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> Registry {
        let mut assets = AddonAssets::new();
        assets.insert(
            "overlay.html".into(),
            Asset {
                content_type: "text/html".into(),
                data: Arc::new(b"<html></html>".to_vec()),
            },
        );
        assets.insert(
            "assets/a b.png".into(),
            Asset {
                content_type: "image/png".into(),
                data: Arc::new(vec![1, 2, 3]),
            },
        );
        let mut reg = Registry::new();
        reg.insert("fixture".into(), Arc::new(assets));
        reg
    }

    #[test]
    fn serves_registered_asset_with_csp() {
        let res = serve_request_from(
            &registry(),
            "luminous-addon://localhost/fixture/overlay.html",
        );
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "text/html");
        let csp = res.headers()["content-security-policy"].to_str().unwrap();
        assert!(csp.contains("connect-src 'none'"));
        assert!(!csp.contains("unsafe-eval"));
        assert_eq!(res.body().as_slice(), b"<html></html>");
    }

    #[test]
    fn accepts_windows_http_form_and_percent_encoding() {
        let res = serve_request_from(
            &registry(),
            "http://luminous-addon.localhost/fixture/assets/a%20b.png?x=1",
        );
        assert_eq!(res.status(), 200);
        assert_eq!(res.body().as_slice(), &[1, 2, 3]);
    }

    #[test]
    fn rejects_traversal_and_unknown() {
        let reg = registry();
        for uri in [
            "luminous-addon://localhost/fixture/../other/overlay.html",
            "luminous-addon://localhost/fixture/%2e%2e/overlay.html",
            "luminous-addon://localhost/fixture/assets//a%20b.png",
            "luminous-addon://localhost/fixture/assets%5Ca%20b.png",
            "luminous-addon://localhost/fixture/missing.js",
            "luminous-addon://localhost/other/overlay.html",
            "luminous-addon://localhost/fixture/",
            "luminous-addon://localhost/",
            "luminous-art://localhost/fixture/overlay.html",
        ] {
            assert_eq!(serve_request_from(&reg, uri).status(), 404, "{uri}");
        }
    }

    #[test]
    fn unregister_stops_serving() {
        register(
            "unit-test-addon",
            registry().remove("fixture").map(|a| (*a).clone()).unwrap(),
        );
        let uri = "luminous-addon://localhost/unit-test-addon/overlay.html";
        assert_eq!(serve_request(uri).status(), 200);
        unregister("unit-test-addon");
        assert_eq!(serve_request(uri).status(), 404);
    }
}
