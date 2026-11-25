<script setup lang="ts">
/**
 * EditorSurface — 内置文本/配置文件编辑器弹窗（v0.18 覆盖面板，v0.20.2 起弹窗形态）。
 *
 * 挂载于主窗口 / 资产窗口的中央区之上（遮罩 --app-scrim + --z-modal，居中窗体）。
 * 由 stores/editor.ts 驱动：多文件 tab + 工具栏 + 编辑区/Markdown 分屏 +
 * 状态栏（行列/编码/EOL/只读原因）+ 打开失败分类面板（含按 GBK 重试）。
 * 拖拽：接收文件面板的内部 MIME 拖入（本地条目 → 逐个打开）。
 * 关闭语义：遮罩空白点击与 Esc 均不关闭（防误关丢未保存内容，见模板内注释）；
 * 关闭走 tab 关闭按钮（dirty 先三选）与 tab 栏收起按钮。
 *
 * 本组件经 defineAsyncComponent 懒加载（CodeMirror 及语言包不进主 bundle）。
 */
import { computed, ref, watch } from 'vue';
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
// 05 A1：预览通道绑定 tabId，切换 tab 即时重置，空串 "" 不回退到旧快照
const previewState = ref<{ tabId: string; text: string } | null>(null);
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

watch(
  () => [store.activeTabId, tab.value?.contentVersion] as const,
  ([id]) => {
    const current = id ? store.tabs.find(t => t.id === id) : null;
    previewState.value =
      current && languageIdForName(current.name) === 'markdown'
        ? { tabId: current.id, text: store.currentContent(current.id) }
        : null;
  },
  { immediate: true }
);

const markdownPreviewContent = computed(() => {
  if (!tab.value) return '';
  if (previewState.value && previewState.value.tabId === tab.value.id) {
    return previewState.value.text;
  }
  return store.currentContent(tab.value.id);
});

function onJump(line: number) {
  if (line > 0) editorHost.value?.gotoLine(line);
  else editorHost.value?.focus();
}

function onCursor(pos: { line: number; column: number; lines: number }) {
  cursor.value = pos;
}

function onHostChange(tabId: string, text: string) {
  if (tabId === store.activeTabId && isMarkdown.value && previewMode.value !== 'off') {
    previewState.value = { tabId, text };
  }
}

