//! Phase 2 continuity on the control channel: clipboard, find my device, mirrored
//! notifications, ecosystem-wide Do Not Disturb, battery history and low-battery alerts.
//!
//! Every incoming request is checked against the permission the *local* user granted
//! the sender (`Permissions`), so a restricted device can't push clipboards or ring
//! this device even if its software tries.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};

use super::{Inner, NodeEvent};
use crate::error::{Error, Result};
use crate::model::{DeviceId, DeviceStatus, Permissions};
use crate::net::now_ms;
use crate::protocol::{CLIPBOARD_MAX_BYTES, ControlMsg, NotificationInfo};
use crate::store::BatterySample;

pub(crate) const SETTING_DND: &str = "dnd";
pub(crate) const SETTING_LOW_BATTERY: &str = "low_battery_threshold";
const DEFAULT_LOW_BATTERY: u8 = 15;

/// A notification mirrored from another device, as the UI shows it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MirroredNotification {
    pub from: DeviceId,
    pub from_name: String,
    pub notif: NotificationInfo,
}

pub(crate) struct State {
    /// Text most recently sent or applied, so applying a received clipboard doesn't
    /// bounce it straight back out.
    last_clipboard: Mutex<Option<String>>,
    /// Notifications this device publishes (resent to peers that reconnect).
    local_notifications: RwLock<HashMap<String, NotificationInfo>>,
    /// Notifications mirrored from peers.
    mirrored: RwLock<HashMap<DeviceId, Vec<NotificationInfo>>>,
    dnd: AtomicBool,
    low_battery: AtomicU8,
    alerted: Mutex<HashSet<DeviceId>>,
}

impl State {
    pub fn new(dnd: bool) -> Self {
        Self {
            last_clipboard: Mutex::new(None),
            local_notifications: RwLock::new(HashMap::new()),
            mirrored: RwLock::new(HashMap::new()),
            dnd: AtomicBool::new(dnd),
            low_battery: AtomicU8::new(DEFAULT_LOW_BATTERY),
            alerted: Mutex::new(HashSet::new()),
        }
    }
}

impl Inner {
    /// What the local user allows `peer` to do here (`None` if it isn't paired).
    pub(crate) fn permissions_of(&self, peer: &DeviceId) -> Option<Permissions> {
        self.peers.read().get(peer).map(|p| p.record.permissions)
    }

    fn peer_name(&self, peer: &DeviceId) -> String {
        self.peers.read().get(peer).map(|p| p.record.info.name.clone()).unwrap_or_default()
    }

    /// Sends one control message to one connected peer.
    pub(crate) fn send_to(&self, peer: &DeviceId, msg: ControlMsg) -> Result<()> {
        let peers = self.peers.read();
        let p = peers.get(peer).ok_or_else(|| Error::UnknownPeer(peer.to_string()))?;
        if matches!(msg, ControlMsg::Clipboard { .. }) && !p.record.permissions.clipboard {
            return Err(Error::InvalidInput("Clipboard sharing is off for this device".into()));
        }
        let s = p.session.as_ref().ok_or_else(|| Error::Connection(format!("{} is offline", p.record.info.name)))?;
        s.tx.send(msg).map_err(|_| Error::Connection("session closed".into()))
    }
}

/// Dispatches a continuity message from `peer`.
pub(crate) fn handle(inner: &Arc<Inner>, peer: DeviceId, msg: ControlMsg) {
    let Some(perms) = inner.permissions_of(&peer) else { return };
    let name = inner.peer_name(&peer);
    match msg {
        ControlMsg::Clipboard { text, ts } => {
            if !perms.clipboard || text.is_empty() || text.len() > CLIPBOARD_MAX_BYTES {
                return;
            }
            *inner.continuity.last_clipboard.lock() = Some(text.clone());
            let _ = inner.store.add_clipboard(ts, Some((&peer, &name)), &text);
            inner.emit(NodeEvent::ClipboardReceived { from: peer, from_name: name, text, ts });
        }
        ControlMsg::Ring { on } => {
            if perms.commands {
                inner.log(Some((&peer, &name)), if on { "ring_requested" } else { "ring_stopped" }, "");
                inner.emit(NodeEvent::RingRequested { from: peer, from_name: name, on });
            }
        }
        ControlMsg::Notification { notif } => {
            if !perms.notifications {
                return;
            }
            let notif = notif.sanitized();
            {
                let mut m = inner.continuity.mirrored.write();
                let list = m.entry(peer).or_default();
                list.retain(|n| n.key != notif.key);
                list.push(notif.clone());
                if list.len() > 200 {
                    list.remove(0);
                }
            }
            inner.emit(NodeEvent::NotificationPosted {
                notification: MirroredNotification { from: peer, from_name: name, notif },
                dnd: inner.continuity.dnd.load(Ordering::SeqCst),
            });
        }
        ControlMsg::NotificationRemoved { key } => {
            if let Some(list) = inner.continuity.mirrored.write().get_mut(&peer) {
                list.retain(|n| n.key != key);
            }
            inner.emit(NodeEvent::NotificationRemoved { from: peer, key });
        }
        ControlMsg::NotificationAction { key, action, reply } => {
            if perms.notifications && inner.continuity.local_notifications.read().contains_key(&key) {
                inner.emit(NodeEvent::NotificationActionRequested { from: peer, key, action, reply });
            }
        }
        ControlMsg::NotificationDismiss { key } => {
            if perms.notifications && inner.continuity.local_notifications.read().contains_key(&key) {
                inner.emit(NodeEvent::NotificationDismissRequested { from: peer, key });
            }
        }
        ControlMsg::Dnd { on } if perms.notifications => {
            apply_dnd(inner, on, Some(peer));
        }
        _ => {}
    }
}

