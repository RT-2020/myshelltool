<script setup>
/**
 * AppShellLayout — 5-region CSS Grid shell.
 *
 * Layout (Tabby/Termius × FinalShell topology):
 *
 *   ┌──────────────────────────────────────────────┐
 *   │ titlebar  (52px, full width)                  │
 *   ├───────────┬───────────────────┬──────────────┤
 *   │           │  center-top       │              │
 *   │           │  (terminal)       │   right      │
 *   │  sidebar  ├───────────────────┤  (resources  │
 *   │  (assets) │  center-bottom    │   + ops)     │
 *   │           │  (files)          │              │
 *   ├───────────┴───────────────────┴──────────────┤
 *   │ statusbar  (28px, full width)                 │
 *   └──────────────────────────────────────────────┘
 *
 * Borders are single-pixel on the inline-end side only — no nested
 * card backgrounds, per Architect recommendation (avoids FinalShell
 * "card-stack" visual weight).
 *
 * Each region exposes a `data-region="<name>"` attribute for ui-smoke
 * Wave 5 selectors.
 *
 * Slots: titlebar | sidebar | center-top | center-bottom | right | statusbar
 *
 * 布局自由化（Wave）：三列宽（--sidebar-w/--right-w）与中间两行高
 * （--center-top-h）由 usePanelResize composable 拖拽控制。本组件渲染
 * 3 个拖拽 divider（绝对定位细条），pointerdown 调用传入的 startResize。
 * 折叠态（sidebar/right）隐藏对应 divider。
 */
defineProps({
  // usePanelResize 暴露的 startResize(event, which)
  startResize: { type: Function, default: () => {} },
  sidebarCollapsed: { type: Boolean, default: false },
  rightCollapsed: { type: Boolean, default: false }
});
</script>

<template>
  <div class="shell-layout" role="application" aria-label="myshelltool 主窗口">
    <header class="shell-region shell-region--titlebar" data-region="titlebar">
      <slot name="titlebar" />
    </header>

    <aside class="shell-region shell-region--sidebar" data-region="sidebar" aria-label="侧栏">
      <slot name="sidebar" />
    </aside>

    <main class="shell-region shell-region--center-top" data-region="center-top" aria-label="终端">
      <slot name="center-top" />
    </main>

    <section class="shell-region shell-region--center-bottom" data-region="center-bottom" aria-label="文件">
      <slot name="center-bottom" />
    </section>

    <aside class="shell-region shell-region--right" data-region="right" aria-label="资源监控与运维摘要">
      <slot name="right" />
    </aside>

    <footer class="shell-region shell-region--statusbar" data-region="statusbar">
      <slot name="statusbar" />
    </footer>

    <!-- 拖拽分隔条（绝对定位，浮于 grid 之上）。折叠态隐藏对应竖条。 -->
    <div
      v-if="!sidebarCollapsed"
      class="resize-divider resize-divider--col resize-divider--sidebar"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整侧栏宽度"
      @pointerdown="startResize($event, 'sidebar')"
    ></div>
    <div
      v-if="!rightCollapsed"
      class="resize-divider resize-divider--col resize-divider--right"
      role="separator"
      aria-orientation="vertical"
      aria-label="调整右侧面板宽度"
      @pointerdown="startResize($event, 'right')"
    ></div>
    <div
      class="resize-divider resize-divider--row resize-divider--center-row"
      role="separator"
      aria-orientation="horizontal"
      aria-label="调整终端与文件区高度"
      @pointerdown="startResize($event, 'center-row')"
    ></div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.shell-layout {
  position: relative; // 拖拽 divider 绝对定位的参照系
  display: grid;
  width: 100vw;
  height: 100vh;
  min-width: 1280px; // Step 5.2: below 1280px → horizontal scroll fallback
  overflow: hidden;
  grid-template-areas:
    'titlebar titlebar titlebar'
    'sidebar center-top right'
    'sidebar center-bottom right'
    'statusbar statusbar statusbar';
  // --center-top-h 控制终端区行高（默认不设 → 两行 1fr 平分；拖拽后 usePanelResize 写入 px）。
  // center-bottom 吃剩余空间（minmax 保证最小）。
  grid-template-rows: 52px var(--center-top-h, minmax(0, 1fr)) minmax(0, 1fr) 28px;
  // sidebar/right 列宽由 --sidebar-w / --right-w 控制；
  // data-assets=collapsed 时全局根覆盖 --sidebar-w 为 44px（ui.js toggleAssets 写 dataset）；
  // data-right=collapsed 时全局根覆盖 --right-w 为 0（整列折叠）。
  // 拖拽时 usePanelResize 写内联 style 同名变量；折叠态优先（composable 跳过拖拽）。
  grid-template-columns: var(--sidebar-w, minmax(220px, 280px)) minmax(0, 1fr) var(--right-w, 280px);
  background: var(--app-bg);
  color: var(--app-text);
  font-family: var(--font-body);
}

