//! UniFFI surface of Devti Core for the mobile companion apps.
//!
//! The surface is deliberately small: one `DevtiMobile` object, two callback
//! interfaces (events out, secure storage in) and JSON for rich data. The JSON is the
//! exact `NodeEvent` / `PeerSnapshot` shape the desktop UI consumes, so all three apps
//! speak one schema (documented in `docs/PROTOCOL.md` § Host API).

use std::path::PathBuf;
use std::sync::Arc;

use devti_core::model::{BatteryState, Capability, DeviceId, DeviceKind, DeviceStatus, Platform};
use devti_core::secret::SecretStore;
use devti_core::{Node, NodeConfig};

uniffi::setup_scaffolding!();

/// Errors surfaced to Kotlin/Swift as exceptions.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum DevtiError {
    #[error("{reason}")]
    InvalidInput { reason: String },
    #[error("{reason}")]
    Failed { reason: String },
    #[error("the ecosystem is paused")]
    Paused,
}

impl From<devti_core::Error> for DevtiError {
    fn from(e: devti_core::Error) -> Self {
        match e {
            devti_core::Error::Paused => DevtiError::Paused,
            devti_core::Error::InvalidInput(m) | devti_core::Error::InvalidPairingUri(m) => {
                DevtiError::InvalidInput { reason: m }
            }
            other => DevtiError::Failed { reason: other.to_string() },
        }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for DevtiError {
    fn from(e: uniffi::UnexpectedUniFFICallbackError) -> Self {
        DevtiError::Failed { reason: e.reason }
    }
}

/// Receives every node event as JSON (`{"type": "...", ...}`), on a background thread.
#[uniffi::export(callback_interface)]
pub trait EventListener: Send + Sync {
    fn on_event(&self, event_json: String);
}

/// Platform secure storage: Android Keystore-wrapped values / iOS Keychain items.
#[uniffi::export(callback_interface)]
pub trait PlatformSecretStore: Send + Sync {
    fn get(&self, key: String) -> Result<Option<Vec<u8>>, DevtiError>;
    fn set(&self, key: String, value: Vec<u8>) -> Result<(), DevtiError>;
    fn delete(&self, key: String) -> Result<(), DevtiError>;
    fn backend_name(&self) -> String;
}

struct SecretBridge(Box<dyn PlatformSecretStore>);

impl SecretStore for SecretBridge {
    fn get(&self, key: &str) -> devti_core::Result<Option<Vec<u8>>> {
        self.0.get(key.to_string()).map_err(|e| devti_core::Error::SecretStore(e.to_string()))
    }
    fn set(&self, key: &str, value: &[u8]) -> devti_core::Result<()> {
        self.0.set(key.to_string(), value.to_vec()).map_err(|e| devti_core::Error::SecretStore(e.to_string()))
    }
    fn delete(&self, key: &str) -> devti_core::Result<()> {
        self.0.delete(key.to_string()).map_err(|e| devti_core::Error::SecretStore(e.to_string()))
    }
    fn backend(&self) -> String {
        self.0.backend_name()
    }
    fn is_os_protected(&self) -> bool {
        true
    }
}

/// Static description of the phone/tablet running the app.
#[derive(Debug, Clone, uniffi::Record)]
pub struct MobileConfig {
    pub data_dir: String,
    pub device_name: String,
    /// `"phone"` or `"tablet"`.
    pub kind: String,
    /// `"android"`, `"ios"` or `"ipados"`.
    pub platform: String,
    pub os_version: String,
    pub model: Option<String>,
    pub app_version: String,
    pub discovery: bool,
}

/// A running Devti node inside the mobile app.
#[derive(uniffi::Object)]
pub struct DevtiMobile {
    runtime: tokio::runtime::Runtime,
    node: Node,
}

#[uniffi::export]
impl DevtiMobile {
    /// Starts the node. Blocks briefly while the identity and database load.
    #[uniffi::constructor]
    pub fn new(
        config: MobileConfig,
        secrets: Box<dyn PlatformSecretStore>,
        listener: Box<dyn EventListener>,
    ) -> Result<Arc<Self>, DevtiError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("devti-core")
            .enable_all()
            .build()
            .map_err(|e| DevtiError::Failed { reason: e.to_string() })?;

        let mut cfg =
            NodeConfig::new(PathBuf::from(&config.data_dir), &config.device_name, DeviceKind::parse(&config.kind));
        cfg.platform = Platform::parse(&config.platform);
        cfg.os_version = config.os_version;
        cfg.model = config.model;
        cfg.app_version = config.app_version;
        cfg.discovery = config.discovery;
        cfg.capabilities = vec![Capability::Battery, Capability::Clipboard];

        let secrets: Arc<dyn SecretStore> = Arc::new(SecretBridge(secrets));
        let node = runtime.block_on(Node::start(cfg, secrets))?;

        let mut events = node.subscribe();
        let listener: Arc<dyn EventListener> = Arc::from(listener);
        runtime.spawn(async move {
            loop {
                match events.recv().await {
                    Ok(ev) => {
                        if let Ok(json) = serde_json::to_string(&ev) {
                            listener.on_event(json);
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        listener.on_event(r#"{"type":"resync"}"#.to_string());
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Arc::new(Self { runtime, node }))
    }

    /// This device as JSON (`LocalSnapshot`).
    pub fn local_json(&self) -> String {
        serde_json::to_string(&self.node.local()).unwrap_or_default()
    }

    /// Paired devices as a JSON array of `PeerSnapshot`.
    pub fn peers_json(&self) -> String {
        serde_json::to_string(&self.node.peers()).unwrap_or_default()
    }

    /// Unpaired Devti devices on the network (JSON array of `DiscoveredDevice`).
    pub fn nearby_json(&self) -> String {
        serde_json::to_string(&self.node.nearby()).unwrap_or_default()
    }

    /// Starts pairing from a scanned `devti://pair?...` link. Returns the pairing id.
    pub fn pair_with_uri(&self, uri: String) -> Result<u64, DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.pair_with_uri(&uri)?)
    }

    /// Starts pairing with a discovered device using its 6-digit code.
    pub fn pair_with_code(&self, device_id: String, code: String) -> Result<u64, DevtiError> {
        let _g = self.runtime.enter();
        let id: DeviceId = device_id.parse()?;
        Ok(self.node.pair_with_code(&id, &code)?)
    }

    /// Lets another computer pair with *this* device using a code shown here (JSON
    /// `PairingWindowInfo`). Phones normally join a Hub instead; this covers the reverse.
    pub fn open_pairing_window_json(&self) -> Result<String, DevtiError> {
        let _g = self.runtime.enter();
        let w = self.node.open_pairing_window()?;
        Ok(serde_json::to_string(&w).unwrap_or_default())
    }

    pub fn close_pairing_window(&self) {
        self.node.close_pairing_window();
    }

    /// Answers "do the codes match?".
    pub fn confirm_pairing(&self, pairing_id: u64, accept: bool) -> Result<(), DevtiError> {
        Ok(self.node.confirm_pairing(pairing_id, accept)?)
    }

    pub fn set_battery(&self, percent: u8, charging: bool) {
        let _g = self.runtime.enter();
        self.node.set_status(DeviceStatus { battery: Some(BatteryState::new(percent, charging)) });
    }

    pub fn rename(&self, name: String) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        self.node.rename(&name)?;
        Ok(())
    }

    pub fn revoke(&self, device_id: String) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        let id: DeviceId = device_id.parse()?;
        Ok(self.node.revoke(&id)?)
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.set_paused(paused)?)
    }

    // ------------------------------------------------------------- Phase 2

    /// Shares clipboard text; returns false when skipped (empty, too big, or just received).
    pub fn send_clipboard(&self, text: String) -> Result<bool, DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.send_clipboard(&text)?)
    }

