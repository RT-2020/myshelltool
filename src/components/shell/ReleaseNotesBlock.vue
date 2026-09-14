<script setup lang="ts">
/**
 * ReleaseNotesBlock — 更新日志渲染块（设置面板「关于与更新」tab，available 态）。
 *
 * gen-changelog.mjs 输出的轻量行级渲染（不引入 markdown 库，项目无此依赖）：
 *   '### ✨ 新功能' → 小标题；'- **scope**: 描述' → 列表项；'_区间: …_' → 去斜体
 *   包裹的元信息段；空行跳过；其余为普通段落。行内 **bold** 按 ** 分段加粗。
 *
 * 自 SettingsPanelContent.vue 拆出（该文件超 500 行 SFC 硬上限，AGENTS.md 质量
 * 红线）。notes 为空时的 GitHub Releases 链接兜底留在父组件（openExternal 在
 * 父组件），父组件保证 notes 非空才渲染本组件。
 */
import { computed } from 'vue';

const props = defineProps<{
  // latest.json notes 原文（useAutoUpdate.releaseNotes，gen-changelog 产物）
  notes: string;
}>();

interface NoteSegment { text: string; bold: boolean }
interface NoteLine { kind: 'heading' | 'item' | 'text'; segments: NoteSegment[] }

function parseNoteSegments(raw: string): NoteSegment[] {
  // split 后奇数下标段落在 ** 包裹内（gen-changelog 的 **scope** 写法），
  // 空段保留以维持奇偶判定（渲染为空 span 无害）。
  return raw.split('**').map((part, index) => ({ text: part, bold: index % 2 === 1 }));
}

const releaseNoteLines = computed<NoteLine[]>(() => {
  const lines: NoteLine[] = [];
  props.notes.split('\n').forEach((raw, index) => {
    const line = raw.trim();
    if (!line) return;
    const heading = line.match(/^#{1,6}\s+(.*)$/);
    if (heading) {
      const text = heading[1].trim();
      // 首行「## vX.Y.Z」与区块标题「vX 更新内容」重复，跳过
      if (index === 0 && /^v[\d.]+$/.test(text)) return;
      lines.push({ kind: 'heading', segments: [{ text, bold: false }] });
      return;
    }
    const item = line.match(/^[-*]\s+(.*)$/);
    if (item) {
      lines.push({ kind: 'item', segments: parseNoteSegments(item[1].trim()) });
      return;
    }
    // '_区间: A → B_' 元信息行：去掉首尾斜体包裹后按普通段落渲染
    const italic = line.match(/^_(.+)_$/);
    lines.push({ kind: 'text', segments: parseNoteSegments(italic ? italic[1] : line) });
  });
  return lines;
});
</script>

<template>
  <div class="release-notes-body">
    <template v-for="(line, i) in releaseNoteLines" :key="i">
      <div v-if="line.kind === 'heading'" class="rn-heading">
        <span v-for="(seg, j) in line.segments" :key="j" :class="{ 'rn-bold': seg.bold }">{{ seg.text }}</span>
      </div>
      <div v-else-if="line.kind === 'item'" class="rn-item">
        <span v-for="(seg, j) in line.segments" :key="j" :class="{ 'rn-bold': seg.bold }">{{ seg.text }}</span>
      </div>
      <p v-else class="rn-text">
        <span v-for="(seg, j) in line.segments" :key="j" :class="{ 'rn-bold': seg.bold }">{{ seg.text }}</span>
      </p>
    </template>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 内层限高滚动：弹窗 body（.app-modal-body）已是滚动容器，限高避免双层长滚
.release-notes-body {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 220px;
  overflow-y: auto;
  padding: 10px 12px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  font-size: 12px;
  line-height: 1.6;
}
.rn-heading {
  margin-top: 6px;
  font-weight: 600;
  color: var(--app-strong);

  &:first-child {
    margin-top: 0;
  }
}
.rn-item {
  position: relative;
  padding-left: 14px;

  &::before {
    content: '·';
    position: absolute;
    left: 4px;
    color: var(--app-muted);
  }
}
.rn-text {
  margin: 0;
  font-size: 11px;
  color: var(--app-muted);
}
.rn-bold {
  font-weight: 600;
}
</style>
