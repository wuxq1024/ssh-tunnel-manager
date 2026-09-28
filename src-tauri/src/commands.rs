//! Tauri 命令层：CRUD / 启停 / 测试连接 / 设置

use std::sync::Arc;
use std::time::Instant;

use tauri::{AppHandle, Manager, State};use crate::config::{load_store, save_store, AppSettings, AuthMethod, TunnelConfig};
use crate::manager::TunnelManager;
use crate::{credentials, known_hosts};

#[tauri::command]
pub fn list_tunnels() -> Vec<TunnelConfig> {
    load_store().tunnels
}

#[tauri::command]
pub fn save_tunnel(
    config: TunnelConfig,
    password: Option<String>,
    passphrase: Option<String>,
) -> Result<TunnelConfig, String> {
    let mut store = load_store();

    // 持久化凭据（若提供了新值）
    if let Some(pw) = &password {
        credentials::set_password(&config.id, pw)?;
    }
    if let Some(pp) = &passphrase {
        credentials::set_passphrase(&config.id, pp)?;
    }

    let mut cfg = config;
    // 更新 has_stored_* 标志
    match &mut cfg.auth_method {
        AuthMethod::Password {
            has_stored_password,
        } => {
            *has_stored_password = credentials::get_password(&cfg.id).is_some();
        }
        AuthMethod::Key {
            has_stored_passphrase,
            ..
        } => {
            *has_stored_passphrase = credentials::get_passphrase(&cfg.id).is_some();
        }
    }

    let now = chrono::Utc::now().timestamp_millis();
    let idx = store.tunnels.iter().position(|t| t.id == cfg.id);
    match idx {
        Some(i) => {
            cfg.created_at = store.tunnels[i].created_at;
            cfg.updated_at = now;
            store.tunnels[i] = cfg.clone();
        }
        None => {
            cfg.created_at = now;
            cfg.updated_at = now;
            store.tunnels.push(cfg.clone());
        }
    }
    save_store(&store).map_err(|e| format!("{e}"))?;
    Ok(cfg)
}

#[tauri::command]
pub fn delete_tunnel(id: String, manager: State<'_, TunnelManager>) -> Result<(), String> {
    manager.stop(&id);
    credentials::purge(&id);
    let mut store = load_store();
    store.tunnels.retain(|t| t.id != id);
    save_store(&store).map_err(|e| format!("{e}"))?;
    Ok(())
}

#[tauri::command]
pub fn start_tunnel(id: String, manager: State<'_, Arc<TunnelManager>>) -> Result<(), String> {
    let store = load_store();
    let cfg = store
        .tunnels
        .into_iter()
        .find(|t| t.id == id)
        .ok_or_else(|| "任务不存在".to_string())?;
    let mgr = manager.inner().clone();
    mgr.start(cfg)
}

#[tauri::command]
pub fn stop_tunnel(id: String, manager: State<'_, Arc<TunnelManager>>) {
    manager.stop(&id);
}

#[tauri::command]
pub fn get_tunnel_stats(
    id: String,
    manager: State<'_, Arc<TunnelManager>>,
) -> Result<crate::manager::TunnelStats, String> {
    manager
        .stats_of(&id)
        .ok_or_else(|| "任务未在运行".to_string())
}

#[tauri::command]
pub fn get_settings() -> AppSettings {
    load_store().settings
}

#[tauri::command]
pub fn save_settings(settings: AppSettings) -> Result<(), String> {
    let mut store = load_store();
    store.settings = settings;
    save_store(&store).map_err(|e| format!("{e}"))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TestReport {
    pub ok: bool,
    pub message: String,
    pub elapsed_ms: u64,
    pub server_fingerprint: Option<String>,
}

/// 测试连接（一次性，不注册转发，用完即断）
#[tauri::command]
pub async fn test_connection(
    host: String,
    port: u16,
    username: String,
    auth_method: AuthMethod,
    password: Option<String>,
    passphrase: Option<String>,
    _use_stored_password: Option<bool>,
) -> Result<TestReport, String> {
    let t0 = Instant::now();

    // 构造临时配置。密码认证时优先用参数传入的密码（测试用，不落存储）
    let cfg = TunnelConfig {
        id: "__test__".into(),
        name: "test".into(),
        host,
        port,
        username,
        auth_method: auth_method.clone(),
        rules: vec![],
        auto_connect: false,
        auto_reconnect: false,
        keepalive_secs: 30,
        created_at: 0,
        updated_at: 0,
    };

    let (ev_tx, _ev_rx) = tokio::sync::mpsc::unbounded_channel();
    let stats = crate::manager::StatsHandle::new("__test__", ev_tx);

    let pp = passphrase.clone();

    // 密钥认证：临时把 passphrase 注入 —— 通过覆盖 load 流程实现：
    // connect_authenticated 从凭据管理器读 passphrase，但测试用例没有存储。
    // 简化：测试时若提供了 passphrase，先临时写入再删除。
    let passphrase_stash = if let AuthMethod::Key { .. } = &cfg.auth_method {
        if let Some(pp) = &pp {
            let _ = crate::credentials::set_passphrase("__test__", pp);
            Some(true)
        } else {
            None
        }
    } else {
        None
    };

    let result = crate::session::connect_authenticated(
        &cfg,
        password.as_deref(),
        stats,
        Arc::new(dashmap::DashMap::<String, (String, u16)>::new()),
    )
    .await;

    // 清理临时 passphrase
    if passphrase_stash.is_some() {
        crate::credentials::delete_passphrase("__test__");
    }

    // 测试连接会因 TOFU 记录指纹，读取给用户看
    match result {
        Ok(handle) => {
            let fp = known_hosts::known_fingerprint(&cfg.host, cfg.port);
            crate::session::disconnect(&handle).await;
            Ok(TestReport {
                ok: true,
                message: "连接成功".into(),
                elapsed_ms: t0.elapsed().as_millis() as u64,
                server_fingerprint: fp,
            })
        }
        Err(e) => Ok(TestReport {
            ok: false,
            message: format!("{e}"),
            elapsed_ms: t0.elapsed().as_millis() as u64,
            server_fingerprint: None,
        }),
    }
}

/// 清除某主机指纹（指纹变更后用户确认服务器安全时用）
#[tauri::command]
pub fn forget_host_key(host: String, port: u16) {
    known_hosts::forget(&host, port);
}

#[tauri::command]
pub fn set_auto_start(app: AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    if enabled {
        app.autolaunch()
            .enable()
            .map_err(|e| format!("开启自启失败: {e}"))?;
    } else {
        app.autolaunch()
            .disable()
            .map_err(|e| format!("关闭自启失败: {e}"))?;
    }
    Ok(())
}

/// 应用启动时拉起 auto_connect 任务
pub fn autostart_tunnels(app: &AppHandle) {
    let store = load_store();
    let manager = app.state::<Arc<TunnelManager>>();
    let mgr: Arc<TunnelManager> = manager.inner().clone();
    let count = store.tunnels.iter().filter(|t| t.auto_connect).count();
    for cfg in store.tunnels.into_iter().filter(|t| t.auto_connect) {
        let _ = mgr.start(cfg);
    }
    if count > 0 {
        use tauri::Emitter;
        let _ = app.emit(
            "tunnel-log",
            serde_json::json!({
                "id": "*", "ts": chrono::Utc::now().timestamp_millis(),
                "level": "info", "message": format!("应用启动：自动连接 {count} 个任务")
            }),
        );
    }
}
