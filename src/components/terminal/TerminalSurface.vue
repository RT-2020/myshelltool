<script setup>
/**
 * TerminalSurface — Wave 3 Step 3.3
 * Center-top container composing all 8 terminal sub-components (Tabby-style).
 * Store-bound (sessions/ui/assets Pinia) — no prop drilling.
 *
 * Migrated from App.vue: terminal-only keyboard shortcuts, search opts,
 * overlay state (cheatsheet / command palette / dangerous paste), paste
 * guard, tab context-menu helpers.
 */
import { computed, onBeforeUnmount, onMounted, reactive, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { AlertTriangle, RotateCw } from 'lucide-vue-next';
import { useSessionsStore } from '@/stores/sessions.js';
import { useUiStore } from '@/stores/ui.js';
import { useAssetsStore } from '@/stores/assets.js';
import { useClipboard } from '@/composables/useClipboard.js';
import { isTauriRuntime } from '@/services/backend.js';
import TerminalTabs from './TerminalTabs.vue';
import TerminalToolbar from './TerminalToolbar.vue';
import TerminalSearchBar from './TerminalSearchBar.vue';
import TerminalPane from './TerminalPane.vue';
import ShortcutCheatsheet from './ShortcutCheatsheet.vue';
import CommandPalette from './CommandPalette.vue';
import DangerousPasteConfirm from './DangerousPasteConfirm.vue';
import { AppContextMenu } from '@/components/ui/index.js';

// ============================================================
// Stores (store-bound wiring — no prop drilling)
// ============================================================
const sessionsStore = useSessionsStore();
const uiStore = useUiStore();
const assetsStore = useAssetsStore();
const { sessions, activeSession, activeSessionId, terminalFontSize, terminalAsideOpen, terminalSearch } =
  storeToRefs(sessionsStore);
const { activeTab } = storeToRefs(uiStore);
const { selectedAsset } = storeToRefs(assetsStore);

// ============================================================
// Local UI state (terminal-only overlays)
// 危险粘贴确认已迁到 sessions store（dangerousPastePrompt，单一入口）；
// 搜索选项/计数为 per-session（session.searchOpts / searchMatch）。
// ============================================================
const shortcutCheatsheetOpen = ref(false);
const commandPaletteOpen = ref(false);
const { copy: clipboardCopy } = useClipboard();

const terminalSubtitle = computed(() => {
  if (activeSession.value) return `${activeSession.value.asset.username}@${activeSession.value.asset.host}`;
  if (selectedAsset.value) return `${selectedAsset.value.username}@${selectedAsset.value.host}`;
  return '点击左侧主机连接';
});

const isTauriCore = computed(() => isTauriRuntime());

// ============================================================
// Keyboard shortcuts — migrated from App.vue handleTerminalKeydown
// ============================================================
function handleKeydown(event) {
  // Esc: dismiss any open overlay (works regardless of active tab)
  if (event.key === 'Escape') {
    if (terminalSearch.value.open) { sessionsStore.closeTerminalSearchInline(); event.preventDefault(); return; }
    if (shortcutCheatsheetOpen.value) { shortcutCheatsheetOpen.value = false; event.preventDefault(); return; }
    if (commandPaletteOpen.value) { commandPaletteOpen.value = false; event.preventDefault(); return; }
    if (sessionsStore.dangerousPastePrompt.open) { sessionsStore.cancelDangerousPaste(); event.preventDefault(); return; }
  }

  // Only intercept terminal shortcuts when terminal tab is active AND not typing
  const isTextInput = ['INPUT', 'TEXTAREA', 'SELECT'].includes(event.target?.tagName);
  if (activeTab.value !== 'terminal' || isTextInput) return;

  const mod = event.ctrlKey || event.metaKey;
  const shift = event.shiftKey;

  // Ctrl+Shift+F: inline search / Ctrl+Shift+P: command palette
  if (mod && shift && (event.key === 'F' || event.key === 'f')) { event.preventDefault(); sessionsStore.openTerminalSearchInline(); return; }
  if (mod && shift && (event.key === 'P' || event.key === 'p')) { event.preventDefault(); commandPaletteOpen.value = true; return; }
  // Ctrl+Shift+C: copy
  if (mod && shift && (event.key === 'C' || event.key === 'c')) { event.preventDefault(); sessionsStore.runTerminalAction('copy'); return; }
  // Ctrl+Shift+V: paste（经 store 统一危险粘贴守卫）
  if (mod && shift && (event.key === 'V' || event.key === 'v')) { event.preventDefault(); sessionsStore.runTerminalAction('paste'); return; }
  // Alt+Enter: fullscreen
  if (event.altKey && event.key === 'Enter') { event.preventDefault(); sessionsStore.runTerminalAction('fullscreen'); return; }
  // Ctrl+= / Ctrl++: font inc / Ctrl+-: font dec / Ctrl+0: font reset
  if (mod && (event.key === '=' || event.key === '+')) { event.preventDefault(); sessionsStore.runTerminalAction('font-inc'); return; }
  if (mod && event.key === '-') { event.preventDefault(); sessionsStore.runTerminalAction('font-dec'); return; }
  if (mod && event.key === '0') { event.preventDefault(); sessionsStore.runTerminalAction('font-reset'); return; }
  // Ctrl+Tab / Ctrl+Shift+Tab: session switch
  if (mod && event.key === 'Tab') {
    event.preventDefault();
    const list = sessions.value;
    if (!list.length) return;
    const idx = list.findIndex(s => s.sessionId === activeSessionId.value);
    const dir = shift ? -1 : 1;
    const next = list[(idx + dir + list.length) % list.length];
    sessionsStore.setActiveSession(next.sessionId);
    return;
  }
  // Ctrl+W: close active session
  if (mod && !shift && (event.key === 'w' || event.key === 'W')) {
    if (activeSession.value) { event.preventDefault(); sessionsStore.disconnectSession(activeSession.value.sessionId); }
    return;
  }
  // Ctrl+Shift+T: connect selected asset
  if (mod && shift && (event.key === 'T' || event.key === 't')) { event.preventDefault(); sessionsStore.connectSelected(); return; }
  // `?`: cheatsheet
  if (event.key === '?' && !mod && !event.altKey) { event.preventDefault(); shortcutCheatsheetOpen.value = true; return; }
}

onMounted(() => window.addEventListener('keydown', handleKeydown));
onBeforeUnmount(() => window.removeEventListener('keydown', handleKeydown));

// ============================================================
// Paste guard — 统一入口在 sessions store（requestDangerousPaste /
// approveDangerousPaste / cancelDangerousPaste + dangerousPastePrompt），
// 命令面板/右键/工具栏/Ctrl+Shift+V 全部经 runTerminalAction('paste') 转发。
// ============================================================
function handlePasteWithGuard() {
  sessionsStore.runTerminalAction('paste');
}

// ============================================================
// Terminal pane context menu (右键复制/粘贴) — emit 来自 TerminalPane
// 复制走 copyTerminalSelection；粘贴经 runTerminalAction('paste') 统一守卫。
// ============================================================
const terminalMenu = reactive({ open: false, x: 0, y: 0, hasSelection: false });

function handleTerminalContextMenu({ x, y, hasSelection }) {
  terminalMenu.x = x;
  terminalMenu.y = y;
  terminalMenu.hasSelection = hasSelection;
  terminalMenu.open = true;
}

const terminalMenuItems = computed(() => [
  { label: '复制选中', action: () => sessionsStore.runTerminalAction('copy'), disabled: !terminalMenu.hasSelection },
  { label: '粘贴', action: () => sessionsStore.runTerminalAction('paste') },
  { separator: true },
  { label: '复制主机地址', action: () => { if (activeSession.value) handleCopyHost(activeSession.value.sessionId); } }
]);

// ============================================================
// Tab context-menu actions — migrated from App.vue
// ============================================================
function handleTabSelect(id) {
  sessionsStore.setActiveSession(id);
  uiStore.setTab('terminal');
}

function handleCloseOthers(keepSessionId) {
  sessions.value
    .filter(s => s.sessionId !== keepSessionId)
    .forEach(s => sessionsStore.disconnectSession(s.sessionId));
}

function handleCloseRight(fromSessionId) {
  const list = sessions.value;
  const idx = list.findIndex(s => s.sessionId === fromSessionId);
  if (idx < 0) return;
  for (let i = list.length - 1; i > idx; i--) sessionsStore.disconnectSession(list[i].sessionId);
}

async function handleCopyHost(sessionId) {
  const s = sessions.value.find(x => x.sessionId === sessionId);
  if (!s) return;
  const text = `${s.asset?.username || ''}@${s.asset?.host || ''}`;
  await clipboardCopy(text);
  uiStore.statusMessage = '已复制主机地址：' + text;
}

function handleCommandPaletteAction(action) {
  if (action === 'cheatsheet') { shortcutCheatsheetOpen.value = true; return; }
  sessionsStore.runTerminalAction(action);
}

function handleTerminalPaneConnect() { sessionsStore.connectSelected(); }

function openAssetEditor() { uiStore.modal = { type: 'assetEditor', asset: null }; }
</script>

<template>
  <div class="region-terminal" :class="{ 'aside-open': terminalAsideOpen }">
    <!-- Row 1: session tab strip（term-tabs 38px）-->
    <TerminalTabs
      :sessions="sessions"
      :active-session-id="activeSession?.sessionId || ''"
      @select="handleTabSelect"
      @close="sessionsStore.disconnectSession"
      @close-others="handleCloseOthers"
      @close-right="handleCloseRight"
      @copy-host="handleCopyHost"
      @new-terminal="sessionsStore.connectSelected"
    />

    <!-- Row 2: toolbar + conditional search bar（term-toolbar-row 44px）-->
    <TerminalToolbar
      :session="activeSession"
      :font-size="terminalFontSize"
      :subtitle="terminalSubtitle"
      :aside-open="terminalAsideOpen"
      :selected-asset="selectedAsset"
      :reconnect-attempt="activeSession?.reconnectAttempt || 0"
      :reconnect-total="activeSession?.reconnectTotal || 0"
      @search="sessionsStore.openTerminalSearchInline()"
      @copy="sessionsStore.runTerminalAction('copy')"
      @paste="handlePasteWithGuard"
      @font-inc="sessionsStore.runTerminalAction('font-inc')"
      @font-dec="sessionsStore.runTerminalAction('font-dec')"
      @clear="sessionsStore.runTerminalAction('clear')"
      @reconnect="sessionsStore.runTerminalAction('reconnect')"
      @cancel-connect="activeSession && sessionsStore.cancelConnect(activeSession.sessionId)"
      @toggle-aside="sessionsStore.toggleTerminalAside()"
      @fullscreen="sessionsStore.runTerminalAction('fullscreen')"
    />
    <TerminalSearchBar
      :open="terminalSearch.open"
      :query="terminalSearch.query"
      :result="terminalSearch.result"
      :match-index="activeSession?.searchMatch?.index || 0"
      :match-total="activeSession?.searchMatch?.total || 0"
      :case-sensitive="activeSession?.searchOpts?.caseSensitive || false"
      :regex="activeSession?.searchOpts?.regex || false"
      :whole-word="activeSession?.searchOpts?.wholeWord || false"
      @update:query="sessionsStore.setTerminalSearchQuery"
      @update:case-sensitive="(v) => activeSession && sessionsStore.setTerminalSearchOpts(activeSession.sessionId, { caseSensitive: v })"
      @update:regex="(v) => activeSession && sessionsStore.setTerminalSearchOpts(activeSession.sessionId, { regex: v })"
      @update:whole-word="(v) => activeSession && sessionsStore.setTerminalSearchOpts(activeSession.sessionId, { wholeWord: v })"
      @next="sessionsStore.findTerminalNext('next')"
      @prev="sessionsStore.findTerminalNext('prev')"
      @close="sessionsStore.closeTerminalSearchInline()"
    />

    <!-- Row 3: term-canvas-wrap（终端 canvas 区，含错误横幅 + watermark + xterm pane + ready 光标）-->
    <div class="term-canvas-wrap">
      <!-- 连接失败错误卡片：active session status==='error' 时显示在 xterm 上方 -->
      <div v-if="activeSession && activeSession.status === 'error'" class="term-error-banner" role="alert">
        <AlertTriangle :size="16" class="term-error-icon" aria-hidden="true" />
        <span class="term-error-text">{{ activeSession.connectError || '连接失败' }}</span>
        <button class="term-error-btn" type="button" @click="sessionsStore.reconnectSession(activeSession.sessionId)"><RotateCw :size="13" /> 重试</button>
        <button class="term-error-btn" type="button" @click="uiStore.modal = { type: 'assetEditor', asset: activeSession.asset }">编辑连接</button>
        <button class="term-error-btn" type="button" @click="sessionsStore.dismissSessionError(activeSession.sessionId)">关闭</button>
      </div>

      <!-- 网格水印背景（app.css term-watermark，始终深色，不滚动）-->
      <div class="term-watermark" aria-hidden="true"></div>

      <!-- xterm pane（TerminalPane 内部渲染 xterm canvas + 空状态）-->
      <TerminalPane
        :store="sessionsStore"
        :has-active-session="!!activeSession"
        :is-tauri-core="isTauriCore"
        :selected-asset="selectedAsset"
        @connect-selected="handleTerminalPaneConnect"
        @open-asset-editor="openAssetEditor"
        @context-menu="handleTerminalContextMenu"
      />

      <!-- ready 光标（app.css term-cursor，仅无活跃会话时显示，提示终端就绪）-->
      <div v-if="!activeSession" class="term-cursor" aria-hidden="true">
        <span>ready</span>
        <span class="ready-bar"></span>
      </div>
    </div>

    <AppContextMenu
      :open="terminalMenu.open"
      :items="terminalMenuItems"
      :x="terminalMenu.x"
      :y="terminalMenu.y"
      @close="terminalMenu.open = false"
    />

    <!-- Overlays (teleported to body by children) -->
    <ShortcutCheatsheet :open="shortcutCheatsheetOpen" @close="shortcutCheatsheetOpen = false" />
    <CommandPalette
      :open="commandPaletteOpen"
      :sessions="sessions"
      :active-session-id="activeSession?.sessionId || ''"
      @close="commandPaletteOpen = false"
      @select-session="(id) => handleTabSelect(id)"
      @run-action="handleCommandPaletteAction"
    />
    <DangerousPasteConfirm
      :open="sessionsStore.dangerousPastePrompt.open"
      :command="sessionsStore.dangerousPastePrompt.command"
      :matched-pattern="sessionsStore.dangerousPastePrompt.matchedPattern"
      @confirm="sessionsStore.approveDangerousPaste"
      @cancel="sessionsStore.cancelDangerousPaste"
    />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// ============================================================
// region-terminal（app.css L529-536 严格同步）
// grid 38px(term-tabs) 44px(term-toolbar-row) 1fr(term-canvas-wrap)
// ============================================================
.region-terminal {
  display: grid;
  grid-template-rows: 38px 44px 1fr;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--app-panel);
  color: var(--app-text);
  font-family: var(--font-body);
  overflow: hidden;
}

