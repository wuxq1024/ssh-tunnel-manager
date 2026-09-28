//! 转发桥接：-R 入站通道 ↔ 本地目标、-L 本地监听 ↔ SSH 通道、-D SOCKS5

use std::sync::Arc;

use russh::client::{Handle, Msg};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::manager::StatsHandle;
use crate::session::ClientHandler;

pub type SharedHandle = Arc<Handle<ClientHandler>>;

/// -R：远程入站 channel 桥接到本地 target_host:target_port
pub fn spawn_remote_bridge(
    channel: russh::Channel<Msg>,
    target_host: &str,
    target_port: u16,
    stats: Arc<StatsHandle>,
) {
    let host = target_host.to_string();
    tokio::spawn(async move {
        stats.session_opened();
        match TcpStream::connect((host.as_str(), target_port)).await {
            Ok(mut tcp) => {
                let mut ssh_stream = channel.into_stream();
                let res = tokio::io::copy_bidirectional(&mut tcp, &mut ssh_stream).await;
                if let Err(e) = res {
                    debug_log(&format!("bridge ended: {e}"));
                }
            }
            Err(e) => {
                stats.log(
                    "warn",
                    &format!("本地目标连接失败 {host}:{target_port}: {e}"),
                );
                let _ = channel.close().await;
            }
        }
        stats.session_closed();
    });
}

/// -L：启动本地监听，每个连接开一条 direct-tcpip 通道
pub fn spawn_local_forward(
    bind_addr: String,
    bind_port: u16,
    target_host: String,
    target_port: u16,
    handle: SharedHandle,
    stats: Arc<StatsHandle>,
) {
    tokio::spawn(async move {
        let listener = match TcpListener::bind((bind_addr.as_str(), bind_port)).await {
            Ok(l) => l,
            Err(e) => {
                stats.log(
                    "error",
                    &format!("-L 本地监听 {bind_addr}:{bind_port} 绑定失败: {e}"),
                );
                return;
            }
        };
        stats.log(
            "info",
            &format!("-L {bind_addr}:{bind_port} → {target_host}:{target_port} 监听中"),
        );

        loop {
            let (mut tcp, _peer) = match listener.accept().await {
                Ok(x) => x,
                Err(e) => {
                    stats.log("warn", &format!("-L accept 失败: {e}"));
                    continue;
                }
            };
            if handle.is_closed() {
                break;
            }
            let h = handle.clone();
            let th = target_host.clone();
            let tp = target_port;
            let st = stats.clone();

            tokio::spawn(async move {
                st.session_opened();
                match h.channel_open_direct_tcpip(&th, tp as u32, "127.0.0.1", 0).await {
                    Ok(channel) => {
                        let mut ssh_stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut ssh_stream).await;
                    }
                    Err(e) => {
                        st.log("warn", &format!("direct-tcpip 打开失败 {th}:{tp}: {e}"));
                    }
                }
                st.session_closed();
            });
        }
    });
}

/// -D：本地 SOCKS5 服务端（域名由远端解析）
pub fn spawn_socks5(
    bind_addr: String,
    bind_port: u16,
    handle: SharedHandle,
    stats: Arc<StatsHandle>,
) {
    tokio::spawn(async move {
        let listener = match TcpListener::bind((bind_addr.as_str(), bind_port)).await {
            Ok(l) => l,
            Err(e) => {
                stats.log(
                    "error",
                    &format!("-D SOCKS5 {bind_addr}:{bind_port} 绑定失败: {e}"),
                );
                return;
            }
        };
        stats.log("info", &format!("-D SOCKS5 {bind_addr}:{bind_port} 监听中"));

        loop {
            let (mut tcp, _peer) = match listener.accept().await {
                Ok(x) => x,
                Err(e) => {
                    stats.log("warn", &format!("-D accept 失败: {e}"));
                    continue;
                }
            };
            if handle.is_closed() {
                break;
            }
            let h = handle.clone();
            let st = stats.clone();

            tokio::spawn(async move {
                st.session_opened();
                if let Err(e) = socks5_serve(&mut tcp, &h).await {
                    debug_log(&format!("socks5 session: {e}"));
                }
                st.session_closed();
            });
        }
    });
}

async fn socks5_serve(tcp: &mut TcpStream, handle: &SharedHandle) -> std::io::Result<()> {
    // ---- 握手 ----
    let mut buf = [0u8; 2];
    tcp.read_exact(&mut buf).await?; // VER NMETHODS
    if buf[0] != 0x05 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not socks5",
        ));
    }
    let n = buf[1] as usize;
    let mut methods = vec![0u8; n];
    tcp.read_exact(&mut methods).await?;
    if methods.contains(&0x00) {
        tcp.write_all(&[0x05, 0x00]).await?;
    } else {
        tcp.write_all(&[0x05, 0xFF]).await?;
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "no acceptable auth",
        ));
    }

    // ---- CONNECT 请求 ----
    let mut head = [0u8; 4];
    tcp.read_exact(&mut head).await?; // VER CMD RSV ATYP
    if head[0] != 0x05 || head[1] != 0x01 {
        let _ = tcp
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await;
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "only CONNECT",
        ));
    }
    let dst_host = match head[3] {
        0x01 => {
            let mut a = [0u8; 4];
            tcp.read_exact(&mut a).await?;
            std::net::Ipv4Addr::from(a).to_string()
        }
        0x03 => {
            let mut l = [0u8; 1];
            tcp.read_exact(&mut l).await?;
            let mut d = vec![0u8; l[0] as usize];
            tcp.read_exact(&mut d).await?;
            String::from_utf8_lossy(&d).to_string()
        }
        0x04 => {
            let mut a = [0u8; 16];
            tcp.read_exact(&mut a).await?;
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "bad ATYP",
            ))
        }
    };
    let mut pb = [0u8; 2];
    tcp.read_exact(&mut pb).await?;
    let dst_port = ((pb[0] as u16) << 8) | pb[1] as u16;

    // ---- 打开 SSH 通道（域名远程解析）----
    let channel = match handle
        .channel_open_direct_tcpip(&dst_host, dst_port as u32, "127.0.0.1", 0)
        .await
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tcp
                .write_all(&[0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("ssh: {e}"),
            ));
        }
    };

    // 成功应答（BND.ADDR 0.0.0.0:0）
    tcp.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await?;

    // ---- 双向桥接 ----
    let mut ssh_stream = channel.into_stream();
    let _ = tokio::io::copy_bidirectional(tcp, &mut ssh_stream).await;
    Ok(())
}

fn debug_log(msg: &str) {
    let _ = msg;
}
