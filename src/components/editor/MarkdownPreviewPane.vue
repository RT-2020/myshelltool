<script setup lang="ts">
/**
 * MarkdownPreviewPane — Markdown 预览（v0.18）。
 *
 * 安全红线：渲染前必经 DOMPurify 消毒（内容是远端/本地文件，视为不可信
 * 输入——不执行其中的脚本/事件）；链接拦截走 openExternal（系统浏览器），
 * 不在应用内导航。debounce 由调用方（CodeMirrorHost 的 change 事件）完成。
 */
import { computed, ref, watch } from 'vue';
import { openExternal } from '@/lib/openExternal';

const props = defineProps<{
  content: string;
}>();

const renderedHtml = ref('');

const sanitized = computed(() => props.content);

async function render() {
  const [{ marked }, DOMPurify] = await Promise.all([
    import('marked'),
    import('dompurify').then(m => m.default)
  ]);
  const raw = await marked.parse(sanitized.value);
  // target=_blank 交给拦截器处理；这里先消毒掉所有脚本/事件/危险协议
  renderedHtml.value = DOMPurify.sanitize(raw, {
    FORBID_TAGS: ['style', 'form', 'input', 'iframe', 'object', 'embed'],
    FORBID_ATTR: ['onerror', 'onclick', 'onload', 'style']
  });
}

watch(sanitized, () => { void render(); }, { immediate: true });

/** 应用内不导航：所有链接交给系统默认浏览器。 */
function onPreviewClick(event: MouseEvent) {
  const anchor = (event.target as HTMLElement | null)?.closest('a');
  if (!anchor) return;
  const href = anchor.getAttribute('href') || '';
  if (!href || href.startsWith('#')) return; // 锚点留给预览内滚动
  event.preventDefault();
  void openExternal(href);
}
</script>

<template>
  <div class="md-preview" data-region="editor-md-preview" @click="onPreviewClick">
    <div class="md-body" v-html="renderedHtml"></div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.md-preview {
  height: 100%;
  min-height: 0;
  overflow: auto;
  padding: var(--space-4);
  background: var(--app-bg);
}

.md-body {
  max-width: 860px;
  margin: 0 auto;
  font-size: var(--text-sm);
  line-height: 1.7;
  color: var(--app-text);

  :deep(h1), :deep(h2), :deep(h3), :deep(h4) {
    color: var(--app-strong);
    margin: 1.2em 0 0.5em;
    line-height: 1.3;
  }

  :deep(h1) { font-size: 1.5em; border-bottom: 1px solid var(--app-border); padding-bottom: 0.3em; }
  :deep(h2) { font-size: 1.3em; border-bottom: 1px solid var(--app-border); padding-bottom: 0.25em; }
  :deep(h3) { font-size: 1.15em; }
  :deep(h4) { font-size: 1em; }

  :deep(p) { margin: 0.6em 0; overflow-wrap: break-word; }

  :deep(a) {
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }

  :deep(code) {
    font-family: var(--font-mono, monospace);
    font-size: 0.9em;
    background: var(--app-hover);
    padding: 0.15em 0.35em;
    border-radius: var(--radius-sm);
  }

  :deep(pre) {
    background: var(--app-panel);
    border: 1px solid var(--app-border);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    overflow: auto;

    code {
      background: transparent;
      padding: 0;
    }
  }

  :deep(blockquote) {
    margin: 0.8em 0;
    padding: 0.2em var(--space-3);
    border-left: 3px solid var(--app-border);
    color: var(--app-muted);
  }

  :deep(ul), :deep(ol) {
    padding-left: 1.6em;
    margin: 0.5em 0;
  }

  :deep(table) {
    border-collapse: collapse;
    margin: 0.8em 0;

    th, td {
      border: 1px solid var(--app-border);
      padding: 4px var(--space-2);
    }

    th { background: var(--app-hover); color: var(--app-strong); }
  }

  :deep(img) {
    max-width: 100%;
    border-radius: var(--radius-sm);
  }

  :deep(hr) {
    border: none;
    border-top: 1px solid var(--app-border);
    margin: 1.5em 0;
  }
}
</style>
