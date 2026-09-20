<script setup lang="ts">
import { ref, onMounted, onBeforeUnmount, watch, nextTick, computed } from 'vue';
import { Plus, MoreHorizontal, X, Copy, FolderX, SquareX } from 'lucide-vue-next';
import { useDragOutsideViewport } from '@/composables/useDragOutsideViewport';
import type { useSessionsStore } from '@/stores/sessions';

/** 会话 tab 数据项：sessions store 的数组元素（store 未导出 SessionEntry，按 store 类型推断）。 */
type SessionTabItem = ReturnType<typeof useSessionsStore>['sessions'][number];

const props = withDefaults(defineProps<{
  sessions?: SessionTabItem[];
  activeSessionId?: string;
}>(), {
  sessions: () => [],
  activeSessionId: ''
});
const emit = defineEmits<{
  select: [sessionId: string];
  close: [sessionId: string];
  'close-others': [sessionId: string];
  'close-right': [sessionId: string];
  'copy-host': [sessionId: string];
  'new-terminal': [];
  'drag-out': [sessionId: string];
}>();

const barRef = ref<HTMLElement | null>(null);
const overflowTriggerRef = ref<HTMLElement | null>(null);
const overflowed = ref<string[]>([]);
const overflowMenuOpen = ref(false);
const overflowMenuStyle = ref<{ left: number | string; top: number | string }>({ left: 0, top: 0 });
const contextMenu = ref({ open: false, x: 0, y: 0, sessionId: '' });

// Tab 拖出（sessionHandoff tearoff 的 UI 侧）
const dragOutside = useDragOutsideViewport();

// 仅 connected tab 可拖（connecting/error/disconnected 无迁移价值）
function onTabDragStart(e: DragEvent, session: SessionTabItem) {
  const dt = e.dataTransfer;
  if (!dt) return;
  dt.setData('text/plain', session.sessionId);
  dt.effectAllowed = 'move';
  dragOutside.attach();
}

function onTabDragEnd(e: DragEvent, session: SessionTabItem) {
  const outside = dragOutside.isOutside(e); // 先取判定（取值后自动复位）再 detach
  dragOutside.detach();
  if (outside && session.status === 'connected') emit('drag-out', session.sessionId); // Esc 窗外取消会误触发（沿用 v1 已知边界）
}

// 浮动菜单 clamp 到视口内（x 超界则翻转），菜单宽/高取近似值
function clampMenu(x: number, y: number, width = 200, height = 180) {
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
  const tabs = Array.from(bar.querySelectorAll<HTMLElement>('[data-session-tab]'));
  const reserve = 90;
  const limit = bar.clientWidth - reserve;
  const next: string[] = [];
  for (const t of tabs) {
    if (t.offsetLeft + t.offsetWidth > limit) {
      next.push(t.dataset.sessionId ?? '');
    }
  }
  overflowed.value = next;
}

let ro: ResizeObserver | null = null;
onMounted(() => {
  if (typeof ResizeObserver !== 'undefined' && barRef.value) {
    ro = new ResizeObserver(updateOverflow);
    ro.observe(barRef.value);
  }
  nextTick(updateOverflow);
  document.addEventListener('keydown', onDocKeydown);
  window.addEventListener('blur', onWindowBlur);
});
onBeforeUnmount(() => {
  if (ro) ro.disconnect();
  document.removeEventListener('keydown', onDocKeydown);
  window.removeEventListener('blur', onWindowBlur);
});
watch(() => props.sessions.length, () => nextTick(updateOverflow));

function onTabClick(sessionId: string) { emit('select', sessionId); }
function onTabClose(e: MouseEvent, sessionId: string) {
  e.stopPropagation();
  emit('close', sessionId);
}
function onContextMenu(e: MouseEvent, sessionId: string) {
  e.preventDefault();
  contextMenu.value = { open: true, x: e.clientX, y: e.clientY, sessionId };
}
function closeAllMenus() {
  contextMenu.value.open = false;
  overflowMenuOpen.value = false;
}

// Esc 关闭与失焦关闭。TerminalSurface 的统一 Esc 解散链只管 search/cheatsheet/
// palette/dangerousPaste，够不到本组件的本地菜单状态，故自处理（速查表承诺
// 「Esc 关闭浮层」）；窗口失焦（系统浮层/任务栏抢焦点）时菜单无法交互，必须收起。
function onDocKeydown(e: KeyboardEvent) {
  if (e.key !== 'Escape') return;
  if (!contextMenu.value.open && !overflowMenuOpen.value) return;
  e.preventDefault();
  closeAllMenus();
}
function onWindowBlur() {
  closeAllMenus();
}

// tablist roving tabindex：方向键在可见 tab 间移动焦点，Enter/Space 触发选择
function onTabKeydown(e: KeyboardEvent, sessionId: string) {
  if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
    e.preventDefault();
    const list = visibleSessions.value;
    const idx = list.findIndex(s => s.sessionId === sessionId);
    if (idx < 0) return;
    const dir = e.key === 'ArrowRight' ? 1 : -1;
    const next = list[(idx + dir + list.length) % list.length];
    const el = barRef.value?.querySelector<HTMLElement>(`[data-session-id="${next.sessionId}"]`);
    el?.focus();
    return;
  }
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault();
    onTabClick(sessionId);
  }
}

