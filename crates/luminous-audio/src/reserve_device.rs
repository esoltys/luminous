//! Sound card reservation over the session bus (Linux).
//!
//! The freedesktop device reservation protocol (`org.freedesktop.ReserveDevice1`)
//! is how a sound server and an app opening a card directly agree on who has
//! it: whoever owns the well-known name `org.freedesktop.ReserveDevice1.Audio<N>`
//! owns card `N`. WirePlumber holds that name for every card at all times (at
//! priority −20) and, when asked through `RequestRelease`, tears down its use
//! of the card and lets the name go.
//!
//! Direct Hardware (ALSA) output reserves its card before opening the `hw:`
//! PCM, so it can take a card the sound server is playing on and hand it back
//! afterwards. A reservation outlives its sink by a short grace period, so the
//! reopen a rate change makes doesn't give the card back to the sound server
//! (which would rebuild its device) only to take it again.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use parking_lot::Mutex;
use zbus::blocking::{connection, Connection, Proxy};
use zbus::fdo::{RequestNameFlags, RequestNameReply};

use crate::sink::{OpenError, OutputFallbackReason};

/// Our claim on a card. Above WirePlumber (−20) and PulseAudio (0), so the
/// sound server hands its card over; an app holding the card at a higher
/// priority keeps it.
pub(crate) const PRIORITY: i32 = 10;

/// How long a card stays reserved after its last sink closes.
const GRACE: Duration = Duration::from_secs(2);
/// How long the current holder gets to answer `RequestRelease`.
const CALL_TIMEOUT: Duration = Duration::from_secs(2);
/// How long a released name's connection stays open, so the app we gave the
/// card to replaces us before the name falls free (and the sound server,
/// waiting for it, grabs it first).
const HANDOVER_LINGER: Duration = Duration::from_secs(5);

const INTERFACE: &str = "org.freedesktop.ReserveDevice1";

fn bus_name(card: u32) -> String {
    format!("org.freedesktop.ReserveDevice1.Audio{card}")
}

fn object_path(card: u32) -> String {
    format!("/org/freedesktop/ReserveDevice1/Audio{card}")
}

/// Whether a request at priority `requester` makes the holder at `holder`
/// give its card up. Equal priorities keep the card where it is.
pub(crate) fn outranks(requester: i32, holder: i32) -> bool {
    requester > holder
}

/// What a sink gives the reservation to call when a higher-priority app
/// asks for the card: it must stop using the card and close its PCM before
/// returning, since the other app opens the card as soon as we answer.
pub(crate) type ReleaseHook = Box<dyn Fn() + Send + Sync>;

