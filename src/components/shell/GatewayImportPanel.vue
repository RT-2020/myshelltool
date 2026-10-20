<script setup lang="ts">
/**
 * 网关「导入配置」面板（跳板机侧，v0.21）——装完 mst 的全部操作就在这一页。
 *
 * - 实时状态卡：独立网关进程 → 状态文件心跳（run.rs 状态通道）→ 5s 轮询，
 *   五态呈现（未配置/连接中/已挂载/重连中/已停止），detail 显示上次错误。
 * - 两栏口径：relay 服务器地址 + 口令（密钥从口令派生，零文件）。
 * - 高级折叠：v0.20 bundle 文件导入（兼容旧交付物）。
 */
import { computed, onUnmounted, ref } from 'vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import { useWorkbenchStore } from '@/stores/workbench';
import {
  invokeBackend,
  gatewayImportBundle,
  gatewayLaunch,
  gatewaySetupFromSecret,
  gatewayStatus,
  type GatewayImportResult,
  type GatewayStatusView
} from '@/services/backend';

const store = useWorkbenchStore();

// ─── 实时状态（5s 轮询；状态文件 + 心跳，见 gateway run.rs 状态通道设计）───
const gwStatus = ref<GatewayStatusView | null>(null);
let statusTimer: number | undefined;
const STATUS_META: Record<string, { label: string; tone: string; hint?: string }> = {
  mounted: { label: '运行中 · 已挂载', tone: 'ok', hint: '网关正常工作，本机可经挂载口访问这台机器的内网' },
  mounting: { label: '连接 relay 中…', tone: 'busy' },
  reconnecting: { label: '连接断开 · 自动重连中', tone: 'warn', hint: '网络波动后自动恢复；持续此状态请检查 relay 与出站网络' },
  stopped: { label: '已停止', tone: 'err', hint: '网关进程未运行（重启机器后未自启？）——重新点「连接并启动网关」' },
  not_configured: { label: '未配置', tone: 'idle' }
};
const statusMeta = computed(() => {
  const st = gwStatus.value?.state || 'not_configured';
  return { key: st, ...(STATUS_META[st] || STATUS_META.not_configured) };
});
async function refreshStatus() {
  try { gwStatus.value = await gatewayStatus(); } catch { /* 轮询失败保持上次态 */ }
}
refreshStatus();
statusTimer = window.setInterval(refreshStatus, 5000);
onUnmounted(() => window.clearInterval(statusTimer));

// ─── 「服务器 + 口令」两栏 ───
const setupHost = ref('');
const setupSecret = ref('');
const showSecret = ref(false);
// 高级折叠（v0.21.2）：控制口/挂载口参数化（安全组受限的中转架构如 iptables
// 80→18080 需要非默认值）+ 内网白名单（执法在本机内嵌 server，空 = 允许全部）
const setupPort = ref(22222);
const setupMountPort = ref(2222);
const allowedTargetsText = ref('');
const setupBusy = ref(false);
const setupError = ref('');
const importResult = ref<GatewayImportResult | null>(null);
const launched = ref(false);

async function runSetupAndLaunch() {
  if (!setupHost.value.trim()) { setupError.value = '请输入 relay 服务器地址'; return; }
  if (!setupSecret.value.trim()) { setupError.value = '请输入口令'; return; }
  setupError.value = '';
  setupBusy.value = true;
  try {
    const allowed = allowedTargetsText.value
      .split(/[\n,，;；]/)
      .map(s => s.trim())
      .filter(Boolean);
    importResult.value = await gatewaySetupFromSecret(
      setupHost.value.trim(),
      setupSecret.value.trim(),
      Number(setupPort.value),
      Number(setupMountPort.value),
      allowed
    );
    await gatewayLaunch();
    launched.value = true;
    store.announce('网关已配置并启动', { level: 'success' });
    refreshStatus();
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
    options: {
      title: '选择网关 bundle 文件',
      filters: [{ name: 'Gateway Bundle', extensions: ['json'] }]
    }
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
    refreshStatus();
  } catch (e) {
    importError.value = String(e);
  } finally {
    importBusy.value = false;
  }
}

function copyText(text: string, what: string) {
  navigator.clipboard?.writeText(text).then(
    () => store.announce(`已复制${what}`, { level: 'success' }),
    () => store.announce('复制失败，请手动选择复制', { level: 'warn' })
  );
}
</script>

