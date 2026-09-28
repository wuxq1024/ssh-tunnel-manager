// === 前端类型定义（与 src-tauri/src/config.rs serde 序列化对齐） ===

export type AuthMethodTag = "password" | "key";

export interface PasswordAuth {
  password: "password";
  /** 凭据管理器中是否已存密码 */
  has_stored_password: boolean;
}

export interface KeyAuth {
  password: "key";
  key_path: string;
  /** 凭据管理器中是否已存 passphrase */
  has_stored_passphrase: boolean;
}

export type AuthMethod = PasswordAuth | KeyAuth;

export interface RemoteRule {
  forward: "remote";
  /** 远端绑定地址。默认 127.0.0.1；非 loopback 需远端 sshd 开启 GatewayPorts */
  bind_addr: string;
  bind_port: number;
  /** 隧道流量最终到达的地址（从 SSH 服务器视角） */
  target_host: string;
  target_port: number;
}

export interface LocalRule {
  forward: "local";
  bind_addr: string;
  bind_port: number;
  target_host: string;
  target_port: number;
}

export interface DynamicRule {
  forward: "dynamic";
  bind_addr: string;
  bind_port: number;
}

export type ForwardRule = RemoteRule | LocalRule | DynamicRule;

export interface TunnelConfig {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_method: AuthMethod;
  rules: ForwardRule[];
  auto_connect: boolean;
  auto_reconnect: boolean;
  keepalive_secs: number;
  created_at: number;
  updated_at: number;
}

export type TunnelState =
  | "stopped"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "error";

export interface TunnelStats {
  id: string;
  state: TunnelState;
  detail: string | null;
  active_sessions: number;
  bytes_in: number;
  bytes_out: number;
  connected_since: number | null;
  reconnect_attempts: number;
}

export interface LogEntry {
  id: string;
  ts: number;
  level: "info" | "warn" | "error" | "success";
  message: string;
}

export interface AppSettings {
  close_to_tray: boolean;
  auto_start: boolean;
  start_minimized: boolean;
}

export interface TestReport {
  ok: boolean;
  message: string;
  /** 连接耗时毫秒 */
  elapsed_ms: number;
  server_fingerprint: string | null;
}
