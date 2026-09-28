<script setup lang="ts">
import { computed } from "vue";
import { NButton, NEmpty, NSpin, useMessage } from "naive-ui";
import { useTunnelStore } from "../stores/tunnels";
import TunnelCard from "./TunnelCard.vue";
import TunnelEditor from "./TunnelEditor.vue";

const store = useTunnelStore();
const message = useMessage();

const sorted = computed(() =>
  [...store.tunnels].sort((a, b) => a.created_at - b.created_at)
);

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
      <TunnelEditor>
        <template #trigger>
          <n-button type="primary" size="small">+ 新建隧道</n-button>
        </template>
      </TunnelEditor>
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
