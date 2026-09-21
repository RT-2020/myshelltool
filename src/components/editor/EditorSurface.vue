<script setup lang="ts">
/**
 * EditorSurface — 内置编辑器覆盖面板（v0.18）。
 *
 * 挂载于主窗口 / 资产窗口的中央区（终端/文件面板之上，z 用 --z-sticky）。
 * 由 stores/editor.ts 驱动：多文件 tab + 工具栏 + 编辑区/Markdown 分屏 +
 * 状态栏（行列/编码/EOL/只读原因）+ 打开失败分类面板（含按 GBK 重试）。
 * 拖拽：接收文件面板的内部 MIME 拖入（本地条目 → 逐个打开）。
 *
 * 本组件经 defineAsyncComponent 懒加载（CodeMirror 及语言包不进主 bundle）。
 */
import { computed, ref } from 'vue';
import { AlertTriangle, RotateCw, FileWarning, PanelLeftClose, PanelLeftOpen, Eye } from 'lucide-vue-next';
import { useEditorStore } from '@/stores/editor';
import { useWorkbenchStore } from '@/stores/workbench';
import { languageIdForName } from '@/lib/editor/editorLanguages';
import { ENCODING_OPTIONS, EOL_OPTIONS, eolDisplayText } from '@/lib/editor/editorCodec';
import { FILE_DRAG_MIME } from '@/lib/fileTypes';
import type { RemoteFileEntry } from '@/types/domain';
import type { EditorTab } from '@/types/editor';
import EditorTabBar from './EditorTabBar.vue';
import EditorToolbar from './EditorToolbar.vue';
import CodeMirrorHost from './CodeMirrorHost.vue';
import MarkdownPreviewPane from './MarkdownPreviewPane.vue';

const store = useEditorStore();
const workbench = useWorkbenchStore();

const editorHost = ref<InstanceType<typeof CodeMirrorHost> | null>(null);
const cursor = ref<{ line: number; column: number; lines: number } | null>(null);
const previewText = ref('');
const dragOver = ref(false);
/** Markdown 预览模式（仅 markdown 语言显示切换）。 */
const previewMode = ref<'off' | 'split' | 'preview'>('split');

const theme = computed<'light' | 'dark'>(() => (workbench.effectiveTheme === 'light' ? 'light' : 'dark'));
const tab = computed<EditorTab | null>(() => store.activeTab);
const language = computed(() => (tab.value ? languageIdForName(tab.value.name) : 'plaintext'));
const isMarkdown = computed(() => language.value === 'markdown');
const saving = computed(() => Boolean(tab.value && store.savingTabIds[tab.value.id]));
/** 实际写盘的 EOL 选择（用户未切过沿用基线推断）。 */
const eolChoice = computed(() => (tab.value ? store.tabEolChoice(tab.value) : 'lf'));
const encodingChoice = computed(() => tab.value?.encodingOverride ?? 'utf-8');

function onJump(line: number) {
  if (line > 0) editorHost.value?.gotoLine(line);
  else editorHost.value?.focus();
}

function onCursor(pos: { line: number; column: number; lines: number }) {
  cursor.value = pos;
}

function onHostChange(text: string) {
  if (isMarkdown.value && previewMode.value !== 'off') previewText.value = text;
}

/** 打开失败的可读文案（按 kind 分类）。 */
function errorText(t: EditorTab): string {
  const e = t.error;
  if (!e) return '';
  switch (e.kind) {
    case 'too-large': return '文件超过编辑器 2 MiB 上限，无法按文本打开（可下载后用本地工具编辑）。';
    case 'binary': return '内容是二进制文件，无法按文本打开。';
    case 'not-utf8': return '文件不是 UTF-8 文本。可能是 GBK 等本地编码——可按 GBK 重试，或在下方切换其他编码。';
    case 'encoding': return '当前选择的编码无法解码这份内容（换一个编码再试）。';
    case 'lossy': return '文件名含无法识别的字符（非 UTF-8 文件名失真），无法安全寻址。';
    case 'dir': return '目标路径是目录，不是文件。';
    default: return e.message;
  }
}

function retryWithGbk() {
  if (tab.value) store.setEncoding(tab.value.id, 'gbk');
}

/** 内部拖拽打开（本地面板条目）。 */
function onDragOver(event: DragEvent) {
  if (event.dataTransfer?.types?.includes(FILE_DRAG_MIME)) {
    event.preventDefault();
    dragOver.value = true;
  }
}

