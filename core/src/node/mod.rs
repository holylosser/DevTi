//! The Devti node: one running instance of the ecosystem on one device.
//!
//! A [`Node`] owns the identity, the database, the QUIC endpoint and the discovery
//! daemon, and keeps one authenticated session per online paired device. Host apps
//! (desktop Hub, Android, iOS) drive it through this API and render the
//! [`NodeEvent`] stream; they never touch sockets or crypto themselves.
//!
//! Internals are split by concern:
//! * [`trust`] — the TLS admission policy (who may even finish a handshake),
//! * [`session`] — accepting/dialling connections, sessions, reconnect with backoff,
//! * [`ceremony`] — the hub and joiner sides of the pairing ceremony.

mod ceremony;
mod continuity;
mod events;
mod session;
mod transfer;
mod trust;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

pub use events::*;

use crate::discovery::{Advertisement, DiscoveredDevice, Discovery, DiscoveryEvent};
use crate::error::{Error, Result};
use crate::identity::{Identity, IdentityOrigin};
use crate::model::{
    Capability, DeviceId, DeviceInfo, DeviceKind, DeviceStatus, Permissions, Platform, TrustRole, sanitize_name,
};
use crate::net::{local_addresses, now_ms};
use crate::pairing::{self, PairingUri, format_code};
use crate::protocol::{ControlMsg, GoodbyeReason, PairMethod, close_code};
use crate::secret::SecretStore;
use crate::store::{ActivityEntry, PeerRecord, Store};
use crate::tls::ServerExpectation;

use session::Session;
use trust::TrustView;

/// Default UDP port. Falls back to a random port when taken.
pub const DEFAULT_PORT: u16 = 47810;
/// How long a pairing window stays open.
pub const PAIRING_WINDOW_TTL: Duration = Duration::from_secs(5 * 60);
/// Wrong secrets allowed before a pairing window closes itself.
pub const MAX_PAIRING_FAILURES: u8 = 3;

const SETTING_DEVICE_NAME: &str = "device_name";
const SETTING_PAUSED: &str = "paused";

/// Static configuration supplied by the host app.
#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub data_dir: PathBuf,
    /// Initial name; a later rename is persisted and wins over this value.
    pub device_name: String,
    pub kind: DeviceKind,
    pub platform: Platform,
    pub os_version: String,
    pub model: Option<String>,
    pub app_version: String,
    pub capabilities: Vec<Capability>,
    pub bind_ip: IpAddr,
    /// `0` lets the OS pick.
    pub port: u16,
    /// mDNS advertising + browsing.
    pub discovery: bool,
    /// Where received files are saved (changeable later in settings).
    pub downloads_dir: PathBuf,
}

impl NodeConfig {
    /// Sensible defaults for a device of `kind` storing data in `data_dir`.
    pub fn new(data_dir: impl Into<PathBuf>, device_name: &str, kind: DeviceKind) -> Self {
        Self {
            data_dir: data_dir.into(),
            device_name: device_name.to_string(),
            kind,
            platform: Platform::current(),
            os_version: String::new(),
            model: None,
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![Capability::Battery],
            bind_ip: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: DEFAULT_PORT,
            discovery: true,
            downloads_dir: data_dir_default_downloads(),
        }
    }
}

fn data_dir_default_downloads() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Downloads")
        .join("Devti")
}

/// Runtime state of one paired device.
pub(crate) struct PeerState {
    pub record: PeerRecord,
    pub session: Option<Session>,
    pub next_attempt: Instant,
    pub attempts: u32,
    pub dialing: bool,
}

/// The pairing window opened by "Pair a new device".
pub(crate) struct PairingWindow {
    pub id: u64,
    pub qr_secret: [u8; pairing::QR_SECRET_LEN],
    pub code: String,
    pub expires: Instant,
    pub failures: u8,
    pub busy: bool,
    pub info: PairingWindowInfo,
}

