import { defineStore } from "pinia";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  LogEntry,
  TunnelConfig,
  TunnelState,
  TunnelStats,
} from "../types";

interface StatePayload {
  id: string;
  state: TunnelState;
  detail: string | null;
}

interface StatsPayload {
  id: string;
  state: TunnelState;
  detail: string | null;
  active_sessions: number;
  bytes_in: number;
  bytes_out: number;
  connected_since: number | null;
  reconnect_attempts: number;
}

interface LogPayload {
  id: string;
  ts: number;
  level: LogEntry["level"];
  message: string;
}

export const useTunnelStore = defineStore("tunnels", {
  state: () => ({
    tunnels: [] as TunnelConfig[],
    stats: {} as Record<string, TunnelStats>,
    logs: {} as Record<string, LogEntry[]>,
    settings: {
      close_to_tray: true,
      auto_start: false,
      start_minimized: false,
    } as AppSettings,
    loaded: false,
    _unlisteners: [] as UnlistenFn[],
  }),

  getters: {
    runningCount(state): number {
      return Object.values(state.stats).filter(
        (s) => s.state === "connected" || s.state === "connecting" || s.state === "reconnecting"
      ).length;
    },
  },

  actions: {
    async init() {
      if (this.loaded) return;
      this.loaded = true;
      await this.refresh();
      await this.refreshSettings();

      this._unlisteners.push(
        await listen<StatePayload>("tunnel-state-changed", (e) => {
          const { id, state, detail } = e.payload;
          const s = this.stats[id] ?? {
            id,
            state,
            detail,
            active_sessions: 0,
            bytes_in: 0,
            bytes_out: 0,
            connected_since: null,
            reconnect_attempts: 0,
          };
          s.state = state;
          s.detail = detail;
          if (state !== "connected" && state !== "reconnecting") {
            s.active_sessions = 0;
            if (state === "stopped" || state === "error") s.connected_since = null;
          }
          this.stats[id] = s;
        })
      );

      this._unlisteners.push(
        await listen<StatsPayload>("tunnel-stats", (e) => {
          this.stats[e.payload.id] = e.payload;
        })
      );

      this._unlisteners.push(
        await listen<LogPayload>("tunnel-log", (e) => {
          const { id, ts, level, message } = e.payload;
          const arr = (this.logs[id] ??= []);
          arr.push({ id, ts, level, message });
          if (arr.length > 1000) arr.splice(0, arr.length - 1000);
        })
      );
    },

    dispose() {
      this._unlisteners.forEach((f) => f());
      this._unlisteners = [];
    },

    async refresh() {
      this.tunnels = await invoke<TunnelConfig[]>("list_tunnels");
      // 拉取所有任务统计
      for (const t of this.tunnels) {
        try {
          this.stats[t.id] = await invoke<TunnelStats>("get_tunnel_stats", { id: t.id });
        } catch {
          /* ignore */
        }
      }
    },

    async refreshSettings() {
      this.settings = await invoke<AppSettings>("get_settings");
    },

    async saveSettings(s: AppSettings) {
      await invoke("save_settings", { settings: s });
      this.settings = s;
    },

    async saveTunnel(cfg: TunnelConfig, password?: string | null, passphrase?: string | null) {
      const saved = await invoke<TunnelConfig>("save_tunnel", {
        config: cfg,
        password: password ?? null,
        passphrase: passphrase ?? null,
      });
      const idx = this.tunnels.findIndex((t) => t.id === saved.id);
      if (idx >= 0) this.tunnels.splice(idx, 1, saved);
      else this.tunnels.push(saved);
      return saved;
    },

    async deleteTunnel(id: string) {
      await invoke("delete_tunnel", { id });
      this.tunnels = this.tunnels.filter((t) => t.id !== id);
      delete this.stats[id];
      delete this.logs[id];
    },

    async startTunnel(id: string) {
      await invoke("start_tunnel", { id });
    },

    async stopTunnel(id: string) {
      await invoke("stop_tunnel", { id });
    },
  },
});