function onDragLeave() {
  dragOver.value = false;
}

function onDrop(event: DragEvent) {
  dragOver.value = false;
  if (!event.dataTransfer?.types?.includes(FILE_DRAG_MIME)) return;
  event.preventDefault();
  event.stopPropagation();
  try {
    const payload = JSON.parse(event.dataTransfer.getData(FILE_DRAG_MIME) || 'null') as
      | { entries?: RemoteFileEntry[] }
      | null;
    const entries = (payload?.entries || []).filter(e => e.kind === 'file');
    if (!entries.length) return;
    void store.openTarget({ kind: 'local', assetId: null, path: entries[0]!.path });
    // 多选拖入逐个打开（上限保护：一次最多开 8 个）
    for (const entry of entries.slice(1, 8)) {
      void store.openTarget({ kind: 'local', assetId: null, path: entry.path });
    }
  } catch {
    workbench.announce('拖拽打开失败：无法解析拖入的文件数据', { level: 'warn' });
  }
}

function onEncodingChange(event: Event) {
  const value = (event.target as HTMLSelectElement).value;
  if (!tab.value) return;
  store.setEncoding(tab.value.id, value === 'utf-8' ? null : value);
}

function onEolChange(event: Event) {
  const value = (event.target as HTMLSelectElement).value as 'lf' | 'crlf';
  if (tab.value) store.setEol(tab.value.id, value);
}
</script>

<template>
  <div
    v-if="store.surfaceVisible"
    class="editor-surface"
    :class="{ 'drag-over': dragOver }"
    data-region="editor-surface"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <EditorTabBar
      :tabs="store.tabs"
      :active-tab-id="store.activeTabId"
      @select="store.setActive"
      @close="store.requestCloseTab"
      @hide="store.hideSurface"
    />

    <template v-if="tab">
      <EditorToolbar :tab="tab" :saving="saving" @jump="onJump" />

      <!-- 主区：加载 / 错误 / 编辑+预览 -->
      <div class="editor-main">
        <div v-if="tab.status === 'loading'" class="state-panel">
          <span class="spin" aria-hidden="true"></span>
          <p>正在读取「{{ tab.name }}」…</p>
        </div>

        <div v-else-if="tab.status === 'error'" class="state-panel error" data-region="editor-error">
          <AlertTriangle :size="20" />
          <p class="error-message">{{ errorText(tab) }}</p>
          <p class="error-raw">{{ tab.error?.message }}</p>
          <div class="error-actions">
            <button
              v-if="tab.error?.kind === 'not-utf8'"
              type="button"
              class="btn primary"
              @click="retryWithGbk"
            >按 GBK 重试</button>
            <button
              v-if="store.errorRetryable(tab)"
              type="button"
              class="btn"
              @click="store.retryTab(tab.id)"
            >
              <RotateCw :size="13" />重试
            </button>
          </div>
        </div>

        <div v-else class="editor-split" :class="`preview-${isMarkdown ? previewMode : 'off'}`">
          <div class="editor-pane">
            <CodeMirrorHost
              :ref="el => (editorHost = el as InstanceType<typeof CodeMirrorHost> | null)"
              :key="tab.id"
              :tab-id="tab.id"
              :language="language"
              :theme="theme"
              :read-only="tab.readOnly"
              :content-version="tab.contentVersion"
              @cursor="onCursor"
              @change="onHostChange"
              @save="store.saveTab(tab.id)"
            />
          </div>
          <div v-if="isMarkdown && previewMode !== 'off'" class="preview-pane">
            <MarkdownPreviewPane :content="previewText || store.contentSnapshot(tab.id)" />
          </div>
        </div>
      </div>

      <!-- 状态栏 -->
      <div class="editor-statusbar">
        <span class="status-item" :title="tab.target.path">{{ tab.target.path }}</span>
        <span v-if="tab.readOnly" class="status-item readonly" :title="tab.readOnlyReason || ''">
          <FileWarning :size="12" />只读
        </span>
        <span class="status-item">{{ cursor ? `行 ${cursor.line}, 列 ${cursor.column}` : '行 -, 列 -' }}</span>
        <span class="status-item">共 {{ cursor?.lines ?? '-' }} 行</span>
        <label class="status-item select-wrap" title="编码（切换将按新编码重新加载原文件）">
          <span>编码</span>
          <select class="mini-select" :value="encodingChoice" @change="onEncodingChange">
            <option v-for="opt in ENCODING_OPTIONS" :key="opt.label" :value="opt.label">{{ opt.text }}</option>
          </select>
        </label>
        <label class="status-item select-wrap" title="换行符（保存时应用）">
          <span>EOL</span>
          <select class="mini-select" :value="eolChoice" @change="onEolChange">
            <option v-for="opt in EOL_OPTIONS" :key="opt.id" :value="opt.id">{{ opt.text }}</option>
          </select>
        </label>
        <span v-if="tab.baseline" class="status-item">{{ eolDisplayText(tab.baseline.eol) === '混合' ? '原文件：混合换行' : '' }}</span>
        <span v-if="isMarkdown" class="status-item preview-toggle">
          <button
            type="button"
            class="tool-btn"
            :title="previewMode === 'off' ? '打开分屏预览' : '关闭预览'"
            @click="previewMode = previewMode === 'off' ? 'split' : 'off'"
          >
            <component :is="previewMode === 'off' ? PanelLeftOpen : PanelLeftClose" :size="13" />
          </button>
          <button
            type="button"
            class="tool-btn"
            :title="previewMode === 'preview' ? '回到编辑' : '仅看预览'"
            @click="previewMode = previewMode === 'preview' ? 'split' : 'preview'"
          >
            <Eye :size="13" />
          </button>
        </span>
      </div>
    </template>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.editor-surface {
  position: absolute;
  inset: 0;
  z-index: var(--z-sticky);
  display: flex;
  flex-direction: column;
  background: var(--app-bg);
  border-top: 1px solid var(--app-border);

  &.drag-over::after {
    content: '拖入以打开文件';
    position: absolute;
    inset: var(--space-2);
    z-index: var(--z-dropdown);
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--accent);
    border-radius: var(--radius-md);
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    color: var(--accent);
    font-size: var(--text-sm);
    pointer-events: none;
  }
}

