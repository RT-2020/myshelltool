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
  type GatewayStatusView
} from '@/services/backend';

const store = useWorkbenchStore();

interface ImportSummary {
  vps_host: string; vps_port: number; mount_port: number;
  listen_port: number; schtasks_command: string;
}

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
const setupBusy = ref(false);
const setupError = ref('');
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
      <p class="muted">
        这台机器是<b>跳板机</b>（客户内网）时用本页：输入 relay 服务器地址与口令，
        一键配置并启动——其余全部自动完成。
      </p>
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

.gw-status-card {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  border: 1px solid var(--border);
  border-radius: var(--radius-sm);
  margin-bottom: var(--space-3);

  .gw-status-dot {
    flex: 0 0 auto;
    width: 10px;
    height: 10px;
    margin-top: 4px;
    border-radius: 50%;
    background: var(--text-secondary);
  }

  .gw-status-body { display: grid; gap: 2px; min-width: 0; }
  .gw-status-label { font-weight: 500; color: var(--text-primary); }
  .gw-status-meta { font-size: var(--font-size-xs); color: var(--text-secondary); font-family: var(--font-mono); }
  .gw-status-hint { font-size: var(--font-size-xs); color: var(--text-secondary); }
  .gw-status-detail {
    font-size: var(--font-size-xs);
    color: var(--warn, #b8860b);
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }

  &[data-tone='ok'] { border-color: var(--ok, #3a9d5d); .gw-status-dot { background: var(--ok, #3a9d5d); } }
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

.secret-row {
  display: flex;
  gap: var(--space-1);
  align-items: center;

  .secret-toggle {
    flex: 0 0 auto;
    font-size: var(--font-size-xs);
    padding: 0 var(--space-2);
    height: 28px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;

    &:hover { border-color: var(--accent, currentColor); color: var(--text-primary); }
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
  border-top: 1px dashed var(--border);
  padding-top: var(--space-2);

  summary {
    cursor: pointer;
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
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
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: var(--font-size-xs);
  }
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

  .link-btn { position: absolute; top: var(--space-1); right: var(--space-1); }
}
</style>
