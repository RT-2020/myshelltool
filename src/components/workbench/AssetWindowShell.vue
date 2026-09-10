<script setup lang="ts">
/**
 * AssetWindowShell — 独立资产工作台窗口壳（Phase 1-A）
 *
 * 资产独立 WebviewWindow 的根布局：上终端 / 下文件 + 右侧监控栏（可收起）。
 * 与主窗口共享同一 Rust 后端，但前端是独立 webview + 独立 Pinia 实例。
 * 复用 TerminalSurface / FileSurface / RightSidebar（均无 props 直连 store）。
 *
 * 布局契约：标题栏 52px / 状态栏 28px，与 WorkbenchShell 同高——
 * usePanelResize.getMainHeight 按此扣减计算 --terminal-h 比例。
 *
 * 样式复用全局 workbench-shell.scss 的 .titlebar / .window-btn / .main /
 * .resize / .statusbar / .sb-* 系列（本组件 scoped 仅写差异），避免双份实现漂移。
 *
 * 关窗流程：onCloseRequested 拦截系统关闭 → 无活跃会话直接 destroy；
 * 有则弹 confirmCloseAssetWindow 确认（body 在 ConfirmCloseAssetWindowContent），
 * 确认后先等 connecting 会话 settle 再断开全部，最后 destroy（绕过
 * close-requested 防递归确认）。
 *
 * 已知限制：
 *  - Esc 取消的窗外拖拽可能误开窗（Phase 2 拖出交互，待实测）；
 *  - 主窗口关闭后本窗口存活，属 Tauri 多窗口正常生命周期。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue';
import { ArrowUpDown, Minus, Moon, PanelRight, Square, Sun, Undo2, X } from 'lucide-vue-next';
import TerminalSurface from '@/components/terminal/TerminalSurface.vue';
import FileSurface from '@/components/files/FileSurface.vue';
import RightSidebar from '@/components/shell/RightSidebar.vue';
import { AppBrandLogo } from '@/components/ui';
import { useSessionsStore } from '@/stores/sessions';
import { pushSessionToMainWindow } from '@/lib/sessionHandoff';
import {
  closeTauriWindow,
  getExistingTauriWebviewWindow,
  getTauriWindow,
  isTauriRuntime,
  isTauriWindowMaximized,
  minimizeTauriWindow,
  startTauriWindowDragging,
  toggleTauriWindowMaximize
} from '@/services/backend';
import type { useWorkbenchStore } from '@/stores/workbench';
import type { usePanelResize, ResizeRegion } from '@/composables/usePanelResize';
import type { ModalState } from '@/types/domain';

/** 本组件消费的窗口可选能力（tauri.d.ts 的 TauriWindowLike 未声明，局部收窄）。 */
interface CloseableTauriWindow {
  onCloseRequested?: (handler: (event: { preventDefault(): void }) => void | Promise<void>) => Promise<() => void>;
  destroy?: () => Promise<void>;
}

const props = withDefaults(defineProps<{
  store: ReturnType<typeof useWorkbenchStore>;
  desktopRuntimeAvailable?: boolean;
  panelResize?: ReturnType<typeof usePanelResize> | null;
}>(), {
  desktopRuntimeAvailable: false,
  panelResize: null
});

const isMaximized = ref(false);
let unlistenClose: (() => void) | null = null;

// ============================================================
// 「移回主窗口」（tab 条已删，回迁入口收敛到标题栏按钮）
// ============================================================
const sessionsStore = useSessionsStore();
// 主窗口存活才显示回迁按钮。getExistingTauriWebviewWindow 是 async（getByLabel
// 返回 Promise），不能进 computed 同步消费（Promise 恒 truthy 会恒显示）。
// 挂载时探测 + 失败短重试（最多 3 次，间隔 2s）：getByLabel 在窗口刚创建的
// 瞬间可能查不到（Tauri 窗口注册时序），一次性探测失败会让按钮永久消失。
const mainAlive = ref(false);
const canPushToMain = computed(() => isTauriRuntime() && mainAlive.value);
let mainProbeAttempts = 0;
async function probeMainAlive() {
  if (mainAlive.value) return;
  const found = Boolean(await getExistingTauriWebviewWindow('main'));
  if (found) {
    mainAlive.value = true;
    return;
  }
  mainProbeAttempts += 1;
  if (mainProbeAttempts < 3) setTimeout(probeMainAlive, 2000);
}
// 仅 connected 会话有迁移价值（connecting 占位 id / error / disconnected 无）
const canMigrateActive = computed(() => props.store.activeSession?.status === 'connected');

