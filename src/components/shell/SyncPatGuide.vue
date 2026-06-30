<script setup>
/**
 * SyncPatGuide — v1.6 PAT（GitHub Token）获取引导子组件。
 *
 * 在 SyncPanelContent 检测到「未配置 PAT」时渲染（阻断视图）。
 * 抽成独立组件因 SyncPanelContent 重写后会超 500 行 SFC 硬上限（AGENTS.md 红线），
 * 参照 McpPanelContent 拆 McpCapabilityList 的先例。
 *
 * 单一职责：引导用户完成「申请 Token → 勾选 gist scope → 粘贴到本地」3 步。
 * opener 插件直达 github.com/settings/tokens（外链），并一键切到 tokenConfig 模态。
 *
 * 视觉语言照搬父组件的 block 范式，零新增 token（AGENTS.md 红线：用 var(--token)）。
 */
import { KeyRound, ShieldCheck, ExternalLink, Lock } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench.js';
import { isTauriRuntime } from '@/services/backend.js';
import AppButton from '@/components/ui/AppButton.vue';

const store = useWorkbenchStore();

// GitHub Token 申请页（opener 直达，Tauri runtime 用插件，浏览器走 window.open）
const TOKEN_URL = 'https://github.com/settings/tokens';
// scope 说明常量（避免模板里散落 magic string）
const REQUIRED_SCOPE = 'gist';

// ─── 外链打开（照 useClipboard 的「动态 import + runtime 检测」范式）───
// 非 Tauri runtime（浏览器预览模式）走 window.open；Tauri 走 opener 插件。
// 动态 import 避免 npm run dev 下静态 import Tauri 插件即崩。
async function openExternal(url) {
  if (!isTauriRuntime()) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch (e) {
    // opener 失败兜底（权限缺失等）—— 至少不让用户卡死
    // eslint-disable-next-line no-console
    console.warn('[sync] opener failed, fallback to window.open:', e);
    window.open(url, '_blank', 'noopener');
  }
}

// 切到 PAT 配置模态（用现成的 tokenConfig type，无需新 action）
function goToTokenConfig() {
  store.modal = { type: 'tokenConfig' };
}
</script>

<template>
  <div class="pat-guide">
    <!-- 3 步上手引导卡 -->
    <section class="block">
      <header class="block-head"><KeyRound :size="12" />3 步开启同步</header>
      <ol class="step-list">
        <li class="step-item">
          <span class="step-num">1</span>
          <div class="step-body">
            <span class="step-title">申请 GitHub Token</span>
            <span class="step-desc muted">前往 GitHub 设置页，生成 Personal Access Token (classic)。</span>
            <AppButton variant="primary" size="sm" class="step-action" @click="openExternal(TOKEN_URL)">
              <ExternalLink :size="12" />打开 GitHub Token 设置
            </AppButton>
          </div>
        </li>
        <li class="step-item">
          <span class="step-num">2</span>
          <div class="step-body">
            <span class="step-title">只勾选 <code class="scope-tag">{{ REQUIRED_SCOPE }}</code> 权限</span>
            <span class="step-desc muted">同步仅需读写 Gist 的权限，无需其他。生成后立即复制 token（页面关闭后不可再查）。</span>
          </div>
        </li>
        <li class="step-item">
          <span class="step-num">3</span>
          <div class="step-body">
            <span class="step-title">粘贴到本地</span>
            <span class="step-desc muted">Token 仅存本地安全存储，不进资产 JSON、不进日志。</span>
            <AppButton variant="subtle" size="sm" class="step-action" @click="goToTokenConfig">
              <KeyRound :size="12" />配置 PAT
            </AppButton>
          </div>
        </li>
      </ol>
    </section>

    <!-- 加密安全说明卡：建立信任，降低用户对「把资产传 GitHub」的顾虑 -->
    <section class="block security-card">
      <header class="block-head"><Lock :size="12" />加密保障</header>
      <ul class="security-list">
        <li><ShieldCheck :size="12" class="icon-ok" />资产用主密码 + <strong>AES-256-GCM</strong> 加密后上传，Gist 上是密文</li>
        <li><ShieldCheck :size="12" class="icon-ok" />创建的是<strong>私有 Gist</strong>，仅你自己可见</li>
        <li><ShieldCheck :size="12" class="icon-ok" />主密码<strong>不存储</strong>，请务必记住——忘了只能清空重来</li>
      </ul>
    </section>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.pat-guide {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}

// ─── block 通用（照父组件 SyncPanelContent / McpPanelContent 范式）───
.block {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
}
.block-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--app-muted);
  padding-bottom: 4px;
  border-bottom: 1px solid var(--app-border);
}
.block-head :deep(svg) { flex-shrink: 0; }

// ─── 3 步引导卡 ───
.step-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
}
.step-item {
  display: flex;
  gap: var(--space-3);
}
.step-num {
  flex-shrink: 0;
  width: 22px;
  height: 22px;
  border-radius: var(--radius-pill);
  background: var(--accent);
  color: var(--accent-on);
  font-size: var(--text-xs);
  font-weight: 600;
  display: inline-flex;
  align-items: center;
  justify-content: center;
}
.step-body {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.step-title {
  font-size: var(--text-sm);
  color: var(--app-strong);
}
.step-desc {
  font-size: var(--text-xs);
  line-height: 1.5;
}
.step-action {
  align-self: flex-start;
  margin-top: 2px;
}
.scope-tag {
  font-family: var(--font-mono);
  font-size: 11px;
  background: var(--app-hover);
  padding: 1px 6px;
  border-radius: var(--radius-sm);
  color: var(--app-strong);
}

// ─── 加密安全说明卡 ───
.security-card .block-head { color: var(--success); }
.security-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.security-list li {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  font-size: var(--text-xs);
  color: var(--app-text);
  line-height: 1.5;
}
.security-list strong { color: var(--app-strong); }
.icon-ok { color: var(--success); flex-shrink: 0; margin-top: 2px; }

.muted { color: var(--app-muted); font-size: var(--text-xs); line-height: 1.5; margin: 0; }
.muted strong { color: var(--app-strong); }
</style>
