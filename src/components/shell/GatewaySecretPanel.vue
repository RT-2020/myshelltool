<script setup lang="ts">
/**
 * 网关「口令模式」面板（本机·运维侧，v0.21.2）——单页直配。
 *
 * - 两序号按钮消费同一次口令派生（gateway_secret_derive 纯函数零状态）：
 *   ① 复制 relay 部署命令（去 VPS 执行一次）② 写入本机配置（派生客户端钥
 *   经常规 saveAsset 落凭据 + 预写 known_hosts + 建/更新网关资产）。
 *   两者无顺序依赖：本机预写是口令确定性派生，不依赖 relay 已部署。
 * - ② 的落地链每步纯或幂等（derive 纯函数 / pretrust 同 key 幂等 /
 *   saveAsset 按 host:port 查重带 id 更新），失败重试无半写态。
 * - known_hosts 已有旧网关公钥（bundle→口令迁移、外部入口口不变的场景）时
 *   pretrust 拒绝，本面板以**内联确认**呈现冲突（错误上下文就在眼前），
 *   用户显式点击后才带 confirm_replace 重调——非静默轮换。
 */
import { ref } from 'vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';
import { useWorkbenchStore } from '@/stores/workbench';
import {
  gatewayGenerateSecret,
  gatewayPretrustHostKey,
  gatewaySecretDerive,
  type GatewaySecretDeriveResult
} from '@/services/backend';

const store = useWorkbenchStore();

const host = ref('');
const vpsPort = ref(22222);
const mountPort = ref(2222);
const secret = ref('');
const showSecret = ref(false);
const busy = ref(false);
const error = ref('');
const done = ref<GatewaySecretDeriveResult | null>(null);
// pretrust 撞旧网关身份 → true：内联呈现冲突与「确认替换」按钮
const pinConflict = ref<GatewaySecretDeriveResult | null>(null);

function validateForm(): string {
  if (!host.value.trim()) return '请输入 relay 服务器地址';
  const p = Number(vpsPort.value);
  const m = Number(mountPort.value);
  if (!(p > 0 && p < 65536)) return 'SSH 控制口必须是 1-65535';
  if (!(m > 0 && m < 65536)) return '挂载口必须是 1-65535';
  const s = secret.value.trim();
  if (s.length < 8) return '口令至少 8 位（口令是网关全部身份的信任根）';
  if (s.includes("'")) return "口令不能包含单引号（会让 relay 部署命令失效）——请改用「生成口令」";
  return '';
}

async function generateSecret() {
  try {
    secret.value = await gatewayGenerateSecret();
    showSecret.value = true;
  } catch (e) {
    error.value = String(e);
  }
}

async function copyText(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text);
    store.announce(`已复制${what}`, { level: 'success' });
  } catch {
    store.announce('复制失败，请手动选择复制', { level: 'warn' });
  }
}

