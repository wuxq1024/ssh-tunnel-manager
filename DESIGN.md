# SSH Tunnel Manager — 设计文档

> Windows 桌面端 SSH 反向代理隧道管理工具
> 技术栈：Rust + Tauri 2 + Vue 3 + TypeScript + russh 0.63

## 1. 目标

替代 `ssh -R 7890:127.0.0.1:7890 root@10.185.220.151` 类命令的手工执行，提供：

- **多任务并行**：每个隧道任务独立连接、独立状态机、互不干扰
- **自动重连**：断线后指数退避重连（1s→2s→4s…上限 60s），认证失败等不可恢复错误不重试
- **可视化状态**：任务列表实时显示状态、流量统计、在线时长、日志
- **桌面常驻**：系统托盘、关闭最小化、开机自启、单实例
- **凭据安全**：密码/passphrase 存 Windows 凭据管理器，配置文件零明文

## 2. 架构

```
┌─ Tauri 2 壳 (WebView2) ──────────────────────────────┐
│ Vue 3 + TS + Pinia + Naive UI                        │
│   任务列表页 / 任务编辑器 / 日志抽屉 / 设置            │
├─ Tauri Commands (invoke) ── Tauri Events (push) ─────┤
│ commands.rs: CRUD/启停/测试连接   事件: state/log/    │
│ manager.rs: TunnelManager (per-task supervisor)      │
│   每任务: 状态机 + 指数退避 + 统计                     │
├─ 隧道内核 ───────────────────────────────────────────┤
│ session.rs  russh 连接/认证/keepalive/指纹校验        │
│ forward.rs  -R 入站桥接、-L 直连通道桥接              │
│ socks5.rs   -D 本地 SOCKS5 服务端                     │
├─ 存储 ───────────────────────────────────────────────┤
│ config.rs      任务配置 JSON (原子写)                 │
│ credentials.rs Windows 凭据管理器 (keyring)           │
│ known_hosts.rs TOFU 主机指纹存储                     │
└──────────────────────────────────────────────────────┘
```

### 模块职责

| 模块 | 职责 |
|---|---|
| `session.rs` | 封装 russh：连接、密码/密钥认证、keepalive 配置、主机指纹回调 |
| `forward.rs` | `-R`: server_channel_open_forwarded_tcpip → 连接本地目标并双向桥接；`-L`: 本地监听 + channel_open_direct_tcpip 桥接 |
| `socks5.rs` | `-D`: 本地 TCP 监听 → SOCKS5 握手（支持域名远程解析）→ direct-tcpip |
| `manager.rs` | 任务注册表；supervisor 循环：connect → forward setup → 断线检测 → 退避重连；状态/统计推送 |
| `commands.rs` | Tauri 命令层：list/create/update/delete/start/stop/test；事件发射 |
| `config.rs` | `TunnelConfig` 持久化，`%APPDATA%/ssh-tunnel-manager/tunnels.json`，temp+rename 原子写 |
| `credentials.rs` | keyring crate 封装，服务名 `ssh-tunnel-manager`，按 tunnel_id 存取 |
| `known_hosts.rs` | `%APPDATA%/ssh-tunnel-manager/known_hosts`（自定义格式，非 OpenSSH 格式） |

## 3. 数据模型

```rust
// config.rs
struct TunnelConfig {
    id: String,              // uuid
    name: String,            // 展示名
    host: String,            // SSH 服务器
    port: u16,               // 默认 22
    username: String,
    auth_method: AuthMethod, // Password | Key { key_path, passphrase_stored }
    rules: Vec<ForwardRule>,
    auto_connect: bool,      // 应用启动时自动启动
    auto_reconnect: bool,    // 默认 true
    keepalive_secs: u32,     // 默认 30
    created_at: i64, updated_at: i64,
}

enum ForwardKind {
    Remote { bind_addr: String, bind_port: u16, target_host: String, target_port: u16 },
    // -R [bind_addr:]bind_port:target_host:target_port
    Local  { bind_addr: String, bind_port: u16, target_host: String, target_port: u16 },
    // -L
    Dynamic { bind_addr: String, bind_port: u16 },
    // -D
}
```

`ssh -R 7890:127.0.0.1:7890 root@10.185.220.151` 等价配置：

```json
{
  "name": "10.185.220.151 反代",
  "host": "10.185.220.151", "port": 22, "username": "root",
  "auth_method": "password",
  "rules": [{ "kind": "Remote", "bind_addr": "127.0.0.1", "bind_port": 7890,
              "target_host": "127.0.0.1", "target_port": 7890 }]
}
```

## 4. 任务状态机

```
Stopped ──start()──▶ Connecting ──成功──▶ Connected ──断线(可重试)──▶ Reconnecting
   ▲                    │                                              │
   │                    └──失败(不可重试:认证失败/指纹不符)──▶ Stopped  │
   └────────────stop()──┴──────────重连成功──────────────────────────┘
```

- `Connecting`：TCP + SSH 握手 + 认证 + 注册转发规则（任一步失败进入对应分支）
- `Reconnecting`：指数退避 `min(60s, 1s * 2^n)`，认证失败/指纹变更/端口冲突视为不可重试
- 每次状态迁移 → Tauri event `tunnel-state-changed`
- 日志行 → Tauri event `tunnel-log`