pub(crate) struct Inner {
    pub identity: Identity,
    pub identity_origin: IdentityOrigin,
    pub secret_backend: String,
    pub secret_os_protected: bool,
    pub store: Store,
    pub endpoint: quinn::Endpoint,
    pub local_addr: SocketAddr,
    pub bind_ip: IpAddr,
    pub info: RwLock<DeviceInfo>,
    pub status: RwLock<DeviceStatus>,
    pub peers: RwLock<HashMap<DeviceId, PeerState>>,
    pub discovered: RwLock<HashMap<DeviceId, DiscoveredDevice>>,
    pub window: Mutex<Option<PairingWindow>>,
    pub decisions: Mutex<HashMap<u64, oneshot::Sender<bool>>>,
    pub events: broadcast::Sender<NodeEvent>,
    pub trust: Arc<TrustView>,
    pub paused: AtomicBool,
    pub discovery_enabled: bool,
    pub discovery: Mutex<Option<Discovery>>,
    pub discovery_tx: mpsc::UnboundedSender<DiscoveryEvent>,
    pub session_seq: AtomicU64,
    pub shutdown: watch::Sender<bool>,
    /// The runtime the node was started on. Every background task is spawned here, so
    /// the public API is safe to call from any thread (UI threads, FFI callbacks).
    pub rt: tokio::runtime::Handle,
    /// Phase 2 continuity state (clipboard echo guard, mirrored notifications, DND…).
    pub continuity: continuity::State,
    /// Active and queued file transfers.
    pub transfers: transfer::State,
}

/// Handle to a running node. Cheap to clone.
#[derive(Clone)]
pub struct Node {
    inner: Arc<Inner>,
}

impl Node {
    /// Loads (or creates) the identity and database, binds the QUIC endpoint and starts
    /// the background tasks. Must be called inside a Tokio runtime.
    pub async fn start(config: NodeConfig, secrets: Arc<dyn SecretStore>) -> Result<Node> {
        std::fs::create_dir_all(&config.data_dir)?;
        let (identity, identity_origin) = Identity::load_or_create(secrets.as_ref(), &config.data_dir)?;
        let store = Store::open(&config.data_dir.join("devti.db"))?;
        let records = store.peers()?;
        let paused = store.setting(SETTING_PAUSED)?.as_deref() == Some("true");
        let store_dnd = store.setting(continuity::SETTING_DND)?.as_deref() == Some("true");
        let downloads = store
            .setting(transfer::SETTING_DOWNLOADS)?
            .map(PathBuf::from)
            .unwrap_or_else(|| config.downloads_dir.clone());

        let trust = Arc::new(TrustView::new(records.iter().map(|r| r.fingerprint)));
        trust.set_paused(paused);
        let endpoint = session::bind_endpoint(&identity, trust.clone(), config.bind_ip, config.port)?;
        let local_addr = endpoint.local_addr()?;

        let name = store
            .setting(SETTING_DEVICE_NAME)?
            .map(|n| sanitize_name(&n))
            .unwrap_or_else(|| sanitize_name(&config.device_name));
        let info = DeviceInfo {
            id: identity.device_id(),
            name,
            kind: config.kind,
            platform: config.platform,
            os_version: config.os_version.clone(),
            model: config.model.clone(),
            app_version: config.app_version.clone(),
            capabilities: config.capabilities.clone(),
        }
        .sanitized();

        let now = Instant::now();
        let peers = records
            .into_iter()
            .map(|record| {
                (record.info.id, PeerState { record, session: None, next_attempt: now, attempts: 0, dialing: false })
            })
            .collect();

        let (events, _) = broadcast::channel(512);
        let (shutdown, _) = watch::channel(false);
        let (discovery_tx, discovery_rx) = mpsc::unbounded_channel();
        let inner = Arc::new(Inner {
            secret_backend: secrets.backend(),
            secret_os_protected: secrets.is_os_protected(),
            identity,
            identity_origin,
            store,
            endpoint,
            local_addr,
            bind_ip: config.bind_ip,
            info: RwLock::new(info),
            status: RwLock::new(DeviceStatus::default()),
            peers: RwLock::new(peers),
            discovered: RwLock::new(HashMap::new()),
            window: Mutex::new(None),
            decisions: Mutex::new(HashMap::new()),
            events,
            trust,
            paused: AtomicBool::new(paused),
            discovery_enabled: config.discovery,
            discovery: Mutex::new(None),
            discovery_tx,
            session_seq: AtomicU64::new(1),
            shutdown,
            rt: tokio::runtime::Handle::current(),
            continuity: continuity::State::new(store_dnd),
            transfers: transfer::State::new(downloads),
        });

        if config.discovery && !paused {
            inner.start_discovery();
        }
        inner.rt.spawn(session::accept_loop(inner.clone()));
        inner.rt.spawn(session::reconnect_loop(inner.clone()));
        inner.rt.spawn(session::discovery_loop(inner.clone(), discovery_rx));
        tracing::info!(id = %inner.identity.device_id(), addr = %local_addr, "devti node started");
        let node = Node { inner };
        node.load_continuity_settings();
        Ok(node)
    }

