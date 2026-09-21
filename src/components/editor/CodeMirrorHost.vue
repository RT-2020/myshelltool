<script setup lang="ts">
/**
 * CodeMirrorHost — CodeMirror 6 实例封装（v0.18 内置编辑器的编辑区）。
 *
 * 职责边界：实例归本组件持有（正文不进响应式，见 stores/editor.ts 模块注释），
 * 经 store 的 registerContentProvider/contentVersion 与 store 同步；语言包
 * dynamic import；主题两套（浅/深）用设计 token 书写，watch theme 热切换；
 * 查找替换（Mod-F / Mod-Shift-F）与跳行（Mod-G）来自 @codemirror/search 内建。
 */
import { onBeforeUnmount, onMounted, ref, shallowRef, watch } from 'vue';
import { Compartment, EditorState } from '@codemirror/state';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  dropCursor,
  rectangularSelection,
  crosshairCursor
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { bracketMatching, foldGutter, foldKeymap, indentOnInput, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import { tags as t } from '@lezer/highlight';
import { useEditorStore } from '@/stores/editor';
import { loadLanguageExtension, type EditorLanguageId } from '@/lib/editor/editorLanguages';

const props = defineProps<{
  tabId: string;
  language: EditorLanguageId;
  theme: 'light' | 'dark';
  readOnly: boolean;
  /** store 内容版本号（reload/恢复草稿后 +1）。 */
  contentVersion: number;
}>();

const emit = defineEmits<{
  (e: 'cursor', pos: { line: number; column: number; lines: number }): void;
  (e: 'save'): void;
  (e: 'change', text: string): void;
}>();

const store = useEditorStore();
const hostEl = ref<HTMLElement | null>(null);
const view = shallowRef<EditorView | null>(null);

const themeComp = new Compartment();
const langComp = new Compartment();
const readOnlyComp = new Compartment();

let programmaticEdit = false;
let changeDebounceTimer: ReturnType<typeof setTimeout> | null = null;

/** 主题两套共用一份 token 化样式（{dark} 决定 CM 内部的深浅色基准）。 */
function themeExtension(dark: boolean): Extension {
  return [
    EditorView.theme(
      {
        '&': { color: 'var(--app-text)', backgroundColor: 'var(--app-panel)', height: '100%', fontSize: '13px' },
        '.cm-scroller': {
          fontFamily: 'var(--font-mono, monospace)',
          lineHeight: '1.6'
        },
        '.cm-content': { caretColor: 'var(--accent)' },
        '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--accent)', borderLeftWidth: '2px' },
        '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': {
          backgroundColor: 'color-mix(in oklab, var(--accent) 30%, transparent)'
        },
        '.cm-gutters': {
          backgroundColor: 'var(--app-panel)',
          color: 'var(--app-muted)',
          border: 'none',
          borderRight: '1px solid var(--app-border)'
        },
        '.cm-activeLine': { backgroundColor: 'color-mix(in oklab, var(--accent) 8%, transparent)' },
        '.cm-activeLineGutter': {
          backgroundColor: 'color-mix(in oklab, var(--accent) 12%, transparent)',
          color: 'var(--app-strong)'
        },
        '.cm-selectionMatch': { backgroundColor: 'color-mix(in oklab, var(--accent) 25%, transparent)' },
        '.cm-panels': { backgroundColor: 'var(--app-panel)', color: 'var(--app-text)', borderColor: 'var(--app-border)' },
        '.cm-panel input, .cm-panel button': { color: 'var(--app-text)' },
        '.cm-searchMatch': { backgroundColor: 'color-mix(in oklab, var(--warn) 35%, transparent)' },
        '.cm-searchMatch-selected': { backgroundColor: 'color-mix(in oklab, var(--danger) 35%, transparent)' },
        '.cm-foldPlaceholder': { backgroundColor: 'var(--app-hover)', border: 'none', color: 'var(--app-muted)' },
        '.cm-tooltip': { backgroundColor: 'var(--app-panel)', border: '1px solid var(--app-border)', color: 'var(--app-text)' }
      },
      { dark }
    ),
    syntaxHighlighting(
      HighlightStyle.define([
        { tag: t.comment, color: 'var(--app-muted)', fontStyle: 'italic' },
        { tag: [t.keyword, t.moduleKeyword, t.controlKeyword], color: 'var(--accent)' },
        { tag: [t.string, t.special(t.string)], color: 'var(--success)' },
        { tag: [t.number, t.bool, t.null], color: 'var(--warn)' },
        { tag: [t.function(t.variableName), t.labelName], color: 'var(--accent)' },
        { tag: [t.typeName, t.className, t.tagName], color: 'var(--success)' },
        { tag: [t.heading], fontWeight: 'bold', color: 'var(--app-strong)' },
        { tag: [t.link, t.url], color: 'var(--accent)', textDecoration: 'underline' },
        { tag: [t.emphasis], fontStyle: 'italic' },
        { tag: [t.strong], fontWeight: 'bold' },
        { tag: [t.meta, t.documentMeta], color: 'var(--app-muted)' },
        { tag: [t.invalid], color: 'var(--danger)' }
      ])
    )
  ];
}

