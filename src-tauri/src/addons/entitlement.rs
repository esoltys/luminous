//! Store entitlement for add-on themes (#1414): which add-ons the signed-in
//! Store user owns, the purchase flow, and the hand-off to the key-release
//! client and bundle installer.
//!
//! Everything platform-specific sits behind [`StoreBackend`]; the Windows
//! implementation is in [`windows_store`], and builds without it (Linux,
//! macOS, unpackaged Windows) get [`Unavailable`]. The frontend never assumes
//! the result of a command: every transition is announced through
//! [`Events::state`] (the `addon-state-changed` event).
//!
//! State changes carry a machine-readable `error` code, never prose, so the UI
//! can localise it. Neither the service ticket, the Store ID key nor the
//! add-on key is ever logged.

use super::keycache::{CachedKey, KeyVault, NoVault};
use super::keyclient::{self, KeyError};
use super::{bundle, verifier};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashSet;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::SystemTime;

/// An add-on this build knows how to sell. Ids are fixed at build time so a
/// compromised server can never ask the Store to purchase an arbitrary product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownAddon {
    pub id: &'static str,
    pub store_id: &'static str,
}

pub const KNOWN_ADDONS: &[KnownAddon] = &[KnownAddon {
    id: "mothman",
    store_id: "9NGWPL7T7LV7",
}];

pub fn known_addon(id: &str) -> Option<&'static KnownAddon> {
    KNOWN_ADDONS.iter().find(|a| a.id == id)
}

/// Lifecycle state, serialised as the frontend's `AddonState`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AddonState {
    Unavailable,
    Unowned,
    Purchasing,
    Downloading,
    Owned,
    Error,
}

/// Machine-readable failure codes carried in `addon-state-changed`.
pub mod error_code {
    pub const STORE: &str = "store";
    pub const NOT_ENTITLED: &str = "not_entitled";
    pub const UPSTREAM: &str = "upstream";
    pub const RATE_LIMITED: &str = "rate_limited";
    pub const OFFLINE: &str = "offline";
    pub const BUNDLE: &str = "bundle";
    pub const UNSUPPORTED_API: &str = "unsupported_api";
    pub const INTERNAL: &str = "internal";
    /// The remembered key has run out and nothing could be reached to renew it.
    pub const RECONFIRM: &str = "reconfirm";
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    /// The Store could not be reached or returned a failure.
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PurchaseOutcome {
    Purchased,
    /// The user closed the dialog or declined; not an error.
    Cancelled,
}

/// The Store's localised price for an add-on, as the signed-in user would pay it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductPrice {
    /// Display string straight from the Store, e.g. `$4.99`; never parsed for amounts.
    pub formatted: String,
    /// True when nothing is charged, so the card says "Get" rather than "Buy for Free".
    pub is_free: bool,
}

impl ProductPrice {
    /// `None` for an empty string. The Store exposes the price only as text,
    /// so "free" is read from it: no digits at all (the localised word for
    /// free) or digits that are all zero (`$0.00`, `0,00 $`).
    pub fn from_formatted(formatted: &str) -> Option<Self> {
        let formatted = formatted.trim();
        if formatted.is_empty() {
            return None;
        }
        let is_free = formatted
            .chars()
            .filter(char::is_ascii_digit)
            .all(|c| c == '0');
        Some(Self {
            formatted: formatted.to_string(),
            is_free,
        })
    }
}

/// The slice of the Store the manager needs. Blocking: the manager runs every
/// call on a blocking thread, which is also what the WinRT `.join()` needs.
pub trait StoreBackend: Send + Sync {
    /// False off-Windows and when the app has no package identity.
    fn is_available(&self) -> bool;
    fn is_owned(&self, store_id: &str) -> Result<bool, StoreError>;
    /// The Store's price for the add-on, or `None` when it reports none. A
    /// build with no Store has no price to show.
    fn price(&self, _store_id: &str) -> Result<Option<ProductPrice>, StoreError> {
        Ok(None)
    }
    /// `hwnd` parents the purchase dialog to the main window.
    fn purchase(&self, store_id: &str, hwnd: isize) -> Result<PurchaseOutcome, StoreError>;
    /// The user's Store ID key for the key-release Worker.
    fn store_id_key(
        &self,
        service_ticket: &str,
        publisher_user_id: &str,
    ) -> Result<String, StoreError>;
}

/// Backend for builds with no Store.
pub struct Unavailable;

impl StoreBackend for Unavailable {
    fn is_available(&self) -> bool {
        false
    }
    fn is_owned(&self, _: &str) -> Result<bool, StoreError> {
        Ok(false)
    }
    fn purchase(&self, _: &str, _: isize) -> Result<PurchaseOutcome, StoreError> {
        Err(StoreError::Failed("the Store is not available".into()))
    }
    fn store_id_key(&self, _: &str, _: &str) -> Result<String, StoreError> {
        Err(StoreError::Failed("the Store is not available".into()))
    }
}

/// The backend for this build: the real Store when packaged on Windows,
/// [`Unavailable`] otherwise.
pub fn default_backend() -> Arc<dyn StoreBackend> {
    #[cfg(debug_assertions)]
    if let Some(fake) = fake::from_env() {
        return Arc::new(fake);
    }
    // Previewing an unpacked folder (#1426) has no Store to ask, so it owns.
    #[cfg(debug_assertions)]
    if super::devloader::dev_dir().is_some() {
        return Arc::new(fake::owned());
    }
    #[cfg(target_os = "windows")]
    {
        if crate::restart_manager::current_application_user_model_id().is_some() {
            return Arc::new(windows_store::WindowsStore);
        }
    }
    Arc::new(Unavailable)
}

/// What the overlay needs to register a theme, sent as `addon-theme-defined`
/// once a bundle is verified. Mirrors the frontend's `AddonTheme`.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThemeDefinition {
    pub id: String,
    pub name: String,
    pub colors: std::collections::HashMap<String, String>,
    pub overlay_entry: Option<String>,
    pub capabilities: Vec<String>,
}

