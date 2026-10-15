<script setup lang="ts">
/**
 * 网关部署向导（v0.22，modal type: gatewayWizard）——自包含两段式：
 *
 * 第一步：VPS 信息 → 生成挂载密钥对（gateway_wizard_start）→ 展示公钥与
 *         已代入公钥的 docker run 模板，用户拿去 VPS 部署 relay。
 * 第二步：用户部署完成后 → 填挂载口/白名单/加密口令/保存路径 →
 *         gateway_wizard_finish（probe 抓 pin + 防轮换 + 导出 bundle）→
 *         gateway_pretrust_host_key 预写 known_hosts → 经**常规 saveAsset 链**
 *         保存网关资产（tag=gateway，私钥走 credentials 常规路径——同步/删除/
 *         Gist 备份全复用，勿另建存储：计划 §5 决策 16 / v3-L5）。
 *
 * 完成态展示跳板机侧三条后续命令（拷文件 / 导入 / schtasks 模板）。
 * 表单状态自持（非 useModalFormSync）：本弹窗不进 dispatchModalSubmit 分发。
 */
import { computed, ref } from 'vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import { useWorkbenchStore } from '@/stores/workbench';
import {
  invokeBackend,
  gatewayWizardStart,
  gatewayWizardFinish,
  gatewayPretrustHostKey,
  gatewayImportBundle,
  gatewayLaunch,
  gatewaySetupFromSecret
} from '@/services/backend';

const store = useWorkbenchStore();

// 顶层页签：「部署」（本机运维侧两段式向导）|「导入」（跳板机侧装完 mst 后
// GUI 点选导入 bundle——v0.20.1 补齐，替代 CLI 拷文件+手敲命令）
const tab = ref<'deploy' | 'import'>('deploy');
const step = ref<1 | 2 | 3>(1); // 1 收集 VPS → 2 部署+导出 → 3 完成指引
const busy = ref(false);
const error = ref('');

// 第一步表单
const vpsHost = ref('');
const vpsPort = ref(22222);
const vpsUsername = ref('mount');

// 第一步产物
const mountPublicKey = ref('');
const dockerTemplate = ref('');

// 第二步表单
const remotePort = ref(2222);
const allowedTargetsText = ref('');
const password = ref('');
const password2 = ref('');
const bundlePath = ref('');

// ─── 导入页签（跳板机侧）───
// v0.21 口径：输入 relay 服务器地址 + 口令即全部（密钥从口令派生，零文件）；
// v0.20 bundle 文件导入保留为高级折叠入口（兼容旧交付物）。
const setupHost = ref('');
const setupSecret = ref('');
const setupBusy = ref(false);
const setupError = ref('');
interface ImportSummary {
  vps_host: string; vps_port: number; mount_port: number;
  listen_port: number; schtasks_command: string;
}
const importResult = ref<ImportSummary | null>(null);
const launched = ref(false);

async function runSetupAndLaunch() {
  if (!setupHost.value.trim()) { setupError.value = '请输入 relay 服务器地址'; return; }
  if (!setupSecret.value.trim()) { setupError.value = '请输入口令'; return; }
  setupError.value = '';
  setupBusy.value = true;
  try {
    importResult.value = await gatewaySetupFromSecret(setupHost.value.trim(), setupSecret.value.trim());
    await gatewayLaunch();
    launched.value = true;
    store.announce('网关已配置并启动', { level: 'success' });
  } catch (e) {
    setupError.value = String(e);
  } finally {
    setupBusy.value = false;
  }
}

// ─── 高级：bundle 文件导入（v0.20 兼容）───
const showFileImport = ref(false);
const importPath = ref('');
const importPassword = ref('');
const importBusy = ref(false);
const importError = ref('');

async function chooseBundleFile() {
  const path = await invokeBackend<string | null>('plugin:dialog|open', {
    title: '选择网关 bundle 文件',
    filters: [{ name: 'Gateway Bundle', extensions: ['json'] }]
  });
  if (path) importPath.value = path;
}

async function runImportAndLaunch() {
  if (!importPath.value.trim()) { importError.value = '请先选择 bundle 文件'; return; }
  if (!importPassword.value) { importError.value = '请输入 bundle 加密口令'; return; }
  importError.value = '';
  importBusy.value = true;
  try {
    importResult.value = await gatewayImportBundle(importPath.value.trim(), importPassword.value);
    await gatewayLaunch();
    launched.value = true;
    store.announce('网关配置已导入并启动', { level: 'success' });
  } catch (e) {
    importError.value = String(e);
  } finally {
    importBusy.value = false;
  }
}

