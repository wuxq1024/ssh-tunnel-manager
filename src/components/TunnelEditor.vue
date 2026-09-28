<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import {
  NButton,
  NDrawer,
  NDrawerContent,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NRadioButton,
  NRadioGroup,
  NSelect,
  NDynamicInput,
  NSpace,
  NSwitch,
  NAlert,
  NDivider,
  useMessage,
} from "naive-ui";
import { invoke } from "@tauri-apps/api/core";
import { useTunnelStore } from "../stores/tunnels";
import type { ForwardRule, TestReport, TunnelConfig } from "../types";

const props = defineProps<{ config?: TunnelConfig | null }>();

const emit = defineEmits<{ (e: "saved"): void }>();
const store = useTunnelStore();
const message = useMessage();

const show = ref(false);
const saving = ref(false);
const testing = ref(false);
const testReport = ref<TestReport | null>(null);

function newId(): string {
  return crypto.randomUUID();
}

function blank(): TunnelConfig {
  const now = Date.now();
  return {
    id: newId(),
    name: "",
    host: "",
    port: 22,
    username: "root",
    auth_method: { password: "password", has_stored_password: false },
    rules: [
      {
        forward: "remote",
        bind_addr: "127.0.0.1",
        bind_port: 7890,
        target_host: "127.0.0.1",
        target_port: 7890,
      },
    ],
    auto_connect: false,
    auto_reconnect: true,
    keepalive_secs: 30,
    created_at: now,
    updated_at: now,
  };
}

const form = reactive<TunnelConfig>(blank());

// 密码/口令输入框（不回显已存凭据）
const passwordInput = ref("");
const passphraseInput = ref("");

watch(show, (v) => {
  if (v) {
    Object.assign(form, JSON.parse(JSON.stringify(props.config ?? blank())));
    passwordInput.value = "";
    passphraseInput.value = "";
    testReport.value = null;
  }
});

const isEdit = computed(() => !!props.config);

const authKind = computed({
  get: () => form.auth_method.password,
  set: (v: "password" | "key") => {
    form.auth_method =
      v === "password"
        ? { password: "password", has_stored_password: false }
        : {
            password: "key",
            key_path: "",
            has_stored_passphrase: false,
          };
  },
});

const keyPath = computed({
  get: () => (form.auth_method.password === "key" ? form.auth_method.key_path : ""),
  set: (v: string) => {
    if (form.auth_method.password === "key") form.auth_method.key_path = v;
  },
});

function onCreateRule(): ForwardRule {
  return {
    forward: "remote",
    bind_addr: "127.0.0.1",
    bind_port: 8080,
    target_host: "127.0.0.1",
    target_port: 80,
  };
}

function onRuleKindChange(rule: ForwardRule, kind: string) {
  if (kind === rule.forward) return;
  const idx = form.rules.indexOf(rule);
  if (idx < 0) return;
  if (kind === "dynamic") {
    form.rules[idx] = { forward: "dynamic", bind_addr: "127.0.0.1", bind_port: rule.bind_port };
  } else {
    form.rules[idx] = {
      forward: kind as "remote" | "local",
      bind_addr: "127.0.0.1",
      bind_port: rule.bind_port,
      target_host: "127.0.0.1",
      target_port: rule.bind_port,
    };
  }
}

const hasRemoteNonLoopback = computed(() =>
  form.rules.some(
    (r) =>
      r.forward === "remote" &&
      !["127.0.0.1", "::1", "localhost"].includes(r.bind_addr)
  )
);

async function pickKeyFile() {
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const f = await open({ multiple: false, directory: false, title: "选择私钥文件" });
    if (typeof f === "string") keyPath.value = f;
  } catch (e) {
    message.error(`打开文件选择器失败: ${e}`);
  }
}

async function onTest() {
  testing.value = true;
  testReport.value = null;
  try {
    testReport.value = await invoke<TestReport>("test_connection", {
      host: form.host,
      port: form.port,
      username: form.username,
      authMethod: form.auth_method,
      password: passwordInput.value || null,
      passphrase: passphraseInput.value || null,
      useStoredPassword: true,
    });
  } catch (e) {
    testReport.value = { ok: false, message: String(e), elapsed_ms: 0, server_fingerprint: null };
  } finally {
    testing.value = false;
  }
}

