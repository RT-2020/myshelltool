<script setup>
import { ref, onMounted, onBeforeUnmount } from 'vue';
import { TerminalSquare, PlugZap } from 'lucide-vue-next';

const props = defineProps({
  store: { type: Object, required: true },
  hasActiveSession: { type: Boolean, default: false },
  isTauriCore: { type: Boolean, default: false },
  selectedAsset: { type: Object, default: null }
});
const emit = defineEmits(['connect-selected', 'open-asset-editor', 'context-menu']);

const mountRef = ref(null);

function setContainer() {
  if (mountRef.value && props.store?.setTerminalContainer) {
    props.store.setTerminalContainer(mountRef.value);
  }
}

onMounted(setContainer);
onBeforeUnmount(() => {
  // 不在 unmount 时清空 store 的容器引用 —— 父组件可能切换 tab 时短暂卸载再加载
});

function onWheel(e) {
  if (!e.ctrlKey) return;
  e.preventDefault();
  if (e.deltaY < 0) {
    props.store?.runTerminalAction?.('font-inc');
  } else if (e.deltaY > 0) {
    props.store?.runTerminalAction?.('font-dec');
  }
}

// 右键弹复制粘贴菜单。仅在有活跃会话时触发（无会话时空状态卡片接管交互）。
// 菜单的 action 逻辑（复制/粘贴/危险守卫）在父 TerminalSurface 持有，靠 emit 桥接，
// 保持 TerminalPane 纯展示职责（DOM 宿主与逻辑宿主分离）。
function onContextMenu(event) {
  if (!props.hasActiveSession) return;
  const hasSelection = Boolean(props.store?.activeSession?.term?.getSelection());
  emit('context-menu', { x: event.clientX, y: event.clientY, hasSelection });
}
</script>

<template>
  <div class="terminal-pane-host" @wheel="onWheel" @contextmenu.prevent="onContextMenu" :class="{ 'has-session': hasActiveSession }">
    <!--
      #terminalContainer 是 xterm 的挂载容器，也是 FitAddon 的测量目标。
      它必须保持尺寸稳定：不要在里面放会消失的兄弟元素（否则连接时空状态卡片
      被移除会让容器内容高度突变 → 位置跳动）。空状态卡片改用绝对定位覆盖在上面。
      容器本身不滚动（overflow:hidden）—— 滚动交给 xterm 的 .xterm-viewport 处理，
      否则两层滚动条互相打架，会触发 ResizeObserver→fit() 反馈循环（输入时 UI 乱跳）。
    -->
    <div id="terminalContainer" ref="mountRef" aria-label="终端区域"></div>
    <div v-if="!hasActiveSession" class="terminal-empty">
      <div class="terminal-empty-card">
        <div class="terminal-empty-icon">
          <TerminalSquare :size="42" />
        </div>
        <h3>{{ isTauriCore ? '打开 SSH 终端' : 'SSH 终端需要桌面客户端' }}</h3>
        <p class="muted">{{ isTauriCore ? '选择左侧主机后点击「连接」，或输入快速连接命令。' : '当前为浏览器预览模式，请运行 npm run tauri:dev 启动桌面运行时。' }}</p>
        <div v-if="isTauriCore" class="terminal-empty-actions">
          <button v-if="selectedAsset" class="btn primary" @click="emit('connect-selected')">
            <PlugZap :size="14" /> 连接 {{ selectedAsset.name }}
          </button>
          <button class="btn" @click="emit('open-asset-editor')">新增连接</button>
          <span class="muted" v-if="!selectedAsset">尚未选择资产</span>
        </div>
        <dl v-if="isTauriCore" class="terminal-empty-help">
          <dt>快捷键</dt>
          <dd>Ctrl+Shift+T 连接 · Ctrl+W 关闭 · Ctrl+Tab 切换</dd>
          <dt>复制粘贴</dt>
          <dd>Ctrl+Shift+C / Ctrl+Shift+V</dd>
          <dt>字体</dt>
          <dd>Ctrl+= / Ctrl+- / Ctrl+0 / Ctrl+滚轮</dd>
          <dt>命令面板</dt>
          <dd>Ctrl+Shift+P · ? 查看完整快捷键</dd>
        </dl>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.terminal-pane-host {
  height: 100%;
  position: relative;
  overflow: hidden;
}

// Pierce scoped styles into xterm.js imperative DOM (WebGL canvas + viewport).
// Without :deep(), Vue's scoped attribute doesn't reach xterm's own DOM nodes,
// and any style we want to apply to the terminal scrollbars / helper UI would be
// silently dropped. Also documents the WebGL constraint in one place.
:deep(.xterm) {
  height: 100%;
  background: var(--terminal-bg);

  .xterm-viewport {
    background: var(--terminal-bg);

    &::-webkit-scrollbar {
      width: 8px;
    }

    &::-webkit-scrollbar-thumb {
      background: var(--app-subtle);
      border-radius: var(--radius-sm);
    }
  }

  .xterm-screen {
    background: var(--terminal-bg);
  }
}

// CRITICAL: do NOT apply transform / filter / opacity on #terminalContainer
// — the WebGL addon canvas breaks under compositor effects.
//
// 容器只负责定位，不负责排版字体 —— 字号/行高全部交给 xterm 自己管。
// 严禁在这里加 padding 或 overflow:auto：
//   - padding 会偏移 xterm 的测量原点（FitAddon 测 content-box，结果位置错）
//   - overflow:auto 会在内容增长时出现/消失 8px 滚动条，宽度反复 ±8px，
//     与未防抖的 ResizeObserver 形成 fit()→resize→重绘→fit() 反馈循环
//     （输入字符 UI 乱跳的根因）。滚动交给 xterm 的 .xterm-viewport。
#terminalContainer {
  height: 100%;
  background: var(--terminal-bg);
  overflow: hidden;
}

// 空状态卡片改为绝对定位覆盖在 #terminalContainer 之上，
// 这样连接时卡片消失不会改变 fit 容器的内容尺寸。
.terminal-empty {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;   // 容器背景层不挡 xterm；交互由卡片自己接收
}

.terminal-empty-card {
  pointer-events: auto;   // 卡片本身恢复交互（按钮可点）
  width: 100%;
  max-width: 540px;
  margin: var(--space-5);
}

// Tabby-style: single thin card, single border, no shadow stacking.
.terminal-empty-card {
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  padding: var(--space-5);
  text-align: center;
}

.terminal-empty-icon {
  color: var(--app-muted);
  margin-block-end: var(--space-3);
  display: flex;
  justify-content: center;
}

.terminal-empty-card h3 {
  margin: 0 0 var(--space-2);
  font-size: var(--text-lg);
  color: var(--app-strong);
}

.terminal-empty-card .muted {
  font-size: var(--text-sm);
  margin: 0 0 var(--space-3);
}

.terminal-empty-actions {
  display: flex;
  gap: var(--space-2);
  justify-content: center;
  flex-wrap: wrap;
  margin-block-end: var(--space-4);
}

.terminal-empty-actions .btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.terminal-empty-help {
  margin: var(--space-4) 0 0;
  text-align: start;
  display: grid;
  grid-template-columns: auto 1fr;
  gap: 6px var(--space-3);
  padding-block-start: var(--space-3);
  border-block-start: 1px solid var(--app-border);
}

.terminal-empty-help dt {
  font-size: var(--text-xs);
  color: var(--app-muted);
  text-transform: uppercase;
  letter-spacing: 0.08em;
}

.terminal-empty-help dd {
  margin: 0;
  font-size: var(--text-xs);
  font-family: var(--font-mono);
  color: var(--app-strong);
}
</style>
