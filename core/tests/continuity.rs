//! Phase 2 end-to-end tests: clipboard, find my device, notifications, DND, battery,
//! and file transfer (including resume after a dropped connection) — real QUIC.

use std::sync::Arc;
use std::time::Duration;

use devti_core::model::{BatteryState, DeviceKind, DeviceStatus, Permissions, TrustRole};
use devti_core::node::{NodeEvent, PairingRole};
use devti_core::protocol::{NotifAction, NotificationInfo};
use devti_core::secret::MemorySecretStore;
use devti_core::{Node, NodeConfig};
use tokio::sync::broadcast::Receiver;

const WAIT: Duration = Duration::from_secs(20);

struct T {
    node: Node,
    ev: Receiver<NodeEvent>,
    dir: tempfile::TempDir,
}

async fn spawn(name: &str, kind: DeviceKind) -> T {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = NodeConfig::new(dir.path(), name, kind);
    cfg.bind_ip = "127.0.0.1".parse().unwrap();
    cfg.port = 0;
    cfg.discovery = false;
    cfg.downloads_dir = dir.path().join("downloads");
    let node = Node::start(cfg, Arc::new(MemorySecretStore::new())).await.unwrap();
    let ev = node.subscribe();
    T { node, ev, dir }
}

async fn wait_for<R>(rx: &mut Receiver<NodeEvent>, what: &str, mut f: impl FnMut(&NodeEvent) -> Option<R>) -> R {
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Ok(e)) => {
                if let Some(v) = f(&e) {
                    return v;
                }
            }
            Ok(Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) => continue,
            Ok(Err(e)) => panic!("channel closed waiting for {what}: {e}"),
            Err(_) => panic!("timed out waiting for {what}"),
        }
    }
}

