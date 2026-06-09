# Autopilot Implementation Plan — v4 centered desktop window

## Files to change

- `src/styles.css`
  - Change `.app-shell` from full-viewport padded shell to centered desktop stage.
  - Change `.window` from `height: 100%` to a finite `width: min(1180px, calc(100vw - 24px))` and `height: min(760px, calc(100vh - 24px))` on desktop.
  - Preserve responsive media queries; under mobile breakpoints, keep full-height/mobile behavior.

- `src-tauri/tauri.conf.json`
  - Set default `width` to `1180` and `height` to `760`.
  - Keep `minWidth`/`minHeight` reasonable for existing responsive layout.
  - Preserve `beforeDevCommand`, `devUrl`, and `frontendDist`.

- `tests/ui-smoke.mjs`
  - Add assertions for `.window` bounding box in the existing 1440×1000 viewport.
  - Assert width/height are near 1180×760 and the window is horizontally/vertically centered.
  - Keep all existing interaction checks.

- `.omc/progress.txt`
  - Record implementation and verification evidence.

## Execution steps

1. Update CSS window sizing and centering.
2. Update Tauri window defaults.
3. Extend UI smoke test for window dimensions.
4. Run syntax/static checks:
   - `node --check src/main.js && node --check tests/ui-smoke.mjs`
   - JSON parse validation for `.omc/prd.json`, `package.json`, `src-tauri/tauri.conf.json`
   - `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1`
   - GitHub token prefix scan excluding `node_modules`
5. Run regression:
   - `npm run build`
   - `npm run test:core`
   - `npm run test:ui`
6. Validate with architect/security/code-reviewer.
7. Clean up autopilot state on success.
