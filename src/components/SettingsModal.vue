<script setup lang="ts">
import { ref } from "vue";
import {
  NButton,
  NCheckbox,
  NModal,
  NSpace,
  useMessage,
} from "naive-ui";
import { invoke } from "@tauri-apps/api/core";
import { useTunnelStore } from "../stores/tunnels";

const store = useTunnelStore();
const message = useMessage();
const show = ref(false);
const working = ref<Record<string, boolean>>({});

async function setAutoStart(v: boolean) {
  working.value.autostart = true;
  try {
    await invoke("set_auto_start", { enabled: v });
    message.success(v ? "已开启开机自启" : "已关闭开机自启");
  } catch (e) {
    message.error(String(e));
  } finally {
    working.value.autostart = false;
  }
}

async function onToggleAutoStart(v: boolean) {
  await setAutoStart(v);
  // 重新拉取设置（后端会校正真实状态）
  await store.refreshSettings();
}
</script>

<template>
  <n-button size="small" quaternary @click="show = true">⚙ 设置</n-button>

  <n-modal v-model:show="show" preset="card" title="设置" style="width: 420px">
    <n-space vertical size="large">
      <div class="setting-row">
        <div>
          <div class="st">关闭窗口时最小化到托盘</div>
          <div class="sd">关闭后仍在后台保持隧道运行</div>
        </div>
        <n-checkbox
          :checked="store.settings.close_to_tray"
          @update:checked="(v: boolean) => store.saveSettings({ ...store.settings, close_to_tray: v })"
        />
      </div>

      <div class="setting-row">
        <div>
          <div class="st">开机自启动</div>
          <div class="sd">登录 Windows 后自动启动本应用（写入注册表/启动文件夹）</div>
        </div>
        <n-checkbox
          :checked="store.settings.auto_start"
          :disabled="working.autostart"
          @update:checked="onToggleAutoStart"
        />
      </div>

      <div class="setting-row">
        <div>
          <div class="st">启动时最小化到托盘</div>
          <div class="sd">随自启启动时不弹主窗口</div>
        </div>
        <n-checkbox
          :checked="store.settings.start_minimized"
          @update:checked="(v: boolean) => store.saveSettings({ ...store.settings, start_minimized: v })"
        />
      </div>
    </n-space>
  </n-modal>
</template>

<style scoped>
.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 16px;
}

.st {
  font-size: 13px;
  font-weight: 500;
}

.sd {
  font-size: 12px;
  color: var(--text2);
  margin-top: 2px;
}
</style>
