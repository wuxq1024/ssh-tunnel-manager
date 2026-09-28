//! SSH Tunnel Manager — Tauri 应用入口

mod commands;
mod config;
mod credentials;
mod forward;
mod known_hosts;
mod manager;
mod session;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};
use tauri_plugin_autostart::MacosLauncher;

use manager::{ManagerEvent, TunnelManager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (manager, mut event_rx) = TunnelManager::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(manager.clone())
        .invoke_handler(tauri::generate_handler![
            commands::list_tunnels,
            commands::save_tunnel,
            commands::delete_tunnel,
            commands::start_tunnel,
            commands::stop_tunnel,
            commands::get_tunnel_stats,
            commands::get_settings,
            commands::save_settings,
            commands::test_connection,
            commands::forget_host_key,
            commands::set_auto_start,
        ])
        .setup(move |app| {
            // ---- 事件转发: manager channel → Tauri events ----
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                while let Some(ev) = event_rx.recv().await {
                    forward_event(&app_handle, ev);
                }
            });

            // ---- 托盘 ----
            let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("SSH Tunnel Manager")
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => show_main_window(app),
                        "quit" => {
                            app.state::<TunnelManager>().stop_all();
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // ---- 关闭按钮 → 最小化到托盘（按设置） ----
            let win = app.get_webview_window("main").unwrap();
            {
                let app_handle = app.handle().clone();
                win.on_window_event(move |e| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                        let settings = config::load_store().settings;
                        if settings.close_to_tray {
                            api.prevent_close();
                            let _ = app_handle.get_webview_window("main").map(|w| w.hide());
                        } else {
                            let mgr = app_handle.state::<TunnelManager>();
                            mgr.stop_all();
                        }
                    }
                });
            }

            // ---- --minimized 启动参数 ----
            if std::env::args().any(|a| a == "--minimized") {
                win.hide()?;
            }

            // ---- 自动连接 ----
            commands::autostart_tunnels(app.handle());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn forward_event(app: &AppHandle, ev: ManagerEvent) {
    match ev {
        ManagerEvent::StateChanged { id, state, detail } => {
            let _ = app.emit(
                "tunnel-state-changed",
                serde_json::json!({ "id": id, "state": state, "detail": detail }),
            );
        }
        ManagerEvent::Log { id, ts, level, message } => {
            let _ = app.emit(
                "tunnel-log",
                serde_json::json!({ "id": id, "ts": ts, "level": level, "message": message }),
            );
        }
        ManagerEvent::Stats(s) => {
            let _ = app.emit("tunnel-stats", serde_json::to_value(s).unwrap_or_default());
        }
    }
}
