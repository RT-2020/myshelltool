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
 */
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
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.shell-layout {
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
  grid-template-rows: 52px minmax(0, 1fr) minmax(0, 1fr) 28px;
  grid-template-columns: minmax(220px, 280px) minmax(0, 1fr) 280px;
  background: var(--app-bg);
  color: var(--app-text);
  font-family: var(--font-body);
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

.shell-region--statusbar {
  grid-area: statusbar;
  background: var(--app-chrome);
  border-block-start: 1px solid var(--app-border);
}
</style>