<template>
  <div class="stack" data-gateway-import>
    <!-- 实时状态卡 -->
    <div class="gw-status-card" :data-tone="statusMeta.tone" :data-state="statusMeta.key" role="status">
      <span class="gw-status-dot" aria-hidden="true"></span>
      <div class="gw-status-body">
        <span class="gw-status-label">{{ statusMeta.label }}</span>
        <span v-if="gwStatus && gwStatus.state === 'mounted'" class="gw-status-meta">
          relay {{ gwStatus.vps_host }}:{{ gwStatus.vps_port }} · 挂载口 {{ gwStatus.mount_port }}
        </span>
        <span v-if="statusMeta.hint" class="gw-status-hint">{{ statusMeta.hint }}</span>
        <span v-if="gwStatus?.detail" class="gw-status-detail" :title="gwStatus.detail">
          {{ gwStatus.detail.length > 60 ? gwStatus.detail.slice(0, 60) + '…' : gwStatus.detail }}
        </span>
      </div>
    </div>

    <template v-if="!importResult">
      <label class="stack"><span>relay 服务器地址</span>
        <AppInput v-model="setupHost" placeholder="例如 47.109.61.34" data-gateway-field="setup-host" />
      </label>
      <label class="stack"><span>口令（运维方部署 relay 时设置的那个）</span>
        <span class="secret-row">
          <AppInput v-model="setupSecret" :type="showSecret ? 'text' : 'password'" data-gateway-field="setup-secret" />
          <button type="button" class="secret-toggle" :title="showSecret ? '隐藏口令' : '显示口令'"
            :aria-label="showSecret ? '隐藏口令' : '显示口令'" @click="showSecret = !showSecret">
            {{ showSecret ? '隐藏' : '显示' }}
          </button>
        </span>
      </label>
      <details class="advanced-fold">
        <summary>高级：端口与内网白名单（标准部署保持默认即可）</summary>
        <div class="port-row" style="margin-top: var(--space-2)">
          <label class="stack"><span>SSH 控制口（默认 22222）</span>
            <AppInput v-model.number="setupPort" type="number" data-gateway-field="setup-port" />
          </label>
          <label class="stack"><span>挂载口（默认 2222）</span>
            <AppInput v-model.number="setupMountPort" type="number" data-gateway-field="setup-mount-port" />
          </label>
        </div>
        <label class="stack" style="margin-top: var(--space-2)"><span>内网目标白名单（可选，空 = 允许全部；每行一条，如 10.0.0.0/8:22）</span>
          <textarea
            v-model="allowedTargetsText"
            class="native-input"
            rows="2"
            spellcheck="false"
            placeholder="172.16.1.0/24"
            data-gateway-field="setup-allowed-targets"
          ></textarea>
        </label>
      </details>

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
        {{ launched ? '✅ 网关已启动，实时状态见上方状态卡' : '✅ 网关配置已导入' }}（relay {{ importResult.vps_host }}:{{ importResult.vps_port }}，
        挂载口 {{ importResult.mount_port }}）。关闭本窗口不影响运行；
        日志在 <code>%APPDATA%\com.redtei.myshelltool\logs\gateway.log</code>。
      </p>
      <p class="muted">最后一步（可选，开机自启）——在管理员 PowerShell 执行：</p>
      <div class="code-block">
        <pre>{{ importResult.schtasks_command }}</pre>
        <button type="button" class="link-btn" @click="copyText(importResult.schtasks_command, '自启命令')">复制</button>
      </div>
    </template>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// .form-error 原定义在 GlobalModals.modal.scss（scoped），子组件内部元素够不着
// ——本组件自 GlobalModals 模板抽出后该类一直未被命中，就地补齐（对齐其视觉）。
.form-error {
  margin: 0;
  padding: 8px 10px;
  border: 1px solid color-mix(in oklab, var(--danger), transparent 50%);
  border-radius: var(--radius-sm);
  background: color-mix(in oklab, var(--danger), transparent 88%);
  color: var(--danger);
  font-size: var(--text-xs);
}

.gw-status-card {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  margin-bottom: var(--space-3);

  .gw-status-dot {
    flex: 0 0 auto;
    width: 10px;
    height: 10px;
    margin-top: 4px;
    border-radius: 50%;
    background: var(--app-muted);
  }

  .gw-status-body { display: grid; gap: 2px; min-width: 0; }
  .gw-status-label { font-weight: 500; color: var(--app-text); }
  .gw-status-meta { font-size: var(--text-xs); color: var(--app-muted); font-family: var(--font-mono); }
  .gw-status-hint { font-size: var(--text-xs); color: var(--app-muted); }
  .gw-status-detail {
    font-size: var(--text-xs);
    color: var(--warn, #b8860b);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  &[data-tone='ok'] { border-color: var(--success); .gw-status-dot { background: var(--success); } }
  &[data-tone='warn'] { border-color: var(--warn, #b8860b); .gw-status-dot { background: var(--warn, #b8860b); } }
  &[data-tone='err'] { border-color: var(--danger, #c53030); .gw-status-dot { background: var(--danger, #c53030); } }
  &[data-tone='busy'] .gw-status-dot { animation: gw-dot-pulse 1.2s ease-in-out infinite; }
}

@media (prefers-reduced-motion: reduce) {
  .gw-status-card[data-tone='busy'] .gw-status-dot { animation: none; }
}

@keyframes gw-dot-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.port-row {
  display: flex;
  gap: var(--space-3);

  > label {
    flex: 1;
  }
}

.native-input {
  width: 100%;
  padding: 6px 10px;
  font-size: var(--text-sm);
  font-family: var(--font-mono);
  color: var(--app-text);
  background: var(--app-control);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
}

.secret-row {
  display: flex;
  gap: var(--space-1);
  align-items: center;

  .secret-toggle {
    flex: 0 0 auto;
    font-size: var(--text-xs);
    padding: 0 var(--space-2);
    height: 28px;
    border: 1px solid var(--app-border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--app-muted);
    cursor: pointer;

    &:hover { border-color: var(--accent, currentColor); color: var(--app-text); }
  }
}

.wizard-actions {
  display: flex;
  gap: var(--space-2);
  justify-content: flex-end;
  margin-top: var(--space-3);
}

.advanced-fold {
  margin-top: var(--space-3);
  border-top: 1px dashed var(--app-border);
  padding-top: var(--space-2);

  summary {
    cursor: pointer;
    color: var(--app-muted);
    font-size: var(--text-sm);
    user-select: none;
  }
}

.path-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);

  .path-value {
    flex: 1; min-width: 0;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--app-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
}

.code-block {
  position: relative;
  background: var(--app-panel-2);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  padding: var(--space-2) var(--space-3);
  margin: var(--space-1) 0;

  pre {
    margin: 0;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: pre-wrap;
    word-break: break-all;
    color: var(--app-text);
  }

  .link-btn { position: absolute; top: var(--space-1); right: var(--space-1); }
}
</style>