async function pushActiveToMain() {
  const sessionId = props.store.activeSession?.sessionId;
  if (!sessionId || !canMigrateActive.value) return;
  await pushSessionToMainWindow({
    sessionsStore,
    workbenchStore: props.store,
    sessionId
  });
}

const asset = computed(() => props.store.selectedAsset || null);
const activeTransferCount = computed(() => props.store.activeTransfers?.length || 0);
// 本窗口独立 Pinia：store.sessions 即本窗口的会话（不含主窗口会话）
const connectedCount = computed(() =>
  (props.store.sessions || []).filter(s => s.status === 'connected').length
);
// 标题栏状态点与状态栏徽标共用：connecting > connected > error > idle
// （status 值域与 TerminalTabs 的 statusFor 映射一致）
const sessionStatus = computed(() => {
  const sessions = props.store.sessions || [];
  if (sessions.some(s => s.status === 'connecting')) return 'connecting';
  if (sessions.some(s => s.status === 'connected')) return 'connected';
  if (sessions.some(s => s.status === 'error')) return 'error';
  return 'idle';
});
// 徽标 class：connected/idle 复用全局 .badge.success/.muted，另两个本组件补
const SESSION_STATUS_LABELS = { connected: '已连接', connecting: '连接中', error: '连接错误', idle: '未连接' };
const SESSION_BADGE_CLASS = { connected: 'success', connecting: 'connecting', error: 'error', idle: 'muted' };
const shellClasses = computed(() => ({ 'right-collapsed': props.store.rightCollapsed }));
const desktopWindowControlsVisible = computed(() => isTauriRuntime());

function onToggleRight() {
  props.store.toggleRight();
  props.panelResize?.syncCollapse?.();
}

function startResize(event: PointerEvent, which: ResizeRegion) {
  props.panelResize?.startResize?.(event, which);
}

function resizeKeydown(event: KeyboardEvent, which: ResizeRegion) {
  props.panelResize?.handleResizeKeydown?.(event, which);
}

function resetPane(which: ResizeRegion) {
  props.panelResize?.resetPane?.(which);
}

async function syncWindowState() {
  isMaximized.value = await isTauriWindowMaximized();
}

async function handleTitlebarPointerDown(event: PointerEvent) {
  if (!isTauriRuntime() || event.button !== 0) return;
  if ((event.target as HTMLElement).closest('button, input, [role="button"], [data-no-drag="true"]')) return;
  await startTauriWindowDragging();
}

async function handleTitlebarDoubleClick(event: MouseEvent) {
  if (!isTauriRuntime()) return;
  if ((event.target as HTMLElement).closest('button, input, [role="button"], [data-no-drag="true"]')) return;
  await toggleWindowMaximize();
}

async function toggleWindowMaximize() {
  await toggleTauriWindowMaximize();
  await syncWindowState();
}

async function minimizeWindow() {
  await minimizeTauriWindow();
}

// 关闭按钮走 close()：触发 onCloseRequested 进入确认流程（与 Alt+F4 同路径）
function requestClose() {
  closeTauriWindow();
}

// ============================================================
// 关窗流程（核心）：无活跃会话直接销毁；有则确认后 断开全部 → 销毁
// ============================================================
async function requestCloseAssetWindow() {
  const active = (props.store.sessions || []).filter(
    s => s.status === 'connected' || s.status === 'connecting'
  );
  if (!active.length) {
    await destroyWindow();
    return;
  }
  props.store.modal = {
    type: 'confirmCloseAssetWindow',
    count: active.length,
    onConfirm: () => closeAfterConfirm()
  } as ModalState;
}

