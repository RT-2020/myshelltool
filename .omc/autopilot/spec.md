# Autopilot Spec — connection asset local persistence

## Source

- User request: `/oh-my-claudecode:autopilot 继续下一步` after the baseline commit/push was completed.
- Existing deep-interview topology deferred this component after the baseline: turn sample connection assets into create/edit/save/load local assets before implementing real SSH/SFTP.
- Current project state: Vite frontend, Tauri v2 shell, Rust core crate, Tauri command bridge, browser preview fallback, UI smoke tests.

## Goal

Upgrade myshelltool from static/sample connection assets to a local asset model that can list, create, edit, save, reload, and render connection assets without storing credentials or implementing real SSH/SFTP yet.

## In scope

1. Extend `myshelltool-core` with a serializable local connection asset store:
   - keep non-secret fields only: id, name, host, port, username, auth method, group, tags, status, last-connected label.
   - provide default demo assets compatible with the current UI.
   - validate required fields and port range.
   - load/save assets from JSON on disk.
   - upsert assets by id.
2. Extend Tauri commands:
   - `list_connection_assets` loads local assets from app data or returns defaults.
   - `save_connection_asset` validates/upserts one asset and persists the store.
   - preserve `backend_status` and `save_sync_settings` token safety semantics.
3. Extend browser preview fallback:
   - use `localStorage` for the same list/save contract.
   - keep token handling as status-only and clear token input after saving.
4. Update the UI:
   - render the connection tree from backend/fallback assets instead of relying only on static DOM.
   - provide a create/edit asset modal.
   - selecting/filtering assets and context panel updates continue working.
5. Extend tests:
   - Rust core tests cover validation, upsert, save/load roundtrip, and no credential fields in stored asset JSON.
   - UI smoke covers creating a local asset, seeing the count increase, filtering it, and verifying it survives reload in browser preview.

## Out of scope

- No real SSH connection execution.
- No real SFTP/SCP file transfer.
- No tunnel backend implementation.
- No password/private-key/passphrase/token persistence.
- No Git/GitHub asset sync yet.
- No commit or push unless explicitly requested later.

## Acceptance criteria

1. `list_connection_assets` returns asset metadata from local storage when present and otherwise returns default demo assets.
2. `save_connection_asset` persists a created or edited asset and `list_connection_assets` returns it on the next call.
3. Stored asset JSON contains no credential material fields such as token, password value, passphrase, private key body, or secret.
4. Browser preview can create an asset through the UI, update the connection tree count, filter/select the new asset, and retain it after page reload.
5. Existing UI smoke assertions continue passing for centered window, theme toggle, sidebar collapse/expand, backend status, tab switching, connection filtering, context updates, token local secure-storage status, and tunnel toggling.
6. Verification passes: `node --check src/main.js && node --check tests/ui-smoke.mjs`, JSON validation, `cargo metadata --manifest-path src-tauri/Cargo.toml --no-deps --format-version 1`, GitHub token prefix scan excluding ignored/dependency dirs, `npm run build`, `npm run test:core`, and `npm run test:ui`.

## Security and privacy constraints

- Do not write, echo, store, or commit any real token.
- Do not add credential value fields to the asset model.
- Token UI may show only local secure-storage status such as `已配置` / `未配置`.
- `.claude/`, `.omc/state/`, `.omc/sessions/`, `dist/`, `node_modules/`, and Rust target outputs remain excluded from commits.
- Commit messages in this project must not add Claude coauthor trailers.