// Row 1: tab strip（由 TerminalTabs 子组件渲染，固定 38px 高）
.region-terminal :deep(.terminal-tabs) {
  flex: 0 0 auto;
}

// Row 2: toolbar + search bar（由 TerminalToolbar/TerminalSearchBar 渲染，固定 44px）
.region-terminal :deep(.terminal-toolbar),
.region-terminal :deep(.terminal-searchbar) {
  flex: 0 0 auto;
}

// ============================================================
// term-canvas-wrap（app.css L619-623）：终端 canvas 区
// ============================================================
.term-canvas-wrap {
  position: relative;
  background: var(--term-bg);
  overflow: hidden;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

// TerminalPane 子组件渲染 xterm canvas，填满 canvas-wrap
.region-terminal :deep(.terminal-pane-host) {
  flex: 1 1 auto;
  min-height: 0;
  position: relative;
  z-index: 1;
}

// ============================================================
// term-error-banner：连接失败错误卡片（status==='error' 时显示在 xterm 上方）
// danger token + 悬浮在 canvas 区顶部，不遮挡输入区主体
// ============================================================
.term-error-banner {
  position: absolute;
  top: 8px;
  left: 8px;
  right: 8px;
  z-index: 3;
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  background: color-mix(in oklab, var(--danger) 14%, var(--app-panel));
  border: 1px solid color-mix(in oklab, var(--danger) 55%, var(--app-border));
  border-radius: var(--radius-sm);
  box-shadow: var(--elev-raised);
  color: var(--app-text);
  font-size: var(--text-sm);
  min-height: 34px;
}

.term-error-icon { color: var(--danger); flex: 0 0 auto; }

.term-error-text {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: var(--font-mono);
  color: var(--danger);
}

.term-error-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 auto;
  padding: 4px 10px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  color: var(--app-text);
  cursor: pointer;
  font-size: var(--text-xs);
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard);
}
.term-error-btn:hover {
  background: var(--app-hover);
  border-color: var(--danger);
  color: var(--danger);
}