// 确认关闭：connecting 会话的 sessionId 还是 pending 占位（断不开，会留
// Rust 侧孤儿连接），先轮询等 settle（250ms 间隔，10s 超时兜底）；再
// allSettled 断开全部活跃会话；最后 destroy。
async function closeAfterConfirm() {
  const deadline = Date.now() + 10000;
  while (
    (props.store.sessions || []).some(s => s.status === 'connecting')
    && Date.now() < deadline
  ) {
    await new Promise(resolve => setTimeout(resolve, 250));
  }
  const targets = (props.store.sessions || []).filter(
    s => s.status === 'connected' || s.status === 'connecting'
  );
  await Promise.allSettled(targets.map(s => props.store.disconnectSession(s.sessionId)));
  await destroyWindow();
}

// destroy 绕过 close-requested（防确认弹窗递归）；失败仅记录，不静默吞
async function destroyWindow() {
  try {
    await (getTauriWindow() as CloseableTauriWindow | null)?.destroy?.();
  } catch (error) {
    console.error('[AssetWindowShell] destroy window failed:', error);
  }
}

onMounted(async () => {
  syncWindowState();
  probeMainAlive();
  const currentWindow = getTauriWindow() as CloseableTauriWindow | null;
  if (typeof currentWindow?.onCloseRequested === 'function') {
    unlistenClose = await currentWindow.onCloseRequested(async event => {
      event.preventDefault();
      await requestCloseAssetWindow();
    });
  }
});

onBeforeUnmount(() => {
  if (typeof unlistenClose === 'function') unlistenClose();
});
</script>

<template>
  <div class="asset-shell" :class="shellClasses" role="application" :aria-label="`独立工作台 ${asset?.name || ''}`">
    <header
      class="titlebar"
      data-region="titlebar"
      @pointerdown="handleTitlebarPointerDown"
      @dblclick="handleTitlebarDoubleClick"
    >
      <div class="tb-left" data-tauri-drag-region>
        <AppBrandLogo :size="20" />
        <div class="tb-asset">
          <span class="tb-name">{{ asset?.name || '未知资产' }}</span>
          <span v-if="asset" class="tb-subtitle">{{ asset.username }}@{{ asset.host }}:{{ asset.port }}</span>
        </div>
        <span
          class="session-dot"
          :class="sessionStatus"
          :title="SESSION_STATUS_LABELS[sessionStatus]"
          aria-hidden="true"
        ></span>
      </div>

      <div class="tb-right">
        <button
          v-if="canPushToMain"
          class="icon-btn"
          type="button"
          aria-label="移回主窗口"
          title="移回主窗口"
          :disabled="!canMigrateActive"
          @click="pushActiveToMain"
        >
          <Undo2 />
        </button>
        <button class="icon-btn" type="button" aria-label="切换主题" title="切换主题" @click="props.store.toggleTheme()">
          <Sun v-if="props.store.effectiveTheme === 'light'" />
          <Moon v-else />
        </button>
        <button
          class="icon-btn"
          type="button"
          aria-label="收起或展开右侧面板"
          :title="props.store.rightCollapsed ? '展开右侧面板' : '收起右侧面板'"
          :aria-pressed="props.store.rightCollapsed ? 'true' : 'false'"
          @click="onToggleRight"
        >
          <PanelRight />
        </button>
        <div v-if="desktopWindowControlsVisible" class="window-controls" data-no-drag="true" aria-label="窗口控制">
          <button class="window-btn" type="button" aria-label="最小化" title="最小化" @click="minimizeWindow">
            <Minus :size="14" />
          </button>
          <button
            class="window-btn"
            type="button"
            :aria-label="isMaximized ? '还原窗口' : '最大化'"
            :title="isMaximized ? '还原窗口' : '最大化'"
            @click="toggleWindowMaximize"
          >
            <Square :size="12" />
          </button>
          <button class="window-btn danger" type="button" aria-label="关闭" title="关闭" @click="requestClose">
            <X :size="14" />
          </button>
        </div>
      </div>
    </header>

    <main class="main">
      <TerminalSurface :show-tabs="false" data-region="center-top" />
      <FileSurface data-region="center-bottom" />
    </main>

    <div
      class="resize resize-v"
      :class="{ active: panelResize?.resizing?.value === 'center-row' }"
      data-target="split"
      role="separator"
      aria-orientation="horizontal"
      aria-label="调整终端与文件高度"
      tabindex="0"
      @pointerdown="startResize($event, 'center-row')"
      @keydown="resizeKeydown($event, 'center-row')"
      @dblclick="resetPane('center-row')"
    ></div>

    <div
      class="resize resize-h"
      :class="{ active: panelResize?.resizing?.value === 'right' }"
      data-target="right"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整右侧栏宽度"
      tabindex="0"
      @pointerdown="startResize($event, 'right')"
      @keydown="resizeKeydown($event, 'right')"
      @dblclick="resetPane('right')"
    ></div>

    <RightSidebar @collapse="onToggleRight" />

    <footer class="statusbar" data-region="statusbar">
      <div class="sb-left">
        <span class="sb-item muted status-text" aria-live="polite">{{ props.store.statusMessage || '就绪' }}</span>
      </div>
      <div class="sb-right">
        <button
          class="sb-item transfer-pill"
          type="button"
          aria-label="打开传输队列"
          @click="props.store.toggleTransferDrawer()"
        >
          <ArrowUpDown :size="12" aria-hidden="true" />
          <span class="mono">{{ activeTransferCount }}</span>
        </button>
        <span class="sb-sep">·</span>
        <span class="badge" :class="SESSION_BADGE_CLASS[sessionStatus]">
          {{ SESSION_STATUS_LABELS[sessionStatus] }}<template v-if="connectedCount"> · {{ connectedCount }}</template>
        </span>
      </div>
    </footer>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 上终端/下文件 + 右栏：标题栏/状态栏跨全宽（52px/28px 布局契约，见文件头注释）。