fn apply_dnd(inner: &Arc<Inner>, on: bool, by: Option<DeviceId>) -> bool {
    let changed = inner.continuity.dnd.swap(on, Ordering::SeqCst) != on;
    if changed {
        let _ = inner.store.set_setting(SETTING_DND, if on { "true" } else { "false" });
        inner.emit(NodeEvent::DndChanged { on, by });
    }
    changed
}

/// A session came up: share our current notifications and DND state with the peer.
pub(crate) fn on_session_up(inner: &Arc<Inner>, peer: DeviceId) {
    let notifs: Vec<NotificationInfo> = inner.continuity.local_notifications.read().values().cloned().collect();
    for notif in notifs {
        let _ = inner.send_to(&peer, ControlMsg::Notification { notif });
    }
    if inner.continuity.dnd.load(Ordering::SeqCst) {
        let _ = inner.send_to(&peer, ControlMsg::Dnd { on: true });
    }
}

/// A session went down: its mirrored notifications are stale now.
pub(crate) fn on_session_down(inner: &Arc<Inner>, peer: DeviceId) {
    let gone = inner.continuity.mirrored.write().remove(&peer).unwrap_or_default();
    for n in gone {
        inner.emit(NodeEvent::NotificationRemoved { from: peer, key: n.key });
    }
}

/// Records battery history for a peer and raises a low-battery alert once per discharge.
pub(crate) fn on_peer_status(inner: &Arc<Inner>, peer: DeviceId, name: &str, status: &DeviceStatus) {
    let Some(b) = status.battery else { return };
    let _ = inner.store.add_battery(&peer, BatterySample { ts: now_ms(), percent: b.percent, charging: b.charging });
    let threshold = inner.continuity.low_battery.load(Ordering::SeqCst);
    let mut alerted = inner.continuity.alerted.lock();
    if !b.charging && b.percent <= threshold {
        if alerted.insert(peer) {
            drop(alerted);
            inner.log(Some((&peer, name)), "low_battery", &format!("{}%", b.percent));
            inner.emit(NodeEvent::LowBattery { id: peer, name: name.to_string(), percent: b.percent });
        }
    } else if b.charging || b.percent > threshold.saturating_add(5) {
        alerted.remove(&peer);
    }
}

// ------------------------------------------------------------------- public API

impl super::Node {
    /// Shares clipboard text with every connected device that accepts it.
    /// Returns `false` when the text was skipped (empty, too large, or just received).
    pub fn send_clipboard(&self, text: &str) -> Result<bool> {
        let inner = &self.inner;
        if text.trim().is_empty() || text.len() > CLIPBOARD_MAX_BYTES {
            return Ok(false);
        }
        {
            let mut last = inner.continuity.last_clipboard.lock();
            if last.as_deref() == Some(text) {
                return Ok(false);
            }
            *last = Some(text.to_string());
        }
        let ts = now_ms();
        inner.store.add_clipboard(ts, None, text)?;
        inner.broadcast(ControlMsg::Clipboard { text: text.to_string(), ts });
        Ok(true)
    }

    /// Queues a user-selected clipboard item for exactly one paired device.
    /// An explicit send may repeat a previous item or send it to a different peer.
    pub fn send_clipboard_to(&self, peer: &DeviceId, text: &str) -> Result<()> {
        let inner = &self.inner;
        inner.ensure_running()?;
        if text.trim().is_empty() || text.len() > CLIPBOARD_MAX_BYTES {
            return Err(Error::InvalidInput("Choose nonempty text of up to 256 KiB".into()));
        }
        let ts = now_ms();
        inner.send_to(peer, ControlMsg::Clipboard { text: text.into(), ts })?;
        // The message is already queued if saving local history fails.
        if let Err(e) = inner.store.add_clipboard(ts, None, text) {
            tracing::warn!(error = %e, "could not save outgoing clipboard history");
        }
        Ok(())
    }

