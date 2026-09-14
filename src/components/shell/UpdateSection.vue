<script setup lang="ts">
/**
 * UpdateSection — 设置面板「关于与更新」tab 的应用更新区块。
 *
 * 单一主操作（检查更新 → 下载安装），状态机文案与进度条复用注入的
 * useAutoUpdate 实例（App.vue 创建，与状态栏 pill 同一状态源）。
 * available 态内嵌更新日志（ReleaseNotesBlock），旧链路无 notes 时给
 * GitHub Releases 链接兜底。
 *
 * 自 SettingsPanelContent.vue 拆出（该文件超 500 行 SFC 硬上限，
 * AGENTS.md 质量红线）。
 */
import { computed, unref } from 'vue';
import { Download, ExternalLink, RefreshCw } from 'lucide-vue-next';
import type { useAutoUpdate } from '@/composables/useAutoUpdate';
import { isTauriRuntime } from '@/services/backend';
import AppButton from '@/components/ui/AppButton.vue';
import AppProgress from '@/components/ui/AppProgress.vue';
import ReleaseNotesBlock from '@/components/shell/ReleaseNotesBlock.vue';

const props = defineProps<{
  // App.vue 的 useAutoUpdate 实例（经 settings modal payload 注入父组件再透传；
  // 必填——父组件已用 v-if 保证仅在注入时渲染本区块，浏览器预览整体隐藏）
  autoUpdate: ReturnType<typeof useAutoUpdate>;
  // 当前应用版本号（「已是最新版本」文案用；父组件异步获取后传入，初始为 —）
  appVersion: string;
}>();

async function openExternal(url: string) {
  if (!isTauriRuntime()) {
    window.open(url, '_blank', 'noopener');
    return;
  }
  try {
    const { openUrl } = await import('@tauri-apps/plugin-opener');
    await openUrl(url);
  } catch (e) {
    console.warn('[settings] opener failed, fallback to window.open:', e);
    window.open(url, '_blank', 'noopener');
  }
}

// —— 更新按钮状态机 ——
// 复用注入的 autoUpdate（来自 App.vue，与状态栏点击同一实例）。
const updateState = computed(() => unref(props.autoUpdate.state) || 'idle');
const newVersion = computed(() => unref(props.autoUpdate.newVersion) || '');
const errorMessage = computed(() => unref(props.autoUpdate.errorMessage) || '');
const downloadProgress = computed(() => unref(props.autoUpdate.downloadProgress) ?? 0);
const releaseNotes = computed(() => unref(props.autoUpdate.releaseNotes) || '');
// 下载进度：useAutoUpdate 把进度写进 statusMessage（文字流），这里展示进度条。
const isBusy = computed(() => updateState.value === 'checking' || updateState.value === 'downloading');

// 主按钮文案随状态机变化（单一主操作，ui-ux-pro-max §4）。
const updateBtnLabel = computed(() => {
  switch (updateState.value) {
    case 'checking': return '检查中…';
    case 'downloading': return downloadProgress.value > 0 ? `下载中 ${downloadProgress.value}%` : '下载中…';
    case 'available': return `下载并安装 v${newVersion.value}`;
    case 'error': return '重试';
    case 'up_to_date': return '重新检查';
    default: return '检查更新';
  }
});

function onUpdateClick() {
  if (updateState.value === 'available') {
    props.autoUpdate.onClick(); // 已发现新版 → 下载安装
  } else {
    props.autoUpdate.check(); // idle / error / checking → (重新)检查
  }
}
</script>

<template>
  <section class="block">
    <header class="block-head"><Download :size="12" />应用更新</header>
    <div class="update-row">
      <AppButton
        variant="primary"
        size="sm"
        :disabled="isBusy"
        @click="onUpdateClick"
      >
        <RefreshCw v-if="isBusy" :size="12" class="spin" />
        {{ updateBtnLabel }}
      </AppButton>
      <span
        class="muted update-hint"
        :class="{
          'is-error': updateState === 'error',
          'is-success': updateState === 'available' || updateState === 'up_to_date'
        }"
      >
        <template v-if="updateState === 'available'">发现新版本 v{{ newVersion }}，点击下载并安装，完成后自动重启</template>
        <template v-else-if="updateState === 'downloading'">正在下载更新安装包（{{ downloadProgress }}%），请稍候…</template>
        <template v-else-if="updateState === 'error'">检查或更新失败：{{ errorMessage || '网络连接失败或超时' }}</template>
        <template v-else-if="updateState === 'up_to_date'">当前已是最新版本 (v{{ appVersion }})</template>
        <template v-else>检查 GitHub releases 是否有新版本</template>
      </span>
    </div>
    <!-- 更新日志：available 态展示 latest.json notes；旧链路无 notes 时给 GitHub 链接兜底。
         行级渲染拆至 ReleaseNotesBlock.vue（500 行 SFC 硬上限约束）。 -->
    <div v-if="updateState === 'available'" class="release-notes">
      <div class="release-notes-head">v{{ newVersion }} 更新内容</div>
      <ReleaseNotesBlock v-if="releaseNotes" :notes="releaseNotes" />
      <div v-else class="muted release-notes-fallback">
        本次发布未附详细更新说明
        <button
          type="button"
          class="link-action"
          @click="openExternal('https://github.com/RT-2020/myshelltool/releases')"
        >
          在 GitHub 查看
          <ExternalLink :size="12" />
        </button>
      </div>
    </div>
    <AppProgress v-if="updateState === 'downloading'" :value="downloadProgress > 0 ? downloadProgress : null" />
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 与 SettingsPanelContent / McpPanelContent 同视觉语言的 section 外框（scoped 不跨组件）
.block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md, 8px);
  background: var(--bg-elevated, var(--app-surface));
}
.block-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-secondary, var(--app-muted));
}

.update-row {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.update-hint {
  font-size: 12px;
  line-height: 1.5;

  &.is-error {
    color: var(--danger);
  }

  &.is-success {
    color: var(--success);
  }
}

.release-notes {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.release-notes-head {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-secondary, var(--app-muted));
}
// 行级渲染与限高滚动容器样式随模板拆至 ReleaseNotesBlock.vue（scoped 不跨组件）
.release-notes-fallback {
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.link-action {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  background: none;
  border: none;
  padding: 0;
  color: var(--accent);
  cursor: pointer;
  font: inherit;

  &:hover {
    color: var(--accent-hover);
    text-decoration: underline;
  }
}

.spin {
  animation: spin 1s linear infinite;
}
@keyframes spin {
  to { transform: rotate(360deg); }
}

// .muted 由全局 _utilities 提供，此处不重复定义。
</style>
