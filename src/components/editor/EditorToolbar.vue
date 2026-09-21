<script setup lang="ts">
/**
 * EditorToolbar — 编辑器工具栏（v0.18）。
 * 保存链路按钮直连 editor store；跳转行经 emit 交给持有 CodeMirrorHost ref
 * 的 EditorSurface。只读/保存中禁用并给出原因（title）。
 */
import { computed, ref } from 'vue';
import {
  Save, FilePlus, FileClock, RefreshCw, Upload, FolderOpen, Globe,
  Braces, Minimize2, ArrowDownToLine, History, ScanSearch
} from 'lucide-vue-next';
import { useEditorStore } from '@/stores/editor';
import { useWorkbenchStore } from '@/stores/workbench';
import type { EditorTab } from '@/types/editor';
import { languageIdForName } from '@/lib/editor/editorLanguages';

const props = defineProps<{
  tab: EditorTab;
  saving: boolean;
}>();

const emit = defineEmits<{
  (e: 'jump', line: number): void;
}>();

const store = useEditorStore();
const workbench = useWorkbenchStore();
const jumpInput = ref('');
const jumpOpen = ref(false);

const language = computed(() => languageIdForName(props.tab.name));
const isJson = computed(() => language.value === 'json');
/** 校验支持的类型（json/yaml/toml/xml；ini/properties 无可靠校验器）。 */
const validatable = computed(() => ['json', 'yaml', 'toml', 'xml'].includes(language.value));

async function runValidate() {
  const { validateForLanguage } = await import('@/lib/editor/editorValidation');
  const issue = await validateForLanguage(language.value, store.currentContent(props.tab.id));
  workbench.announce(issue.ok ? '语法检查通过' : issue.message, { level: issue.ok ? 'success' : 'warn' });
}

function runJsonTransform(kind: 'format' | 'minify') {
  const content = store.currentContent(props.tab.id);
  import('@/lib/editor/editorValidation').then(({ validateJsonText, formatJsonText, minifyJsonText }) => {
    const issue = validateJsonText(content);
    if (!issue.ok) {
      workbench.announce(issue.message, { level: 'warn' });
      return;
    }
    store.replaceContent(props.tab.id, kind === 'format' ? formatJsonText(content) : minifyJsonText(content));
  });
}

function submitJump() {
  const line = Number(jumpInput.value);
  jumpOpen.value = false;
  if (Number.isFinite(line) && line > 0) emit('jump', line);
}
</script>

<template>
  <div class="editor-toolbar" data-region="editor-toolbar">
    <div class="group">
      <button type="button" class="tool-btn" title="输入本地文件路径打开（支持 ~ 家目录）"
        @click="store.openFromPathPrompt('local')">
        <FolderOpen :size="14" /><span>打开本地…</span>
      </button>
      <button type="button" class="tool-btn" title="输入远端文件路径打开（当前选中资产的会话）"
        @click="store.openFromPathPrompt('remote')">
        <Globe :size="14" /><span>打开远端…</span>
      </button>
    </div>
    <div class="group">
      <button type="button" class="tool-btn" :disabled="saving || tab.readOnly || tab.status !== 'ready'"
        :title="tab.readOnly ? (tab.readOnlyReason || '只读') : '保存到原位置（Ctrl+S）'"
        @click="store.saveTab(tab.id)">
        <Save :size="14" /><span>保存</span>
      </button>
      <button type="button" class="tool-btn" :disabled="tab.status !== 'ready'" title="另存为（远端输入路径 / 本地系统对话框）"
        @click="store.openSaveAsDialog(tab)">
        <FilePlus :size="14" /><span>另存为</span>
      </button>
      <button type="button" class="tool-btn" :disabled="tab.status !== 'ready'" title="保存草稿到本机（不写目标文件，重开时可恢复）"
        @click="store.saveDraftAction(tab.id)">
        <FileClock :size="14" /><span>草稿</span>
      </button>
      <button type="button" class="tool-btn" :disabled="tab.status !== 'ready'" title="丢弃本地修改，重新加载文件"
        @click="store.reloadTab(tab.id)">
        <RefreshCw :size="14" /><span>重载</span>
      </button>
      <button v-if="tab.target.kind === 'local'" type="button" class="tool-btn" :disabled="tab.status !== 'ready'"
        title="把当前内容写入远端路径（当前选中资产的会话）"
        @click="store.uploadToRemote(tab.id)">
        <Upload :size="14" /><span>上传到远端</span>
      </button>
      <button type="button" class="tool-btn" title="查看保存前自动备份（最近 3 份）"
        @click="store.listBackupsAction(tab.id)">
        <History :size="14" /><span>备份</span>
      </button>
    </div>

    <div class="group">
      <div v-if="jumpOpen" class="jump-box">
        <input v-model="jumpInput" class="jump-input" type="text" placeholder="行号" autofocus
          @keyup.enter="submitJump" @keyup.esc="jumpOpen = false" @blur="jumpOpen = false" />
        <button type="button" class="tool-btn" @mousedown.prevent="submitJump">跳转</button>
      </div>
      <button v-else type="button" class="tool-btn" title="跳转到行（编辑器内也可 Ctrl+G）" @click="jumpOpen = true">
        <ArrowDownToLine :size="14" /><span>跳行</span>
      </button>
      <button v-if="validatable" type="button" class="tool-btn" title="语法校验（json/yaml/toml/xml）" @click="runValidate">
        <ScanSearch :size="14" /><span>校验</span>
      </button>
      <template v-if="isJson">
        <button type="button" class="tool-btn" title="格式化 JSON（2 空格缩进）" @click="runJsonTransform('format')">
          <Braces :size="14" /><span>格式化</span>
        </button>
        <button type="button" class="tool-btn" title="压缩 JSON（去除空白）" @click="runJsonTransform('minify')">
          <Minimize2 :size="14" /><span>压缩</span>
        </button>
      </template>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.editor-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
  border-bottom: 1px solid var(--app-border);
  background: var(--app-panel);
  flex-wrap: wrap;
}

.group {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-wrap: wrap;
}

.tool-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px var(--space-2);
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--app-muted);
  font-size: var(--text-xs);
  cursor: pointer;

  &:hover:not(:disabled) {
    background: var(--app-hover);
    color: var(--app-text);
  }

  &:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
}

.jump-box {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.jump-input {
  width: 72px;
  padding: 2px 6px;
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
}
</style>
