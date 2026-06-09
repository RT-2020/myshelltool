# Autopilot Implementation Plan — connection asset local persistence

## Files to change

- `crates/myshelltool-core/Cargo.toml`
  - Move `serde_json` into runtime dependencies for asset store JSON load/save.

- `crates/myshelltool-core/src/lib.rs`
  - Extend `ConnectionAsset` with grouping/tags/status display metadata.
  - Add `ConnectionAssetStore` JSON document.
  - Add validation, default assets, upsert, load, and save helpers.
  - Add unit tests for validation, upsert, roundtrip persistence, and secret-field exclusion.

- `src-tauri/src/lib.rs`
  - Store an app-data JSON path during setup.
  - Load assets from that path in `list_connection_assets`.
  - Add `save_connection_asset` command that persists one upserted asset.

- `src/index.html`
  - Route the sidebar add button and context-menu edit button to the asset editor modal.
  - Keep static markup as initial/no-JS fallback only.

- `src/main.js`
  - Add default browser-preview assets and localStorage persistence.
  - Render the connection tree from returned assets.
  - Add create/edit asset modal and save flow.
  - Keep token flow status-only and non-persistent.

- `tests/ui-smoke.mjs`
  - Clear browser preview storage at test start for deterministic assertions.
  - Verify default asset count.
  - Create a new connection asset, verify count/filter/context update, reload, and verify persistence.

- `.omc/progress.txt`
  - Record implementation and verification evidence.

## Execution steps

1. Implement Rust core asset model and tests.
2. Wire Tauri local asset store commands.
3. Update browser fallback and dynamic UI rendering.
4. Extend UI smoke coverage.
5. Run syntax/static checks:
   - `node --check src/main.js && node --check tests/ui-smoke.mjs`
   - JSON parse validation for `.omc/prd.json`, `package.json`, `src-tauri/tauri.conf.json`
   - `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1`
   - GitHub token prefix scan excluding dependency/build/state dirs
6. Run regression:
   - `npm run build`
   - `npm run test:core`
   - `npm run test:ui`
7. Validate with architect, security-reviewer, and code-reviewer.
8. Clean up OMC active state on success.