function onTabFocus(sessionId: string) {
  // 焦点所在 tab 置 tabindex=0，其余 -1（roving tabindex 约定）
  for (const s of visibleSessions.value) {
    const el = barRef.value?.querySelector<HTMLElement>(`[data-session-id="${s.sessionId}"]`);
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

function statusFor(s: SessionTabItem) {
  if (s.status === 'connected') return 'connected';
  if (s.status === 'connecting') return 'connecting';
  if (s.status === 'disconnected') return 'disconnected';
  if (s.status === 'error') return 'error';
  return 'idle';
}
function tooltipFor(s: SessionTabItem) {
  const parts = [s.asset?.name || '', s.asset?.host || ''];
  if (s.oscTitle) parts.push(s.oscTitle);
  return parts.filter(Boolean).join(' · ');
}
// 同资产多会话场景：为重名 tab 追加 sessionId 末 4 位后缀（如 " (#a3f9)"）以便区分。
// 用后端会话 id 而非数组序号：关闭中间 tab 不会引起后缀重排，锚点稳定。
// 取末位是因为连接中的占位 id 为 'pending-<assetId>-<时间戳>'，同资产前缀相同，
// 末 4 位（时间戳尾 / UUID 尾）在任何阶段都唯一。连接成功后占位 id 被替换为
// 真实 UUID，后缀会一次性变化，之后固定。
function dupSuffixFor(session: SessionTabItem) {
  const same = props.sessions.filter(s => s.asset?.id && s.asset?.id === session.asset?.id);
  if (same.length < 2) return '';
  const id = String(session.sessionId || '');
  return ' (#' + id.slice(-4) + ')';
}
</script>

<template>
  <div class="terminal-tabs-host">
    <div
      class="term-tabs terminal-tabs"
      ref="barRef"
      role="tablist"
      aria-label="SSH 会话标签"
    >
      <button class="tab workspace-tab terminal-tab-new" role="tab" @click="emit('new-terminal')" title="新建会话" aria-label="新建会话">
        <Plus :size="14" />
      </button>
      <!-- 已知限制：溢出折叠进菜单的 tab 不可拖 -->
      <div
        v-for="session in visibleSessions"
        :key="session.sessionId"
        :class="['tab', 'workspace-tab', 'session-tab', { active: session.sessionId === activeSessionId }]"
        role="tab"
        :tabindex="session.sessionId === activeSessionId ? 0 : -1"
        :aria-selected="session.sessionId === activeSessionId ? 'true' : 'false'"
        :data-session-id="session.sessionId"
        data-session-tab
        :title="tooltipFor(session)"
        :draggable="session.status === 'connected'"
        @click="onTabClick(session.sessionId)"
        @keydown="onTabKeydown($event, session.sessionId)"
        @focus="onTabFocus(session.sessionId)"
        @contextmenu="onContextMenu($event, session.sessionId)"
        @mousedown.middle.prevent="emit('close', session.sessionId)"
        @dragstart="onTabDragStart($event, session)"
        @dragend="onTabDragEnd($event, session)"
      >
        <span :class="['dot', statusFor(session)]"></span>
        <span class="tab-name session-tab-name">{{ session.asset?.name }}{{ dupSuffixFor(session) }}</span>
        <button class="tab-close" aria-label="关闭会话" @click="onTabClose($event, session.sessionId)" @mousedown.stop><X :size="12" /></button>
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
      <Transition name="tab-menu">
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
      </Transition>
      <Transition name="tab-menu">
        <div v-if="contextMenu.open" class="tab-context-menu" :style="contextMenuStyle" @click.stop>
          <button @click="emit('copy-host', contextMenu.sessionId); closeAllMenus()"><Copy :size="14" /> 复制主机地址</button>
          <button @click="emit('close-others', contextMenu.sessionId); closeAllMenus()"><FolderX :size="14" /> 关闭其他</button>
          <button @click="emit('close-right', contextMenu.sessionId); closeAllMenus()"><SquareX :size="14" /> 关闭右侧</button>
          <button class="danger" @click="emit('close', contextMenu.sessionId); closeAllMenus()"><X :size="14" /> 关闭此标签</button>
        </div>
      </Transition>
      <Transition name="tab-fade">
        <div v-if="overflowMenuOpen || contextMenu.open" class="tab-overlay" @click="closeAllMenus" @contextmenu.prevent="closeAllMenus"></div>
      </Transition>
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
  // 条底分隔线用 inset shadow 而非 border：激活标签的不透明背景会盖住它，
  // 其下划线因此能直接衔接下方内容；inset shadow 也不会被自身 overflow:hidden 裁剪
  box-shadow: inset 0 -1px 0 var(--app-border);
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
  transition: background var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);

  &:hover {
    color: var(--accent);
    background: var(--app-hover);
  }
}

.session-tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  border: none;
  background: transparent;
  color: var(--app-muted);
  border-radius: 0;
  cursor: pointer;
  max-width: 220px;
  flex: 0 1 auto;
  min-width: 0;
  border-right: 1px solid var(--app-border-soft);
  position: relative;
  transition: background var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);

  &:hover {
    background: var(--app-hover);
    color: var(--app-strong);
  }

  // 激活下划线贴住标签底边（= 条底边），与下方内容无缝衔接
  &.active {
    background: var(--app-panel);
    color: var(--app-strong);

    &::after {
      content: '';
      position: absolute;
      left: 0;
      right: 0;
      bottom: 0;
      height: 2px;
      background: var(--accent);
    }
  }
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
  transition: opacity var(--dur-fast) var(--ease-standard),
    background var(--dur-fast) var(--ease-standard);
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
</style>

<!-- 浮动菜单（overflow/右键菜单/捕获层）样式外置：本文件贴近 500 行硬上限，
     见 TerminalTabs.menus.scss 头注释 -->
<style scoped lang="scss" src="./TerminalTabs.menus.scss"></style>
