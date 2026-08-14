<script setup>
import { ref, onMounted, onBeforeUnmount, watch, nextTick, computed } from 'vue';
import { Plus, MoreHorizontal, X, Copy, FolderX, SquareX } from 'lucide-vue-next';

const props = defineProps({
  sessions: { type: Array, default: () => [] },
  activeSessionId: { type: String, default: '' }
});
const emit = defineEmits(['select', 'close', 'close-others', 'close-right', 'copy-host', 'new-terminal']);

const barRef = ref(null);
const overflowTriggerRef = ref(null);
const overflowed = ref([]);
const overflowMenuOpen = ref(false);
const overflowMenuStyle = ref({ left: 0, top: 0 });
const contextMenu = ref({ open: false, x: 0, y: 0, sessionId: '' });

// 浮动菜单 clamp 到视口内（x 超界则翻转），菜单宽/高取近似值
function clampMenu(x, y, width = 200, height = 180) {
  const pad = 8;
  let left = x;
  let top = y;
  if (left + width > window.innerWidth - pad) left = window.innerWidth - width - pad;
  if (left < pad) left = pad;
  if (top + height > window.innerHeight - pad) top = window.innerHeight - height - pad;
  if (top < pad) top = pad;
  return { left, top };
}

const contextMenuStyle = computed(() => {
  const { left, top } = clampMenu(contextMenu.value.x, contextMenu.value.y);
  return { left: left + 'px', top: top + 'px' };
});

function updateOverflow() {
  const bar = barRef.value;
  if (!bar) return;
  const tabs = Array.from(bar.querySelectorAll('[data-session-tab]'));
  const reserve = 90;
  const limit = bar.clientWidth - reserve;
  const next = [];
  for (const t of tabs) {
    if (t.offsetLeft + t.offsetWidth > limit) {
      next.push(t.dataset.sessionId);
    }
  }
  overflowed.value = next;
}

let ro = null;
onMounted(() => {
  if (typeof ResizeObserver !== 'undefined' && barRef.value) {
    ro = new ResizeObserver(updateOverflow);
    ro.observe(barRef.value);
  }
  nextTick(updateOverflow);
});
onBeforeUnmount(() => { if (ro) ro.disconnect(); });
watch(() => props.sessions.length, () => nextTick(updateOverflow));

function onTabClick(sessionId) { emit('select', sessionId); }
function onTabClose(e, sessionId) {
  e.stopPropagation();
  emit('close', sessionId);
}
function onContextMenu(e, sessionId) {
  e.preventDefault();
  contextMenu.value = { open: true, x: e.clientX, y: e.clientY, sessionId };
}
function closeAllMenus() {
  contextMenu.value.open = false;
  overflowMenuOpen.value = false;
}

// tablist roving tabindex：方向键在可见 tab 间移动焦点，Enter/Space 触发选择
function onTabKeydown(e, sessionId) {
  if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    e.preventDefault();
    const list = visibleSessions.value;
    const idx = list.findIndex(s => s.sessionId === sessionId);
    if (idx < 0) return;
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    const next = list[(idx + dir + list.length) % list.length];
    const el = barRef.value?.querySelector(`[data-session-id="${next.sessionId}"]`);
    el?.focus();
    return;
  }
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault();
    onTabClick(sessionId);
  }
}

function onTabFocus(sessionId) {
  // 焦点所在 tab 置 tabindex=0，其余 -1（roving tabindex 约定）
  for (const s of visibleSessions.value) {
    const el = barRef.value?.querySelector(`[data-session-id="${s.sessionId}"]`);
    if (el) el.tabIndex = s.sessionId === sessionId ? 0 : -1;
  }
}

function toggleOverflowMenu() {
  overflowMenuOpen.value = !overflowMenuOpen.value;
  if (overflowMenuOpen.value) {
    const rect = overflowTriggerRef.value?.getBoundingClientRect();
    if (rect) {
      const { left, top } = clampMenu(rect.right - 200, rect.bottom + 4, 200, 220);
      overflowMenuStyle.value = { left: left + 'px', top: top + 'px' };
    }
  }
}

const visibleSessions = computed(() => props.sessions.filter(s => !overflowed.value.includes(s.sessionId)));
const hiddenSessions = computed(() => props.sessions.filter(s => overflowed.value.includes(s.sessionId)));

