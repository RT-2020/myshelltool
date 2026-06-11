# Autopilot Implementation Plan — SSH Terminal MVP

## Goal

Implement minimum SSH terminal connectivity: double-click a connection asset to open a live xterm.js terminal backed by a Rust SSH session via Tauri IPC.

## Architecture

- **Rust SSH**: `russh` (pure Rust, no C deps) + `tokio` for async
- **Terminal emulator**: `xterm.js` + `@xterm/addon-fit`
- **Communication**: Tauri events (Rust → JS for output) + Tauri commands (JS → Rust for input/control)
- **Auth**: Password auth from SecretStore or prompt modal
- **Browser fallback**: "SSH 需要桌面客户端" message with simulated display

## Completion status

**All items COMPLETE.**

| # | File | Status | Notes |
|---|------|--------|-------|
| 1 | `src-tauri/Cargo.toml` | Done | `russh`, `tokio`, `uuid`, `async-trait`, `serde_json` added |
| 2 | `src-tauri/src/ssh.rs` | Done | `SshSessionManager`, `SshClient` with host key verification, `ssh_connect`/`write`/`resize`/`disconnect`/`confirm_host_key` commands, known_hosts JSON management, oneshot-channel async IPC |
| 3 | `src-tauri/src/lib.rs` | Done | All SSH commands registered, session manager with known_hosts path |
| 4 | `package.json` | Done | `@xterm/xterm`, `@xterm/addon-fit` |
| 5 | `src/index.html` | Done | Dynamic tab bar, `terminalContainer` div |
| 6 | `src/main.js` | Done | Multi-session terminal, credential prompts, host key dialog, dynamic tab management |
| 7 | `src/styles.css` | Done | xterm container sizing |
| 8 | `tests/ui-smoke.mjs` | Done | Terminal fallback verification, dynamic tabs |

## Implemented features (ultragoal ssh-auth-multitab)

### G001 — Password dialog (complete)
- `openCredentialPrompt(asset, opts)` — Promise-based modal for password/passphrase input
- Checks SecretStore for existing credential before prompting
- Save-to-store option on prompt

### G002 — Public key auth (complete)
- `ssh_connect` dual auth: Password (resolve from SecretStore) vs PrivateKey (load key file + passphrase)
- `SecretStore::read()` for credential resolution
- `private_key_path` field on `ConnectionAsset`

### G003 — Multi-tab sessions (complete)
- `sessions` Map for concurrent SSH sessions
- Dynamic tab creation/removal per session
- Per-session xterm instance + `ssh-output-{sessionId}` event listener
- `switchToSession`, `removeSession` for tab management

### G004 — Host key verification (complete)
- `SshClient` with state: app handle, host_port, known_hosts_path, pending decisions
- `check_server_key`: known_hosts lookup → SHA256 fingerprint display → oneshot channel for async frontend confirmation
- `known_hosts.json`: `{ "host:port": { "key_type": "...", "key_hex": "..." } }`
- `showHostKeyDialog()` frontend: first-connection fingerprint confirmation + key-change warning
- `ssh_confirm_host_key` Tauri command

## Test evidence

- 14 core crate tests pass (`cargo test --manifest-path crates/myshelltool-core/Cargo.toml`)
- UI smoke test passes (`node tests/ui-smoke.mjs`)
- Frontend build passes (`npm run build`)
