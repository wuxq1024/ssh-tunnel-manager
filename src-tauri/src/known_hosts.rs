//! TOFU (Trust On First Use) 主机指纹存储
//! 文件格式（每行一条）: host:port algorithm:base64(sha256(public_key_blob))

use russh::keys::PublicKeyOrCertificate;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

static CACHE: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

fn known_hosts_path() -> PathBuf {
    let p = crate::config::app_data_dir().join("known_hosts");
    if !p.exists() {
        fs::write(&p, "").ok();
    }
    p
}

fn load_map() -> HashMap<String, String> {
    let path = known_hosts_path();
    let mut map = HashMap::new();
    if let Ok(text) = fs::read_to_string(&path) {
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, ' ');
            if let (Some(hostport), Some(fp)) = (parts.next(), parts.next()) {
                map.insert(hostport.to_string(), fp.to_string());
            }
        }
    }
    map
}

fn persist(map: &HashMap<String, String>) {
    let path = known_hosts_path();
    let mut lines: Vec<String> = map
        .iter()
        .map(|(hostport, fp)| format!("{hostport} {fp}"))
        .collect();
    lines.sort();
    let _ = fs::write(&path, lines.join("\n") + "\n");
}

/// 计算服务器公钥指纹，格式与 ssh-keygen 一致: "SHA256:base64(无padding)"
pub fn fingerprint(key: &PublicKeyOrCertificate) -> String {
    use base64::engine::general_purpose::STANDARD_NO_PAD;
    use base64::Engine;

    let pk = match key {
        PublicKeyOrCertificate::PublicKey { key, .. } => key,
        PublicKeyOrCertificate::Certificate(_) => {
            return "SSH-CERTIFICATE(not-verified)".to_string()
        }
    };
    // 用 ssh-key 的 Fingerprint API（与 ssh-keygen 相同的编码方式）
    let fp = pk.fingerprint(russh::keys::ssh_key::HashAlg::Sha256);
    format!("SHA256:{}", STANDARD_NO_PAD.encode(fp.as_ref()))
}

#[derive(Debug, PartialEq)]
pub enum HostKeyVerdict {
    /// 首次见到，已记录（TOFU 放行）
    TrustedNew,
    /// 与记录一致
    Trusted,
    /// 与记录不一致 —— 疑似中间人攻击！
    Changed,
}

pub fn check_and_remember(host: &str, port: u16, key: &PublicKeyOrCertificate) -> HostKeyVerdict {
    let hostport = format!("{host}:{port}");
    let fp = fingerprint(key);

    let mut guard = CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(load_map);

    match map.get(&hostport) {
        Some(known) if *known == fp => HostKeyVerdict::Trusted,
        Some(_known) => HostKeyVerdict::Changed,
        None => {
            map.insert(hostport, fp);
            persist(map);
            HostKeyVerdict::TrustedNew
        }
    }
}

/// 删除某台主机的指纹记录（UI 提供"信任新指纹"时用）
pub fn forget(host: &str, port: u16) {
    let mut guard = CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(load_map);
    map.remove(&format!("{host}:{port}"));
    persist(map);
}

pub fn known_fingerprint(host: &str, port: u16) -> Option<String> {
    let mut guard = CACHE.lock().unwrap();
    let map = guard.get_or_insert_with(load_map);
    map.get(&format!("{host}:{port}")).cloned()
}