.editor-main {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.editor-split {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;

  &.preview-split .editor-pane { border-right: 1px solid var(--app-border); }
  &.preview-preview .editor-pane { display: none; }
  &.preview-off .preview-pane,
  &.preview-preview .preview-pane { flex: 1 1 100%; }
}

.editor-pane {
  flex: 1 1 50%;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.preview-pane {
  flex: 1 1 50%;
  min-width: 0;
}

.state-panel {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-3);
  padding: var(--space-6);
  color: var(--app-muted);
  font-size: var(--text-sm);

  &.error {
    color: var(--danger);

    .error-message { color: var(--app-text); max-width: 560px; text-align: center; }
    .error-raw {
      color: var(--app-muted);
      font-size: var(--text-xs);
      max-width: 560px;
      text-align: center;
      overflow-wrap: break-word;
    }
    .error-actions { display: flex; gap: var(--space-2); color: var(--app-text); }

// 错误面板按钮（modal 体系外的本地按钮样式，token 驱动）
.error-actions .btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  color: var(--app-text);
  font-size: var(--text-xs);
  cursor: pointer;

  &:hover { background: var(--app-hover); }

  &.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-contrast, #fff);
  }
}
  }
}

.spin {
  width: 18px;
  height: 18px;
  border: 2px solid var(--app-border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: editor-spin 0.8s linear infinite;
}
@keyframes editor-spin {
  to { transform: rotate(360deg); }
}

.editor-statusbar {
  display: flex;
  align-items: center;
  gap: var(--space-3);
  padding: 2px var(--space-3);
  min-height: 26px;
  border-top: 1px solid var(--app-border);
  background: var(--app-panel);
  font-size: var(--text-xs);
  color: var(--app-muted);
  overflow-x: auto;
  white-space: nowrap;
}

.status-item {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 auto;

  &:first-child {
    min-width: 0;
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  &.readonly { color: var(--warn); }
}

.select-wrap {
  gap: 4px;
}

.mini-select {
  padding: 0 2px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-bg);
  color: var(--app-text);
  font-size: var(--text-xs);
  outline: none;

  &:focus { border-color: var(--accent); }
}

.preview-toggle {
  gap: 0;
}

.tool-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;

  &:hover {
    background: var(--app-hover);
    color: var(--app-text);
  }
}
</style>
