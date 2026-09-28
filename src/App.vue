<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { NConfigProvider, NMessageProvider, zhCN, dateZhCN } from "naive-ui";
import { useTunnelStore } from "./stores/tunnels";
import TunnelList from "./components/TunnelList.vue";
import SettingsModal from "./components/SettingsModal.vue";

const store = useTunnelStore();

onMounted(() => store.init());
onUnmounted(() => store.dispose());
</script>

<template>
  <n-config-provider :theme="null" :locale="zhCN" :date-locale="dateZhCN" style="height: 100%">
    <n-message-provider>
      <div class="app-shell">
        <header class="app-header">
          <div class="brand">
            <img src="/icon.png" alt="logo" class="logo" />
            <span class="title">SSH Tunnel Manager</span>
          </div>
          <div class="header-right">
            <span class="running-badge" v-if="store.runningCount > 0">
              {{ store.runningCount }} 个运行中
            </span>
            <SettingsModal />
          </div>
        </header>

        <main class="app-main">
          <TunnelList />
        </main>
      </div>
    </n-message-provider>
  </n-config-provider>
</template>

<style>
:root {
  --bg: #f5f7fb;
  --panel: #ffffff;
  --border: #e5e9f2;
  --text: #1f2430;
  --text2: #5a6478;
  --accent: #3b5bdb;
  --green: #18a058;
  --red: #d03050;
  --orange: #f0a020;
}

html,
body,
#app {
  height: 100%;
  margin: 0;
  font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif;
  background: var(--bg);
  color: var(--text);
  overflow: hidden;
}

.app-shell {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.app-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 18px;
  background: var(--panel);
  border-bottom: 1px solid var(--border);
  -webkit-user-select: none;
}

.brand {
  display: flex;
  align-items: center;
  gap: 10px;
}

.logo {
  width: 26px;
  height: 26px;
}

.title {
  font-size: 15px;
  font-weight: 600;
  letter-spacing: 0.2px;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.running-badge {
  background: rgba(24, 160, 88, 0.12);
  color: var(--green);
  font-size: 12px;
  padding: 3px 10px;
  border-radius: 10px;
  font-weight: 500;
}

.app-main {
  flex: 1;
  overflow: auto;
  padding: 16px 18px;
}
</style>