impl From<&verifier::Manifest> for ThemeDefinition {
    fn from(m: &verifier::Manifest) -> Self {
        Self {
            id: m.id.clone(),
            name: m.name.clone(),
            colors: m.colors.clone(),
            overlay_entry: m.overlay_entry.clone(),
            capabilities: m.capabilities.clone(),
        }
    }
}

/// Outbound events, abstracted so tests can record them.
pub trait Events: Send + Sync {
    fn state(&self, id: &str, state: AddonState, error: Option<&str>);
    fn theme_defined(&self, theme: &ThemeDefinition);
    fn price_defined(&self, _id: &str, _price: &ProductPrice) {}
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

/// The network half of activation, abstracted so the flow can be tested
/// without a Worker or a CDN.
pub trait Provisioner: Send + Sync {
    fn service_ticket(&self) -> BoxFuture<Result<String, KeyError>>;
    fn release_key(
        &self,
        store_id_key: String,
        addin_id: String,
    ) -> BoxFuture<Result<[u8; 32], KeyError>>;
    fn install(
        &self,
        id: String,
        key: [u8; 32],
    ) -> BoxFuture<Result<verifier::Manifest, bundle::InstallError>>;
}

/// The provisioner for this build: the network, or in debug builds an unpacked
/// folder named by `LUMINOUS_ADDON_DEV_DIR` (#1426).
pub fn default_provisioner(cache_dir: PathBuf) -> Arc<dyn Provisioner> {
    #[cfg(debug_assertions)]
    if let Some(dir) = super::devloader::dev_dir() {
        return Arc::new(super::devloader::DevFolderProvisioner::new(dir));
    }
    Arc::new(NetworkProvisioner::new(cache_dir))
}

pub struct NetworkProvisioner {
    pub key_base_url: String,
    pub bundle_base_url: String,
    pub cache_dir: PathBuf,
}

impl NetworkProvisioner {
    pub fn new(cache_dir: PathBuf) -> Self {
        #[cfg_attr(not(debug_assertions), allow(unused_mut))]
        let mut p = Self {
            key_base_url: keyclient::DEFAULT_KEY_BASE_URL.to_string(),
            bundle_base_url: bundle::DEFAULT_BASE_URL.to_string(),
            cache_dir,
        };
        // Debug builds can point at a local mock; release builds never read these.
        #[cfg(debug_assertions)]
        {
            if let Ok(v) = std::env::var("LUMINOUS_ADDON_KEY_URL") {
                p.key_base_url = v;
            }
            if let Ok(v) = std::env::var("LUMINOUS_ADDON_BUNDLE_URL") {
                p.bundle_base_url = v;
            }
        }
        p
    }
}

impl Provisioner for NetworkProvisioner {
    fn service_ticket(&self) -> BoxFuture<Result<String, KeyError>> {
        let base = self.key_base_url.clone();
        Box::pin(async move { keyclient::fetch_service_ticket(&base).await })
    }

    fn release_key(
        &self,
        store_id_key: String,
        addin_id: String,
    ) -> BoxFuture<Result<[u8; 32], KeyError>> {
        let base = self.key_base_url.clone();
        Box::pin(async move { keyclient::release_key(&base, &store_id_key, &addin_id).await })
    }

    fn install(
        &self,
        id: String,
        key: [u8; 32],
    ) -> BoxFuture<Result<verifier::Manifest, bundle::InstallError>> {
        let base = self.bundle_base_url.clone();
        let dir = self.cache_dir.clone();
        Box::pin(async move { bundle::install(&base, &dir, &id, &key).await })
    }
}

fn key_error_code(e: &KeyError) -> &'static str {
    match e {
        KeyError::NotEntitled => error_code::NOT_ENTITLED,
        KeyError::RateLimited { .. } => error_code::RATE_LIMITED,
        KeyError::Upstream => error_code::UPSTREAM,
        KeyError::UnknownAddin | KeyError::BadRequest | KeyError::BadResponse => {
            error_code::INTERNAL
        }
    }
}

fn install_error_code(e: &bundle::InstallError) -> &'static str {
    match e {
        bundle::InstallError::Unreachable(_) => error_code::OFFLINE,
        bundle::InstallError::UnsupportedApi(_) => error_code::UNSUPPORTED_API,
        bundle::InstallError::InvalidId | bundle::InstallError::Io(_) => error_code::INTERNAL,
        bundle::InstallError::Verify(_) | bundle::InstallError::Rollback { .. } => {
            error_code::BUNDLE
        }
    }
}

/// Where the manager reads the time, so the key's 30 days can be tested.
pub type Clock = Arc<dyn Fn() -> SystemTime + Send + Sync>;

/// Drives refresh and acquire for every known add-on.
pub struct AddonManager {
    backend: Arc<dyn StoreBackend>,
    provisioner: Arc<dyn Provisioner>,
    events: Arc<dyn Events>,
    /// Released keys remembered for [`super::keycache::GRACE`] (#1429).
    keys: Arc<dyn KeyVault>,
    clock: Clock,
    /// Ids with a flow in flight, so a double click can't start two purchases.
    busy: Mutex<HashSet<String>>,
}

struct BusyGuard<'a> {
    set: &'a Mutex<HashSet<String>>,
    id: String,
}

impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.set.lock().remove(&self.id);
    }
}

impl AddonManager {
    pub fn new(
        backend: Arc<dyn StoreBackend>,
        provisioner: Arc<dyn Provisioner>,
        events: Arc<dyn Events>,
    ) -> Self {
        Self {
            backend,
            provisioner,
            events,
            keys: Arc::new(NoVault),
            clock: Arc::new(SystemTime::now),
            busy: Mutex::new(HashSet::new()),
        }
    }

