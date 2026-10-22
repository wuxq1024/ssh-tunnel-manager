<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NButton, NEmpty, NSpin, NSelect, useMessage } from "naive-ui";
import { useTunnelStore } from "../stores/tunnels";
import TunnelCard from "./TunnelCard.vue";
import TunnelEditor from "./TunnelEditor.vue";

const store = useTunnelStore();
const message = useMessage();

// ---- 排序 / 过滤（持久化到 localStorage）----
type SortKey = "created" | "host" | "name" | "state";
type SortOrder = "asc" | "desc";
type StateFilter = "all" | "running" | "stopped" | "error";

const LS_KEY = "tunnel-list-sort";
type SavedPref = { by: SortKey; order: SortOrder; filter: StateFilter };
const saved = (() => {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return null;
    const o = JSON.parse(raw);
    const okBy = ["created", "host", "name", "state"].includes(o.by);
    const okOrder = ["asc", "desc"].includes(o.order);
    const okFilter = ["all", "running", "stopped", "error"].includes(o.filter ?? "all");
    if (okBy && okOrder && okFilter) {
      return { by: o.by, order: o.order, filter: o.filter ?? "all" } as SavedPref;
    }
  } catch {
    /* ignore */
  }
  return null;
})();

const sortBy = ref<SortKey>(saved?.by ?? "created");
const sortOrder = ref<SortOrder>(saved?.order ?? "desc");
const stateFilter = ref<StateFilter>(saved?.filter ?? "all");

watch([sortBy, sortOrder, stateFilter], () => {
  localStorage.setItem(
    LS_KEY,
    JSON.stringify({ by: sortBy.value, order: sortOrder.value, filter: stateFilter.value })
  );
});

const sortOptions = [
  { label: "创建时间", value: "created" },
  { label: "IP 地址", value: "host" },
  { label: "任务名称", value: "name" },
  { label: "运行状态", value: "state" },
];

const filterOptions = [
  { label: "全部状态", value: "all" },
  { label: "运行中", value: "running" },
  { label: "已停止", value: "stopped" },
  { label: "错误", value: "error" },
];

// 任务运行状态（来自 store.stats，无记录视为 stopped）
function stateOf(id: string) {
  return store.stats[id]?.state ?? "stopped";
}
// 状态分组：运行中（含连接/重连）/ 错误 / 已停止
function groupOf(id: string): "running" | "error" | "stopped" {
  const s = stateOf(id);
  if (s === "connected" || s === "connecting" || s === "reconnecting") return "running";
  if (s === "error") return "error";
  return "stopped";
}
// 状态排序权重：运行中 → 错误 → 已停止
const STATE_RANK: Record<string, number> = { running: 0, error: 1, stopped: 2 };

const filtered = computed(() => {
  if (stateFilter.value === "all") return store.tunnels;
  return store.tunnels.filter((t) => groupOf(t.id) === stateFilter.value);
});

const sorted = computed(() => {
  const dir = sortOrder.value === "asc" ? 1 : -1;
  return [...filtered.value].sort((a, b) => {
    let r = 0;
    switch (sortBy.value) {
      case "created":
        r = a.created_at - b.created_at;
        break;
      case "host":
        r = a.host.localeCompare(b.host, undefined, { numeric: true });
        break;
      case "name":
        r = a.name.localeCompare(b.name, "zh-Hans-CN");
        break;
      case "state":
        r = STATE_RANK[groupOf(a.id)] - STATE_RANK[groupOf(b.id)];
        if (r !== 0) {
          // 同组内再按细粒度状态区分（connected < connecting < reconnecting）
          const order: Record<string, number> = { connected: 0, connecting: 1, reconnecting: 2, error: 3, stopped: 4 };
          r = (order[stateOf(a.id)] ?? 9) - (order[stateOf(b.id)] ?? 9);
        }
        break;
    }
    // 同值时按创建时间倒序兜底，保证稳定顺序
    if (r === 0) return b.created_at - a.created_at;
    return r * dir;
  });
});

function toggleOrder() {
  sortOrder.value = sortOrder.value === "asc" ? "desc" : "asc";
}

async function onStart(id: string) {
  try {
    await store.startTunnel(id);
  } catch (e) {
    message.error(String(e));
  }
}

async function onStop(id: string) {
  try {
    await store.stopTunnel(id);
  } catch (e) {
    message.error(String(e));
  }
}
</script>

<template>
  <div class="tunnel-list">
    <div class="list-toolbar">
      <span class="count">共 {{ store.tunnels.length }} 个任务<template v-if="stateFilter !== 'all'"> · 当前显示 {{ sorted.length }} 个</template></span>
      <div class="toolbar-right">
        <n-select
          v-model:value="stateFilter"
          :options="filterOptions"
          size="small"
          class="filter-select"
        />
        <div class="sort-group">
          <n-select
            v-model:value="sortBy"
            :options="sortOptions"
            size="small"
            class="sort-select"
          />
          <n-button size="small" :title="sortOrder === 'asc' ? '升序（点击切换降序）' : '降序（点击切换升序）'" @click="toggleOrder">
            {{ sortOrder === "asc" ? "↑ 升序" : "↓ 降序" }}
          </n-button>
        </div>
        <TunnelEditor>
          <template #trigger>
            <n-button type="primary" size="small">+ 新建隧道</n-button>
          </template>
        </TunnelEditor>
      </div>
    </div>

    <div v-if="!store.loaded" class="loading">
      <n-spin size="large" />
    </div>

    <n-empty
      v-else-if="store.tunnels.length === 0"
      description="还没有隧道任务"
      class="empty"
    >
      <template #extra>
        <TunnelEditor>
          <template #trigger>
            <n-button type="primary">创建第一个任务</n-button>
          </template>
        </TunnelEditor>
      </template>
    </n-empty>

    <n-empty
      v-else-if="sorted.length === 0"
      description="没有符合当前状态过滤条件的任务"
      class="empty"
    >
      <template #extra>
        <n-button size="small" @click="stateFilter = 'all'">显示全部</n-button>
      </template>
    </n-empty>

    <TransitionGroup v-else name="card" tag="div" class="cards">
      <TunnelCard
        v-for="t in sorted"
        :key="t.id"
        :config="t"
        @start="onStart"
        @stop="onStop"
      />
    </TransitionGroup>
  </div>
</template>

<style scoped>
.tunnel-list {
  max-width: 980px;
  margin: 0 auto;
}

.list-toolbar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 14px;
}

.count {
  font-size: 13px;
  color: var(--text2);
}

.toolbar-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.sort-group {
  display: flex;
  align-items: center;
  gap: 6px;
}

.sort-select {
  width: 110px;
}

.filter-select {
  width: 100px;
}

.loading {
  display: flex;
  justify-content: center;
  padding: 80px 0;
}

.empty {
  padding: 80px 0;
}

.cards {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.card-enter-active,
.card-leave-active {
  transition: all 0.25s ease;
}

.card-enter-from,
.card-leave-to {
  opacity: 0;
  transform: translateY(8px);
}
</style>