function statusFor(s) {
  if (s.status === 'connected') return 'connected';
  if (s.status === 'connecting') return 'connecting';
  if (s.status === 'disconnected') return 'disconnected';
  if (s.status === 'error') return 'error';
  return 'idle';
}
function tooltipFor(s) {
  const parts = [s.asset?.name || '', s.asset?.host || ''];
  if (s.oscTitle) parts.push(s.oscTitle);
  return parts.filter(Boolean).join(' · ');
}
// 同资产多会话场景：为重名 tab 追加 sessionId 末 4 位后缀（如 " (#a3f9)"）以便区分。
// 用后端会话 id 而非数组序号：关闭中间 tab 不会引起后缀重排，锚点稳定。
// 取末位是因为连接中的占位 id 为 'pending-<assetId>-<时间戳>'，同资产前缀相同，
// 末 4 位（时间戳尾 / UUID 尾）在任何阶段都唯一。连接成功后占位 id 被替换为
// 真实 UUID，后缀会一次性变化，之后固定。
function dupSuffixFor(session) {
  const same = props.sessions.filter(s => s.asset?.id && s.asset?.id === session.asset?.id);
  if (same.length < 2) return '';
  const id = String(session.sessionId || '');
  return ' (#' + id.slice(-4) + ')';
}
</script>

<template>
  <div class="terminal-tabs-host">
    <div class="term-tabs terminal-tabs" ref="barRef" role="tablist" aria-label="SSH 会话标签">
      <button class="tab workspace-tab terminal-tab-new" role="tab" @click="emit('new-terminal')" title="新建会话">
        <Plus :size="14" />
        <span class="new-label">新建</span>
      </button>
      <div
        v-for="session in visibleSessions"
        :key="session.sessionId"
        :class="['tab', 'workspace-tab', 'session-tab', { active: session.sessionId === activeSessionId }]"
        role="tab"
        :tabindex="session.sessionId === activeSessionId ? 0 : -1"
        :aria-selected="String(session.sessionId === activeSessionId)"
        :data-session-id="session.sessionId"
        data-session-tab
        :title="tooltipFor(session)"
        @click="onTabClick(session.sessionId)"
        @keydown="onTabKeydown($event, session.sessionId)"
        @focus="onTabFocus(session.sessionId)"
        @contextmenu="onContextMenu($event, session.sessionId)"
        @mousedown.middle.prevent="emit('close', session.sessionId)"
      >
        <span :class="['dot', statusFor(session)]"></span>
        <span class="tab-name session-tab-name">{{ session.asset?.name }}{{ dupSuffixFor(session) }}</span>
        <span class="session-tab-host">{{ session.asset?.host }}</span>
        <span v-if="session.oscTitle" class="session-tab-osc">· {{ session.oscTitle }}</span>
        <button class="tab-close" aria-label="关闭会话" @click="onTabClose($event, session.sessionId)"><X :size="12" /></button>
      </div>
      <div class="term-tabs-spacer"></div>
      <button
        v-if="hiddenSessions.length"
        ref="overflowTriggerRef"
        class="tab workspace-tab overflow-trigger"
        :class="{ active: overflowMenuOpen }"
        aria-haspopup="menu"
        :aria-expanded="overflowMenuOpen"
        @click="toggleOverflowMenu"
        title="更多会话"
      >
        <MoreHorizontal :size="14" />
      </button>
    </div>

    <Teleport to="body">
      <div v-if="overflowMenuOpen && hiddenSessions.length" class="overflow-menu" :style="overflowMenuStyle" @click.stop role="menu" aria-label="更多会话">
        <div
          v-for="session in hiddenSessions"
          :key="session.sessionId"
          :class="['overflow-item', { active: session.sessionId === activeSessionId }]"
          role="menuitem"
          @click="() => { onTabClick(session.sessionId); overflowMenuOpen = false; }"
        >
          <span :class="['dot', statusFor(session)]"></span>
          <span class="overflow-name">{{ session.asset?.name }}{{ dupSuffixFor(session) }}</span>
          <span class="overflow-host muted">{{ session.asset?.host }}</span>
          <button class="overflow-close" aria-label="关闭会话" @click.stop="emit('close', session.sessionId)"><X :size="12" /></button>
        </div>
      </div>
      <div v-if="contextMenu.open" class="tab-context-menu" :style="contextMenuStyle" @click.stop>
        <button @click="emit('copy-host', contextMenu.sessionId); closeAllMenus()"><Copy :size="14" /> 复制主机地址</button>
        <button @click="emit('close-others', contextMenu.sessionId); closeAllMenus()"><FolderX :size="14" /> 关闭其他</button>
        <button @click="emit('close-right', contextMenu.sessionId); closeAllMenus()"><SquareX :size="14" /> 关闭右侧</button>
        <button class="danger" @click="emit('close', contextMenu.sessionId); closeAllMenus()"><X :size="14" /> 关闭此标签</button>
      </div>
      <div v-if="overflowMenuOpen || contextMenu.open" class="tab-overlay" @click="closeAllMenus" @contextmenu.prevent="closeAllMenus"></div>
    </Teleport>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.terminal-tabs-host {
  position: relative;
  min-width: 0;
  height: 100%;
}