async function copyDockerCommand() {
  error.value = '';
  const v = validateForm();
  if (v) { error.value = v; return; }
  busy.value = true;
  try {
    const r = await gatewaySecretDerive(host.value.trim(), secret.value.trim(), Number(vpsPort.value), Number(mountPort.value));
    await copyText(r.docker_command, 'relay 部署命令');
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}

// 三步链：derive（纯）→ pretrust（幂等/可确认替换）→ saveAsset（查重带 id 幂等）
async function writeLocal(confirmReplace = false) {
  error.value = '';
  const v = validateForm();
  if (v) { error.value = v; return; }
  busy.value = true;
  try {
    const r = await gatewaySecretDerive(host.value.trim(), secret.value.trim(), Number(vpsPort.value), Number(mountPort.value));
    try {
      await gatewayPretrustHostKey(r.known_host_port, r.known_key_type, r.known_key_hex, confirmReplace);
    } catch (e) {
      // 前缀标记识别冲突（与 [download:cancelled] 同款跨层契约，不依赖文案措辞）
      if (!confirmReplace && String(e).includes('[pretrust:conflict]')) {
        pinConflict.value = r; // 挂起等待用户显式确认，不静默覆盖
        return;
      }
      throw e;
    }
    pinConflict.value = null;
    // 幂等：同 host:port 的既有网关资产带 id 更新（免重复堆资产；凭据随 upsert 覆盖）
    const existing = store.assets.find(
      a => a.tags?.includes('gateway') && a.host === r.asset_host && a.port === r.asset_port
    );
    await store.saveAsset(
      {
        id: existing?.id,
        name: `网关 ${r.asset_host}:${r.asset_port}`,
        host: r.asset_host,
        port: r.asset_port,
        username: 'gateway',
        auth_method: 'PrivateKey',
        group: existing?.group ?? '未分组',
        tags: ['gateway'],
        jump_host: null,
        private_key_path: null
      },
      { privateKey: r.client_private_key }
    );
    done.value = r;
    store.announce('本机网关配置已写入（资产 + known_hosts 预写）', { level: 'success' });
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="stack" data-gateway-secret>
    <label class="stack"><span>relay 服务器地址</span>
      <AppInput v-model="host" placeholder="例如 117.72.25.115" data-gateway-field="secret-host" />
    </label>

    <details class="advanced-fold">
      <summary>高级：端口（默认 22222 / 2222；安全组受限的中转架构按实际填）</summary>
      <div class="port-row" style="margin-top: var(--space-2)">
        <label class="stack"><span>SSH 控制口（relay 默认 22222）</span>
          <AppInput v-model.number="vpsPort" type="number" data-gateway-field="secret-vps-port" />
        </label>
        <label class="stack"><span>挂载口（relay 默认 2222）</span>
          <AppInput v-model.number="mountPort" type="number" data-gateway-field="secret-mount-port" />
        </label>
      </div>
    </details>

    <label class="stack"><span>口令（relay 与跳板机共用同一口令，请经受控渠道发给客户）</span>
      <span class="secret-row">
        <AppInput v-model="secret" :type="showSecret ? 'text' : 'password'" data-gateway-field="secret-input" />
        <button type="button" class="secret-toggle" :title="showSecret ? '隐藏口令' : '显示口令'"
          :aria-label="showSecret ? '隐藏口令' : '显示口令'" @click="showSecret = !showSecret">
          {{ showSecret ? '隐藏' : '显示' }}
        </button>
        <button type="button" class="secret-toggle" title="生成 20 位强随机口令" @click="generateSecret">生成</button>
        <button type="button" class="secret-toggle" title="复制口令" @click="copyText(secret, '口令')">复制</button>
      </span>
    </label>

    <p v-if="error" class="form-error" style="white-space:pre-wrap">{{ error }}</p>

    <!-- 冲突确认：错误上下文就在眼前，内联呈现（非全局弹窗）——显式点击才替换 -->
    <div v-if="pinConflict" class="conflict-box">
      <p class="form-error">
        「{{ pinConflict.known_host_port }}」的 known_hosts 已有不同公钥（可能是 bundle 时代的旧网关身份）。
        替换后旧网关的连接将失配；如确认切换到新口令网关，请点击下方按钮。
      </p>
      <div class="wizard-actions">
        <AppButton variant="danger" :disabled="busy" @click="writeLocal(true)">确认替换为新网关身份</AppButton>
        <AppButton :disabled="busy" @click="pinConflict = null">取消</AppButton>
      </div>
    </div>

    <div class="wizard-actions">
      <AppButton :disabled="busy" @click="copyDockerCommand">① 复制 relay 部署命令</AppButton>
      <AppButton variant="primary" :disabled="busy || !!pinConflict" @click="writeLocal(false)">
        {{ busy ? '正在写入…' : '② 写入本机配置' }}
      </AppButton>
    </div>

    <div v-if="done" class="done-box">
      <p>
        本机配置完成：网关资产「{{ done.asset_host }}:{{ done.asset_port }}」已就绪，known_hosts 已预写。
        新建/编辑内网资产时，「跳板机」字段填
        <code>{{ done.asset_host }}:{{ done.asset_port }}</code>
        即可经网关直达客户内网。
      </p>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

/* 面板骨架类（port-row / secret-row+secret-toggle / advanced-fold / wizard-actions）
   对齐同弹窗 GatewayImportPanel 的既有视觉规格——同容器内两 tab 的控件形态必须
   一致；.stack 走全局 utility（_utilities.scss），不本地遮蔽。 */

.port-row {
  display: flex;
  gap: var(--space-3);

  > label {
    flex: 1;
  }
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

.wizard-actions {
  display: flex;
  gap: var(--space-2);
  justify-content: flex-end;
  margin-top: var(--space-3);
}

.conflict-box {
  padding: var(--space-2);
  border: 1px solid var(--danger);
  border-radius: var(--radius-sm);
  background: var(--app-control);
}

.done-box {
  padding: var(--space-2);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--app-muted);

  code {
    font-family: var(--font-mono);
    color: var(--app-strong);
  }
}
</style>
