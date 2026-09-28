//! 隧道任务配置：数据模型 + JSON 持久化（原子写）

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

// ---------- 数据模型 ----------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "forward", rename_all = "lowercase")]
pub enum ForwardRule {
    /// -R [bind_addr:]bind_port:target_host:target_port
    Remote {
        bind_addr: String,
        bind_port: u16,
        target_host: String,
        target_port: u16,
    },
    /// -L
    Local {
        bind_addr: String,
        bind_port: u16,
        target_host: String,
        target_port: u16,
    },
    /// -D (SOCKS5)
    Dynamic {
        bind_addr: String,
        bind_port: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "password", rename_all = "lowercase")]
pub enum AuthMethod {
    Password {
        /// 凭据管理器中是否已存密码（不含密码本体）
        has_stored_password: bool,
    },
    Key {
        key_path: String,
        has_stored_passphrase: bool,
    },
}

impl Default for AuthMethod {
    fn default() -> Self {
        AuthMethod::Password {
            has_stored_password: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TunnelConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: AuthMethod,
    #[serde(default)]
    pub rules: Vec<ForwardRule>,
    #[serde(default)]
    pub auto_connect: bool,
    #[serde(default = "default_true")]
    pub auto_reconnect: bool,
    #[serde(default = "default_keepalive")]
    pub keepalive_secs: u32,
    pub created_at: i64,
    pub updated_at: i64,
}

fn default_true() -> bool {
    true
}

fn default_keepalive() -> u32 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct AppSettings {
    pub close_to_tray: bool,
    pub auto_start: bool,
    pub start_minimized: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            close_to_tray: true,
            auto_start: false,
            start_minimized: false,
        }
    }
}

// ---------- 持久化 ----------

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 序列化错误: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigStore {
    pub version: u32,
    pub tunnels: Vec<TunnelConfig>,
    #[serde(default)]
    pub settings: AppSettings,
}

pub fn app_data_dir() -> PathBuf {
    let base = dirs::data_dir()
        .expect("无法确定 AppData 目录")
        .join("ssh-tunnel-manager");
    fs::create_dir_all(&base).ok();
    base
}

fn config_path() -> PathBuf {
    app_data_dir().join("tunnels.json")
}

pub fn load_store() -> ConfigStore {
    match fs::read_to_string(config_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => ConfigStore::default(),
    }
}

/// 原子写：先写临时文件再 rename，避免崩溃损坏
pub fn save_store(store: &ConfigStore) -> Result<(), ConfigError> {
    let path = config_path();
    let tmp = path.with_extension("json.tmp");

    let json = serde_json::to_string_pretty(store)?;
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.flush()?;
    }
    // Windows 上 rename 到已存在目标需要先移除
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(&tmp, &path)?;
    Ok(())
}