    /// Subscribes to node events. Lagging receivers should re-read [`Node::peers`].
    pub fn subscribe(&self) -> broadcast::Receiver<NodeEvent> {
        self.inner.events.subscribe()
    }

    pub fn device_id(&self) -> DeviceId {
        self.inner.identity.device_id()
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.inner.local_addr
    }

    /// Snapshot of this device.
    pub fn local(&self) -> LocalSnapshot {
        self.inner.local_snapshot()
    }

    /// Every paired device, online ones first.
    pub fn peers(&self) -> Vec<PeerSnapshot> {
        let peers = self.inner.peers.read();
        let mut out: Vec<PeerSnapshot> = peers.values().map(|p| self.inner.snapshot(p)).collect();
        out.sort_by(|a, b| b.online.cmp(&a.online).then(a.paired_at.cmp(&b.paired_at)));
        out
    }

    pub fn peer(&self, id: &DeviceId) -> Option<PeerSnapshot> {
        self.inner.peers.read().get(id).map(|p| self.inner.snapshot(p))
    }

    /// Devti devices currently visible on the LAN that are *not* paired yet.
    pub fn nearby(&self) -> Vec<DiscoveredDevice> {
        let peers = self.inner.peers.read();
        let mut v: Vec<_> =
            self.inner.discovered.read().values().filter(|d| !peers.contains_key(&d.id)).cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    // ------------------------------------------------------------------ pairing

    /// Opens (or refreshes) the pairing window and returns what the Hub should display.
    pub fn open_pairing_window(&self) -> Result<PairingWindowInfo> {
        self.inner.open_pairing_window()
    }

    /// Closes the pairing window (if any).
    pub fn close_pairing_window(&self) {
        self.inner.close_pairing_window("closed");
    }

    pub fn pairing_window(&self) -> Option<PairingWindowInfo> {
        self.inner.window.lock().as_ref().map(|w| w.info.clone())
    }

    /// Joiner: starts pairing from a scanned QR link. Progress arrives as events.
    pub fn pair_with_uri(&self, uri: &str) -> Result<u64> {
        self.inner.ensure_running()?;
        let uri = PairingUri::parse(uri)?;
        let pairing_id = pairing::new_id()?;
        let target = ceremony::JoinTarget {
            addrs: uri.socket_addrs(),
            expect: ServerExpectation::Pinned(uri.fingerprint),
            method: PairMethod::Qr,
            password: crate::pairing::pake::password_for_qr(&uri.secret),
        };
        self.inner.rt.spawn(ceremony::run_joiner(self.inner.clone(), pairing_id, target));
        Ok(pairing_id)
    }

    /// Joiner: pairs with a discovered device using the 6-digit code it displays.
    pub fn pair_with_code(&self, device: &DeviceId, code: &str) -> Result<u64> {
        let found = self
            .inner
            .discovered
            .read()
            .get(device)
            .cloned()
            .ok_or_else(|| Error::InvalidInput("that device is no longer visible on the network".into()))?;
        let expect = found
            .fingerprint
            .as_deref()
            .and_then(|f| crate::model::Fingerprint::from_b64(f).ok())
            .map(ServerExpectation::Pinned)
            .unwrap_or(ServerExpectation::AnyForPairing);
        self.pair_with_code_at(found.addrs, expect, code)
    }

    /// Joiner: pairs with a device at a manually entered address using its 6-digit code.
    pub fn pair_with_code_at_address(&self, addr: SocketAddr, code: &str) -> Result<u64> {
        self.pair_with_code_at(vec![addr], ServerExpectation::AnyForPairing, code)
    }

    fn pair_with_code_at(&self, addrs: Vec<SocketAddr>, expect: ServerExpectation, code: &str) -> Result<u64> {
        self.inner.ensure_running()?;
        let digits: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() != 6 {
            return Err(Error::InvalidInput("the pairing code has 6 digits".into()));
        }
        let pairing_id = pairing::new_id()?;
        let target = ceremony::JoinTarget {
            addrs,
            expect,
            method: PairMethod::Code,
            password: crate::pairing::pake::password_for_code(&digits),
        };
        self.inner.rt.spawn(ceremony::run_joiner(self.inner.clone(), pairing_id, target));
        Ok(pairing_id)
    }

    /// The user's answer to "do both screens show the same code?".
    pub fn confirm_pairing(&self, pairing_id: u64, accept: bool) -> Result<()> {
        let tx = self
            .inner
            .decisions
            .lock()
            .remove(&pairing_id)
            .ok_or_else(|| Error::InvalidInput("no pairing is waiting for confirmation".into()))?;
        let _ = tx.send(accept);
        Ok(())
    }

    // ---------------------------------------------------------- management

    /// Removes a paired device everywhere and disconnects it.
    pub fn revoke(&self, id: &DeviceId) -> Result<()> {
        self.inner.remove_peer(id, RemovalReason::RevokedHere)
    }

    /// Changes a device's role and permissions.
    pub fn set_access(&self, id: &DeviceId, role: TrustRole, permissions: Permissions) -> Result<PeerSnapshot> {
        let snap = {
            let mut peers = self.inner.peers.write();
            let p = peers.get_mut(id).ok_or_else(|| Error::UnknownPeer(id.to_string()))?;
            p.record.role = role;
            p.record.permissions = permissions;
            self.inner.store.set_peer_access(id, role, &permissions)?;
            self.inner.snapshot(p)
        };
        self.inner.log(Some((id, &snap.info.name)), "permissions_changed", role.as_str());
        self.inner.emit(NodeEvent::PeerChanged { peer: snap.clone() });
        Ok(snap)
    }

    /// Kill switch: disconnects everyone and refuses all connections until resumed.
    pub fn set_paused(&self, paused: bool) -> Result<()> {
        self.inner.set_paused(paused)
    }

    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::SeqCst)
    }

    /// Renames this device and tells every connected peer.
    pub fn rename(&self, name: &str) -> Result<LocalSnapshot> {
        let name = sanitize_name(name);
        let info = {
            let mut info = self.inner.info.write();
            info.name = name.clone();
            info.clone()
        };
        self.inner.store.set_setting(SETTING_DEVICE_NAME, &name)?;
        self.inner.broadcast(ControlMsg::Info { info });
        self.inner.refresh_advertisement();
        let local = self.inner.local_snapshot();
        self.inner.emit(NodeEvent::LocalChanged { local: local.clone() });
        Ok(local)
    }

    /// Updates live status (battery…) and pushes it to connected peers.
    pub fn set_status(&self, status: DeviceStatus) {
        let status = status.sanitized();
        {
            let mut cur = self.inner.status.write();
            if *cur == status {
                return;
            }
            *cur = status.clone();
        }
        if let Some(b) = status.battery {
            let sample = crate::store::BatterySample { ts: now_ms(), percent: b.percent, charging: b.charging };
            let _ = self.inner.store.add_battery(&self.device_id(), sample);
        }
        self.inner.broadcast(ControlMsg::Status { status });
        self.inner.emit(NodeEvent::LocalChanged { local: self.inner.local_snapshot() });
    }

    /// Retry every offline device now (e.g. after the network changed or the app resumed).
    pub fn reconnect_now(&self) {
        let now = Instant::now();
        for p in self.inner.peers.write().values_mut() {
            p.attempts = 0;
            p.next_attempt = now;
        }
    }

    /// Newest-first activity log page.
    pub fn activity(
        &self,
        device: Option<&DeviceId>,
        limit: u32,
        before_id: Option<i64>,
    ) -> Result<Vec<ActivityEntry>> {
        self.inner.store.activity(device, limit, before_id)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.inner.store.setting(key)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.inner.store.set_setting(key, value)
    }

    /// Every stored setting (UI preferences live here too).
    pub fn settings(&self) -> Result<Vec<(String, String)>> {
        self.inner.store.settings()
    }

    /// Says goodbye to every peer, closes the endpoint and stops background tasks.
    pub async fn shutdown(&self) {
        let _ = self.inner.shutdown.send(true);
        self.inner.broadcast(ControlMsg::Goodbye { reason: GoodbyeReason::Shutdown });
        tokio::time::sleep(Duration::from_millis(50)).await;
        self.inner.endpoint.close(quinn::VarInt::from_u32(close_code::SHUTDOWN), b"shutdown");
        if let Some(d) = self.inner.discovery.lock().take() {
            d.stop();
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), self.inner.endpoint.wait_idle()).await;
    }
}