    /// Remember released keys in `keys`; without one every launch asks the Worker.
    pub fn with_key_vault(mut self, keys: Arc<dyn KeyVault>) -> Self {
        self.keys = keys;
        self
    }

    #[cfg(test)]
    fn with_clock(mut self, clock: Clock) -> Self {
        self.clock = clock;
        self
    }

    /// A remembered key that is still inside its 30 days.
    fn fresh_key(&self, id: &str) -> Option<CachedKey> {
        self.keys.load(id).filter(|c| c.is_fresh((self.clock)()))
    }

    pub fn is_available(&self) -> bool {
        self.backend.is_available()
    }

    fn claim(&self, id: &str) -> Option<BusyGuard<'_>> {
        self.busy.lock().insert(id.to_string()).then(|| BusyGuard {
            set: &self.busy,
            id: id.to_string(),
        })
    }

    fn emit(&self, id: &str, state: AddonState) {
        self.events.state(id, state, None);
    }

    fn fail(&self, id: &str, code: &str) {
        self.events.state(id, AddonState::Error, Some(code));
    }

    /// Announce the state of every known add-on, activating the owned ones.
    /// Called at startup and whenever the Settings view asks.
    pub async fn refresh_all(self: &Arc<Self>) {
        for addon in KNOWN_ADDONS {
            self.refresh(addon.id).await;
        }
    }

    /// Re-check one add-on's ownership and move it to the matching state.
    pub async fn refresh(self: &Arc<Self>, id: &str) {
        let Some(addon) = known_addon(id) else {
            return;
        };
        if !self.backend.is_available() {
            self.emit(id, AddonState::Unavailable);
            return;
        }
        let Some(_guard) = self.claim(id) else {
            return;
        };
        match self.owned(addon).await {
            Ok(true) => self.activate(addon).await,
            Ok(false) => {
                super::unregister(id);
                self.keys.forget(id);
                self.announce_price(addon).await;
                self.emit(id, AddonState::Unowned);
            }
            Err(e) => {
                log::warn!("add-on {id}: ownership check failed: {e:?}");
                // Could not ask the Store (offline, say): a key remembered
                // within its 30 days stands in for the answer.
                if self.fresh_key(id).is_some() {
                    self.activate(addon).await;
                } else {
                    self.fail(id, error_code::STORE);
                }
            }
        }
    }

    /// Open the Store purchase dialog; activate on success.
    pub async fn acquire(self: &Arc<Self>, id: &str, hwnd: isize) {
        let Some(addon) = known_addon(id) else {
            return;
        };
        if !self.backend.is_available() {
            self.emit(id, AddonState::Unavailable);
            return;
        }
        let Some(_guard) = self.claim(id) else {
            return;
        };
        self.emit(id, AddonState::Purchasing);
        let backend = Arc::clone(&self.backend);
        let outcome = tokio::task::spawn_blocking(move || backend.purchase(addon.store_id, hwnd))
            .await
            .unwrap_or_else(|e| Err(StoreError::Failed(e.to_string())));
        match outcome {
            Ok(PurchaseOutcome::Purchased) => self.activate(addon).await,
            Ok(PurchaseOutcome::Cancelled) => self.emit(id, AddonState::Unowned),
            Err(e) => {
                log::warn!("add-on {}: purchase failed: {e:?}", addon.id);
                self.fail(addon.id, error_code::STORE);
            }
        }
    }

    /// Tell the UI what the add-on costs. A failed lookup only means the card
    /// keeps its plain label, so it is logged and otherwise ignored.
    async fn announce_price(&self, addon: &KnownAddon) {
        let backend = Arc::clone(&self.backend);
        let store_id = addon.store_id;
        let looked_up = tokio::task::spawn_blocking(move || backend.price(store_id))
            .await
            .unwrap_or_else(|e| Err(StoreError::Failed(e.to_string())));
        match looked_up {
            Ok(Some(price)) => self.events.price_defined(addon.id, &price),
            Ok(None) => {}
            Err(e) => log::warn!("add-on {}: price lookup failed: {e:?}", addon.id),
        }
    }

    async fn owned(&self, addon: &KnownAddon) -> Result<bool, StoreError> {
        let backend = Arc::clone(&self.backend);
        let store_id = addon.store_id;
        tokio::task::spawn_blocking(move || backend.is_owned(store_id))
            .await
            .unwrap_or_else(|e| Err(StoreError::Failed(e.to_string())))
    }

    /// Ticket → Store ID key → add-on key → bundle → register → announce.
    async fn activate(&self, addon: &KnownAddon) {
        let id = addon.id;
        self.emit(id, AddonState::Downloading);
        match self.provision(id).await {
            Ok(manifest) => {
                self.events.theme_defined(&ThemeDefinition::from(&manifest));
                self.emit(id, AddonState::Owned);
            }
            Err(code) => {
                // A failed key release must not leave a stale registration servable.
                if code == error_code::NOT_ENTITLED {
                    super::unregister(id);
                }
                self.fail(id, code);
            }
        }
    }

    /// Remembered key → bundle, or ticket → Store ID key → add-on key → bundle.
    async fn provision(&self, id: &str) -> Result<verifier::Manifest, &'static str> {
        let now = (self.clock)();
        let remembered = self.keys.load(id);
        let ran_out = remembered.as_ref().is_some_and(|c| !c.is_fresh(now));

        if let Some(cached) = remembered.filter(|c| c.is_fresh(now)) {
            match self.provisioner.install(id.to_string(), cached.key).await {
                Ok(manifest) => return Ok(manifest),
                // The key no longer fits the bundle (it was replaced on the
                // Worker): forget it and ask again.
                Err(bundle::InstallError::Verify(e)) => {
                    log::warn!("add-on {id}: remembered key rejected ({e}), asking the Worker");
                    self.keys.forget(id);
                }
                Err(e) => {
                    log::warn!("add-on {id}: install failed: {e}");
                    return Err(install_error_code(&e));
                }
            }
        }

        let key = match self.release_key(id).await {
            Ok(key) => key,
            Err(code) => {
                if code == error_code::NOT_ENTITLED {
                    self.keys.forget(id);
                }
                let unreachable = code == error_code::UPSTREAM || code == error_code::STORE;
                return Err(if ran_out && unreachable {
                    error_code::RECONFIRM
                } else {
                    code
                });
            }
        };
        self.keys.save(
            id,
            &CachedKey {
                key,
                checked_at: now,
            },
        );

        self.provisioner
            .install(id.to_string(), key)
            .await
            .map_err(|e| {
                log::warn!("add-on {id}: install failed: {e}");
                install_error_code(&e)
            })
    }

    /// Ticket → Store ID key → add-on key, each step on its own error code.
    async fn release_key(&self, id: &str) -> Result<[u8; 32], &'static str> {
        let ticket = self.provisioner.service_ticket().await.map_err(|e| {
            log::warn!("add-on {id}: service ticket failed: {e}");
            key_error_code(&e)
        })?;

        let backend = Arc::clone(&self.backend);
        let publisher_user_id = publisher_user_id();
        let store_id_key =
            tokio::task::spawn_blocking(move || backend.store_id_key(&ticket, &publisher_user_id))
                .await
                .unwrap_or_else(|e| Err(StoreError::Failed(e.to_string())))
                .map_err(|e| {
                    log::warn!("add-on {id}: Store ID key failed: {e:?}");
                    error_code::STORE
                })?;

        self.provisioner
            .release_key(store_id_key, id.to_string())
            .await
            .map_err(|e| {
                log::warn!("add-on {id}: key release failed: {e}");
                key_error_code(&e)
            })
    }
}

