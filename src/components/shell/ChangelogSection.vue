<script setup lang="ts">
/**
 * ChangelogSection — 设置面板「关于与更新」tab 的更新日志区块（常驻入口）。
 *
 * 数据源是构建时生成的 src/generated/changelog.json（npm run build 首步跑
 * scripts/gen-changelog-history.mjs，遍历全部 v* tag 产出全历史），随安装包
 * 离线分发——与 UpdateSection 的 available 态展示互补：那边只在「发现新版本」
 * 窗口期渲染 latest.json notes，用户已是最新版本后靠本区块回看任意版本
 * 更新了什么。
 *
 * 渲染复用 ReleaseNotesBlock（gen-changelog 产物的行级渲染器）；待安装新版的
 * notes 由 UpdateSection 紧贴「下载并安装」按钮展示，此处不重复。
 */
import { ref, watch } from 'vue';
import { ChevronRight, ExternalLink, History } from 'lucide-vue-next';
import changelogData from '@/generated/changelog.json';
import { openExternal } from '@/lib/openExternal';
import ReleaseNotesBlock from '@/components/shell/ReleaseNotesBlock.vue';

const props = defineProps<{
  // 当前应用版本号（SettingsPanelContent 异步获取后传入，初始为 —）；
  // 用于「当前」标记与默认展开条目，匹配不到时回退展开最新一条
  appVersion: string;
}>();

interface ChangelogEntry { version: string; tag: string; date: string; notes: string }

// changelog.json 是构建产物（resolveJsonModule 推断宽类型），此处窄化一次
const entries = ((changelogData as { entries?: ChangelogEntry[] }).entries ?? []);

const RELEASES_URL = 'https://github.com/RT-2020/myshelltool/releases';

// 展开状态按版本号记录（多开允许——回看历史时对比两个版本是合理场景）。
// 默认展开当前版本；appVersion 异步到达前先空着，watch 一次性初始化。
const expanded = ref<string[]>([]);
let initialized = false;

watch(
  () => props.appVersion,
  ver => {
    if (initialized) return;
    initialized = true;
    const current = entries.find(e => e.version === ver);
    expanded.value = [current ? current.version : entries[0]?.version ?? ''].filter(Boolean);
  },
  { immediate: true }
);

// 兜底：appVersion 获取失败一直是 —（watch 已触发但没匹配到）时上面的回退已
// 覆盖（entries[0]）；entries 为空则保持空，模板走 GitHub 链接兜底。

function toggle(version: string) {
  expanded.value = expanded.value.includes(version)
    ? expanded.value.filter(v => v !== version)
    : [...expanded.value, version];
}
</script>

<template>
  <section class="block">
    <header class="block-head"><History :size="12" />更新日志</header>

    <template v-if="entries.length">
      <p class="muted cl-hint">当前版本与历史版本的更新内容（共 {{ entries.length }} 个版本，随安装包离线提供）</p>
      <div class="cl-list">
        <div v-for="entry in entries" :key="entry.tag" class="cl-entry">
          <button type="button" class="cl-row" @click="toggle(entry.version)">
            <ChevronRight :size="12" class="cl-chevron" :class="{ 'is-open': expanded.includes(entry.version) }" />
            <span class="cl-version num">v{{ entry.version }}</span>
            <span v-if="entry.date" class="cl-date num">{{ entry.date }}</span>
            <span v-if="entry.version === appVersion" class="cl-current">当前</span>
          </button>
          <ReleaseNotesBlock v-show="expanded.includes(entry.version)" :notes="entry.notes" />
        </div>
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

// 内层限高滚动：弹窗 body（.app-modal-body）已是滚动容器，19 个版本的折叠头
// 也会撑高弹窗，限高后历史翻阅在本区内完成。
.cl-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 320px;
  overflow-y: auto;
}

.cl-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  color: var(--app-strong);
  cursor: pointer;
  font: inherit;
  font-size: 12px;
  text-align: left;
  transition: background var(--motion-fast) var(--ease-standard);

  &:hover {
    background: var(--app-hover);
  }
}
.cl-chevron {
  flex-shrink: 0;
  color: var(--app-muted);
  transition: transform var(--motion-fast) var(--ease-standard);

  &.is-open {
    transform: rotate(90deg);
  }
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