impl Inner {
    pub(crate) fn emit(&self, event: NodeEvent) {
        let _ = self.events.send(event);
    }

    pub(crate) fn log(&self, device: Option<(&DeviceId, &str)>, action: &str, detail: &str) {
        if let Err(e) = self.store.log_activity(now_ms(), device, action, detail) {
            tracing::warn!(error = %e, "failed to write activity log");
        }
    }

    pub(crate) fn ensure_running(&self) -> Result<()> {
        if *self.shutdown.borrow() {
            return Err(Error::Shutdown);
        }
        if self.paused.load(Ordering::SeqCst) {
            return Err(Error::Paused);
        }
        Ok(())
    }

    pub(crate) fn info(&self) -> DeviceInfo {
        self.info.read().clone()
    }

    pub(crate) fn advertised_addresses(&self) -> Vec<IpAddr> {
        if self.bind_ip.is_unspecified() { local_addresses() } else { vec![self.bind_ip] }
    }

    pub(crate) fn local_snapshot(&self) -> LocalSnapshot {
        LocalSnapshot {
            info: self.info(),
            fingerprint: self.identity.fingerprint().grouped_hex(),
            port: self.local_addr.port(),
            addresses: self.advertised_addresses().iter().map(|a| a.to_string()).collect(),
            status: self.status.read().clone(),
            paused: self.paused.load(Ordering::SeqCst),
            discovery_enabled: self.discovery_enabled,
            identity_origin: match self.identity_origin {
                IdentityOrigin::Created => "created",
                IdentityOrigin::Loaded => "loaded",
                IdentityOrigin::Regenerated => "regenerated",
            }
            .into(),
            secret_backend: self.secret_backend.clone(),
            secret_os_protected: self.secret_os_protected,
        }
    }