/// An opaque per-install identifier the Store ties the Store ID key to. It
/// carries no account information.
fn publisher_user_id() -> String {
    "luminous".to_string()
}

#[cfg(target_os = "windows")]
mod windows_store {
    use super::{ProductPrice, PurchaseOutcome, StoreBackend, StoreError};
    use windows::core::{Interface, HSTRING};
    use windows::Services::Store::{StoreContext, StorePurchaseStatus};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::IInitializeWithWindow;
    use windows_collections::IIterable;

    pub struct WindowsStore;

    fn failed(e: windows::core::Error) -> StoreError {
        StoreError::Failed(format!("{e}"))
    }

    fn context(hwnd: Option<isize>) -> Result<StoreContext, StoreError> {
        let ctx = StoreContext::GetDefault().map_err(failed)?;
        if let Some(hwnd) = hwnd {
            // Desktop apps must parent Store dialogs to a window themselves.
            let init: IInitializeWithWindow = ctx.cast().map_err(failed)?;
            unsafe { init.Initialize(HWND(hwnd as *mut _)) }.map_err(failed)?;
        }
        Ok(ctx)
    }

    impl StoreBackend for WindowsStore {
        fn is_available(&self) -> bool {
            true
        }

        fn is_owned(&self, store_id: &str) -> Result<bool, StoreError> {
            let ctx = context(None)?;
            let kinds = IIterable::<HSTRING>::from(vec![HSTRING::from("Durable")]);
            let result = ctx
                .GetUserCollectionAsync(&kinds)
                .map_err(failed)?
                .join()
                .map_err(failed)?;
            if let Ok(err) = result.ExtendedError() {
                if err.is_err() {
                    return Err(StoreError::Failed(format!("{err}")));
                }
            }
            let products = result.Products().map_err(failed)?;
            for entry in products {
                let product = entry.Value().map_err(failed)?;
                if product.StoreId().map_err(failed)?.to_string_lossy() == store_id {
                    return Ok(true);
                }
            }
            Ok(false)
        }

        fn price(&self, store_id: &str) -> Result<Option<ProductPrice>, StoreError> {
            let ctx = context(None)?;
            let kinds = IIterable::<HSTRING>::from(vec![HSTRING::from("Durable")]);
            let ids = IIterable::<HSTRING>::from(vec![HSTRING::from(store_id)]);
            let result = ctx
                .GetStoreProductsAsync(&kinds, &ids)
                .map_err(failed)?
                .join()
                .map_err(failed)?;
            if let Ok(err) = result.ExtendedError() {
                if err.is_err() {
                    return Err(StoreError::Failed(format!("{err}")));
                }
            }
            for entry in result.Products().map_err(failed)? {
                let product = entry.Value().map_err(failed)?;
                if product.StoreId().map_err(failed)?.to_string_lossy() == store_id {
                    let formatted = product
                        .Price()
                        .map_err(failed)?
                        .FormattedPrice()
                        .map_err(failed)?
                        .to_string_lossy();
                    return Ok(ProductPrice::from_formatted(&formatted));
                }
            }
            Ok(None)
        }

        fn purchase(&self, store_id: &str, hwnd: isize) -> Result<PurchaseOutcome, StoreError> {
            let ctx = context(Some(hwnd))?;
            let result = ctx
                .RequestPurchaseAsync(&HSTRING::from(store_id))
                .map_err(failed)?
                .join()
                .map_err(failed)?;
            map_purchase_status(result.Status().map_err(failed)?)
        }

        fn store_id_key(
            &self,
            service_ticket: &str,
            publisher_user_id: &str,
        ) -> Result<String, StoreError> {
            let ctx = context(None)?;
            let key = ctx
                .GetCustomerCollectionsIdAsync(
                    &HSTRING::from(service_ticket),
                    &HSTRING::from(publisher_user_id),
                )
                .map_err(failed)?
                .join()
                .map_err(failed)?;
            Ok(key.to_string_lossy())
        }
    }

