# Autopilot Spec — v4 centered desktop window redesign

## Source

- Design file: `C:\Users\lenovo\Downloads\index-v4-before-window-redesign.md`
- User requirement: default UI should no longer occupy the full screen; it should render as a centered desktop-window proportion around `1180×760`.
- Existing project state: Vite frontend with split `src/index.html`, `src/styles.css`, `src/main.js`; Tauri shell; Rust core crate and Tauri command bridge from Phase 2.

## Goal

Update the current myshelltool prototype so the default desktop presentation is a centered, finite application window instead of a full-viewport shell, while preserving existing workbench features and backend bridge behavior.

## In scope

1. Apply the v4 window layout behavior to the split frontend files.
2. Make `.window` render centered at approximately `1180×760` on a larger desktop viewport.
3. Keep responsive behavior usable on smaller viewports by allowing the window to shrink to the available viewport and retain mobile/tablet fallbacks.
4. Update Tauri window defaults to match the new desktop ratio: width around `1180`, height around `760`, with reasonable minimum dimensions.
5. Preserve existing features:
   - theme toggle with localStorage
   - asset sidebar collapse/expand with localStorage
   - workspace tabs and tab-target shortcuts
   - connection filtering and context update
   - token modal local secure-storage semantics
   - backend bridge fallback and Tauri invoke contract
   - tunnel toggle behavior
6. Extend UI smoke coverage so the centered window dimensions and positioning are verified.

## Out of scope

- No real SSH/SFTP/tunnel backend implementation.
- No real credential persistence.
- No GitHub token or secret handling beyond existing safe UI semantics.
- No git commit or push.
- No broad redesign outside the window sizing/layout request.

## Acceptance criteria

1. `src/styles.css` centers `.window` inside `.app-shell` on desktop viewports and constrains it to approximately `1180×760` instead of full-screen height/width.
2. The layout still fits within smaller viewports using existing responsive breakpoints without causing the main window to overflow the viewport.
3. `src-tauri/tauri.conf.json` sets the default app window to approximately `1180×760` and keeps the Vite dev/build settings intact.
4. `tests/ui-smoke.mjs` asserts the desktop `.window` bounding box is approximately `1180×760` and centered in a `1440×1000` viewport.
5. Existing UI smoke assertions continue passing for theme, sidebar, backend fallback status, tab switching, connection filtering, context updates, token local secure-storage status, and tunnel toggling.
6. `npm run build`, `npm run test:ui`, `npm run test:core`, JSON validation, JS syntax check, `cargo metadata`, and GitHub token prefix scan pass.

## Security and privacy constraints

- Do not write, echo, or commit any real token.
- Token UI may show only local secure-storage status such as `已配置` / `未配置`.
- Exclude `.claude/` from any commit recommendation.
