<script setup lang="ts">
/**
 * ChangelogSection — 设置面板「关于与更新」tab 的更新日志区块（常驻入口）。
 *
 * 只渲染当前安装版本那一条，作为「本次更新更新了什么」的离线回看入口；
 * 历史版本去 GitHub Releases（区块底部链接）。数据源是构建时生成的
 * src/generated/changelog.json（npm run build 首步跑
 * scripts/gen-changelog-history.mjs 遍历全部 v* tag），随安装包离线分发，
 * 渲染不发网络请求。
 *
 * 与 UpdateSection 互补：那边只在「发现新版本」窗口期渲染在线拉的
 * latest.json notes，用户装完新版本后靠本区块离线回看该版本的内容。
 *
 * 渲染复用 ReleaseNotesBlock（gen-changelog 产物的行级渲染器，其跳过首行
 * 「## vX.Y.Z」标题的行为与外层版本行正好互补）。
 */
import { computed } from 'vue';
import { ExternalLink, History } from 'lucide-vue-next';
import changelogData from '@/generated/changelog.json';
import { openExternal } from '@/lib/openExternal';
import ReleaseNotesBlock from '@/components/shell/ReleaseNotesBlock.vue';

const props = defineProps<{
  // 当前应用版本号（SettingsPanelContent 异步获取后传入，初始为 —）
  appVersion: string;
}>();

interface ChangelogEntry { version: string; tag: string; date: string; notes: string }

// changelog.json 是构建产物（resolveJsonModule 推断宽类型），此处窄化一次。
// 产物仍全量生成（未来恢复历史列表零成本），本组件只取一条。
const entries = ((changelogData as { entries?: ChangelogEntry[] }).entries ?? []);

const RELEASES_URL = 'https://github.com/RT-2020/myshelltool/releases';

// 正式安装包由发版 CI 在 tag checkout 上重新生成产物，必然包含当前版本、
// 精确匹配；匹配不到只剩两种无害场景——appVersion 异步未到达（—），或本地
// dev 的产物落后一版（如 v0.16.0 的代码配最新只到 v0.15.3 的产物）。回退
// 最新一条，hint 文案相应显示「最新发布」而非「当前版本」，不误导。
const currentEntry = computed<ChangelogEntry | undefined>(
  () => entries.find(e => e.version === props.appVersion) ?? entries[0]
);
</script>

<template>
  <section class="block">
    <header class="block-head"><History :size="12" />更新日志</header>

    <template v-if="currentEntry">
      <p class="muted cl-hint">
        {{ currentEntry.version === appVersion ? '当前版本' : '最新发布' }}
        v{{ currentEntry.version }} 的更新内容（随安装包离线提供）
      </p>
      <div class="cl-entry">
        <div class="cl-row">
          <span class="cl-version num">v{{ currentEntry.version }}</span>
          <span v-if="currentEntry.date" class="cl-date num">{{ currentEntry.date }}</span>
          <span v-if="currentEntry.version === appVersion" class="cl-current">当前</span>
        </div>
        <ReleaseNotesBlock :notes="currentEntry.notes" />
      </div>
      <div>
        <button type="button" class="link-action" @click="openExternal(RELEASES_URL)">
          在 GitHub 查看历史版本
          <ExternalLink :size="12" />
        </button>
      </div>
    </template>

    <div v-else class="muted cl-fallback">
      本构建未打包更新日志数据
      <button type="button" class="link-action" @click="openExternal(RELEASES_URL)">
        在 GitHub 查看
        <ExternalLink :size="12" />
      </button>
    </div>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 与 UpdateSection / SettingsPanelContent 的 section 外框同一视觉语言（scoped 不跨组件）
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

.cl-hint {
  font-size: 12px;
  line-height: 1.5;
}

.cl-entry {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.cl-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 2px 6px;
  color: var(--app-strong);
  font-size: 12px;
}
.cl-version {
  font-weight: 600;
}
.cl-date {
  font-size: 11px;
  color: var(--app-muted);
}
.cl-current {
  padding: 0 6px;
  border-radius: var(--radius-sm);
  background: var(--accent-soft);
  color: var(--accent);
  font-size: 10px;
  line-height: 18px;
}

.cl-fallback {
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

// .muted / .num 由全局 _utilities / _base 提供，此处不重复定义。
</style>
