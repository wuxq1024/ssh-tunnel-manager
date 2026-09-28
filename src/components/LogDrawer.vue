<script setup lang="ts">
import { computed } from "vue";
import { NButton, NDrawer, NDrawerContent, NScrollbar } from "naive-ui";
import { useTunnelStore } from "../stores/tunnels";

const props = defineProps<{
  show: boolean;
  tunnelId: string;
  tunnelName: string;
}>();

const emit = defineEmits<{ (e: "update:show", v: boolean): void }>();
const store = useTunnelStore();

const lines = computed(() => (store.logs[props.tunnelId] ?? []).map(
  (l) => `${new Date(l.ts).toLocaleTimeString()} [${l.level.toUpperCase()}] ${l.message}`
));

const hasLines = computed(() => lines.value.length > 0);
</script>

<template>
  <n-drawer
    :show="show"
    :width="560"
    placement="right"
    @update:show="emit('update:show', $event)"
  >
    <n-drawer-content :title="`日志 — ${tunnelName}`" closable>
      <n-scrollbar v-if="hasLines" class="log-scroll">
        <div class="log-list">
          <div
            v-for="(l, i) in (store.logs[tunnelId] ?? [])"
            :key="i"
            class="log-line"
            :class="l.level"
          >
            <span class="ts">{{ new Date(l.ts).toLocaleTimeString() }}</span>
            <span class="lv">{{ l.level.toUpperCase() }}</span>
            <span class="msg">{{ l.message }}</span>
          </div>
        </div>
      </n-scrollbar>
      <div v-else class="placeholder">暂无日志</div>
      <template #footer>
        <n-button size="small" @click="store.logs[tunnelId] = []">清空</n-button>
      </template>
    </n-drawer-content>
  </n-drawer>
</template>

<style scoped>
.log-scroll {
  height: 100%;
}

.log-list {
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  padding: 4px 0;
}

.log-line {
  display: flex;
  gap: 8px;
  padding: 3px 10px;
  line-height: 1.5;
}

.log-line:hover {
  background: #f5f7fb;
}

.log-line .ts {
  color: #98a2b8;
  flex-shrink: 0;
}

.log-line .lv {
  width: 56px;
  flex-shrink: 0;
  font-weight: 600;
}

.log-line.info .lv {
  color: #3b5bdb;
}

.log-line.success .lv {
  color: #18a058;
}

.log-line.warn .lv {
  color: #f0a020;
}

.log-line.error .lv {
  color: #d03050;
}

.log-line .msg {
  white-space: pre-wrap;
  word-break: break-all;
}

.placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
  color: #98a2b8;
  font-size: 13px;
}
</style>
