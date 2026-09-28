//! Windows 凭据管理器封装（keyring crate）
//! 服务名: ssh-tunnel-manager
//! 用户名: {tunnel_id}            -> SSH 登录密码
//!         {tunnel_id}:passphrase -> 私钥口令

use keyring::{Entry, Error as KeyringError};

const SERVICE: &str = "ssh-tunnel-manager";

pub fn entry_password(tunnel_id: &str) -> Entry {
    Entry::new(SERVICE, tunnel_id).expect("创建 keyring entry 失败")
}

pub fn entry_passphrase(tunnel_id: &str) -> Entry {
    Entry::new(SERVICE, &format!("{tunnel_id}:passphrase")).expect("创建 keyring entry 失败")
}

pub fn set_password(tunnel_id: &str, password: &str) -> Result<(), String> {
    entry_password(tunnel_id)
        .set_password(password)
        .map_err(|e| format!("写入凭据管理器失败: {e}"))
}

pub fn get_password(tunnel_id: &str) -> Option<String> {
    match entry_password(tunnel_id).get_password() {
        Ok(p) => Some(p),
        Err(KeyringError::NoEntry) => None,
        Err(e) => {
            eprintln!("[credentials] 读取凭据失败: {e}");
            None
        }
    }
}

pub fn delete_password(tunnel_id: &str) {
    let _ = entry_password(tunnel_id).delete_credential();
}

pub fn set_passphrase(tunnel_id: &str, passphrase: &str) -> Result<(), String> {
    entry_passphrase(tunnel_id)
        .set_password(passphrase)
        .map_err(|e| format!("写入凭据管理器失败: {e}"))
}

pub fn get_passphrase(tunnel_id: &str) -> Option<String> {
    match entry_passphrase(tunnel_id).get_password() {
        Ok(p) => Some(p),
        Err(KeyringError::NoEntry) => None,
        Err(e) => {
            eprintln!("[credentials] 读取口令失败: {e}");
            None
        }
    }
}

pub fn delete_passphrase(tunnel_id: &str) {
    let _ = entry_passphrase(tunnel_id).delete_credential();
}

/// 删除任务时清理全部凭据
pub fn purge(tunnel_id: &str) {
    delete_password(tunnel_id);
    delete_passphrase(tunnel_id);
}