    pub(crate) fn snapshot(&self, p: &PeerState) -> PeerSnapshot {
        let connection = p.session.as_ref().map(|s| ConnectionSnapshot {
            kind: crate::model::ConnectionKind::Lan,
            address: s.conn.remote_address().to_string(),
            rtt_ms: s.conn.rtt().as_millis().min(u32::MAX as u128) as u32,
            since: s.connected_at,
        });
        PeerSnapshot {
            info: p.record.info.clone(),
            fingerprint: p.record.fingerprint.grouped_hex(),
            role: p.record.role,
            permissions: p.record.permissions,
            paired_at: p.record.paired_at,
            last_seen: p.record.last_seen,
            online: p.session.is_some(),
            connection,
            status: p.record.last_status.clone(),
        }
    }

    /// Sends a control message to every connected peer.
    pub(crate) fn broadcast(&self, msg: ControlMsg) {
        for p in self.peers.read().values() {
            if matches!(msg, ControlMsg::Clipboard { .. }) && !p.record.permissions.clipboard {
                continue;
            }
            if let Some(s) = &p.session {
                let _ = s.tx.send(msg.clone());
            }
        }
    }

    fn advertisement(&self) -> Advertisement {
        let info = self.info();
        Advertisement {
            id: info.id,
            name: info.name,
            kind: info.kind,
            platform: info.platform,
            port: self.local_addr.port(),
            fingerprint: self.identity.fingerprint(),
            pairing_open: self.window.lock().is_some(),
        }
    }

    pub(crate) fn start_discovery(&self) {
        let mut slot = self.discovery.lock();
        if slot.is_some() {
            return;
        }
        match Discovery::start(&self.advertisement(), self.discovery_tx.clone()) {
            Ok(d) => *slot = Some(d),
            Err(e) => tracing::warn!(error = %e, "mDNS discovery unavailable; QR and manual pairing still work"),
        }
    }

    pub(crate) fn refresh_advertisement(&self) {
        let ad = self.advertisement();
        if let Some(d) = self.discovery.lock().as_mut()
            && let Err(e) = d.update(&ad)
        {
            tracing::warn!(error = %e, "failed to refresh mDNS advertisement");
        }
    }

