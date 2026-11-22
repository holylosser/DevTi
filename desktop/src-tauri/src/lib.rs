//! Devti Hub — the Tauri host around Devti Core.
//!
//! Sections:
//! * [`run`] — boots logging, the core node, plugins, tray and the event bridge.
//! * [`commands`] — everything the React UI may ask for (thin wrappers over `Node`).
//! * [`tray`] — tray icon and menu; the app keeps running when the window closes.
//! * [`bluetooth`] — live BLE scan used by onboarding.
//! * [`system`] — battery polling, OS details, diagnostics export.

mod bluetooth;
mod commands;
mod continuity;
mod system;
mod tray;

use std::sync::Arc;

use devti_core::model::{Capability, DeviceKind};
use devti_core::{Node, NodeConfig};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;

pub(crate) struct AppState {
    pub node: Node,
    pub bluetooth: bluetooth::Scanner,
    pub log_dir: std::path::PathBuf,
    pub data_dir: std::path::PathBuf,
}

/// Opens (or focuses) the main window. The window is destroyed when closed to the
/// tray, so this recreates it from the config when needed.
pub(crate) fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    let Some(cfg) = app.config().app.windows.iter().find(|w| w.label == "main").cloned() else { return };
    match WebviewWindowBuilder::from_config(app, &cfg).and_then(|b| b.build()) {
        Ok(w) => {
            let _ = w.set_focus();
        }
        Err(e) => tracing::error!(error = %e, "could not open the Devti window"),
    }
}

fn init_logging(log_dir: &std::path::Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::prelude::*;
    let _ = std::fs::create_dir_all(log_dir);
    let file = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("devti")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,devti_core=debug,devti_desktop_lib=debug"));
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::fmt::layer().json().with_writer(writer))
        .try_init();
    Some(guard)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let start_hidden = std::env::args().any(|a| a == "--hidden");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            show_main_window(app);
            continuity::request_send(app, continuity::send_args(&args));
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            let log_dir = handle.path().app_log_dir()?;
            let guard = init_logging(&log_dir);
            // Keep the log writer alive for the whole process.
            std::mem::forget(guard);

            let has_battery = system::read_battery().is_some();
            let kind = if has_battery { DeviceKind::Laptop } else { DeviceKind::Desktop };
            let mut cfg = NodeConfig::new(&data_dir, &system::default_device_name(), kind);
            cfg.os_version = system::os_version();
            cfg.app_version = env!("CARGO_PKG_VERSION").to_string();
            cfg.capabilities = vec![Capability::Battery, Capability::BluetoothHub, Capability::Clipboard];
            let secrets = devti_core::secret::best_available("app.devti.hub", data_dir.join("secrets"))?;
            let node = tauri::async_runtime::block_on(Node::start(cfg, Arc::from(secrets)))?;

            // Forward every core event to the UI.
            let mut events = node.subscribe();
            let emitter = handle.clone();
            let event_node = node.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    match events.recv().await {
                        Ok(ev) => {
                            tray::on_node_event(&emitter, &ev);
                            continuity::on_node_event(&emitter, &event_node, &ev);
                            let _ = emitter.emit("devti://event", ev);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                            let _ = emitter.emit("devti://event", serde_json::json!({ "type": "resync" }));
                        }
                        Err(_) => break,
                    }
                }
            });

            system::spawn_battery_poller(node.clone());
            continuity::spawn_clipboard_watcher(node.clone());
            app.manage(AppState { node: node.clone(), bluetooth: bluetooth::Scanner::default(), log_dir, data_dir });
            tray::build(&handle, node.is_paused())?;
            if !start_hidden {
                show_main_window(&handle);
            }
            let send = continuity::send_args(&std::env::args().collect::<Vec<_>>());
            continuity::request_send(&handle, send);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::snapshot,
            commands::system_info,
            commands::open_pairing_window,
            commands::close_pairing_window,
            commands::confirm_pairing,
            commands::pair_with_code,
            commands::pair_with_code_at,
            commands::revoke,
            commands::set_access,
            commands::set_paused,
            commands::rename,
            commands::activity,
            commands::set_setting,
            commands::set_autostart,
            commands::bluetooth_scan_start,
            commands::bluetooth_scan_stop,
            commands::export_diagnostics,
            commands::take_ui_action,
            commands::take_pending_send,
            commands::clipboard_history,
            commands::clear_clipboard_history,
            commands::copy_to_clipboard,
            commands::share_clipboard,
            commands::ring,
            commands::notifications,
            commands::notification_action,
            commands::dismiss_notification,
            commands::set_dnd,
            commands::pick_files,
            commands::send_files,
            commands::transfers,
            commands::cancel_transfer,
            commands::retry_transfer,
            commands::clear_finished_transfers,
            commands::open_path,
            commands::reveal_path,
            commands::choose_downloads_dir,
            commands::battery_history,
            commands::set_low_battery_threshold,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Devti");

    app.run(|app, event| match event {
        // Closing the last window keeps Devti alive in the tray (unless the user turned
        // that off); its webview memory is released because the window is destroyed.
        RunEvent::ExitRequested { api, code: None, .. } => {
            let keep = app
                .try_state::<AppState>()
                .and_then(|s| s.node.setting("close_to_tray").ok().flatten())
                .is_none_or(|v| v != "false");
            if keep {
                api.prevent_exit();
                tray::notify_still_running(app);
            }
        }
        RunEvent::Exit => {
            if let Some(state) = app.try_state::<AppState>() {
                tauri::async_runtime::block_on(state.node.shutdown());
            }
        }
        _ => {}
    });
}