// ============================================================
// term-watermark（app.css L625-632）：网格水印背景
// 始终深色，固定父容器，不滚动，pointer-events:none
// ============================================================
.term-watermark {
  position: absolute;
  inset: 0;
  background-image:
    linear-gradient(rgba(255, 255, 255, 0.025) 1px, transparent 1px),
    linear-gradient(90deg, rgba(255, 255, 255, 0.025) 1px, transparent 1px);
  background-size: 24px 24px;
  pointer-events: none;
  z-index: 0;
}

// ============================================================
// term-cursor（app.css L668-685）：右下角 ready 光标
// 仅无活跃会话时显示（提示终端就绪可输入）
// ============================================================
.term-cursor {
  position: absolute;
  right: 24px;
  bottom: 20px;
  display: flex;
  align-items: center;
  gap: 8px;
  font-family: var(--font-mono);
  font-size: 11px;
  color: var(--term-muted);
  pointer-events: none;
  z-index: 2;
}
.ready-bar {
  width: 8px;
  height: 16px;
  background: rgba(255, 255, 255, 0.75);
  animation: term-blink 1.1s steps(1) infinite;
}
@keyframes term-blink {
  0%, 50% { opacity: 1; }
  51%, 100% { opacity: 0; }
}

// reduced-motion 加固（app.css L940-948）
@media (prefers-reduced-motion: reduce) {
  .ready-bar { animation: none; opacity: 1; }
}
</style>