/// A sink's hold on its card, from [`reserve_card`]. Dropping it (after the
/// PCM is closed) starts the grace period; another sink on the card within
/// it takes the reservation over.
pub(crate) struct CardReservation {
    /// `None` when the card was opened without a reservation: no session bus,
    /// or one we couldn't use.
    held: Option<(&'static Reserver, u32, u64)>,
    handed_over: bool,
}

impl CardReservation {
    /// Whether this reservation was just taken from another holder, which
    /// may still be closing the card for a moment.
    pub(crate) fn handed_over(&self) -> bool {
        self.handed_over
    }
}

impl Drop for CardReservation {
    fn drop(&mut self) {
        if let Some((reserver, card, generation)) = self.held {
            reserver.unuse(card, generation);
        }
    }
}

/// Reserves card `card` (its ALSA index) for a sink on `device_id`, asking
/// the current holder to release it when ours is the higher priority.
/// `on_release` runs if a higher-priority app later asks us for the card.
///
/// Succeeds without a reservation when there's no session bus or it won't
/// let us take part (e.g. a sandbox without the name), so the card opens as
/// it would without the protocol. Fails with `DeviceInUse` only when the
/// holder outranks or refuses us.
pub(crate) fn reserve_card(
    card: u32,
    device_id: &str,
    on_release: ReleaseHook,
) -> Result<CardReservation, OpenError> {
    static SESSION: LazyLock<Reserver> = LazyLock::new(|| {
        Reserver::new(
            || Ok(connection::Builder::session()?.method_timeout(CALL_TIMEOUT)),
            GRACE,
        )
    });
    SESSION.reserve(card, device_id, on_release)
}

/// What both a slot and its D-Bus object see.
struct Shared {
    /// The open sink's hook; `None` while no sink uses the card (grace).
    on_release: Mutex<Option<ReleaseHook>>,
}

/// A reserved card.
struct Slot {
    conn: Connection,
    shared: Arc<Shared>,
    generation: u64,
    /// Open sinks on the card (one in practice).
    users: usize,
    /// Bumped each time `users` reaches 0, so a stale grace timer can tell.
    idle_token: u64,
}

/// Starts a connection to the bus the reservations are on.
type Connect = dyn Fn() -> zbus::Result<connection::Builder<'static>> + Send + Sync;

/// The cards this process has reserved, on one bus.
pub(crate) struct Reserver {
    connect: Box<Connect>,
    grace: Duration,
    slots: Mutex<HashMap<u32, Slot>>,
    next_generation: AtomicU64,
}

impl Reserver {
    pub(crate) fn new(
        connect: impl Fn() -> zbus::Result<connection::Builder<'static>> + Send + Sync + 'static,
        grace: Duration,
    ) -> Self {
        Self {
            connect: Box::new(connect),
            grace,
            slots: Mutex::new(HashMap::new()),
            next_generation: AtomicU64::new(1),
        }
    }

    pub(crate) fn reserve(
        &'static self,
        card: u32,
        device_id: &str,
        on_release: ReleaseHook,
    ) -> Result<CardReservation, OpenError> {
        if let Some(slot) = self.slots.lock().get_mut(&card) {
            slot.users += 1;
            *slot.shared.on_release.lock() = Some(on_release);
            return Ok(CardReservation {
                held: Some((self, card, slot.generation)),
                handed_over: false,
            });
        }

        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let shared = Arc::new(Shared {
            on_release: Mutex::new(Some(on_release)),
        });
        let object = ReserveDevice {
            reserver: self,
            card,
            generation,
            device_id: device_id.to_string(),
            shared: Arc::clone(&shared),
        };
        // Served from the builder, not `Connection::object_server()`: the
        // blocking `object_server()` starts its task outside any runtime,
        // which panics when the app build turns on zbus's tokio feature.
        let conn = match (self.connect)()
            .and_then(|b| b.serve_at(object_path(card), object))
            .and_then(|b| b.build())
        {
            Ok(conn) => conn,
            Err(e) => {
                log::debug!("No session bus for card reservation ({e}); opening unreserved");
                return Ok(unreserved());
            }
        };
        let handed_over = match acquire(&conn, card) {
            Ok(handed_over) => handed_over,
            Err(Acquire::Refused(message)) => {
                return Err(OpenError {
                    reason: OutputFallbackReason::DeviceInUse,
                    message,
                })
            }
            Err(Acquire::Unusable(e)) => {
                log::warn!("Card {card} reservation unavailable ({e}); opening unreserved");
                return Ok(unreserved());
            }
        };
        if handed_over {
            log::info!("Reserved card {card}: the previous holder released it");
        }
        self.slots.lock().insert(
            card,
            Slot {
                conn,
                shared,
                generation,
                users: 1,
                idle_token: 0,
            },
        );
        Ok(CardReservation {
            held: Some((self, card, generation)),
            handed_over,
        })
    }