// 其余 chrome 样式复用全局 workbench-shell.scss（.titlebar/.main/.resize/.statusbar 等）。
.asset-shell {
  position: relative;
  --right-w-collapsed: 0px;
  display: grid;
  grid-template-columns: minmax(0, 1fr) var(--right-w, 280px);
  grid-template-rows: 52px minmax(0, 1fr) 28px;
  grid-template-areas:
    'titlebar titlebar'
    'main right'
    'statusbar statusbar';
  width: 100vw;
  height: 100vh;
  overflow: hidden;
  background: var(--app-bg);
  color: var(--app-text);
  font-family: var(--font-body);
}

// 右栏折叠：ui store 的 dataset 写在本窗口 documentElement（per-window 隔离），
// 视觉折叠由组件 class 覆盖 --right-w（与 WorkbenchShell 的 .right-collapsed 模式一致）
.asset-shell.right-collapsed { --right-w: var(--right-w-collapsed); }

// 全局 .titlebar 是主窗口三段 grid（左/搜索/右）；asset 壳只有两段，覆盖为 flex
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

// 资产名 + user@host:port 副标题 + 会话状态点（asset 壳特有）
.tb-asset {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}

.tb-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--app-text);
  letter-spacing: -0.01em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.tb-subtitle {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: var(--app-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.session-dot {
  flex-shrink: 0;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--app-subtle);
}

.session-dot.connected { background: var(--success); }
.session-dot.connecting { background: var(--accent); }
.session-dot.error { background: var(--danger); }

// 标题栏「移回主窗口」disabled 态（全局 .icon-btn 未覆盖）：无可迁移会话时置灰
.icon-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}
.icon-btn:disabled:hover {
  background: transparent;
  color: var(--app-muted);
}

// 全局 split divider 从左栏起定位（left: var(--sidebar-w)）；asset 壳无左栏
.resize[data-target='split'] { left: 0; }
.asset-shell.right-collapsed .resize[data-target='right'] { display: none; }

// 会话状态徽标（不渲染 sync/MCP badge：asset 模式未初始化对应 store）。
// success/muted 复用全局 .badge.*；connecting/error 补齐（含 ::before 圆点对齐）
.badge.connecting { color: var(--accent); }
.badge.connecting::before { background: var(--accent); }
.badge.error { color: var(--danger); }
.badge.error::before { background: var(--danger); }
</style>