    fn open_pairing_window(self: &Arc<Self>) -> Result<PairingWindowInfo> {
        self.ensure_running()?;
        let id = pairing::new_id()?;
        let qr_secret = pairing::new_qr_secret()?;
        let code = pairing::new_code()?;
        let addrs = self.advertised_addresses();
        if addrs.is_empty() {
            return Err(Error::Connection(
                "this device has no network connection — connect to Wi-Fi or Ethernet first".into(),
            ));
        }
        let info = self.info();
        let uri = PairingUri {
            hub_id: info.id,
            fingerprint: self.identity.fingerprint(),
            secret: qr_secret,
            port: self.local_addr.port(),
            addrs: addrs.clone(),
            name: info.name,
            kind: info.kind,
        };
        let expires_at_ms = now_ms() + PAIRING_WINDOW_TTL.as_millis() as i64;
        let window_info = PairingWindowInfo {
            id,
            uri: uri.to_uri(),
            code: format_code(&code),
            expires_at: expires_at_ms,
            addresses: addrs.iter().map(|a| format!("{a}:{}", self.local_addr.port())).collect(),
        };
        *self.window.lock() = Some(PairingWindow {
            id,
            qr_secret,
            code,
            expires: Instant::now() + PAIRING_WINDOW_TTL,

            failures: 0,
            busy: false,
            info: window_info.clone(),
        });
        self.trust.set_pairing_open(true);
        self.refresh_advertisement();
        self.emit(NodeEvent::PairingWindowChanged { window: Some(window_info.clone()) });

        // Expire the window automatically.
        let inner = self.clone();
        self.rt.spawn(async move {
            tokio::time::sleep(PAIRING_WINDOW_TTL).await;
            let expired = inner.window.lock().as_ref().is_some_and(|w| w.id == id && !w.busy);
            if expired {
                inner.close_pairing_window("expired");
            }
        });
        Ok(window_info)
    }

    pub(crate) fn close_pairing_window(&self, why: &str) {
        let had = self.window.lock().take().is_some();
        self.trust.set_pairing_open(false);
        if had {
            tracing::debug!(why, "pairing window closed");
            self.refresh_advertisement();
            self.emit(NodeEvent::PairingWindowChanged { window: None });
        }
    }

    pub(crate) fn remove_peer(&self, id: &DeviceId, reason: RemovalReason) -> Result<()> {
        let removed = self.peers.write().remove(id);
        let state = removed.ok_or_else(|| Error::UnknownPeer(id.to_string()))?;
        self.trust.remove(&state.record.fingerprint);
        self.store.remove_peer(id)?;
        if let Some(s) = state.session {
            if reason == RemovalReason::RevokedHere {
                let _ = s.tx.send(ControlMsg::Goodbye { reason: GoodbyeReason::Revoked });
            }
            let conn = s.conn.clone();
            self.rt.spawn(async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                conn.close(quinn::VarInt::from_u32(close_code::REVOKED), b"revoked");
            });
        }
        let action = match reason {
            RemovalReason::RevokedHere => "revoked",
            RemovalReason::RevokedByPeer => "revoked_by_peer",
        };
        self.log(Some((id, &state.record.info.name)), action, "");
        self.emit(NodeEvent::PeerRemoved { id: *id, reason });
        Ok(())
    }

    fn set_paused(&self, paused: bool) -> Result<()> {
        if self.paused.swap(paused, Ordering::SeqCst) == paused {
            return Ok(());
        }
        self.trust.set_paused(paused);
        self.store.set_setting(SETTING_PAUSED, if paused { "true" } else { "false" })?;
        if paused {
            self.close_pairing_window("paused");
            let sessions: Vec<quinn::Connection> = {
                let peers = self.peers.read();
                peers
                    .values()
                    .filter_map(|p| p.session.as_ref())
                    .map(|s| {
                        let _ = s.tx.send(ControlMsg::Goodbye { reason: GoodbyeReason::Paused });
                        s.conn.clone()
                    })
                    .collect()
            };
            self.rt.spawn(async move {
                tokio::time::sleep(Duration::from_millis(100)).await;
                for c in sessions {
                    c.close(quinn::VarInt::from_u32(close_code::PAUSED), b"paused");
                }
            });
            if let Some(d) = self.discovery.lock().take() {
                d.stop();
            }
            self.discovered.write().clear();
        } else {
            let now = Instant::now();
            for p in self.peers.write().values_mut() {
                p.attempts = 0;
                p.next_attempt = now;
            }
            if self.discovery_enabled {
                self.start_discovery();
            }
        }
        self.log(None, if paused { "ecosystem_paused" } else { "ecosystem_resumed" }, "");
        self.emit(NodeEvent::Paused { paused });
        self.emit(NodeEvent::LocalChanged { local: self.local_snapshot() });
        Ok(())
    }
}
