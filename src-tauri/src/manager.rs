//! TunnelManager：每任务一个 supervisor 协程
//! 状态机: stopped → connecting → connected → (reconnecting) → ...

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::mpsc;

use crate::config::{ForwardRule, TunnelConfig};
use crate::forward;
use crate::session::{self, SessionError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelState {
    Stopped,
    Connecting,
    Connected,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TunnelStats {
    pub id: String,
    pub state: TunnelState,
    pub detail: Option<String>,
    pub active_sessions: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub connected_since: Option<i64>,
    pub reconnect_attempts: u32,
}

/// 供 handler / 桥接任务上报统计与日志的句柄
pub struct StatsHandle {
    pub id: String,
    state: Mutex<TunnelState>,
    detail: Mutex<Option<String>>,
    sessions: AtomicU64,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    connected_since: AtomicI64, // millis; 0 = none
    reconnect_attempts: Mutex<u32>,
    host_port: Mutex<(String, u16)>,
    /// 日志/状态事件出口
    events: mpsc::UnboundedSender<ManagerEvent>,
}

impl StatsHandle {
    pub fn new(id: &str, events: mpsc::UnboundedSender<ManagerEvent>) -> Arc<Self> {
        Arc::new(Self {
            id: id.to_string(),
            state: Mutex::new(TunnelState::Stopped),
            detail: Mutex::new(None),
            sessions: AtomicU64::new(0),
            bytes_in: AtomicU64::new(0),
            bytes_out: AtomicU64::new(0),
            connected_since: AtomicI64::new(0),
            reconnect_attempts: Mutex::new(0),
            host_port: Mutex::new((String::new(), 0)),
            events,
        })
    }

    pub fn set_state(&self, s: TunnelState, detail: Option<String>) {
        *self.state.lock().unwrap() = s;
        *self.detail.lock().unwrap() = detail.clone();
        if s == TunnelState::Connected {
            self.connected_since.store(
                chrono::Utc::now().timestamp_millis(),
                Ordering::Relaxed,
            );
        } else if s == TunnelState::Stopped || s == TunnelState::Error {
            self.connected_since.store(0, Ordering::Relaxed);
        }
        let _ = self.events.send(ManagerEvent::StateChanged {
            id: self.id.clone(),
            state: s,
            detail,
        });
    }

    pub fn set_host_port(&self, host: String, port: u16) {
        *self.host_port.lock().unwrap() = (host, port);
    }

    pub fn host_port(&self) -> (String, u16) {
        self.host_port.lock().unwrap().clone()
    }

    pub fn log(&self, level: &str, message: &str) {
        let _ = self.events.send(ManagerEvent::Log {
            id: self.id.clone(),
            ts: chrono::Utc::now().timestamp_millis(),
            level: level.to_string(),
            message: message.to_string(),
        });
    }

    pub fn session_opened(&self) {
        self.sessions.fetch_add(1, Ordering::Relaxed);
        self.push_stats();
    }

    pub fn session_closed(&self) {
        self.sessions.fetch_sub(1, Ordering::Relaxed);
        self.push_stats();
    }

    #[allow(dead_code)]
    pub fn add_bytes(&self, _in: u64, _out: u64) {
        self.bytes_in.fetch_add(_in, Ordering::Relaxed);
        self.bytes_out.fetch_add(_out, Ordering::Relaxed);
        self.push_stats();
    }

    pub fn snapshot(&self) -> TunnelStats {
        TunnelStats {
            id: self.id.clone(),
            state: *self.state.lock().unwrap(),
            detail: self.detail.lock().unwrap().clone(),
            active_sessions: self.sessions.load(Ordering::Relaxed),
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            connected_since: {
                let ms = self.connected_since.load(Ordering::Relaxed);
                if ms == 0 { None } else { Some(ms) }
            },
            reconnect_attempts: *self.reconnect_attempts.lock().unwrap(),
        }
    }

    fn push_stats(&self) {
        let _ = self.events.send(ManagerEvent::Stats(self.snapshot()));
    }
}

#[derive(Debug)]
pub enum ManagerEvent {
    StateChanged {
        id: String,
        state: TunnelState,
        detail: Option<String>,
    },
    Log {
        id: String,
        ts: i64,
        level: String,
        message: String,
    },
    Stats(TunnelStats),
}

#[derive(Debug)]
enum SupervisorCommand {
    Stop,
}

struct RunningTask {
    cmd_tx: mpsc::UnboundedSender<SupervisorCommand>,
    stats: Arc<StatsHandle>,
    join: tokio::task::JoinHandle<()>,
}

pub struct TunnelManager {
    tasks: Mutex<HashMap<String, RunningTask>>,
    events_tx: mpsc::UnboundedSender<ManagerEvent>,
}

impl TunnelManager {
    /// 返回 (manager, 事件流)。事件流由调用方消费（lib.rs 转发为 Tauri 事件）
    pub fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<ManagerEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Arc::new(Self {
                tasks: Mutex::new(HashMap::new()),
                events_tx: tx,
            }),
            rx,
        )
    }

    #[allow(dead_code)]
    pub fn is_running(&self, id: &str) -> bool {
        self.tasks.lock().unwrap().contains_key(id)
    }

    pub fn stats_of(&self, id: &str) -> Option<TunnelStats> {
        let tasks = self.tasks.lock().unwrap();
        tasks.get(id).map(|t| t.stats.snapshot())
    }

    pub fn start(self: &Arc<Self>, cfg: TunnelConfig) -> Result<(), String> {
        let mut tasks = self.tasks.lock().unwrap();
        if tasks.contains_key(&cfg.id) {
            return Ok(()); // 已在运行
        }

        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<SupervisorCommand>();
        let stats = StatsHandle::new(&cfg.id, self.events_tx.clone());
        let stats_for_task = stats.clone();
        let manager = self.clone();
        let cfg_id = cfg.id.clone();

        let join = tokio::spawn(async move {
            manager.supervisor(cfg, stats, &mut cmd_rx).await;
        });

        tasks.insert(
            cfg_id,
            RunningTask { cmd_tx, stats: stats_for_task, join },
        );
        Ok(())
    }

    pub fn stop(&self, id: &str) {
        let task = self.tasks.lock().unwrap().remove(id);
        if let Some(t) = task {
            let _ = t.cmd_tx.send(SupervisorCommand::Stop);
            // 不等待 join —— supervisor 会自行收尾并设置 stopped 状态
            tokio::spawn(async move {
                let _ = t.join.await;
            });
        }
    }

    pub fn stop_all(&self) {
        let ids: Vec<String> = self.tasks.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }

    /// supervisor 主循环
    async fn supervisor(
        &self,
        cfg: TunnelConfig,
        stats: Arc<StatsHandle>,
        cmd_rx: &mut mpsc::UnboundedReceiver<SupervisorCommand>,
    ) {
        let mut attempt: u32 = 0;

        'outer: loop {
            attempt += 1;
            let is_retry = attempt > 1;
            *stats.reconnect_attempts.lock().unwrap() = attempt.saturating_sub(1);

            stats.set_state(
                if is_retry { TunnelState::Reconnecting } else { TunnelState::Connecting },
                None,
            );
            if is_retry {
                stats.log("warn", &format!("第 {attempt} 次尝试连接"));
            } else {
                stats.log("info", &format!("连接 {u}@{h}:{p} …", u = cfg.username, h = cfg.host, p = cfg.port));
            }

            // 连接 + 认证
            let remote_targets = Arc::new(dashmap::DashMap::new());
            // 预填 -R 规则映射: "bind:port" -> target
            for rule in &cfg.rules {
                if let ForwardRule::Remote { bind_addr, bind_port, target_host, target_port } = rule {
                    remote_targets
                        .insert(format!("{bind_addr}:{bind_port}"), (target_host.clone(), *target_port));
                }
            }

            match session::connect_authenticated(&cfg, None, stats.clone(), remote_targets.clone()).await {
                Ok(handle) => {
                    let handle = Arc::new(handle);
                    stats.log("success", "SSH 连接与认证成功");
                    // 注册 -R 转发
                    let mut forward_ok = true;
                    for rule in &cfg.rules {
                        match rule {
                            ForwardRule::Remote { bind_addr, bind_port, target_host, target_port } => {
                                match handle.tcpip_forward(bind_addr.clone(), *bind_port as u32).await {
                                    Ok(bound) => {
                                        let shown = if *bind_port == 0 { bound } else { *bind_port as u32 };
                                        stats.log("info", &format!(
                                            "-R {bind_addr}:{shown} → {target_host}:{target_port} 已注册（远端实际端口 {bound}）"
                                        ));
                                    }
                                    Err(e) => {
                                        stats.log("error", &format!("-R {bind_addr}:{bind_port} 注册失败: {e}"));
                                        forward_ok = false;
                                    }
                                }
                            }
                            ForwardRule::Local { bind_addr, bind_port, target_host, target_port } => {
                                forward::spawn_local_forward(
                                    bind_addr.clone(), *bind_port,
                                    target_host.clone(), *target_port,
                                    handle.clone(), stats.clone(),
                                );
                            }
                            ForwardRule::Dynamic { bind_addr, bind_port } => {
                                forward::spawn_socks5(
                                    bind_addr.clone(), *bind_port,
                                    handle.clone(), stats.clone(),
                                );
                            }
                        }
                    }

                    if !forward_ok {
                        session::disconnect(&handle).await;
                        stats.set_state(TunnelState::Error, Some("远程端口转发注册失败".into()));
                        // 端口冲突走重连（远端旧连接可能未释放）
                        if !cfg.auto_reconnect {
                            break 'outer;
                        }
                    } else {
                        stats.set_state(TunnelState::Connected, None);
                        attempt = 0; // 连接成功重置退避

                        // 等待: 命令 或 会话结束
                        // 注: Handle 实现 Future 但需要 &mut（与 Arc 共享冲突），
                        // 这里用轮询 is_closed() + tokio::time::sleep 实现
                        loop {
                            tokio::select! {
                                cmd = cmd_rx.recv() => {
                                    if matches!(cmd, Some(SupervisorCommand::Stop) | None) {
                                        session::disconnect(&handle).await;
                                        stats.log("info", "已停止");
                                        stats.set_state(TunnelState::Stopped, None);
                                        break 'outer;
                                    }
                                }
                                _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {
                                    if handle.is_closed() {
                                        // 会话结束（断线/keepalive 超时）
                                        stats.log("warn", "连接断开");
                                        break; // 内层 break → 重连判断
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let msg = format!("{e}");
                    let retryable = matches!(
                        e,
                        SessionError::Unreachable { .. }
                            | SessionError::ForwardFailed(_)
                            | SessionError::Protocol(_)
                    );
                    stats.log("error", &msg);
                    if !retryable {
                        stats.set_state(TunnelState::Error, Some(msg));
                        break 'outer;
                    }
                }
            }

            // 到这里：需要重连。先检查是否有 stop 命令
            if !cfg.auto_reconnect {
                stats.set_state(TunnelState::Stopped, Some("连接断开（未开启自动重连）".into()));
                break 'outer;
            }
            if let Ok(SupervisorCommand::Stop) = cmd_rx.try_recv() {
                stats.set_state(TunnelState::Stopped, None);
                break 'outer;
            }

            // 指数退避: 1s,2s,4s,…,60s
            let delay = Duration::from_millis(
                (1000u64 * (1u64 << (attempt - 1).min(6))).min(60_000)
            );
            stats.set_state(TunnelState::Reconnecting, Some(format!(
                "{} 后第 {} 次重连",
                humantize_delay(delay),
                attempt
            )));
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                cmd = cmd_rx.recv() => {
                    if matches!(cmd, Some(SupervisorCommand::Stop) | None) {
                        stats.set_state(TunnelState::Stopped, None);
                        break 'outer;
                    }
                }
            }
        }

        // supervisor 退出时推送最终统计
        let _ = self.events_tx.send(ManagerEvent::Stats(stats.snapshot()));
    }
}

fn humantize_delay(d: Duration) -> String {
    let s = d.as_secs_f64();
    if s < 1.0 {
        format!("{}ms", d.as_millis())
    } else {
        format!("{s:.0}s")
    }
}