// :global 选择器：dataset 在 <html> 上，scoped 样式够不着，必须用全局选择器响应收起态列宽。
:global(:root[data-assets='collapsed']) {
  --sidebar-w: 44px;
}
:global(:root[data-right='collapsed']) {
  --right-w: 0px;
}

.shell-region {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.shell-region--titlebar {
  grid-area: titlebar;
  background: var(--app-chrome);
  border-block-end: 1px solid var(--app-border);
}

.shell-region--sidebar {
  grid-area: sidebar;
  background: var(--app-panel);
  border-inline-end: 1px solid var(--app-border);
}

.shell-region--center-top {
  grid-area: center-top;
  background: var(--app-window);
  border-block-end: 1px solid var(--app-border);
}

.shell-region--center-bottom {
  grid-area: center-bottom;
  background: var(--app-window);
}

.shell-region--right {
  grid-area: right;
  background: var(--app-panel);
  border-inline-start: 1px solid var(--app-border);
}
// 右侧整列折叠（--right-w:0）时，去掉边框避免 0 宽度下残影边线。
// 注意：绝不用 visibility:hidden —— 它会因继承/scoped-global 选择器组合导致整页白（实测 bug）。
// 0 宽列已自然裁掉内容，只需去边框 + min-width:0 防子项撑开。
:global(:root[data-right='collapsed']) .shell-region--right {
  border-inline-start: none;
  min-width: 0;
}

.shell-region--statusbar {
  grid-area: statusbar;
  background: var(--app-chrome);
  border-block-start: 1px solid var(--app-border);
}

// ============================================================
// 拖拽分隔条：绝对定位细条，浮于 grid 之上。位置由 CSS 变量驱动
// （--sidebar-w / --right-w / --center-top-h），与列/行边界对齐。
// 默认透明不可见，hover/drag 显色。z-index 高于 region 但低于 modal。
// ============================================================
.resize-divider {
  position: absolute;
  z-index: var(--z-sticky, 100);
}

// 竖向分隔条（列宽）：4px 宽，纵向覆盖中间区（titlebar 下到 statusbar 上）
.resize-divider--col {
  top: 52px;          // titlebar 高
  bottom: 28px;       // statusbar 高
  width: 5px;
  cursor: col-resize;
  // 拖拽热区：透明，hover 时显色。用 margin 扩大命中区。
  margin-inline-start: -2px;
}

// sidebar 分隔条贴在 sidebar 列右边缘
.resize-divider--sidebar {
  left: var(--sidebar-w, 280px);
}
// right 分隔条贴在 right 列左边缘
.resize-divider--right {
  right: var(--right-w, 280px);
}

// 横向分隔条（行高）：覆盖中间区全宽，贴在 center-top 底部。
// fallback 用视口中点（减去 titlebar52+statusbar28=80 后平分），与 grid 默认两行 1fr 平分一致——
// 之前用 50% 会相对包含块算偏低（落在文件区），拖拽后才跳到正确位置（初始错位 bug）。
.resize-divider--row {
  left: var(--sidebar-w, 280px);
  right: var(--right-w, 280px);
  top: calc(52px + var(--center-top-h, calc((100vh - 80px) / 2)));
  height: 5px;
  cursor: row-resize;
  margin-block-start: -2px;
}

// 默认：极淡背景（几乎不可见，提示可拖）。hover/active 显色加粗。
.resize-divider--col,
.resize-divider--row {
  background: transparent;
  transition: background var(--motion-fast, 0.12s) ease;
}
.resize-divider--col:hover,
.resize-divider--row:hover {
  background: var(--accent);
}
// 拖拽中（composable 设 body.userSelect=none，这里用 :active 兜底视觉）
.resize-divider--col:active,
.resize-divider--row:active {
  background: var(--accent);
}

// 折叠态下变量被 dataset 覆盖（44px/0），分隔条位置自动跟随；
// 折叠列的 divider 已被 v-if 隐藏，无需额外处理。
</style>
