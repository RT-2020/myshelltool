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
import { useSessionsStore } from '@/stores/sessions.js';
import { useUiStore } from '@/stores/ui.js';
import { useAssetsStore } from '@/stores/assets.js';
import { useClipboard } from '@/composables/useClipboard.js';
import { detectDangerousCommand } from '@/lib/dangerousCommands.js';
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
// Local UI state (terminal-only overlays / search opts)
// ============================================================
const shortcutCheatsheetOpen = ref(false);
const commandPaletteOpen = ref(false);
const dangerousPasteState = reactive({ open: false, command: '', matchedPattern: '', pendingAction: null });
const terminalSearchOpts = reactive({ caseSensitive: false, regex: false, wholeWord: false });
const { copy: clipboardCopy, paste: clipboardPaste } = useClipboard();

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
    if (dangerousPasteState.open) { cancelDangerousPaste(); event.preventDefault(); return; }
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
  // Ctrl+Shift+V: paste (with danger guard)
  if (mod && shift && (event.key === 'V' || event.key === 'v')) { event.preventDefault(); handlePasteWithGuard(); return; }
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
// Paste guard — migrated from App.vue handlePasteWithGuard
// ============================================================
async function handlePasteWithGuard() {
  const text = await clipboardPaste();
  if (!text) { uiStore.statusMessage = '剪贴板为空或不可用'; return; }
  const danger = detectDangerousCommand(text);
  if (danger) {
    dangerousPasteState.open = true;
    dangerousPasteState.command = text;
    dangerousPasteState.matchedPattern = danger.pattern;
    dangerousPasteState.pendingAction = 'paste';
    return;
  }
  sessionsStore.writeToActiveTerminal(text);
}

function confirmDangerousPaste() {
  const text = dangerousPasteState.command;
  dangerousPasteState.open = false;
  if (text) sessionsStore.writeToActiveTerminal(text);
}

function cancelDangerousPaste() {
  dangerousPasteState.open = false;
  dangerousPasteState.command = '';
  dangerousPasteState.matchedPattern = '';
  dangerousPasteState.pendingAction = null;
}

// ============================================================
// Terminal pane context menu (右键复制/粘贴) — emit 来自 TerminalPane
// 复用 copyTerminalSelection / pasteToTerminal（经 runTerminalAction）+ 危险粘贴守卫。
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
  { label: '粘贴', action: () => handlePasteWithGuard() },
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
  <div class="terminal-surface" :class="{ 'aside-open': terminalAsideOpen }">
    <!-- Row 1: session tab strip -->
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

    <!-- Row 2: toolbar + conditional search bar -->
    <TerminalToolbar
      :session="activeSession"
      :font-size="terminalFontSize"
      :subtitle="terminalSubtitle"
      :aside-open="terminalAsideOpen"
      :selected-asset="selectedAsset"
      @search="sessionsStore.openTerminalSearchInline()"
      @copy="sessionsStore.runTerminalAction('copy')"
      @paste="handlePasteWithGuard"
      @font-inc="sessionsStore.runTerminalAction('font-inc')"
      @font-dec="sessionsStore.runTerminalAction('font-dec')"
      @clear="sessionsStore.runTerminalAction('clear')"
      @reconnect="sessionsStore.runTerminalAction('reconnect')"
      @toggle-aside="sessionsStore.toggleTerminalAside()"
      @fullscreen="sessionsStore.runTerminalAction('fullscreen')"
    />
    <TerminalSearchBar
      :open="terminalSearch.open"
      :query="terminalSearch.query"
      :result="terminalSearch.result"
      :case-sensitive="terminalSearchOpts.caseSensitive"
      :regex="terminalSearchOpts.regex"
      :whole-word="terminalSearchOpts.wholeWord"
      @update:query="sessionsStore.setTerminalSearchQuery"
      @update:case-sensitive="(v) => (terminalSearchOpts.caseSensitive = v)"
      @update:regex="(v) => (terminalSearchOpts.regex = v)"
      @update:whole-word="(v) => (terminalSearchOpts.wholeWord = v)"
      @next="sessionsStore.findTerminalNext('next')"
      @prev="sessionsStore.findTerminalNext('prev')"
      @close="sessionsStore.closeTerminalSearchInline()"
    />

    <!-- Row 3: xterm pane (flex:1, fills remaining height) -->
    <TerminalPane
      :store="sessionsStore"
      :has-active-session="!!activeSession"
      :is-tauri-core="isTauriCore"
      :selected-asset="selectedAsset"
      @connect-selected="handleTerminalPaneConnect"
      @open-asset-editor="openAssetEditor"
      @context-menu="handleTerminalContextMenu"
    />
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
      :open="dangerousPasteState.open"
      :command="dangerousPasteState.command"
      :matched-pattern="dangerousPasteState.matchedPattern"
      @confirm="confirmDangerousPaste"
      @cancel="cancelDangerousPaste"
    />
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// No nested card chrome — 无圆角无外框：与 grid region 贴合平齐，靠 region
// 间 1px 分隔线（AppShellLayout 的 border-block-end 等）划分边界，保持整体性。
.terminal-surface {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  min-height: 0;
  background: var(--app-panel);
  color: var(--app-text);
  font-family: var(--font-body);
  overflow: hidden;
}

// Row 1: tab strip.
.terminal-surface :deep(.terminal-tabs) {
  flex: 0 0 auto;
  border-block-end: 1px solid var(--app-border);
  padding: var(--space-1) var(--space-2);
}

// Rows 2-3: toolbar + search bar.
.terminal-surface :deep(.terminal-toolbar),
.terminal-surface :deep(.terminal-searchbar) {
  flex: 0 0 auto;
}

// Row 4: xterm pane fills remaining height. CRITICAL: do not apply
// transform / filter / opacity here — it breaks the WebGL addon canvas.
.terminal-surface :deep(.terminal-pane-host) {
  flex: 1 1 auto;
  min-height: 0;
}
</style>