async function onSave() {
  if (!form.name.trim()) return message.warning("请填写任务名称");
  if (!form.host.trim()) return message.warning("请填写服务器地址");
  if (!form.username.trim()) return message.warning("请填写用户名");
  if (form.rules.length === 0) return message.warning("至少添加一条转发规则");

  saving.value = true;
  try {
    const cfg = JSON.parse(JSON.stringify(form)) as TunnelConfig;
    cfg.updated_at = Date.now();
    await store.saveTunnel(
      cfg,
      passwordInput.value || null,
      form.auth_method.password === "key" ? passphraseInput.value || null : null
    );
    message.success(isEdit.value ? "已保存" : "已创建");
    show.value = false;
    emit("saved");
  } catch (e) {
    message.error(`保存失败: ${e}`);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <span @click.stop>
    <span @click="show = true" style="display: inline-flex">
      <slot name="trigger" />
    </span>

    <n-drawer v-model:show="show" :width="520" placement="right">
      <n-drawer-content :title="isEdit ? '编辑隧道' : '新建隧道'" closable>
        <n-form label-placement="top" size="medium" class="editor-form">
          <n-form-item label="任务名称" required>
            <n-input v-model:value="form.name" placeholder="例如：办公网代理" />
          </n-form-item>

          <div class="row-2">
            <n-form-item label="服务器地址" required class="grow">
              <n-input v-model:value="form.host" placeholder="10.185.220.151" />
            </n-form-item>
            <n-form-item label="端口" style="width: 110px">
              <n-input-number v-model:value="form.port" :min="1" :max="65535" />
            </n-form-item>
          </div>

          <n-form-item label="用户名" required>
            <n-input v-model:value="form.username" placeholder="root" />
          </n-form-item>

          <n-form-item label="认证方式">
            <n-radio-group v-model:value="authKind">
              <n-radio-button value="password">密码</n-radio-button>
              <n-radio-button value="key">私钥</n-radio-button>
            </n-radio-group>
          </n-form-item>

          <template v-if="authKind === 'password'">
            <n-form-item label="密码">
              <n-input
                v-model:value="passwordInput"
                type="password"
                show-password-on="click"
                :placeholder="
                  form.auth_method.password === 'password' && form.auth_method.has_stored_password
                    ? '已保存在凭据管理器，留空则沿用'
                    : '输入密码（将保存在 Windows 凭据管理器）'
                "
              />
            </n-form-item>
          </template>

          <template v-else>
            <n-form-item label="私钥路径" required>
              <n-input v-model:value="keyPath" placeholder="C:\Users\me\.ssh\id_ed25519">
                <template #suffix>
                  <n-button size="tiny" @click="pickKeyFile">浏览…</n-button>
                </template>
              </n-input>
            </n-form-item>
            <n-form-item label="Passphrase（可选）">
              <n-input
                v-model:value="passphraseInput"
                type="password"
                show-password-on="click"
                :placeholder="
                  keyPath && form.auth_method.password === 'key' && form.auth_method.has_stored_passphrase
                    ? '已保存在凭据管理器，留空则沿用'
                    : '若私钥有口令则填写'
                "
              />
            </n-form-item>
          </template>

          <n-divider title-placement="left" class="sec">转发规则</n-divider>

          <n-alert v-if="hasRemoteNonLoopback" type="warning" :show-icon="true" class="gw-tip">
            检测到 -R 规则绑定了非 loopback 地址：需在远程 sshd 配置
            <code>GatewayPorts yes</code>（或 <code>clientspecified</code>）才会对外监听。
          </n-alert>

          <n-dynamic-input v-model:value="form.rules" :on-create="onCreateRule">
            <template #default="{ value: rule }">
              <div class="rule-row">
                <n-select
                  :value="rule.forward"
                  :options="[
                    { label: '-R 远程', value: 'remote' },
                    { label: '-L 本地', value: 'local' },
                    { label: '-D 动态', value: 'dynamic' },
                  ]"
                  style="width: 108px"
                  @update:value="(v: string) => onRuleKindChange(rule, v)"
                />
                <n-input v-model:value="rule.bind_addr" placeholder="绑定地址" style="width: 118px" />
                <n-input-number v-model:value="rule.bind_port" placeholder="端口" :min="1" :max="65535" style="width: 104px" :show-button="false" />
                <template v-if="rule.forward !== 'dynamic'">
                  <span class="arrow">→</span>
                  <n-input v-model:value="rule.target_host" placeholder="目标主机" style="width: 128px" />
                  <n-input-number v-model:value="rule.target_port" :min="1" :max="65535" style="width: 96px" :show-button="false" />
                </template>
                <span v-else class="socks-hint">SOCKS5</span>
              </div>
            </template>
          </n-dynamic-input>

          <n-divider title-placement="left" class="sec">高级</n-divider>

          <div class="row-2">
            <n-form-item label="断线自动重连">
              <n-switch v-model:value="form.auto_reconnect" />
            </n-form-item>
            <n-form-item label="应用启动时自动连接">
              <n-switch v-model:value="form.auto_connect" />
            </n-form-item>
          </div>

          <n-form-item label="Keepalive 间隔（秒）">
            <n-input-number v-model:value="form.keepalive_secs" :min="5" :max="600" style="width: 160px" />
          </n-form-item>

          <n-form-item label=" ">
            <n-space>
              <n-button :loading="testing" @click="onTest">测试连接</n-button>
            </n-space>
            <div v-if="testReport" class="test-report" :class="testReport.ok ? 'ok' : 'fail'">
              <template v-if="testReport.ok">
                ✅ 连接成功（{{ testReport.elapsed_ms }}ms）
                <div v-if="testReport.server_fingerprint" class="fp mono">
                  指纹: {{ testReport.server_fingerprint }}
                </div>
              </template>
              <template v-else>❌ {{ testReport.message }}</template>
            </div>
          </n-form-item>
        </n-form>

        <template #footer>
          <n-space>
            <n-button @click="show = false">取消</n-button>
            <n-button type="primary" :loading="saving" @click="onSave">保存</n-button>
          </n-space>
        </template>
      </n-drawer-content>
    </n-drawer>
  </span>
</template>

<style scoped>
.editor-form {
  padding-right: 6px;
}

.row-2 {
  display: flex;
  gap: 12px;
}

.row-2 .grow {
  flex: 1;
}

.rule-row {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  flex-wrap: wrap;
}

.arrow {
  color: #98a2b8;
}

.socks-hint {
  font-size: 12px;
  color: #3b5bdb;
  background: #f0f4ff;
  padding: 2px 8px;
  border-radius: 6px;
}

.sec {
  margin-top: 6px;
}

.gw-tip {
  margin-bottom: 10px;
  font-size: 12px;
}

.gw-tip code {
  background: rgba(240, 160, 32, 0.15);
  padding: 0 4px;
  border-radius: 3px;
}

.test-report {
  margin-left: 14px;
  font-size: 13px;
}

.test-report.ok {
  color: #18a058;
}

.test-report.fail {
  color: #d03050;
  word-break: break-all;
}

.fp {
  font-size: 11px;
  color: #98a2b8;
}

.mono {
  font-family: Consolas, monospace;
}
</style>
