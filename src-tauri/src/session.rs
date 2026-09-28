//! russh SSH 会话封装：连接、认证、keepalive、主机指纹校验

use std::sync::Arc;
use std::time::Duration;

use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::PublicKeyOrCertificate;
use russh::{client, Disconnect};

use crate::config::{AuthMethod, TunnelConfig};
use crate::known_hosts::{self, HostKeyVerdict};

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("认证失败：用户名或密码/密钥不被服务器接受")]
    AuthFailed,
    #[error("服务器主机指纹与历史记录不一致，已拒连（可能存在中间人攻击）")]
    HostKeyChanged,
    #[error("无法连接服务器 {host}:{port}: {source}")]
    Unreachable {
        host: String,
        port: u16,
        source: russh::Error,
    },
    #[allow(dead_code)]
    #[error("远程端口转发注册失败（端口可能被占用）: {0}")]
    ForwardFailed(String),
    #[error("SSH 协议错误: {0}")]
    Protocol(#[from] russh::Error),
    #[error("缺少凭据：请先在任务中保存密码")]
    MissingCredential,
    #[error("私钥文件无法读取或口令错误: {0}")]
    KeyError(String),
}

/// SSH 客户端事件 handler
pub struct ClientHandler {
    /// -R 规则表: remote "bind_addr:bind_port" -> 本地目标 (host, port)
    pub remote_targets: Arc<dashmap::DashMap<String, (String, u16)>>,
    /// 统计/日志句柄
    pub stats: Arc<crate::manager::StatsHandle>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let (host, port) = self.stats.host_port();
        match known_hosts::check_and_remember(&host, port, server_public_key) {
            HostKeyVerdict::Trusted => Ok(true),
            HostKeyVerdict::TrustedNew => {
                let fp = known_hosts::fingerprint(server_public_key);
                self.stats
                    .log("info", &format!("首次连接，已记录服务器指纹 {fp}"));
                Ok(true)
            }
            HostKeyVerdict::Changed => {
                self.stats.log(
                    "error",
                    "服务器指纹与历史记录不一致！已拒绝连接（疑似中间人攻击）。若确认服务器已更换密钥，请清除该主机指纹后重试。",
                );
                Ok(false)
            }
        }
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: russh::Channel<russh::client::Msg>,
        connected_address: &str,
        connected_port: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: russh::client::ChannelOpenHandle,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // 查找对应的本地转发目标
        let key = format!("{connected_address}:{connected_port}");
        let target = self.remote_targets.get(&key).map(|e| e.value().clone());

        match target {
            Some((th, tp)) => {
                reply.accept().await;
                crate::forward::spawn_remote_bridge(channel, &th, tp, self.stats.clone());
            }
            None => {
                reply
                    .reject(russh::ChannelOpenFailure::AdministrativelyProhibited)
                    .await;
            }
        }
        Ok(())
    }
}

/// 建立已认证的 SSH 连接
#[allow(clippy::too_many_arguments)]
pub async fn connect_authenticated(
    cfg: &TunnelConfig,
    password_override: Option<&str>,
    stats: Arc<crate::manager::StatsHandle>,
    remote_targets: Arc<dashmap::DashMap<String, (String, u16)>>,
) -> Result<client::Handle<ClientHandler>, SessionError> {
    let handler = ClientHandler {
        remote_targets,
        stats: stats.clone(),
    };
    // handler 需要知道 host/port 用于指纹校验
    stats.set_host_port(cfg.host.clone(), cfg.port);

    let ssh_config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(cfg.keepalive_secs.max(5) as u64)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });

    let addr = (cfg.host.as_str(), cfg.port);
    let mut handle = client::connect(ssh_config, addr, handler)
        .await
        .map_err(|e| {
            // check_server_key 返回 false 时 russh 报 UnknownKey
            if matches!(e, russh::Error::UnknownKey) {
                SessionError::HostKeyChanged
            } else {
                SessionError::Unreachable {
                    host: cfg.host.clone(),
                    port: cfg.port,
                    source: e,
                }
            }
        })?;

    // 认证
    match &cfg.auth_method {
        AuthMethod::Password { .. } => {
            let password = password_override
                .map(|s| s.to_string())
                .or_else(|| crate::credentials::get_password(&cfg.id))
                .ok_or(SessionError::MissingCredential)?;
            let res = handle.authenticate_password(&cfg.username, password).await?;
            if !res.success() {
                return Err(SessionError::AuthFailed);
            }
        }
        AuthMethod::Key {
            key_path,
            has_stored_passphrase: _,
        } => {
            let passphrase = crate::credentials::get_passphrase(&cfg.id);
            let key = russh::keys::load_secret_key(key_path, passphrase.as_deref())
                .map_err(|e| SessionError::KeyError(format!("{e}")))?;
            let hash_alg = handle.best_supported_rsa_hash().await?.flatten();
            let key_with_hash = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);
            let res = handle
                .authenticate_publickey(&cfg.username, key_with_hash)
                .await?;
            if !res.success() {
                return Err(SessionError::AuthFailed);
            }
        }
    }

    Ok(handle)
}

/// 优雅断开
pub async fn disconnect(handle: &client::Handle<ClientHandler>) {
    let _ = handle
        .disconnect(Disconnect::ByApplication, "client closing", "en")
        .await;
}
