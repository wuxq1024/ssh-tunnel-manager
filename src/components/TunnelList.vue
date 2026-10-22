<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NButton, NEmpty, NSpin, NSelect, useMessage } from "naive-ui";
import { useTunnelStore } from "../stores/tunnels";
import TunnelCard from "./TunnelCard.vue";
import TunnelEditor from "./TunnelEditor.vue";

const store = useTunnelStore();
const message = useMessage();

// ---- 排序（持久化到 localStorage）----
type SortKey = "created" | "host" | "name";
type SortOrder = "asc" | "desc";

const LS_KEY = "tunnel-list-sort";
const saved = (() => {
  try {
    const raw = localStorage.getItem(LS_KEY);
    if (!raw) return null;
    const o = JSON.parse(raw);
    if (["created", "host", "name"].includes(o.by) && ["asc", "desc"].includes(o.order)) {
      return o as { by: SortKey; order: SortOrder };
    }
  } catch {
    /* ignore */
  }
  return null;
})();

const sortBy = ref<SortKey>(saved?.by ?? "created");
const sortOrder = ref<SortOrder>(saved?.order ?? "desc");

watch([sortBy, sortOrder], () => {
  localStorage.setItem(LS_KEY, JSON.stringify({ by: sortBy.value, order: sortOrder.value }));
});

const sortOptions = [
  { label: "创建时间", value: "created" },
  { label: "IP 地址", value: "host" },
  { label: "任务名称", value: "name" },
];

const sorted = computed(() => {
  const dir = sortOrder.value === "asc" ? 1 : -1;
  return [...store.tunnels].sort((a, b) => {
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
      <span class="count">共 {{ store.tunnels.length }} 个任务</span>
      <div class="toolbar-right">
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