    /// Shares text only with the paired device selected in the phone UI.
    pub fn send_clipboard_to(&self, device_id: String, text: String) -> Result<(), DevtiError> {
        Ok(self.node.send_clipboard_to(&device_id.parse()?, &text)?)
    }

    /// Changes this device's clipboard grant while preserving the other permissions.
    pub fn set_clipboard_access(&self, device_id: String, allowed: bool) -> Result<(), DevtiError> {
        Ok(self.node.set_clipboard_access(&device_id.parse()?, allowed)?)
    }

    pub fn clear_clipboard_history(&self) -> Result<(), DevtiError> {
        Ok(self.node.clear_clipboard_history()?)
    }

    /// Clipboard history as JSON (`ClipboardEntry[]`).
    pub fn clipboard_history_json(&self) -> String {
        serde_json::to_string(&self.node.clipboard_history().unwrap_or_default()).unwrap_or_default()
    }

    /// Find my device: ring `device_id` (or stop).
    pub fn ring(&self, device_id: String, on: bool) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.ring(&device_id.parse()?, on)?)
    }

    /// Publishes a notification from this phone (JSON `NotificationInfo`).
    pub fn publish_notification(&self, json: String) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        let info: devti_core::protocol::NotificationInfo =
            serde_json::from_str(&json).map_err(|e| DevtiError::InvalidInput { reason: e.to_string() })?;
        self.node.publish_notification(info);
        Ok(())
    }

    pub fn remove_notification(&self, key: String) {
        let _g = self.runtime.enter();
        self.node.remove_notification(&key);
    }

    /// Notifications mirrored from other devices (JSON `MirroredNotification[]`).
    pub fn notifications_json(&self) -> String {
        serde_json::to_string(&self.node.notifications()).unwrap_or_default()
    }

    pub fn notification_action(
        &self,
        device_id: String,
        key: String,
        action: String,
        reply: Option<String>,
    ) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.notification_action(&device_id.parse()?, &key, &action, reply)?)
    }

    pub fn dismiss_notification(&self, device_id: String, key: String) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.dismiss_notification(&device_id.parse()?, &key)?)
    }

    pub fn set_dnd(&self, on: bool) {
        let _g = self.runtime.enter();
        self.node.set_dnd(on);
    }

    pub fn dnd(&self) -> bool {
        self.node.dnd()
    }

    /// Sends a file (a readable path, e.g. `/proc/self/fd/N` for an Android content URI).
    pub fn send_file(&self, device_id: String, path: String) -> Result<u64, DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.send_file(&device_id.parse()?, std::path::Path::new(&path))?)
    }

    pub fn cancel_transfer(&self, id: u64) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.cancel_transfer(id)?)
    }

    pub fn retry_transfer(&self, id: u64) -> Result<(), DevtiError> {
        let _g = self.runtime.enter();
        Ok(self.node.retry_transfer(id)?)
    }

    /// Recent transfers (JSON `TransferRecord[]`).
    pub fn transfers_json(&self) -> String {
        serde_json::to_string(&self.node.transfers().unwrap_or_default()).unwrap_or_default()
    }

    pub fn set_downloads_dir(&self, path: String) -> Result<(), DevtiError> {
        Ok(self.node.set_downloads_dir(std::path::Path::new(&path))?)
    }

    /// Battery samples since `since` (Unix ms) for a device, or this phone when `None`.
    pub fn battery_history_json(&self, device_id: Option<String>, since: i64) -> Result<String, DevtiError> {
        let id = device_id.map(|d| d.parse()).transpose()?;
        let h = self.node.battery_history(id.as_ref(), since)?;
        Ok(serde_json::to_string(&h).unwrap_or_default())
    }

    /// Call when the network changes or the app returns to the foreground.
    pub fn reconnect_now(&self) {
        self.node.reconnect_now();
    }

    /// Graceful stop (says goodbye to peers).
    pub fn shutdown(&self) {
        self.runtime.block_on(self.node.shutdown());
    }
}

