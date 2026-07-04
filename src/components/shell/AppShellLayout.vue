<script setup>
/**
 * AppShellLayout — 五区域 CSS Grid 外壳（app.html 全量还原）。
 *
 * Layout（open-design app.html 严格同步）：
 *
 *   ┌──────────────────────────────────────────────┐
 *   │ titlebar  (--titlebar-h 52px, full width)     │
 *   ├───────────┬───────────────────┬──────────────┤
 *   │           │  .main > terminal  │              │
 *   │           │  (--center-top-h)  │   right      │
 *   │  sidebar  ├───────────────────┤  (resources  │
 *   │  (assets) │  .main > files     │   + ops)     │
 *   │           │  (1fr 吃剩余)      │              │
 *   ├───────────┴───────────────────┴──────────────┤
 *   │ statusbar (--statusbar-h 28px, full width)    │
 *   └──────────────────────────────────────────────┘
 *
 * 与旧版差异（app.html 还原）：
 *   - grid 从扁平 4 行改为 3 行 + .main 二级 grid（terminal/files 由 .main 内部分配）
 *   - class .shell-layout → .app；shell-region--* → 直接 grid-area 命名
 *   - 拖拽条 class resize-divider → resize（data-target="sidebar|right|split"）
 *   - 折叠态：.app.sidebar-collapsed / .app.right-collapsed（类名驱动，替代旧 dataset）
 *
 * Slots: titlebar | sidebar | center-top(terminal) | center-bottom(files) | right | statusbar
 *   注：slot 名保留 center-top/center-bottom（App.vue 不用改），内部由 .main 包裹做二级 grid。
 *
 * 变量驱动：
 *   --sidebar-w / --right-w：列宽，usePanelResize 拖拽写入 + 折叠态覆盖
 *   --center-top-h：终端区行高（usePanelResize 写入），.main grid-template-rows 消费
 */
defineProps({
  // usePanelResize 暴露的 startResize(event, which)
  startResize: { type: Function, default: () => {} },
  sidebarCollapsed: { type: Boolean, default: false },
  rightCollapsed: { type: Boolean, default: false }
});
</script>

<template>
  <div
    class="app"
    role="application"
    aria-label="myshelltool 主窗口"
    :class="{
      'sidebar-collapsed': sidebarCollapsed,
      'right-collapsed': rightCollapsed
    }"
  >
    <slot name="titlebar" />
    <slot name="sidebar" />

    <!-- .main 二级 grid：terminal + files 上下分栏（--center-top-h 驱动）-->
    <main class="main" data-region="main">
      <slot name="center-top" />
      <slot name="center-bottom" />
    </main>

    <slot name="right" />
    <slot name="statusbar" />

    <!-- 拖拽分隔条（绝对定位，浮于 grid 之上）。折叠态由 CSS 隐藏对应条。 -->
    <div
      class="resize resize-h"
      data-target="sidebar"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整侧栏宽度"
      @pointerdown="startResize($event, 'sidebar')"
    ></div>
    <div
      class="resize resize-h"
      data-target="right"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整右侧面板宽度"
      @pointerdown="startResize($event, 'right')"
    ></div>
    <div
      class="resize resize-v"
      data-target="split"
      role="separator"
      aria-orientation="horizontal"
      aria-label="调整终端与文件区高度"
      @pointerdown="startResize($event, 'center-row')"
    ></div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// ============================================================
// 五区域 Grid（app.css L191-204 严格同步）
// ============================================================
.app {
  position: relative; // 拖拽 divider 绝对定位的参照系
  display: grid;
  grid-template-columns: var(--sidebar-w, 260px) minmax(0, 1fr) var(--right-w, 280px);
  grid-template-rows: var(--titlebar-h, 52px) minmax(0, 1fr) var(--statusbar-h, 28px);
  grid-template-areas:
    'titlebar  titlebar  titlebar'
    'sidebar   main      right'
    'statusbar statusbar statusbar';
  width: 100vw;
  height: 100vh;
  min-width: 1280px; // 低于 1280px → 横向滚动 fallback
  overflow: hidden;
  background: var(--app-bg);
  color: var(--app-text);
  font-family: var(--font-body);
}