    /// A sink on `card` closed: once none is left for the grace period, the
    /// card goes back to whoever wants it.
    fn unuse(&'static self, card: u32, generation: u64) {
        let mut slots = self.slots.lock();
        let Some(slot) = slots.get_mut(&card).filter(|s| s.generation == generation) else {
            return; // Already released to another app.
        };
        slot.users = slot.users.saturating_sub(1);
        if slot.users > 0 {
            return;
        }
        *slot.shared.on_release.lock() = None;
        slot.idle_token += 1;
        let token = slot.idle_token;
        let grace = self.grace;
        let spawned = std::thread::Builder::new()
            .name("luminous-card-release".into())
            .spawn(move || {
                std::thread::sleep(grace);
                let mut slots = self.slots.lock();
                let idle = slots.get(&card).is_some_and(|s| {
                    s.generation == generation && s.users == 0 && s.idle_token == token
                });
                if let Some(slot) = idle.then(|| slots.remove(&card)).flatten() {
                    drop(slots);
                    let _ = slot.conn.release_name(bus_name(card));
                    log::info!("Released card {card} reservation");
                }
            });
        if spawned.is_err() {
            // No timer: release now rather than hold the card forever.
            if let Some(slot) = slots.remove(&card) {
                drop(slots);
                let _ = slot.conn.release_name(bus_name(card));
            }
        }
    }

    /// A higher-priority app took `card`: forget it, keeping the connection
    /// (and so the name) until the app has had time to replace us.
    fn surrender(&self, card: u32, generation: u64) {
        let slot = {
            let mut slots = self.slots.lock();
            match slots.get(&card) {
                Some(s) if s.generation == generation => slots.remove(&card),
                _ => None,
            }
        };
        if let Some(slot) = slot {
            let _ = std::thread::Builder::new()
                .name("luminous-card-handover".into())
                .spawn(move || {
                    std::thread::sleep(HANDOVER_LINGER);
                    drop(slot);
                });
        }
    }
}

fn unreserved() -> CardReservation {
    CardReservation {
        held: None,
        handed_over: false,
    }
}

enum Acquire {
    /// The holder keeps the card.
    Refused(String),
    /// The bus wouldn't let us take part.
    Unusable(zbus::Error),
}

/// Takes the card's name on `conn`, which already serves our side of the
/// protocol. `Ok(true)` when the name was taken from another holder.
fn acquire(conn: &Connection, card: u32) -> Result<bool, Acquire> {
    let name = bus_name(card);
    let path = object_path(card);
    let flags = RequestNameFlags::DoNotQueue | RequestNameFlags::AllowReplacement;
    // zbus reports a name someone else owns as `NameTaken`, not `Exists`.
    match conn.request_name_with_flags(name.as_str(), flags) {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => return Ok(false),
        Ok(RequestNameReply::Exists | RequestNameReply::InQueue) | Err(zbus::Error::NameTaken) => {}
        Err(e) => return Err(Acquire::Unusable(e)),
    }

    // Someone has the card: ask them for it, if we outrank them.
    let refused = |why: String| Acquire::Refused(format!("Card {card} is reserved: {why}"));
    let holder = Proxy::new(conn, name.as_str(), path.as_str(), INTERFACE)
        .map_err(|e| refused(e.to_string()))?;
    let priority: i32 = holder
        .get_property("Priority")
        .map_err(|e| refused(format!("its holder's priority is unknown ({e})")))?;
    if !outranks(PRIORITY, priority) {
        let app: String = holder.get_property("ApplicationName").unwrap_or_default();
        return Err(refused(format!("held by {app} at priority {priority}")));
    }
    let released: bool = holder
        .call("RequestRelease", &(PRIORITY,))
        .map_err(|e| refused(format!("release request failed ({e})")))?;
    if !released {
        return Err(refused("its holder refused to release it".into()));
    }
    match conn
        .request_name_with_flags(name.as_str(), flags | RequestNameFlags::ReplaceExisting)
        .map_err(|e| refused(e.to_string()))?
    {
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner => Ok(true),
        _ => Err(refused("its holder released it but kept the name".into())),
    }
}

/// Our side of the protocol, served at the card's object path.
struct ReserveDevice {
    reserver: &'static Reserver,
    card: u32,
    generation: u64,
    device_id: String,
    shared: Arc<Shared>,
}

#[zbus::interface(name = "org.freedesktop.ReserveDevice1")]
impl ReserveDevice {
    /// Another app wants the card: give it up if it outranks us, closing the
    /// PCM first.
    fn request_release(&self, priority: i32) -> bool {
        if !outranks(priority, PRIORITY) {
            return false;
        }
        log::info!(
            "Releasing card {} to an app at priority {priority}",
            self.card
        );
        if let Some(on_release) = self.shared.on_release.lock().take() {
            on_release();
        }
        self.reserver.surrender(self.card, self.generation);
        true
    }