/// Devti Core version string.
#[uniffi::export]
pub fn core_version() -> String {
    devti_core::VERSION.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemStore(Mutex<HashMap<String, Vec<u8>>>);
    impl PlatformSecretStore for MemStore {
        fn get(&self, key: String) -> Result<Option<Vec<u8>>, DevtiError> {
            Ok(self.0.lock().unwrap().get(&key).cloned())
        }
        fn set(&self, key: String, value: Vec<u8>) -> Result<(), DevtiError> {
            self.0.lock().unwrap().insert(key, value);
            Ok(())
        }
        fn delete(&self, key: String) -> Result<(), DevtiError> {
            self.0.lock().unwrap().remove(&key);
            Ok(())
        }
        fn backend_name(&self) -> String {
            "test".into()
        }
    }

    struct Sink(Arc<Mutex<Vec<String>>>);
    impl EventListener for Sink {
        fn on_event(&self, json: String) {
            self.0.lock().unwrap().push(json);
        }
    }

    #[test]
    fn mobile_node_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let m = DevtiMobile::new(
            MobileConfig {
                data_dir: dir.path().to_string_lossy().into(),
                device_name: "Test Phone".into(),
                kind: "phone".into(),
                platform: "android".into(),
                os_version: "Android 15".into(),
                model: Some("Pixel 8".into()),
                app_version: "0.1.0".into(),
                discovery: false,
            },
            Box::new(MemStore::default()),
            Box::new(Sink(events.clone())),
        )
        .unwrap();
        let local: serde_json::Value = serde_json::from_str(&m.local_json()).unwrap();
        assert_eq!(local["info"]["name"], "Test Phone");
        assert_eq!(local["info"]["platform"], "android");
        assert_eq!(local["secret_backend"], "test");
        assert_eq!(m.peers_json(), "[]");
        assert_eq!(m.nearby_json(), "[]");
        m.set_battery(80, true);
        m.rename("Renamed".into()).unwrap();
        assert!(matches!(m.pair_with_uri("garbage".into()), Err(DevtiError::InvalidInput { .. })));
        assert!(m.pair_with_code("bad".into(), "123456".into()).is_err());
        assert!(m.confirm_pairing(1, true).is_err());
        assert!(m.revoke("AAAAAAAAAAAAAAAA".into()).is_err());
        m.set_paused(true).unwrap();
        assert!(matches!(
            m.pair_with_uri("devti://pair".into()),
            Err(DevtiError::InvalidInput { .. }) | Err(DevtiError::Paused)
        ));
        m.set_paused(false).unwrap();
        m.reconnect_now();
        assert!(m.send_clipboard("hi".into()).unwrap());
        assert!(m.clipboard_history_json().contains("hi"));
        assert!(m.publish_notification("not json".into()).is_err());
        m.publish_notification(r#"{"key":"k","app_id":"a","app_name":"A","title":"t","text":"x","ts":1}"#.into())
            .unwrap();
        m.remove_notification("k".into());
        assert_eq!(m.notifications_json(), "[]");
        m.set_dnd(true);
        assert!(m.dnd());
        assert_eq!(m.transfers_json(), "[]");
        assert!(m.send_file("AAAAAAAAAAAAAAAA".into(), "/nope".into()).is_err());
        assert!(m.battery_history_json(None, 0).unwrap().starts_with('['));
        assert!(m.ring("bad".into(), true).is_err());
        std::thread::sleep(std::time::Duration::from_millis(100));
        assert!(events.lock().unwrap().iter().any(|e| e.contains("local_changed")));
        m.shutdown();
        assert!(!core_version().is_empty());
    }
}