### 错误分类

| 类别 | 判定 | 处理 |
|---|---|--- |
| AuthFailed | authenticate_* 返回失败 | 终止任务，提示改凭据 |
| HostKeyChanged | TOFU 比对不一致 | 终止 + 醒目告警 |
| Unreachable | TCP 连不上/超时 | 退避重连 |
| RemotePortConflict | tcpip_forward 失败 | 退避重连（远端旧监听可能未释放） |
| ProtocolError | russh 其他错误 | 退避重连 |

## 5. 关键实现细节

### 5.1 -R 转发（核心）

1. `handle.tcpip_forward(bind_addr, bind_port)` 向服务器注册监听
2. 服务器收到入站连接 → 回调 `server_channel_open_forwarded_tcpip(channel, ...)`
3. 回调中 `reply.accept()` 后，`tokio::spawn` 桥接任务：
   `TcpStream::connect(target_host, target_port)` ↔ `channel.into_stream()`
   双向 copy（`tokio::io::copy_bidirectional`）
4. 每桥接一条连接计一次会话数；字节数计入统计

> **GatewayPorts**：`-R` 默认绑远端 loopback；要对外暴露需远端 sshd 开 `GatewayPorts`。
> UI 在 Remote 规则 bind_addr 非.loopback 时显示内联提示。

### 5.2 -L 转发

1. `tokio::net::TcpListener::bind(bind_addr, bind_port)` 本地监听
2. accept 后 `handle.channel_open_direct_tcpip(target_host, target_port, ...)` 打开通道
3. `tokio::io::copy_bidirectional(stream, channel.into_stream())`

### 5.3 -D 动态转发 (SOCKS5)

1. 本地 TcpListener；accept 后读 SOCKS5 头
2. 仅支持 CONNECT（05 01 00 / 05 03 00 直通；UDP/BIND 拒绝）
3. 地址类型 IPv4/IPv6/域名：**域名直接传给 SSH 服务器解析**（remote DNS）
4. `channel_open_direct_tcpip(dst_host, dst_port)` → 桥接

### 5.4 keepalive 与死链检测

`client::Config.keepalive_interval = Some(30s)`，`keepalive_max = 3`。
russh 自动处理：3 次无响应即断开会话，supervisor 感知 `handle` 完成并触发重连。

### 5.5 主机指纹 (TOFU)

- `check_server_key` 回调中：查 `known_hosts`，无记录 → 记录并放行（日志提示）；
  不一致 → 返回 false 拒连 + HostKeyChanged 错误
- 指纹格式：`algorithm:base64(sha256(public_key_openssh_blob))`

### 5.6 凭据

- Windows 凭据管理器：服务 `ssh-tunnel-manager`，用户名 = tunnel_id
- `has_password` 布尔字段指示凭据管理器中是否有存储的密码
- 私钥 passphrase 同样存凭据管理器（键 `tunnel_id:passphrase`）

## 6. Tauri 命令与事件契约

### Commands

| 命令 | 参数 | 返回 |
|---|---|---|
| `list_tunnels` | – | `Vec<TunnelConfig>` |
| `save_tunnel` | `TunnelConfig, password?: Option<String>` | `TunnelConfig` |
| `delete_tunnel` | `id` | `()` |
| `start_tunnel` / `stop_tunnel` | `id` | `()` |
| `test_connection` | `host, port, username, auth...` | `Result<TestReport, String>` |
| `get_tunnel_stats` | `id` | `TunnelStats` |
| `get_settings` / `save_settings` | – | `AppSettings` |
| `reveal_in_explorer` | `path` | `()` |

### Events (Rust → JS)

| 事件 | payload |
|---|---|
| `tunnel-state-changed` | `{ id, state, detail? }` |
| `tunnel-log` | `{ id, ts, level, message }` |
| `tunnel-stats` | `{ id, active_sessions, bytes_in, bytes_out, connected_since }` |

## 7. 前端结构

```
src/
  main.ts            入口（Naive UI 按需注册）
  App.vue            主布局（左侧任务列表/右侧详情）
  stores/tunnels.ts  Pinia store: 列表 + invoke 封装 + 事件订阅
  components/
    TunnelCard.vue   任务卡片: 状态徽标/启停按钮/流量/时长
    TunnelEditor.vue 新建/编辑抽屉: 连接+认证+规则表格+高级
    LogDrawer.vue    实时日志（环形缓冲 1000 行）
    StatsBar.vue     统计条
  types.ts           TS 类型（与 Rust 侧 serde 对齐）
```

## 8. 里程碑

| 里程碑 | 内容 | 验收 |
|---|---|---|
| M1 内核 | session/forward/socks5/manager 编译通过 + 单任务跑通 | cargo build 零错误 |
| M2 界面 | 任务 CRUD + 并行启停 + 实时状态/日志 | UI 操作全链路可用 |
| M3 打磨 | 托盘/自启/指纹告警/安装包 | 交付 release exe |