// grid-area 映射。区域节点由 slot 子组件提供，避免和子组件根节点形成双壳。
.app > :global(.titlebar)      { grid-area: titlebar; }
.app > :global(.sidebar)       { grid-area: sidebar; }
.main                          { grid-area: main; }
.app > :global(.right-sidebar) { grid-area: right; }
.app > :global(.app-status-bar),
.app > :global(.statusbar)     { grid-area: statusbar; }

// 区域基础：min-width/min-height 防 grid 子项撑爆
.app > :global(.titlebar),
.app > :global(.sidebar),
.main,
.app > :global(.right-sidebar),
.app > :global(.app-status-bar),
.app > :global(.statusbar) {
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

// 区域边框（app.css：chrome 用 --app-border）
.app > :global(.titlebar) {
  background: var(--app-chrome);
  border-bottom: 1px solid var(--app-border);
}
.app > :global(.sidebar) {
  background: var(--app-panel);
  border-right: 1px solid var(--app-border);
}
.app > :global(.right-sidebar) {
  background: var(--app-panel);
  border-left: 1px solid var(--app-border);
}
.app > :global(.app-status-bar),
.app > :global(.statusbar) {
  background: var(--app-chrome);
  border-top: 1px solid var(--app-border);
}

// ============================================================
// .main 二级 grid：terminal + files 上下分栏（app.css L453-460）
// --center-top-h 由 usePanelResize 写入（拖拽），fallback 1fr（上下平分）
// ============================================================
.main {
  display: grid;
  grid-template-rows: var(--center-top-h, 1fr) minmax(0, 1fr);
  background: var(--app-bg);
  position: relative;
  min-width: 0;
  min-height: 0;
}
.main > :global(.region-terminal) {
  background: var(--app-panel);
  border-bottom: 1px solid var(--app-border);
  min-height: 0;
  display: flex;
  flex-direction: column;
}
.main > :global(.region-files) {
  background: var(--app-panel);
  min-height: 0;
  display: flex;
  flex-direction: column;
}

// ============================================================
// 折叠态：在 .app 根元素加类，覆盖列宽变量（app.css L435-450）
// 注：ui.js 现在写 dataset 到 <html>，本组件通过 props sidebarCollapsed/rightCollapsed
//   在 .app 上加 .sidebar-collapsed / .right-collapsed 类（App.vue 已传 props）
// ============================================================
.app.sidebar-collapsed {
  --sidebar-w: var(--sidebar-w-collapsed, 44px);
}
.app.right-collapsed {
  --right-w: var(--right-w-collapsed, 0px);
}
// 右侧整列折叠（--right-w:0）时去边框，避免 0 宽度下残影边线。
// 绝不用 visibility:hidden —— 会因继承/scoped 选择器组合导致整页白（实测 bug）。
.app.right-collapsed > :global(.right-sidebar) {
  border-left: none;
  min-width: 0;
}

// ============================================================
// 拖拽分隔条（app.css L212-250 严格同步）
// ============================================================
.resize {
  position: absolute;
  z-index: var(--z-sticky, 200);
  background: transparent;
}
.resize-h { width: 5px; cursor: col-resize; }
.resize-v { height: 5px; cursor: row-resize; }

.resize[data-target='sidebar'] {
  left: calc(var(--sidebar-w, 260px) - 2px);
  top: var(--titlebar-h, 52px);
  bottom: var(--statusbar-h, 28px);
  width: 5px;
}
.resize[data-target='right'] {
  right: calc(var(--right-w, 280px) - 2px);
  top: var(--titlebar-h, 52px);
  bottom: var(--statusbar-h, 28px);
  width: 5px;
}
.resize[data-target='split'] {
  left: var(--sidebar-w, 260px);
  right: var(--right-w, 280px);
  height: 5px;
  top: calc(var(--titlebar-h, 52px) + var(--center-top-h, calc((100vh - 80px) / 2)) - 2px);
}

// hover/active 显色（::before 覆盖热区）
.resize::before {
  content: '';
  position: absolute;
  inset: 0;
}
.resize:hover::before,
.resize:focus-visible::before {
  background: var(--accent);
  opacity: 0.4;
}
.resize:active::before {
  background: var(--accent);
  opacity: 0.6;
}

// 折叠态隐藏对应分界
.app.sidebar-collapsed .resize[data-target='sidebar'] { display: none; }
.app.right-collapsed .resize[data-target='right'] { display: none; }
</style>