async fn eventually(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !f() {
        assert!(tokio::time::Instant::now() < deadline, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn pair(hub: &mut T, phone: &mut T) {
    let w = hub.node.open_pairing_window().unwrap();
    let jid = phone.node.pair_with_uri(&w.uri).unwrap();
    let hid = wait_for(&mut hub.ev, "hub verify", |e| match e {
        NodeEvent::PairingVerify { pairing_id, role: PairingRole::Hub, .. } => Some(*pairing_id),
        _ => None,
    })
    .await;
    wait_for(&mut phone.ev, "joiner verify", |e| matches!(e, NodeEvent::PairingVerify { .. }).then_some(())).await;
    hub.node.confirm_pairing(hid, true).unwrap();
    phone.node.confirm_pairing(jid, true).unwrap();
    let (h, p) = (hub.node.clone(), phone.node.clone());
    let (hid, pid) = (h.device_id(), p.device_id());
    eventually("online", || h.peer(&pid).is_some_and(|x| x.online) && p.peer(&hid).is_some_and(|x| x.online)).await;
}

fn notif(key: &str) -> NotificationInfo {
    NotificationInfo {
        key: key.into(),
        app_id: "org.chat".into(),
        app_name: "Chat".into(),
        title: "Ayesha".into(),
        text: "Are you coming?".into(),
        ts: 1,
        actions: vec![NotifAction { id: "reply".into(), label: "Reply".into(), reply: true }],
        icon_png: None,
        dismissable: true,
    }
}

#[tokio::test]
async fn clipboard_syncs_without_echo_and_respects_permission() {
    let mut hub = spawn("Hub", DeviceKind::Laptop).await;
    let mut phone = spawn("Phone", DeviceKind::Phone).await;
    pair(&mut hub, &mut phone).await;

    assert!(hub.node.send_clipboard("hello from the laptop").unwrap());
    let text = wait_for(&mut phone.ev, "clipboard", |e| match e {
        NodeEvent::ClipboardReceived { text, from_name, .. } => {
            assert_eq!(from_name, "Hub");
            Some(text.clone())
        }
        _ => None,
    })
    .await;
    assert_eq!(text, "hello from the laptop");
    // The phone writes it to its OS clipboard, notices the change and tries to share it
    // back: suppressed, no loop.
    assert!(!phone.node.send_clipboard("hello from the laptop").unwrap());
    assert!(!hub.node.send_clipboard("   ").unwrap());
    assert_eq!(phone.node.clipboard_history().unwrap()[0].device_id, Some(hub.node.device_id()));
    assert_eq!(hub.node.clipboard_history().unwrap()[0].device_id, None);

    // The hub stops accepting the phone's clipboard.
    let pid = phone.node.device_id();
    let mut perms = Permissions::for_role(TrustRole::Trusted);
    perms.clipboard = false;
    hub.node.set_access(&pid, TrustRole::Trusted, perms).unwrap();
    assert!(phone.node.send_clipboard("secret").unwrap());
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(hub.node.clipboard_history().unwrap().iter().all(|c| c.text != "secret"));
    hub.node.clear_clipboard_history().unwrap();
    assert!(hub.node.clipboard_history().unwrap().is_empty());
}

#[tokio::test]
async fn targeted_clipboard_only_reaches_the_selected_peer() {
    let mut phone = spawn("Phone", DeviceKind::Phone).await;
    let mut laptop = spawn("Laptop", DeviceKind::Laptop).await;
    let mut other = spawn("Other", DeviceKind::Desktop).await;
    pair(&mut laptop, &mut phone).await;
    pair(&mut other, &mut phone).await;
    let id = laptop.node.device_id();
    phone.node.send_clipboard_to(&id, "for laptop only").unwrap();
    // A subsequent broadcast on the same ordered control stream bounds the negative assertion.
    assert!(phone.node.send_clipboard("barrier").unwrap());
    eventually("both received barrier", || {
        [&laptop, &other].iter().all(|p| p.node.clipboard_history().unwrap().iter().any(|c| c.text == "barrier"))
    })
    .await;
    assert!(laptop.node.clipboard_history().unwrap().iter().any(|c| c.text == "for laptop only"));
    assert!(other.node.clipboard_history().unwrap().iter().all(|c| c.text != "for laptop only"));

    let mut permissions = Permissions::for_role(TrustRole::Trusted);
    permissions.clipboard = false;
    phone.node.set_access(&id, TrustRole::Trusted, permissions).unwrap();
    assert!(phone.node.send_clipboard_to(&id, "blocked").is_err());
    assert!(phone.node.send_clipboard("only enabled devices").unwrap());
    // Re-enable and send a final marker to the laptop. Both paths share its control stream.
    permissions.clipboard = true;
    phone.node.set_access(&id, TrustRole::Trusted, permissions).unwrap();
    phone.node.send_clipboard_to(&id, "done").unwrap();
    eventually("ordered send", || laptop.node.clipboard_history().unwrap()[0].text == "done").await;
    assert!(laptop.node.clipboard_history().unwrap().iter().all(|c| c.text != "only enabled devices"));
    assert!(phone.node.send_clipboard_to(&id, " ").is_err());
    assert!(phone.node.send_clipboard_to(&id, &"a".repeat(256 * 1024 + 1)).is_err());
    phone.node.set_paused(true).unwrap();
    assert!(phone.node.send_clipboard_to(&id, "paused").is_err());
}

#[tokio::test]
async fn find_my_device_rings_and_restricted_devices_cannot() {
    let mut hub = spawn("Hub", DeviceKind::Laptop).await;
    let mut phone = spawn("Phone", DeviceKind::Phone).await;
    pair(&mut hub, &mut phone).await;
    let pid = phone.node.device_id();
    hub.node.ring(&pid, true).unwrap();
    let on = wait_for(&mut phone.ev, "ring", |e| match e {
        NodeEvent::RingRequested { on, .. } => Some(*on),
        _ => None,
    })
    .await;
    assert!(on);
    hub.node.ring(&pid, false).unwrap();
    assert!(
        !wait_for(&mut phone.ev, "stop", |e| match e {
            NodeEvent::RingRequested { on, .. } => Some(*on),
            _ => None,
        })
        .await
    );

    // The phone restricts the hub: no more ringing.
    let hid = hub.node.device_id();
    phone.node.set_access(&hid, TrustRole::Restricted, Permissions::for_role(TrustRole::Restricted)).unwrap();
    hub.node.ring(&pid, true).unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    while let Ok(e) = phone.ev.try_recv() {
        assert!(!matches!(e, NodeEvent::RingRequested { .. }), "restricted device rang the phone");
    }
    assert!(hub.node.ring(&devti_core::model::DeviceId::from_bytes([0; 10]), true).is_err());
}

#[tokio::test]
async fn notifications_mirror_with_actions_dismiss_and_dnd() {
    let mut hub = spawn("Hub", DeviceKind::Laptop).await;
    let mut phone = spawn("Phone", DeviceKind::Phone).await;
    pair(&mut hub, &mut phone).await;
    let pid = phone.node.device_id();

    phone.node.publish_notification(notif("chat:1"));
    let posted = wait_for(&mut hub.ev, "posted", |e| match e {
        NodeEvent::NotificationPosted { notification, dnd } => Some((notification.clone(), *dnd)),
        _ => None,
    })
    .await;
    assert_eq!(posted.0.notif.title, "Ayesha");
    assert_eq!(posted.0.from_name, "Phone");
    assert!(!posted.1);
    assert_eq!(hub.node.notifications().len(), 1);

    // Quick reply from the hub reaches the phone.
    hub.node.notification_action(&pid, "chat:1", "reply", Some("On my way".into())).unwrap();
    let (action, reply) = wait_for(&mut phone.ev, "action", |e| match e {
        NodeEvent::NotificationActionRequested { action, reply, .. } => Some((action.clone(), reply.clone())),
        _ => None,
    })
    .await;
    assert_eq!(action, "reply");
    assert_eq!(reply.as_deref(), Some("On my way"));

    // Dismiss on the hub removes it there at once and asks the phone to dismiss it too.
    hub.node.dismiss_notification(&pid, "chat:1").unwrap();
    assert!(hub.node.notifications().is_empty());
    wait_for(&mut phone.ev, "dismiss", |e| matches!(e, NodeEvent::NotificationDismissRequested { .. }).then_some(()))
        .await;

    // The phone withdraws a notification: gone on the hub.
    phone.node.publish_notification(notif("chat:2"));
    wait_for(&mut hub.ev, "posted 2", |e| matches!(e, NodeEvent::NotificationPosted { .. }).then_some(())).await;
    phone.node.remove_notification("chat:2");
    wait_for(&mut hub.ev, "removed", |e| match e {
        NodeEvent::NotificationRemoved { key, .. } if key == "chat:2" => Some(()),
        _ => None,
    })
    .await;

    // DND set on the hub reaches the phone, and new notifications are marked silent.
    hub.node.set_dnd(true);
    assert!(hub.node.dnd());
    wait_for(&mut phone.ev, "dnd", |e| matches!(e, NodeEvent::DndChanged { on: true, by: Some(_) }).then_some(()))
        .await;
    assert!(phone.node.dnd());
    phone.node.publish_notification(notif("chat:3"));
    let dnd = wait_for(&mut hub.ev, "silent", |e| match e {
        NodeEvent::NotificationPosted { dnd, .. } => Some(*dnd),
        _ => None,
    })
    .await;
    assert!(dnd);

    // Hostile sizes are capped.
    let mut big = notif("chat:4");
    big.text = "x".repeat(100_000);
    phone.node.publish_notification(big);
    let len = wait_for(&mut hub.ev, "capped", |e| match e {
        NodeEvent::NotificationPosted { notification, .. } if notification.notif.key == "chat:4" => {
            Some(notification.notif.text.len())
        }
        _ => None,
    })
    .await;
    assert_eq!(len, 4096);

    // When the phone goes away its notifications leave with it.
    phone.node.shutdown().await;
    eventually("cleared", || hub.node.notifications().is_empty()).await;
}

#[tokio::test]
async fn battery_history_and_low_battery_alert() {
    let mut hub = spawn("Hub", DeviceKind::Laptop).await;
    let mut phone = spawn("Phone", DeviceKind::Phone).await;
    pair(&mut hub, &mut phone).await;
    hub.node.set_low_battery_threshold(20).unwrap();
    assert_eq!(hub.node.low_battery_threshold(), 20);
    let pid = phone.node.device_id();
    for pct in [50u8, 30, 19] {
        phone.node.set_status(DeviceStatus { battery: Some(BatteryState::new(pct, false)) });
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
    let pct = wait_for(&mut hub.ev, "low battery", |e| match e {
        NodeEvent::LowBattery { percent, .. } => Some(*percent),
        _ => None,
    })
    .await;
    assert_eq!(pct, 19);
    let hist = hub.node.battery_history(Some(&pid), 0).unwrap();
    assert!(hist.len() >= 3, "{hist:?}");
    assert_eq!(hist.last().unwrap().percent, 19);
    // The phone records its own history too.
    assert!(!phone.node.battery_history(None, 0).unwrap().is_empty());
}

#[tokio::test]
async fn file_transfer_verifies_and_resumes_after_a_drop() {
    let mut hub = spawn("Hub", DeviceKind::Laptop).await;
    let mut phone = spawn("Phone", DeviceKind::Phone).await;
    pair(&mut hub, &mut phone).await;
    let hid = hub.node.device_id();

    // A small file arrives intact under a safe, unique name.
    let small = phone.dir.path().join("note.txt");
    std::fs::write(&small, b"hi there").unwrap();
    std::fs::create_dir_all(hub.node.downloads_dir()).unwrap();
    std::fs::write(hub.node.downloads_dir().join("note.txt"), b"existing").unwrap();
    let id = phone.node.send_file(&hid, &small).unwrap();
    let done = wait_for(&mut hub.ev, "small done", |e| match e {
        NodeEvent::Transfer { transfer, .. } if transfer.id == id && transfer.state == "done" => Some(transfer.clone()),
        _ => None,
    })
    .await;
    assert!(done.path.ends_with("note (2).txt"), "{}", done.path);
    assert_eq!(std::fs::read(&done.path).unwrap(), b"hi there");

    // A large file is interrupted half way (kill switch) and then resumes.
    let big = phone.dir.path().join("video.bin");
    let data: Vec<u8> = (0..48 * 1024 * 1024u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
    std::fs::write(&big, &data).unwrap();
    let id = phone.node.send_file(&hid, &big).unwrap();
    wait_for(&mut hub.ev, "progress", |e| match e {
        NodeEvent::Transfer { transfer, .. } if transfer.id == id && transfer.done > 4 * 1024 * 1024 => Some(()),
        _ => None,
    })
    .await;
    hub.node.set_paused(true).unwrap();
    let paused_at = wait_for(&mut hub.ev, "paused", |e| match e {
        NodeEvent::Transfer { transfer, .. } if transfer.id == id && transfer.state == "paused" => Some(transfer.done),
        _ => None,
    })
    .await;
    assert!(paused_at > 0 && paused_at < data.len() as u64, "paused at {paused_at}");
    hub.node.set_paused(false).unwrap();
    phone.node.reconnect_now();
    // On resume the receiver restarts from where it stopped, not from zero.
    let resumed_from = wait_for(&mut hub.ev, "resumed", |e| match e {
        NodeEvent::Transfer { transfer, .. } if transfer.id == id && transfer.state == "active" => Some(transfer.done),
        _ => None,
    })
    .await;
    assert!(
        resumed_from >= paused_at.saturating_sub(1024 * 1024),
        "resumed from {resumed_from}, paused at {paused_at}"
    );
    assert!(resumed_from > 0);
    let done = wait_for(&mut hub.ev, "big done", |e| match e {
        NodeEvent::Transfer { transfer, .. } if transfer.id == id && transfer.state == "done" => Some(transfer.clone()),
        _ => None,
    })
    .await;
    assert_eq!(std::fs::read(&done.path).unwrap(), data, "content must match byte for byte");
    // No partial files are left behind.
    let leftovers: Vec<_> = std::fs::read_dir(hub.node.downloads_dir())
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
        .collect();
    assert!(leftovers.is_empty());
    let p = phone.node.clone();
    eventually("sender marks done", || p.transfers().unwrap().iter().any(|t| t.id == id && t.state == "done")).await;
}

#[tokio::test]
async fn files_are_refused_without_permission_and_can_be_cancelled() {
    let mut hub = spawn("Hub", DeviceKind::Laptop).await;
    let mut phone = spawn("Phone", DeviceKind::Phone).await;
    pair(&mut hub, &mut phone).await;
    let (hid, pid) = (hub.node.device_id(), phone.node.device_id());
    let mut perms = Permissions::for_role(TrustRole::Trusted);
    perms.files = false;
    hub.node.set_access(&pid, TrustRole::Trusted, perms).unwrap();

    let f = phone.dir.path().join("x.txt");
    std::fs::write(&f, b"data").unwrap();
    let id = phone.node.send_file(&hid, &f).unwrap();
    wait_for(&mut phone.ev, "declined", |e| match e {
        NodeEvent::Transfer { transfer, .. } if transfer.id == id && transfer.state == "failed" => Some(()),
        _ => None,
    })
    .await;
    assert!(phone.node.send_file(&hid, phone.dir.path()).is_err(), "folders are refused");
    assert!(phone.node.send_file(&hid, &phone.dir.path().join("missing")).is_err());

    // A queued transfer to an offline device can be cancelled.
    hub.node.shutdown().await;
    eventually("hub offline", || phone.node.peer(&hid).is_some_and(|p| !p.online)).await;
    let id = phone.node.send_file(&hid, &f).unwrap();
    phone.node.cancel_transfer(id).unwrap();
    assert!(phone.node.transfers().unwrap().iter().any(|t| t.id == id && t.state == "cancelled"));
    phone.node.clear_finished_transfers().unwrap();
    assert!(phone.node.cancel_transfer(999).is_err());
}