/** 打开失败的可读文案（按 kind 分类；05 A7 补齐四要素下一步指引）。 */
function errorText(t: EditorTab): string {
  const e = t.error;
  if (!e) return '';
  switch (e.kind) {
    case 'too-large': return '文件超过编辑器 2 MiB 上限，无法按文本打开。可在文件面板下载后用本地工具编辑。';
    case 'binary': return '内容是二进制文件，无法按文本打开。请到文件面板下载后用对应工具打开。';
    case 'not-utf8': return '文件不是 UTF-8 文本。可能是 GBK 等本地编码——可按 GBK 重试，或在下方切换其他编码。';
    case 'encoding': return '当前选择的编码无法解码这份内容（换一个编码再试）。';
    case 'lossy': return '文件名含无法识别的字符（非 UTF-8 文件名失真），无法安全寻址。可从文件面板检查或重命名。';
    case 'dir': return '目标路径是目录，不是文件。请返回文件面板选择文件。';
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
  <Transition name="editor-win">
  <div
    v-if="store.surfaceVisible"
    class="editor-surface"
    :class="{ 'drag-over': dragOver }"
    data-region="editor-surface"
    @dragover="onDragOver"
    @dragleave="onDragLeave"
    @drop="onDrop"
  >
    <!-- 弹窗形态（v0.20.2）：遮罩 + 居中窗，遮罩用 var(--app-scrim)（与 GlobalModals
         同源）。产品决策——遮罩空白点击与 Esc 均【不】关闭编辑器（防误关丢未保存
         内容）：关闭只走 tab 关闭按钮（dirty 先过三选弹窗）与 tab 栏收起按钮
         （隐藏面板不丢 tab）。不要给本组件接 useModalDismiss 的 closeModal 语义。 -->
    <div class="editor-window">
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
          <AlertTriangle :size="20" aria-hidden="true" />
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
              <RotateCw :size="13" aria-hidden="true" />重试
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
            <MarkdownPreviewPane :content="markdownPreviewContent" />
          </div>
        </div>
      </div>

      <!-- 状态栏 -->
      <div class="editor-statusbar">
        <span class="status-item" :title="tab.target.path">{{ tab.target.path }}</span>
        <span
          v-if="tab.readOnly"
          class="status-item readonly"
          :title="tab.readOnlyReason || ''"
          :aria-label="`只读${tab.readOnlyReason ? '：' + tab.readOnlyReason : ''}`"
        >
          <FileWarning :size="12" aria-hidden="true" />只读
        </span>
        <span class="status-item num">{{ cursor ? `行 ${cursor.line}, 列 ${cursor.column}` : '行 -, 列 -' }}</span>
        <span class="status-item num">共 {{ cursor?.lines ?? '-' }} 行</span>
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
            :aria-label="previewMode === 'off' ? '打开分屏预览' : '关闭预览'"
            :title="previewMode === 'off' ? '打开分屏预览' : '关闭预览'"
            @click="previewMode = previewMode === 'off' ? 'split' : 'off'"
          >
            <component :is="previewMode === 'off' ? PanelLeftOpen : PanelLeftClose" :size="13" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="tool-btn"
            :aria-label="previewMode === 'preview' ? '回到编辑' : '仅看预览'"
            :title="previewMode === 'preview' ? '回到编辑' : '仅看预览'"
            @click="previewMode = previewMode === 'preview' ? 'split' : 'preview'"
          >
            <Eye :size="13" aria-hidden="true" />
          </button>
        </span>
      </div>
      </template>
    </div>
  </div>
  </Transition>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 遮罩层：与 GlobalModals 的 .modal-layer 同源（--app-scrim + --z-modal）。
// 不挂遮罩点击关闭、不接 Esc——见模板内产品决策注释。
.editor-surface {
  position: absolute;
  inset: 0;
  z-index: var(--z-modal);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-6);
  background: var(--app-scrim);

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

.editor-win-enter-active {
  transition: opacity var(--dur-base) var(--ease-standard);
}
.editor-win-enter-active .editor-window {
  transition:
    transform var(--dur-slow) var(--ease-emphasized),
    opacity var(--dur-base) var(--ease-standard);
}
.editor-win-leave-active {
  transition: opacity var(--dur-fast) var(--ease-standard);
  pointer-events: none;
}
.editor-win-leave-active .editor-window {
  transition:
    transform var(--dur-fast) var(--ease-standard),
    opacity var(--dur-fast) var(--ease-standard);
}
.editor-win-enter-from,
.editor-win-leave-to {
  opacity: 0;
}
.editor-win-enter-from .editor-window,
.editor-win-leave-to .editor-window {
  transform: scale(0.985) translateY(4px);
  opacity: 0;
}

// 弹窗窗体：编辑器本体（多 tab + 工具栏 + 主区 + 状态栏）。
.editor-window {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  max-width: 1200px;
  background: var(--app-bg);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
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
  gap: var(--space-1);
  padding: var(--space-1) var(--space-3);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  color: var(--app-text);
  font-size: var(--text-xs);
  cursor: pointer;

  &:hover { background: var(--app-hover); }
  &:active { background: var(--app-active); }

  &.primary {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-on);

    &:hover {
      border-color: var(--accent-hover);
      background: var(--accent-hover);
    }
    &:active {
      border-color: var(--accent-active);
      background: var(--accent-active);
    }
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
  min-height: 28px;
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
  gap: var(--space-1);
  flex: 0 0 auto;

  &:first-child {
    min-width: 0;
    max-width: 340px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  &.num {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }

  &.readonly { color: var(--warn); }
}

.select-wrap {
  gap: var(--space-1);
}

.mini-select {
  padding: 0 2px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-bg);
  color: var(--app-text);
  font-size: var(--text-xs);
  outline: none;

  &:focus {
    border-color: var(--accent);
    box-shadow: var(--focus-ring);
  }
  &:active { background: var(--app-hover); }
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
  &:active {
    background: var(--app-active);
  }
}
</style>