    fn map_purchase_status(status: StorePurchaseStatus) -> Result<PurchaseOutcome, StoreError> {
        match status {
            StorePurchaseStatus::Succeeded | StorePurchaseStatus::AlreadyPurchased => {
                Ok(PurchaseOutcome::Purchased)
            }
            StorePurchaseStatus::NotPurchased => Ok(PurchaseOutcome::Cancelled),
            other => Err(StoreError::Failed(format!("purchase status {}", other.0))),
        }
    }
}

/// Debug-only stand-in so the rest of the app can be exercised without the
/// Store. Compiled out of release builds. Set `LUMINOUS_ADDON_FAKE_STORE` to
/// `owned` or `unowned`; a purchase flips `unowned` to owned.
#[cfg(debug_assertions)]
mod fake {
    use super::{ProductPrice, PurchaseOutcome, StoreBackend, StoreError};
    use std::sync::atomic::{AtomicBool, Ordering};

    pub struct FakeStore {
        owned: AtomicBool,
    }

    pub fn owned() -> FakeStore {
        FakeStore {
            owned: AtomicBool::new(true),
        }
    }

    pub fn from_env() -> Option<FakeStore> {
        match std::env::var("LUMINOUS_ADDON_FAKE_STORE").ok()?.as_str() {
            "owned" => Some(FakeStore {
                owned: AtomicBool::new(true),
            }),
            "unowned" => Some(FakeStore {
                owned: AtomicBool::new(false),
            }),
            _ => None,
        }
    }

    impl StoreBackend for FakeStore {
        fn is_available(&self) -> bool {
            true
        }
        fn is_owned(&self, _: &str) -> Result<bool, StoreError> {
            Ok(self.owned.load(Ordering::SeqCst))
        }
        /// `LUMINOUS_ADDON_FAKE_PRICE` (e.g. `$4.99`) stands in for the Store's price.
        fn price(&self, _: &str) -> Result<Option<ProductPrice>, StoreError> {
            Ok(std::env::var("LUMINOUS_ADDON_FAKE_PRICE")
                .ok()
                .and_then(|p| ProductPrice::from_formatted(&p)))
        }

