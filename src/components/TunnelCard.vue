<script setup lang="ts">
import { computed, onUnmounted, ref } from "vue";
import { NButton, NDropdown, NTag, useMessage } from "naive-ui";
import { useTunnelStore } from "../stores/tunnels";
import TunnelEditor from "./TunnelEditor.vue";
import LogDrawer from "./LogDrawer.vue";
import type { TunnelConfig, TunnelState } from "../types";

const props = defineProps<{ config: TunnelConfig }>();
const emit = defineEmits<{
  (e: "start", id: string): void;
  (e: "stop", id: string): void;
}>();

const store = useTunnelStore();
const message = useMessage();
const showLog = ref(false);

// 响应式时钟：驱动 uptime/统计重渲染
const nowTick = ref(Date.now());
let tickTimer: number | undefined;
tickTimer = window.setInterval(() => {
  nowTick.value = Date.now();
}, 1000);
onUnmounted(() => window.clearInterval(tickTimer));

const stats = computed(() => store.stats[props.config.id]);
const state = computed<TunnelState>(() => stats.value?.state ?? "stopped");
const running = computed(
  () => state.value === "connected" || state.value === "connecting" || state.value === "reconnecting"
);

const stateMeta = computed(() => {
  switch (state.value) {
    case "connected":
      return { label: "已连接", color: "success" as const };
    case "connecting":
      return { label: "连接中", color: "info" as const };
    case "reconnecting":
      return { label: "重连中", color: "warning" as const };
    case "error":
      return { label: "错误", color: "error" as const };
    default:
      return { label: "已停止", color: "default" as const };
  }
});

const uptime = computed(() => {
  void nowTick.value; // 依赖时钟触发重算
  if (state.value !== "connected" || !stats.value?.connected_since) return null;
  return nowTick.value - stats.value.connected_since;
});

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

function fmtDuration(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  if (h > 0) return `${h}时${m}分`;
  if (m > 0) return `${m}分${ss}秒`;
  return `${ss}秒`;
}

const ruleSummary = computed(() =>
  props.config.rules.map((r) => {
    if (r.forward === "remote")
      return `R ${r.bind_addr}:${r.bind_port} → ${r.target_host}:${r.target_port}`;
    if (r.forward === "local")
      return `L ${r.bind_addr}:${r.bind_port} → ${r.target_host}:${r.target_port}`;
    return `D ${r.bind_addr}:${r.bind_port} (SOCKS5)`;
  })
);

async function onDelete() {
  await store.deleteTunnel(props.config.id);
  message.success("已删除");
}

function onMenuAction(key: string) {
  if (key === "log") showLog.value = true;
  if (key === "delete") onDelete();
}
</script>

<template>
  <div class="card" :class="{ running }">
    <div class="card-head">
      <div class="head-left">
        <span class="dot" :class="state" />
        <span class="name">{{ config.name }}</span>
        <n-tag size="small" :type="stateMeta.color" :bordered="false" round>
          {{ stateMeta.label }}
        </n-tag>
      </div>
      <div class="head-right">
        <n-button
          v-if="!running"
          size="small"
          type="primary"
          :loading="state === 'connecting'"
          @click="emit('start', config.id)"
        >
          启动
        </n-button>
        <n-button v-else size="small" @click="emit('stop', config.id)">停止</n-button>
        <n-dropdown
          trigger="click"
          :options="[
            { label: '查看日志', key: 'log' },
            { label: '删除任务', key: 'delete' },
          ]"
          @select="onMenuAction"
        >
          <n-button size="small" quaternary>⋯</n-button>
        </n-dropdown>
      </div>
    </div>

    <div class="card-body">
      <div class="endpoint">
        <span class="mono">{{ config.username }}@{{ config.host }}:{{ config.port }}</span>
        <span class="auth-chip">
          {{ config.auth_method.password === "password" ? "密码认证" : "密钥认证" }}
        </span>
      </div>

      <div class="rules">
        <span v-for="(r, i) in ruleSummary" :key="i" class="rule-chip mono">{{ r }}</span>
        <span v-if="ruleSummary.length === 0" class="no-rule">（无转发规则）</span>
      </div>
    </div>

    <div class="card-foot" v-if="running || stats">
      <span v-if="state === 'connected' && uptime !== null" class="meta">
        ⏱ {{ fmtDuration(uptime) }}
      </span>
      <span v-if="stats" class="meta">🔗 {{ stats.active_sessions }} 会话</span>
      <span v-if="stats" class="meta">↓ {{ fmtBytes(stats.bytes_in) }}</span>
      <span v-if="stats" class="meta">↑ {{ fmtBytes(stats.bytes_out) }}</span>
      <span v-if="state === 'reconnecting' && stats" class="meta warn">
        第 {{ stats.reconnect_attempts }} 次重连
      </span>
      <span v-if="stats?.detail && state === 'error'" class="meta err" :title="stats.detail">
        ⚠ {{ stats.detail }}
      </span>
    </div>

    <LogDrawer v-model:show="showLog" :tunnel-id="config.id" :tunnel-name="config.name" />

    <div class="edit-fab">
      <TunnelEditor :config="config">
        <template #trigger>
          <n-button size="tiny" quaternary type="primary">编辑</n-button>
        </template>
      </TunnelEditor>
    </div>
  </div>
</template>

<style scoped>
.card {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 12px 14px;
  transition: box-shadow 0.2s, border-color 0.2s;
  position: relative;
}

.card:hover {
  box-shadow: 0 2px 12px rgba(30, 40, 90, 0.08);
}

.card.running {
  border-color: rgba(24, 160, 88, 0.4);
}

.card-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.head-left {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  background: #c2c8d4;
  flex-shrink: 0;
}

.dot.connected {
  background: var(--green);
  box-shadow: 0 0 6px rgba(24, 160, 88, 0.7);
  animation: pulse 2s infinite;
}

.dot.connecting,
.dot.reconnecting {
  background: var(--orange);
  animation: pulse 1s infinite;
}

.dot.error {
  background: var(--red);
}

@keyframes pulse {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.4;
  }
}

.name {
  font-weight: 600;
  font-size: 14px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.head-right {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.endpoint {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 8px;
  font-size: 13px;
  color: var(--text2);
}

.auth-chip {
  font-size: 11px;
  background: #eef1f8;
  padding: 1px 8px;
  border-radius: 8px;
  color: #5a6478;
}

.rules {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

.rule-chip {
  font-size: 12px;
  background: #f0f4ff;
  border: 1px solid #dde5ff;
  color: #3b5bdb;
  padding: 2px 8px;
  border-radius: 6px;
}

.no-rule {
  font-size: 12px;
  color: #b0b8c8;
}

.card-foot {
  display: flex;
  gap: 14px;
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px dashed var(--border);
  font-size: 12px;
  color: var(--text2);
  flex-wrap: wrap;
}

.meta.warn {
  color: var(--orange);
}

.meta.err {
  color: var(--red);
  max-width: 420px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mono {
  font-family: Consolas, "Courier New", monospace;
}

.edit-fab {
  position: absolute;
  right: 14px;
  bottom: 10px;
}
</style>