    #[zbus(property)]
    fn priority(&self) -> i32 {
        PRIORITY
    }

    #[zbus(property)]
    fn application_name(&self) -> String {
        "Luminous".into()
    }

    #[zbus(property)]
    fn application_device_name(&self) -> String {
        self.device_id.clone()
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::AtomicUsize;

    use zbus::blocking::fdo::DBusProxy;

    use super::*;

    #[test]
    fn only_a_strictly_higher_priority_takes_a_card() {
        assert!(outranks(PRIORITY, -20), "WirePlumber hands its cards over");
        assert!(outranks(PRIORITY, 0), "so does PulseAudio");
        assert!(!outranks(PRIORITY, PRIORITY));
        assert!(!outranks(PRIORITY, PRIORITY + 1));
    }

    /// A private session bus for one test, shut down on drop.
    struct Bus {
        daemon: Child,
        address: String,
    }

    impl Bus {
        /// `None` (the test is skipped) where `dbus-daemon` isn't installed.
        fn start() -> Option<Self> {
            let mut daemon = Command::new("dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .stdout(Stdio::piped())
                .spawn()
                .ok()?;
            let mut address = String::new();
            BufReader::new(daemon.stdout.take()?)
                .read_line(&mut address)
                .ok()?;
            Some(Self {
                daemon,
                address: address.trim().to_string(),
            })
        }

        fn builder(&self) -> connection::Builder<'static> {
            connection::Builder::address(self.address.as_str())
                .expect("a valid bus address")
                .method_timeout(CALL_TIMEOUT)
        }

        fn connect(&self) -> Connection {
            self.builder().build().expect("connects to the test bus")
        }

        fn reserver(&self, grace: Duration) -> &'static Reserver {
            let address = self.address.clone();
            Box::leak(Box::new(Reserver::new(
                move || connection::Builder::address(address.as_str()),
                grace,
            )))
        }

        fn owner(&self, card: u32) -> Option<String> {
            DBusProxy::new(&self.connect())
                .ok()?
                .get_name_owner(bus_name(card).as_str().try_into().ok()?)
                .ok()
                .map(|o| o.to_string())
        }
    }

    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
        }
    }

    /// A sound server holding a card, like WirePlumber.
    struct FakeHolder {
        priority: i32,
        grant: bool,
        asked: Arc<AtomicUsize>,
    }

    #[zbus::interface(name = "org.freedesktop.ReserveDevice1")]
    impl FakeHolder {
        fn request_release(&self, _priority: i32) -> bool {
            self.asked.fetch_add(1, Ordering::SeqCst);
            self.grant
        }

        #[zbus(property)]
        fn priority(&self) -> i32 {
            self.priority
        }

        #[zbus(property)]
        fn application_name(&self) -> String {
            "Fake".into()
        }
    }

    /// Holds `card` on its own connection; returns it and its release count.
    fn hold(bus: &Bus, card: u32, priority: i32, grant: bool) -> (Connection, Arc<AtomicUsize>) {
        let asked = Arc::new(AtomicUsize::new(0));
        let holder = FakeHolder {
            priority,
            grant,
            asked: Arc::clone(&asked),
        };
        let conn = bus
            .builder()
            .serve_at(object_path(card), holder)
            .unwrap()
            .build()
            .unwrap();
        let reply = conn
            .request_name_with_flags(
                bus_name(card).as_str(),
                RequestNameFlags::DoNotQueue | RequestNameFlags::AllowReplacement,
            )
            .unwrap();
        assert_eq!(reply, RequestNameReply::PrimaryOwner);
        (conn, asked)
    }

    fn no_hook() -> ReleaseHook {
        Box::new(|| {})
    }

    fn unique_name(conn: &Connection) -> String {
        conn.unique_name().unwrap().to_string()
    }

    #[test]
    fn a_free_card_is_reserved_without_a_handover() {
        let Some(bus) = Bus::start() else { return };
        let reservation = bus
            .reserver(GRACE)
            .reserve(0, "hw:CARD=X,DEV=0", no_hook())
            .unwrap();
        assert!(!reservation.handed_over());
        assert!(bus.owner(0).is_some());
    }

    #[test]
    fn a_lower_priority_holder_hands_the_card_over() {
        let Some(bus) = Bus::start() else { return };
        let (holder, asked) = hold(&bus, 1, -20, true);
        let reservation = bus
            .reserver(GRACE)
            .reserve(1, "hw:CARD=X,DEV=0", no_hook())
            .unwrap();
        assert!(reservation.handed_over());
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert_ne!(bus.owner(1), Some(unique_name(&holder)));
    }

    #[test]
    fn a_holder_that_outranks_or_refuses_us_keeps_the_card() {
        let Some(bus) = Bus::start() else { return };
        let reserver = bus.reserver(GRACE);
        for (card, priority, grant) in [(2, PRIORITY, true), (3, i32::MAX, true), (4, -20, false)] {
            let (holder, _) = hold(&bus, card, priority, grant);
            let err = reserver
                .reserve(card, "hw:CARD=X,DEV=0", no_hook())
                .err()
                .expect("the holder keeps the card");
            assert_eq!(err.reason, OutputFallbackReason::DeviceInUse);
            assert_eq!(bus.owner(card), Some(unique_name(&holder)));
        }
    }

    #[test]
    fn no_bus_opens_the_card_unreserved() {
        let reserver: &'static Reserver = Box::leak(Box::new(Reserver::new(
            || Err(zbus::Error::Address("no bus".into())),
            GRACE,
        )));
        let reservation = reserver.reserve(0, "hw:CARD=X,DEV=0", no_hook()).unwrap();
        assert!(!reservation.handed_over());
    }

    #[test]
    fn a_reopen_within_the_grace_period_keeps_the_card() {
        let Some(bus) = Bus::start() else { return };
        let (_holder, asked) = hold(&bus, 5, -20, true);
        let reserver = bus.reserver(Duration::from_millis(300));
        let first = reserver.reserve(5, "hw:CARD=X,DEV=0", no_hook()).unwrap();
        let ours = bus.owner(5);
        drop(first);
        let second = reserver.reserve(5, "hw:CARD=X,DEV=0", no_hook()).unwrap();
        assert!(!second.handed_over(), "the reopen reused the reservation");
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(bus.owner(5), ours, "still held while a sink is open");

        drop(second);
        std::thread::sleep(Duration::from_millis(600));
        assert_ne!(bus.owner(5), ours, "released after the grace period");
    }

    #[test]
    fn a_higher_priority_app_gets_the_card_after_we_close_it() {
        let Some(bus) = Bus::start() else { return };
        let closed = Arc::new(AtomicUsize::new(0));
        let hook = {
            let closed = Arc::clone(&closed);
            Box::new(move || {
                closed.fetch_add(1, Ordering::SeqCst);
            })
        };
        let _reservation = bus
            .reserver(GRACE)
            .reserve(6, "hw:CARD=X,DEV=0", hook)
            .unwrap();
        let asker = bus.connect();
        let ours = Proxy::new(&asker, bus_name(6), object_path(6), INTERFACE).unwrap();

        let refused: bool = ours.call("RequestRelease", &(PRIORITY,)).unwrap();
        assert!(!refused);
        assert_eq!(closed.load(Ordering::SeqCst), 0);

        let granted: bool = ours.call("RequestRelease", &(PRIORITY + 1,)).unwrap();
        assert!(granted);
        assert_eq!(closed.load(Ordering::SeqCst), 1, "the PCM closed first");
        let reply = asker
            .request_name_with_flags(
                bus_name(6).as_str(),
                RequestNameFlags::DoNotQueue | RequestNameFlags::ReplaceExisting,
            )
            .unwrap();
        assert_eq!(reply, RequestNameReply::PrimaryOwner);
    }
}