// 完成指引的跳板机导入命令（含 bundle 文件名）
const importCommand = computed(() =>
  bundlePath.value ? `myshelltool.exe --gateway --import "${bundlePath.value.split(/[\\/]/).pop()}"` : ''
);
// 跳板机服务化模板（占位路径避免盘符假设——按实际安装位置替换；详细说明见部署指南）
const schtasksCommand =
  'schtasks /Create /TN myshelltool-gateway /TR "\\"<myshelltool安装路径>\\myshelltool.exe\\" --gateway" /SC ONSTART /RU %USERNAME% /RP';

function copyText(text: string, what: string) {
  navigator.clipboard?.writeText(text).then(
    () => store.announce(`已复制${what}`, { level: 'success' }),
    () => store.announce('复制失败，请手动选择复制', { level: 'warn' })
  );
}

async function runStep1() {
  if (!vpsHost.value.trim() || !vpsUsername.value.trim()) {
    error.value = 'VPS 地址与挂载用户名不能为空';
    return;
  }
  if (!(Number(vpsPort.value) > 0 && Number(vpsPort.value) < 65536)) {
    error.value = 'VPS SSH 端口须为 1-65535';
    return;
  }
  error.value = '';
  busy.value = true;
  try {
    const r = await gatewayWizardStart(vpsHost.value.trim(), Number(vpsPort.value), vpsUsername.value.trim());
    mountPublicKey.value = r.mount_public_key;
    dockerTemplate.value = r.docker_template;
    step.value = 2;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

async function chooseSavePath() {
  const path = await invokeBackend<string | null>('plugin:dialog|save', {
    title: '保存网关 bundle（加密文件）',
    defaultPath: 'gateway-bundle.json',
    filters: [{ name: 'Gateway Bundle', extensions: ['json'] }]
  });
  if (path) bundlePath.value = path;
}

async function runStep2() {
  if (!bundlePath.value.trim()) {
    error.value = '请先选择 bundle 保存位置';
    return;
  }
  if (password.value.length < 6) {
    error.value = '加密口令至少 6 位（bundle 含 VPS 凭据与网关私钥）';
    return;
  }
  if (password.value !== password2.value) {
    error.value = '两次输入的加密口令不一致';
    return;
  }
  error.value = '';
  busy.value = true;
  try {
    const r = await gatewayWizardFinish({
      vpsHost: vpsHost.value.trim(),
      vpsPort: Number(vpsPort.value),
      vpsUsername: vpsUsername.value.trim(),
      remotePort: Number(remotePort.value),
      allowedTargets: allowedTargetsText.value
        .split(/[\n,，;；]/)
        .map(s => s.trim())
        .filter(Boolean),
      password: password.value,
      savePath: bundlePath.value.trim()
    });
    bundlePath.value = r.bundle_path;

    // 预写 known_hosts（"VPS:挂载口" → 网关 host key）：全链路首次连接零弹窗
    await gatewayPretrustHostKey(r.known_host_port, r.known_key_type, r.known_key_hex);

    // 网关资产走常规 saveAsset（凭据常规链；tag gateway 驱动徽标与连接拦截）
    await store.saveAsset(
      {
        name: `网关 ${r.asset_host}:${r.asset_port}`,
        host: r.asset_host,
        port: r.asset_port,
        username: 'gateway',
        auth_method: 'PrivateKey',
        group: '未分组',
        tags: ['gateway'],
        jump_host: null,
        private_key_path: null
      },
      { privateKey: r.client_private_key }
    );

    store.announce('网关 bundle 已导出，网关资产已保存', { level: 'success' });
    step.value = 3;
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="stack" data-gateway-wizard>
    <!-- 顶层页签：本机走向导部署 | 跳板机（已装 mst）GUI 导入 -->
    <div class="wizard-tabs" role="tablist">
      <button type="button" :class="['tab-btn', { active: tab === 'deploy' }]" role="tab" :aria-selected="tab === 'deploy'" @click="tab = 'deploy'">部署向导（本机）</button>
      <button type="button" :class="['tab-btn', { active: tab === 'import' }]" role="tab" :aria-selected="tab === 'import'" @click="tab = 'import'">导入配置（跳板机）</button>
    </div>

    <!-- ═══ 导入页签：跳板机侧「服务器 + 口令」两栏口径 ═══ -->
    <template v-if="tab === 'import'">
      <template v-if="!importResult">
        <p class="muted">
          这台机器是<b>跳板机</b>（客户内网）时用本页：输入 relay 服务器地址与口令，
          一键配置并启动——其余全部自动完成。
        </p>
        <label class="stack"><span>relay 服务器地址</span>
          <AppInput v-model="setupHost" placeholder="例如 47.109.61.34" data-gateway-field="setup-host" />
        </label>
        <label class="stack"><span>口令（运维方部署 relay 时设置的那个）</span>
          <AppInput v-model="setupSecret" type="password" data-gateway-field="setup-secret" />
        </label>
        <p v-if="setupError" class="form-error" style="white-space:pre-wrap">{{ setupError }}</p>
        <div class="wizard-actions">
          <AppButton variant="primary" :disabled="setupBusy" @click="runSetupAndLaunch">
            {{ setupBusy ? '正在配置…' : '连接并启动网关' }}
          </AppButton>
        </div>
        <details class="advanced-fold">
          <summary>高级：从 bundle 文件导入（旧版交付物）</summary>
          <div class="stack" style="margin-top: var(--space-2)">
            <div class="path-row">
              <span class="path-value">{{ importPath || '未选择文件' }}</span>
              <AppButton :disabled="importBusy" @click="chooseBundleFile">选择文件…</AppButton>
            </div>
            <AppInput v-model="importPassword" type="password" placeholder="bundle 加密口令" data-gateway-field="import-password" />
            <p v-if="importError" class="form-error" style="white-space:pre-wrap">{{ importError }}</p>
            <div class="wizard-actions">
              <AppButton :disabled="importBusy" @click="runImportAndLaunch">
                {{ importBusy ? '正在导入…' : '导入并启动' }}
              </AppButton>
            </div>
          </div>
        </details>
      </template>
      <template v-else>
        <p class="muted">
          {{ launched ? '✅ 网关已导入并在后台运行' : '✅ 网关配置已导入' }}（VPS {{ importResult.vps_host }}:{{ importResult.vps_port }}，
          挂载口 {{ importResult.mount_port }}）。关闭本窗口不影响运行；
          日志在 <code>%APPDATA%\com.redtei.myshelltool\logs\gateway.log</code>。
        </p>
        <p class="muted">最后一步（可选，开机自启）——在管理员 PowerShell 执行：</p>
        <div class="code-block">
          <pre>{{ importResult.schtasks_command }}</pre>
          <button type="button" class="link-btn" @click="copyText(importResult.schtasks_command, '自启命令')">复制</button>
        </div>
      </template>
    </template>

    <!-- ═══ 部署页签：原两段式向导 ═══ -->
    <template v-else>
    <!-- 第一步：VPS 信息 → 生成挂载密钥 -->
    <template v-if="step === 1">
      <p class="muted">
        在任意一台公网服务器上部署 relay（Docker 一条命令），本机即可经它访问客户内网跳板机。
        VPS 前提：已装 Docker。
      </p>
      <label class="stack"><span>VPS 地址</span>
        <AppInput v-model="vpsHost" placeholder="例如 117.72.25.115" data-gateway-field="vps-host" />
      </label>
      <label class="stack"><span>SSH 控制口（relay 默认 22222）</span>
        <AppInput v-model="vpsPort" type="number" data-gateway-field="vps-port" />
      </label>
      <label class="stack"><span>挂载用户名（relay 内置账户，默认 mount）</span>
        <AppInput v-model="vpsUsername" data-gateway-field="vps-user" />
      </label>
      <p v-if="error" class="form-error">{{ error }}</p>
      <div class="wizard-actions">
        <AppButton variant="primary" :disabled="busy" @click="runStep1">
          {{ busy ? '生成中…' : '生成挂载密钥' }}
        </AppButton>
      </div>
    </template>

    <!-- 第二步：部署确认 + 导出 -->
    <template v-else-if="step === 2">
      <p class="muted">第一步：在 VPS 上执行以下命令部署 relay（公钥已代入）：</p>
      <div class="code-block">
        <pre>{{ dockerTemplate }}</pre>
        <button type="button" class="link-btn" @click="copyText(dockerTemplate, 'docker 命令')">复制</button>
      </div>
      <p class="muted">
        部署并启动后，回到这里完成导出。挂载密钥公钥（已包含在命令里）：
        <code class="key-fragment">{{ mountPublicKey.slice(0, 50) }}…</code>
      </p>
      <label class="stack"><span>挂载口（VPS 上对外暴露的端口，默认 2222）</span>
        <AppInput v-model="remotePort" type="number" data-gateway-field="mount-port" />
      </label>
      <label class="stack">
        <span>内网目标白名单（可选，空 = 允许全部；每行一条，如 10.0.0.0/8:22）</span>
        <textarea
          v-model="allowedTargetsText"
          class="native-input"
          rows="2"
          spellcheck="false"
          placeholder="10.0.0.0/8&#10;192.168.0.0/16:3389"
          data-gateway-field="allowed-targets"
        ></textarea>
      </label>
      <label class="stack"><span>bundle 加密口令（至少 6 位，拷去跳板机导入时要用）</span>
        <AppInput v-model="password" type="password" data-gateway-field="password" />
      </label>
      <label class="stack"><span>确认口令</span>
        <AppInput v-model="password2" type="password" data-gateway-field="password2" />
      </label>
      <label class="stack"><span>bundle 保存位置</span>
        <div class="path-row">
          <span class="path-value">{{ bundlePath || '未选择' }}</span>
          <AppButton :disabled="busy" @click="chooseSavePath">选择位置…</AppButton>
        </div>
      </label>
      <p v-if="error" class="form-error" style="white-space:pre-wrap">{{ error }}</p>
      <div class="wizard-actions">
        <AppButton :disabled="busy" @click="step = 1">上一步</AppButton>
        <AppButton variant="primary" :disabled="busy" @click="runStep2">
          {{ busy ? '正在探测并导出…' : '我已在 VPS 部署完成，生成 bundle' }}
        </AppButton>
      </div>
    </template>

    <!-- 第三步：完成指引 -->
    <template v-else>
      <p class="muted">bundle 已导出、网关资产已保存。接下来在<b>跳板机</b>上完成三步：</p>
      <ol class="steps">
        <li>
          把加密文件拷到跳板机（U 盘 / 受控 IM）：<code>{{ bundlePath }}</code>
        </li>
        <li>
          安装 myshelltool 后，在有终端的 PowerShell 中执行：
          <div class="code-block">
            <pre>{{ importCommand }}</pre>
            <button type="button" class="link-btn" @click="copyText(importCommand, '导入命令')">复制</button>
          </div>
        </li>
        <li>
          注册开机自启（无需登录桌面也运行；<b>必须与导入时的 Windows 用户一致</b>）：
          <div class="code-block">
            <pre>{{ schtasksCommand }}</pre>
            <button type="button" class="link-btn" @click="copyText(schtasksCommand, '服务化命令')">复制</button>
          </div>
        </li>
      </ol>
      <p class="muted">
        完成后在本机新建内网机器资产，「跳板机」填
        <code>{{ vpsHost }}:{{ remotePort }}</code> 即可经网关连接（终端/文件/隧道全通）。
        完整文档见 docs/网关模式部署指南.md。
      </p>
    </template>
    </template><!-- /部署页签 -->
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.advanced-fold {
  margin-top: var(--space-3);
  border-top: 1px dashed var(--border);
  padding-top: var(--space-2);

  summary {
    cursor: pointer;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    user-select: none;
  }
}

.wizard-tabs {
  display: flex;
  gap: var(--space-1);
  margin-bottom: var(--space-2);
  border-bottom: 1px solid var(--border);

  .tab-btn {
    padding: var(--space-1) var(--space-2);
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    cursor: pointer;

    &.active {
      color: var(--text-primary);
      border-bottom-color: var(--accent, currentColor);
      font-weight: 500;
    }
  }
}

.wizard-actions {
  display: flex;
  gap: var(--space-2);
  justify-content: flex-end;
  margin-top: var(--space-3);
}

.code-block {
  position: relative;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  padding: var(--space-2) var(--space-3);
  margin: var(--space-1) 0;

  pre {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--text-primary);
  }

  .link-btn {
    position: absolute;
    top: var(--space-1);
    right: var(--space-1);
  }
}

.path-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);

  .path-value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
}

.key-fragment {
  font-family: var(--font-mono);
  font-size: var(--font-size-xs);
  word-break: break-all;
}

.steps {
  padding-left: 1.2em;
  display: grid;
  gap: var(--space-2);
}
</style>