        fn purchase(&self, _: &str, _: isize) -> Result<PurchaseOutcome, StoreError> {
            self.owned.store(true, Ordering::SeqCst);
            Ok(PurchaseOutcome::Purchased)
        }
        fn store_id_key(&self, _: &str, _: &str) -> Result<String, StoreError> {
            Ok("fake-store-id-key".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::keycache::GRACE;
    use super::*;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[derive(Default)]
    struct Recorder {
        states: Mutex<Vec<(String, AddonState, Option<String>)>>,
        themes: Mutex<Vec<ThemeDefinition>>,
        prices: Mutex<Vec<(String, ProductPrice)>>,
    }

    impl Events for Recorder {
        fn state(&self, id: &str, state: AddonState, error: Option<&str>) {
            self.states
                .lock()
                .push((id.to_string(), state, error.map(str::to_string)));
        }
        fn theme_defined(&self, theme: &ThemeDefinition) {
            self.themes.lock().push(theme.clone());
        }
        fn price_defined(&self, id: &str, price: &ProductPrice) {
            self.prices.lock().push((id.to_string(), price.clone()));
        }
    }

    impl Recorder {
        fn trail(&self) -> Vec<(AddonState, Option<String>)> {
            self.states
                .lock()
                .iter()
                .map(|(_, s, e)| (*s, e.clone()))
                .collect()
        }
    }

    struct FakeBackend {
        available: bool,
        owned: Result<bool, StoreError>,
        purchase: Result<PurchaseOutcome, StoreError>,
        key: Result<String, StoreError>,
        price: Result<Option<ProductPrice>, StoreError>,
        purchases: AtomicUsize,
    }

    impl Default for FakeBackend {
        fn default() -> Self {
            Self {
                available: true,
                owned: Ok(true),
                purchase: Ok(PurchaseOutcome::Purchased),
                key: Ok("sik".into()),
                price: Ok(None),
                purchases: AtomicUsize::new(0),
            }
        }
    }

    impl StoreBackend for FakeBackend {
        fn is_available(&self) -> bool {
            self.available
        }
        fn is_owned(&self, _: &str) -> Result<bool, StoreError> {
            self.owned.clone()
        }
        fn purchase(&self, _: &str, _: isize) -> Result<PurchaseOutcome, StoreError> {
            self.purchases.fetch_add(1, Ordering::SeqCst);
            self.purchase.clone()
        }
        fn price(&self, _: &str) -> Result<Option<ProductPrice>, StoreError> {
            self.price.clone()
        }
        fn store_id_key(&self, _: &str, _: &str) -> Result<String, StoreError> {
            self.key.clone()
        }
    }

    struct FakeNet {
        ticket: Result<String, fn() -> KeyError>,
        key: Result<[u8; 32], fn() -> KeyError>,
        install: Result<(), fn() -> bundle::InstallError>,
        /// A key the bundle no longer decrypts with, as after a key change on the Worker.
        rejects: Option<[u8; 32]>,
        /// Shared so a test can still read them once the net is inside the manager.
        tickets: Arc<AtomicUsize>,
        releases: Arc<AtomicUsize>,
        installed_with: Arc<Mutex<Vec<[u8; 32]>>>,
    }

    impl Default for FakeNet {
        fn default() -> Self {
            Self {
                ticket: Ok("t".into()),
                key: Ok([7; 32]),
                install: Ok(()),
                rejects: None,
                tickets: Arc::default(),
                releases: Arc::default(),
                installed_with: Arc::default(),
            }
        }
    }

    fn manifest(id: &str) -> verifier::Manifest {
        serde_json::from_value(serde_json::json!({
            "schemaVersion": 1, "id": id, "name": "Mothman", "version": "1.0.0",
            "storeProductId": "9NGWPL7T7LV7", "minApiVersion": 1,
            "colors": {"color-accent": "#fff"}, "overlayEntry": "overlay.html",
            "assets": [], "capabilities": ["spectrum"]
        }))
        .unwrap()
    }

    impl Provisioner for FakeNet {
        fn service_ticket(&self) -> BoxFuture<Result<String, KeyError>> {
            self.tickets.fetch_add(1, Ordering::SeqCst);
            let r = self.ticket.clone().map_err(|f| f());
            Box::pin(async move { r })
        }
        fn release_key(&self, _: String, _: String) -> BoxFuture<Result<[u8; 32], KeyError>> {
            self.releases.fetch_add(1, Ordering::SeqCst);
            let r = self.key.map_err(|f| f());
            Box::pin(async move { r })
        }
        fn install(
            &self,
            id: String,
            key: [u8; 32],
        ) -> BoxFuture<Result<verifier::Manifest, bundle::InstallError>> {
            self.installed_with.lock().push(key);
            let r = if self.rejects == Some(key) {
                Err(bundle::InstallError::Verify(verifier::Error::DecryptFailed))
            } else {
                self.install.map(|_| manifest(&id)).map_err(|f| f())
            };
            Box::pin(async move { r })
        }
    }

    fn manager(b: FakeBackend, n: FakeNet) -> (Arc<AddonManager>, Arc<Recorder>) {
        let rec = Arc::new(Recorder::default());
        let m = Arc::new(AddonManager::new(
            Arc::new(b),
            Arc::new(n),
            rec.clone() as Arc<dyn Events>,
        ));
        (m, rec)
    }

    /// A key vault in memory; the real one is covered in keycache.rs.
    #[derive(Default)]
    struct MemVault(Mutex<HashMap<String, CachedKey>>);

    impl KeyVault for MemVault {
        fn load(&self, id: &str) -> Option<CachedKey> {
            self.0.lock().get(id).cloned()
        }
        fn save(&self, id: &str, key: &CachedKey) {
            self.0.lock().insert(id.to_string(), key.clone());
        }
        fn forget(&self, id: &str) {
            self.0.lock().remove(id);
        }
    }

    /// A clock the test moves by hand; every date in these tests comes from it.
    struct Clock0(Arc<Mutex<SystemTime>>);

    impl Clock0 {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000),
            )))
        }
        fn now(&self) -> SystemTime {
            *self.0.lock()
        }
        fn advance(&self, by: Duration) {
            *self.0.lock() += by;
        }
        fn clock(&self) -> Clock {
            let shared = Arc::clone(&self.0);
            Arc::new(move || *shared.lock())
        }
    }

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn remembering_manager(
        b: FakeBackend,
        n: FakeNet,
        vault: &Arc<MemVault>,
        clock: &Clock0,
    ) -> (Arc<AddonManager>, Arc<Recorder>) {
        let rec = Arc::new(Recorder::default());
        let m = Arc::new(
            AddonManager::new(Arc::new(b), Arc::new(n), rec.clone() as Arc<dyn Events>)
                .with_key_vault(vault.clone() as Arc<dyn KeyVault>)
                .with_clock(clock.clock()),
        );
        (m, rec)
    }

    #[test]
    fn mothman_is_known_and_unknown_ids_are_not() {
        assert_eq!(known_addon("mothman").unwrap().store_id, "9NGWPL7T7LV7");
        assert!(known_addon("../etc").is_none());
    }

    #[test]
    fn unavailable_backend_reports_nothing_owned() {
        let u = Unavailable;
        assert!(!u.is_available());
        assert_eq!(u.is_owned("x"), Ok(false));
        assert!(u.purchase("x", 0).is_err());
    }

    #[tokio::test]
    async fn unavailable_platform_reports_unavailable() {
        let (m, rec) = manager(
            FakeBackend {
                available: false,
                ..Default::default()
            },
            FakeNet::default(),
        );
        m.refresh("mothman").await;
        assert_eq!(rec.trail(), vec![(AddonState::Unavailable, None)]);
    }

    #[tokio::test]
    async fn owned_addon_downloads_then_becomes_owned_and_defines_its_theme() {
        let (m, rec) = manager(FakeBackend::default(), FakeNet::default());
        m.refresh("mothman").await;
        assert_eq!(
            rec.trail(),
            vec![(AddonState::Downloading, None), (AddonState::Owned, None)]
        );
        let themes = rec.themes.lock();
        assert_eq!(themes[0].id, "mothman");
        assert_eq!(themes[0].overlay_entry.as_deref(), Some("overlay.html"));
        assert_eq!(themes[0].capabilities, vec!["spectrum".to_string()]);
    }

    #[tokio::test]
    async fn unowned_addon_is_unowned() {
        let (m, rec) = manager(
            FakeBackend {
                owned: Ok(false),
                ..Default::default()
            },
            FakeNet::default(),
        );
        m.refresh("mothman").await;
        assert_eq!(rec.trail(), vec![(AddonState::Unowned, None)]);
    }

    #[test]
    fn price_is_free_when_the_text_has_no_non_zero_digit() {
        for free in ["Free", "Gratuit", "$0.00", "0,00 $", " 0 "] {
            assert!(
                ProductPrice::from_formatted(free).unwrap().is_free,
                "{free}"
            );
        }
        for paid in ["$4.99", "4,99 $", "\u{20ac}10", "CA$1.00"] {
            assert!(
                !ProductPrice::from_formatted(paid).unwrap().is_free,
                "{paid}"
            );
        }
        assert_eq!(ProductPrice::from_formatted("  "), None);
        assert_eq!(
            ProductPrice::from_formatted(" $4.99 ").unwrap().formatted,
            "$4.99"
        );
    }

    #[tokio::test]
    async fn unowned_addon_announces_the_store_price_before_the_state() {
        let (m, rec) = manager(
            FakeBackend {
                owned: Ok(false),
                price: Ok(ProductPrice::from_formatted("$4.99")),
                ..Default::default()
            },
            FakeNet::default(),
        );
        m.refresh("mothman").await;
        assert_eq!(
            rec.prices.lock().as_slice(),
            &[(
                "mothman".to_string(),
                ProductPrice::from_formatted("$4.99").unwrap()
            )]
        );
        assert_eq!(rec.trail(), vec![(AddonState::Unowned, None)]);
    }

    #[tokio::test]
    async fn failed_or_missing_price_still_ends_unowned_without_a_price() {
        for price in [Err(StoreError::Failed("down".into())), Ok(None)] {
            let (m, rec) = manager(
                FakeBackend {
                    owned: Ok(false),
                    price,
                    ..Default::default()
                },
                FakeNet::default(),
            );
            m.refresh("mothman").await;
            assert!(rec.prices.lock().is_empty());
            assert_eq!(rec.trail(), vec![(AddonState::Unowned, None)]);
        }
    }

    #[tokio::test]
    async fn owned_addon_does_not_look_up_a_price() {
        let (m, rec) = manager(
            FakeBackend {
                price: Ok(ProductPrice::from_formatted("$4.99")),
                ..Default::default()
            },
            FakeNet::default(),
        );
        m.refresh("mothman").await;
        assert!(rec.prices.lock().is_empty());
    }

    #[tokio::test]
    async fn store_failure_is_an_error_not_unowned() {
        let (m, rec) = manager(
            FakeBackend {
                owned: Err(StoreError::Failed("offline".into())),
                ..Default::default()
            },
            FakeNet::default(),
        );
        m.refresh("mothman").await;
        assert_eq!(
            rec.trail(),
            vec![(AddonState::Error, Some(error_code::STORE.into()))]
        );
    }

    #[tokio::test]
    async fn purchase_success_activates() {
        let (m, rec) = manager(FakeBackend::default(), FakeNet::default());
        m.acquire("mothman", 1).await;
        assert_eq!(
            rec.trail(),
            vec![
                (AddonState::Purchasing, None),
                (AddonState::Downloading, None),
                (AddonState::Owned, None)
            ]
        );
    }

    #[tokio::test]
    async fn cancelled_purchase_returns_to_unowned_without_error() {
        let (m, rec) = manager(
            FakeBackend {
                purchase: Ok(PurchaseOutcome::Cancelled),
                ..Default::default()
            },
            FakeNet::default(),
        );
        m.acquire("mothman", 1).await;
        assert_eq!(
            rec.trail(),
            vec![(AddonState::Purchasing, None), (AddonState::Unowned, None)]
        );
    }

    #[tokio::test]
    async fn failed_purchase_is_an_error() {
        let (m, rec) = manager(
            FakeBackend {
                purchase: Err(StoreError::Failed("network".into())),
                ..Default::default()
            },
            FakeNet::default(),
        );
        m.acquire("mothman", 1).await;
        assert_eq!(
            rec.trail().last(),
            Some(&(AddonState::Error, Some(error_code::STORE.into())))
        );
    }

    type Case = (fn() -> KeyError, &'static str);

    #[tokio::test]
    async fn key_release_failures_map_to_distinct_codes() {
        let cases: [Case; 3] = [
            (|| KeyError::NotEntitled, error_code::NOT_ENTITLED),
            (|| KeyError::Upstream, error_code::UPSTREAM),
            (
                || KeyError::RateLimited {
                    retry_after_secs: None,
                },
                error_code::RATE_LIMITED,
            ),
        ];
        for (make, code) in cases {
            let (m, rec) = manager(
                FakeBackend::default(),
                FakeNet {
                    key: Err(make),
                    ..Default::default()
                },
            );
            m.refresh("mothman").await;
            assert_eq!(
                rec.trail().last(),
                Some(&(AddonState::Error, Some(code.into())))
            );
        }
    }

    #[tokio::test]
    async fn unreachable_bundle_without_cache_is_offline() {
        let (m, rec) = manager(
            FakeBackend::default(),
            FakeNet {
                install: Err(|| bundle::InstallError::Unreachable("dns".into())),
                ..Default::default()
            },
        );
        m.refresh("mothman").await;
        assert_eq!(
            rec.trail().last(),
            Some(&(AddonState::Error, Some(error_code::OFFLINE.into())))
        );
    }

    #[tokio::test]
    async fn a_remembered_key_spares_the_worker_for_the_grace_period() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let net = FakeNet::default();
        let (tickets, releases) = (net.tickets.clone(), net.releases.clone());
        let (m, rec) = remembering_manager(FakeBackend::default(), net, &vault, &clock);

        m.refresh("mothman").await;
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(
            vault.load("mothman"),
            Some(CachedKey {
                key: [7; 32],
                checked_at: clock.now()
            })
        );

        clock.advance(GRACE);
        m.refresh("mothman").await;
        // Still owned, without asking the Worker again.
        assert_eq!(tickets.load(Ordering::SeqCst), 1);
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(rec.trail().last(), Some(&(AddonState::Owned, None)));
    }

    #[tokio::test]
    async fn after_the_grace_period_a_fresh_key_is_fetched_and_the_clock_resets() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let net = FakeNet::default();
        let releases = net.releases.clone();
        let (m, _) = remembering_manager(FakeBackend::default(), net, &vault, &clock);

        m.refresh("mothman").await;
        clock.advance(GRACE + Duration::from_secs(1));
        m.refresh("mothman").await;
        assert_eq!(releases.load(Ordering::SeqCst), 2);
        assert_eq!(vault.load("mothman").unwrap().checked_at, clock.now());
    }

    #[tokio::test]
    async fn without_a_remembered_key_every_launch_asks_the_worker() {
        let clock = Clock0::new();
        let net = FakeNet::default();
        let releases = net.releases.clone();
        let rec = Arc::new(Recorder::default());
        let m = Arc::new(
            AddonManager::new(
                Arc::new(FakeBackend::default()),
                Arc::new(net),
                rec.clone() as Arc<dyn Events>,
            )
            .with_clock(clock.clock()),
        );
        m.refresh("mothman").await;
        m.refresh("mothman").await;
        assert_eq!(releases.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn a_store_that_cannot_be_asked_still_serves_a_fresh_remembered_key() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let (online, _) =
            remembering_manager(FakeBackend::default(), FakeNet::default(), &vault, &clock);
        online.refresh("mothman").await;

        clock.advance(10 * DAY);
        let net = FakeNet::default();
        let releases = net.releases.clone();
        let (offline, rec) = remembering_manager(
            FakeBackend {
                owned: Err(StoreError::Failed("offline".into())),
                ..Default::default()
            },
            net,
            &vault,
            &clock,
        );
        offline.refresh("mothman").await;
        assert_eq!(rec.trail().last(), Some(&(AddonState::Owned, None)));
        assert_eq!(releases.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_store_that_cannot_be_asked_with_no_remembered_key_is_a_store_error() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let (m, rec) = remembering_manager(
            FakeBackend {
                owned: Err(StoreError::Failed("offline".into())),
                ..Default::default()
            },
            FakeNet::default(),
            &vault,
            &clock,
        );
        m.refresh("mothman").await;
        assert_eq!(
            rec.trail().last(),
            Some(&(AddonState::Error, Some(error_code::STORE.into())))
        );
    }

    #[tokio::test]
    async fn a_run_out_key_with_nothing_reachable_asks_the_user_to_reconfirm() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let (online, _) =
            remembering_manager(FakeBackend::default(), FakeNet::default(), &vault, &clock);
        online.refresh("mothman").await;

        clock.advance(GRACE + DAY);
        for ticket_error in [|| KeyError::Upstream] {
            let (m, rec) = remembering_manager(
                FakeBackend::default(),
                FakeNet {
                    ticket: Err(ticket_error),
                    ..Default::default()
                },
                &vault,
                &clock,
            );
            m.refresh("mothman").await;
            assert_eq!(
                rec.trail().last(),
                Some(&(AddonState::Error, Some(error_code::RECONFIRM.into())))
            );
        }
    }

    #[tokio::test]
    async fn an_unreachable_worker_with_no_remembered_key_keeps_its_plain_error() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let (m, rec) = remembering_manager(
            FakeBackend::default(),
            FakeNet {
                ticket: Err(|| KeyError::Upstream),
                ..Default::default()
            },
            &vault,
            &clock,
        );
        m.refresh("mothman").await;
        assert_eq!(
            rec.trail().last(),
            Some(&(AddonState::Error, Some(error_code::UPSTREAM.into())))
        );
    }

    #[tokio::test]
    async fn learning_the_add_on_is_not_owned_forgets_the_key() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let (online, _) =
            remembering_manager(FakeBackend::default(), FakeNet::default(), &vault, &clock);
        online.refresh("mothman").await;
        assert!(vault.load("mothman").is_some());

        let (refunded, _) = remembering_manager(
            FakeBackend {
                owned: Ok(false),
                ..Default::default()
            },
            FakeNet::default(),
            &vault,
            &clock,
        );
        refunded.refresh("mothman").await;
        assert_eq!(vault.load("mothman"), None);
    }

    #[tokio::test]
    async fn a_worker_that_refuses_the_account_forgets_the_key() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let (online, _) =
            remembering_manager(FakeBackend::default(), FakeNet::default(), &vault, &clock);
        online.refresh("mothman").await;

        clock.advance(GRACE + DAY);
        let (m, _) = remembering_manager(
            FakeBackend::default(),
            FakeNet {
                key: Err(|| KeyError::NotEntitled),
                ..Default::default()
            },
            &vault,
            &clock,
        );
        m.refresh("mothman").await;
        assert_eq!(vault.load("mothman"), None);
    }

    #[tokio::test]
    async fn a_remembered_key_the_bundle_rejects_is_replaced_by_a_fresh_one() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        vault.save(
            "mothman",
            &CachedKey {
                key: [9; 32],
                checked_at: clock.now(),
            },
        );
        let net = FakeNet {
            rejects: Some([9; 32]),
            ..Default::default()
        };
        let (releases, installed_with) = (net.releases.clone(), net.installed_with.clone());
        let (m, rec) = remembering_manager(FakeBackend::default(), net, &vault, &clock);
        m.refresh("mothman").await;
        assert_eq!(rec.trail().last(), Some(&(AddonState::Owned, None)));
        assert_eq!(releases.load(Ordering::SeqCst), 1);
        assert_eq!(installed_with.lock().as_slice(), &[[9; 32], [7; 32]]);
        assert_eq!(vault.load("mothman").unwrap().key, [7; 32]);
    }

    #[tokio::test]
    async fn a_clock_set_back_does_not_stretch_the_grace_period() {
        let vault = Arc::new(MemVault::default());
        let clock = Clock0::new();
        let stamped = clock.now() + 10 * DAY;
        vault.save(
            "mothman",
            &CachedKey {
                key: [9; 32],
                checked_at: stamped,
            },
        );
        let net = FakeNet::default();
        let releases = net.releases.clone();
        let (m, _) = remembering_manager(FakeBackend::default(), net, &vault, &clock);
        m.refresh("mothman").await;
        assert_eq!(releases.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unknown_ids_are_ignored() {
        let (m, rec) = manager(FakeBackend::default(), FakeNet::default());
        m.refresh("nope").await;
        m.acquire("nope", 1).await;
        assert!(rec.trail().is_empty());
    }

    #[tokio::test]
    async fn a_second_flow_for_the_same_id_is_ignored_while_one_is_running() {
        let (m, rec) = manager(FakeBackend::default(), FakeNet::default());
        let guard = m.claim("mothman").unwrap();
        m.acquire("mothman", 1).await;
        // The in-flight flow already announced `Purchasing`; the duplicate adds nothing.
        assert!(rec.trail().is_empty());
        drop(guard);
        m.acquire("mothman", 1).await;
        assert!(!rec.trail().is_empty());
    }
}