.term-tabs {
  display: flex;
  align-items: center;
  height: 100%;
  overflow: hidden;
  background: var(--app-chrome);
  border-bottom: 1px solid var(--app-border);
}

.term-tabs-spacer {
  flex: 1;
  min-width: var(--space-2);
}

.tab,
.workspace-tab {
  min-width: 0;
  height: 100%;
}

.terminal-tab-new {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex: 0 0 auto;
  padding: 0 10px;
  border: none;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: 0;
  border-right: 1px solid var(--app-border-soft);
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}

.terminal-tab-new:hover {
  color: var(--accent);
  background: var(--app-hover);
}

.session-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  border: none;
  background: transparent;
  border-radius: 0;
  cursor: pointer;
  max-width: 220px;
  flex: 0 1 auto;
  min-width: 0;
  border-right: 1px solid var(--app-border-soft);
  border-block-end: 2px solid transparent;
  transition: background var(--motion-fast) var(--ease-standard),
    border-color var(--motion-fast) var(--ease-standard);
}

.session-tab:hover {
  background: var(--app-hover);
}

.session-tab.active {
  background: var(--app-panel);
  border-block-end-color: var(--accent);
  color: var(--app-strong);
}

// 会话状态圆点已收敛为全局 .dot（_utilities.scss 单一权威实现）

.session-tab-name {
  font-size: var(--text-xs);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 110px;
}

.session-tab-host {
  font-size: 10px;
  color: var(--app-muted);
  font-family: var(--font-mono);
  white-space: nowrap;
}

.session-tab-osc {
  font-size: 10px;
  color: var(--app-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100px;
}

.tab-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  border: none;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  opacity: 0;
  transition: opacity var(--motion-fast) var(--ease-standard),
    background var(--motion-fast) var(--ease-standard);
  flex: 0 0 auto;
}

.session-tab:hover .tab-close,
.session-tab.active .tab-close { opacity: 1; }

.tab-close:hover {
  background: color-mix(in oklab, var(--danger) 20%, transparent);
  color: var(--danger);
}

.overflow-trigger {
  flex: 0 0 auto;
  padding: 0 10px;
  border: none;
  background: transparent;
  border-radius: 0;
  cursor: pointer;
  color: var(--app-muted);
}

.overflow-trigger:hover,
.overflow-trigger.active {
  color: var(--accent);
  background: var(--app-hover);
}

// ============================================================
// Floating menus (overflow + context) — low-weight, single border
// ============================================================
.overflow-menu,
.tab-context-menu {
  position: fixed;
  z-index: var(--z-dropdown);
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  box-shadow: var(--elev-raised);
  padding: 4px;
  min-width: 180px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.overflow-item,
.tab-context-menu button {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px var(--space-2);
  background: transparent;
  border: none;
  color: var(--app-text);
  cursor: pointer;
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  text-align: left;
}

.overflow-item:hover,
.tab-context-menu button:hover { background: var(--app-hover); }

.overflow-item.active {
  background: color-mix(in oklab, var(--accent) 15%, transparent);
  color: var(--accent);
}

.overflow-name {
  font-size: var(--text-xs);
  font-weight: 500;
}

.overflow-host {
  font-size: 10px;
  font-family: var(--font-mono);
  margin-left: auto;
}

.overflow-close {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  border: none;
  background: transparent;
  color: var(--app-muted);
  cursor: pointer;
  border-radius: var(--radius-sm);
  flex: 0 0 auto;
  transition: background var(--motion-fast) var(--ease-standard),
    color var(--motion-fast) var(--ease-standard);
}
.overflow-close:hover {
  background: color-mix(in oklab, var(--danger) 18%, transparent);
  color: var(--danger);
}

.tab-context-menu button.danger:hover {
  background: color-mix(in oklab, var(--danger) 18%, transparent);
  color: var(--danger);
}

.tab-overlay {
  position: fixed;
  inset: 0;
  z-index: calc(var(--z-dropdown) - 1);
  background: transparent;
}
</style>