function reportCursor(v: EditorView) {
  const head = v.state.selection.main.head;
  const line = v.state.doc.lineAt(head);
  emit('cursor', { line: line.number, column: head - line.from + 1, lines: v.state.doc.lines });
}

function emitDebouncedChange(v: EditorView) {
  // 300ms 去抖把全文快照交给上层（Markdown 分屏预览用；纯 UI 去抖）
  if (changeDebounceTimer) clearTimeout(changeDebounceTimer);
  changeDebounceTimer = setTimeout(() => {
    changeDebounceTimer = null;
    if (view.value) emit('change', view.value.state.doc.toString());
  }, 300);
}

onMounted(async () => {
  if (!hostEl.value) return;
  const initial = store.currentContent(props.tabId);
  const v = new EditorView({
    state: EditorState.create({
      doc: initial,
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        history(),
        foldGutter(),
        drawSelection(),
        dropCursor(),
        rectangularSelection(),
        crosshairCursor(),
        indentOnInput(),
        bracketMatching(),
        highlightSelectionMatches(),
        keymap.of([
          ...defaultKeymap,
          ...historyKeymap,
          ...searchKeymap,
          ...foldKeymap,
          indentWithTab,
          { key: 'Mod-s', preventDefault: true, run: () => { emit('save'); return true; } }
        ]),
        themeComp.of(themeExtension(props.theme === 'dark')),
        langComp.of([]),
        readOnlyComp.of([
          EditorState.readOnly.of(props.readOnly),
          EditorView.editable.of(!props.readOnly)
        ]),
        EditorView.updateListener.of(update => {
          if (update.docChanged) {
            if (!programmaticEdit) {
              store.markDirty(props.tabId, true);
              emitDebouncedChange(update.view);
            }
          }
          if (update.selectionSet || update.docChanged) reportCursor(update.view);
        }),
        EditorView.lineWrapping
      ]
    }),
    parent: hostEl.value
  });
  view.value = v;
  store.registerContentProvider(props.tabId, {
    get: () => v.state.doc.toString(),
    set: (text: string) => {
      programmaticEdit = true;
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: text } });
      programmaticEdit = false;
      reportCursor(v);
    }
  });
  reportCursor(v);
  // 语言包懒加载后注入
  const langExt = await loadLanguageExtension(props.language);
  if (langExt && view.value) {
    view.value.dispatch({ effects: langComp.reconfigure(langExt) });
  }
});

onBeforeUnmount(() => {
  if (changeDebounceTimer) clearTimeout(changeDebounceTimer);
  store.registerContentProvider(props.tabId, null);
  view.value?.destroy();
  view.value = null;
});

watch(
  () => props.contentVersion,
  () => {
    const v = view.value;
    if (!v) return;
    // 与快照（最近一次读取/替换）对比；currentContent 此时返回活动 doc，会自比较恒等
    const next = store.contentSnapshot(props.tabId);
    if (v.state.doc.toString() !== next) {
      programmaticEdit = true;
      v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: next } });
      programmaticEdit = false;
      reportCursor(v);
    }
  }
);

watch(
  () => props.theme,
  next => {
    view.value?.dispatch({ effects: themeComp.reconfigure(themeExtension(next === 'dark')) });
  }
);

watch(
  () => props.readOnly,
  next => {
    view.value?.dispatch({
      effects: readOnlyComp.reconfigure([EditorState.readOnly.of(next), EditorView.editable.of(!next)])
    });
  }
);

watch(
  () => props.language,
  async next => {
    const langExt = await loadLanguageExtension(next);
    if (view.value) view.value.dispatch({ effects: langComp.reconfigure(langExt ?? []) });
  }
);

function focus() {
  view.value?.focus();
}

/** 跳转到 1 基行号（钳制到文档范围，滚动居中并聚焦）。 */
function gotoLine(line: number) {
  const v = view.value;
  if (!v) return;
  const clamped = Math.max(1, Math.min(line, v.state.doc.lines));
  const target = v.state.doc.line(clamped);
  v.dispatch({
    selection: { anchor: target.from },
    effects: EditorView.scrollIntoView(target.from, { y: 'center' })
  });
  v.focus();
}

defineExpose({ focus, gotoLine });
</script>

<template>
  <div ref="hostEl" class="cm-host" data-region="editor-code"></div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.cm-host {
  height: 100%;
  min-height: 0;
  overflow: hidden;

  :global(.cm-editor) {
    height: 100%;
  }

  :global(.cm-editor.cm-focused) {
    outline: none;
  }
}
</style>