    pub fn clipboard_history(&self) -> Result<Vec<crate::store::ClipboardEntry>> {
        self.inner.store.clipboard_history()
    }

    pub fn clear_clipboard_history(&self) -> Result<()> {
        self.inner.store.clear_clipboard()
    }

    /// Find my device: makes `peer` ring at full volume (or stop).
    pub fn ring(&self, peer: &DeviceId, on: bool) -> Result<()> {
        self.inner.send_to(peer, ControlMsg::Ring { on })?;
        let name = self.inner.peer_name(peer);
        self.inner.log(Some((peer, &name)), if on { "ring_sent" } else { "ring_stop_sent" }, "");
        Ok(())
    }

    /// Publishes (or updates) a notification from this device to the ecosystem.
    pub fn publish_notification(&self, notif: NotificationInfo) {
        let notif = notif.sanitized();
        self.inner.continuity.local_notifications.write().insert(notif.key.clone(), notif.clone());
        self.inner.broadcast(ControlMsg::Notification { notif });
    }

    /// Withdraws a notification this device published.
    pub fn remove_notification(&self, key: &str) {
        if self.inner.continuity.local_notifications.write().remove(key).is_some() {
            self.inner.broadcast(ControlMsg::NotificationRemoved { key: key.to_string() });
        }
    }

    /// Notifications mirrored from connected devices, newest first.
    pub fn notifications(&self) -> Vec<MirroredNotification> {
        let peers = self.inner.peers.read();
        let mut out: Vec<MirroredNotification> = self
            .inner
            .continuity
            .mirrored
            .read()
            .iter()
            .flat_map(|(id, list)| {
                let name = peers.get(id).map(|p| p.record.info.name.clone()).unwrap_or_default();
                list.iter().map(move |n| MirroredNotification { from: *id, from_name: name.clone(), notif: n.clone() })
            })
            .collect();
        out.sort_by_key(|n| std::cmp::Reverse(n.notif.ts));
        out
    }

    /// Runs an action (or sends a quick reply) on a notification owned by `peer`.
    pub fn notification_action(&self, peer: &DeviceId, key: &str, action: &str, reply: Option<String>) -> Result<()> {
        let reply = reply.map(|r| r.chars().take(2000).collect());
        self.inner.send_to(peer, ControlMsg::NotificationAction { key: key.into(), action: action.into(), reply })
    }

    /// Dismisses a notification everywhere: here at once, and on the device that owns it.
    pub fn dismiss_notification(&self, peer: &DeviceId, key: &str) -> Result<()> {
        if let Some(list) = self.inner.continuity.mirrored.write().get_mut(peer) {
            list.retain(|n| n.key != key);
        }
        self.inner.emit(NodeEvent::NotificationRemoved { from: *peer, key: key.to_string() });
        self.inner.send_to(peer, ControlMsg::NotificationDismiss { key: key.into() })
    }

    /// Ecosystem-wide Do Not Disturb.
    pub fn set_dnd(&self, on: bool) {
        if apply_dnd(&self.inner, on, None) {
            self.inner.broadcast(ControlMsg::Dnd { on });
        }
    }

    pub fn dnd(&self) -> bool {
        self.inner.continuity.dnd.load(Ordering::SeqCst)
    }

    /// Battery samples for a device (or this one when `device` is `None`) since `since` (Unix ms).
    pub fn battery_history(&self, device: Option<&DeviceId>, since: i64) -> Result<Vec<BatterySample>> {
        let own = self.device_id();
        self.inner.store.battery_history(device.unwrap_or(&own), since)
    }

    /// Battery level at or below which a low-battery alert fires (5–50 %).
    pub fn set_low_battery_threshold(&self, percent: u8) -> Result<()> {
        let p = percent.clamp(5, 50);
        self.inner.continuity.low_battery.store(p, Ordering::SeqCst);
        self.inner.store.set_setting(SETTING_LOW_BATTERY, &p.to_string())
    }

    pub fn low_battery_threshold(&self) -> u8 {
        self.inner.continuity.low_battery.load(Ordering::SeqCst)
    }

    /// Loads persisted continuity settings (called once at start).
    pub(crate) fn load_continuity_settings(&self) {
        if let Ok(Some(v)) = self.inner.store.setting(SETTING_LOW_BATTERY)
            && let Ok(p) = v.parse::<u8>()
        {
            self.inner.continuity.low_battery.store(p.clamp(5, 50), Ordering::SeqCst);
        }
    }
}
