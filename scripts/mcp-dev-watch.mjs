#!/usr/bin/env node
/**
 * MCP 开发监听脚本（零依赖，纯 Node.js 内置模块）。
 *
 * 监听 MCP 相关源码变化，自动触发 cargo build --bin myshelltool-mcp。
 * 与 tauri:dev 并行运行，改 MCP 代码后自动重建 exe。
 *
 * 用法：
 *   node scripts/mcp-dev-watch.mjs          # 监听 + 首次构建
 *   node scripts/mcp-dev-watch.mjs --no-initial  # 仅监听，不首次构建
 *
 * 已知限制：GUI 进程运行时会锁住 cdylib 输出文件，导致重建失败。
 * 脚本会检测到这种情况并提示「请先重启 GUI」（Windows 文件锁机制）。
 */

import { watch, readdirSync, statSync } from 'node:fs';
import { join, extname } from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname } from 'node:path';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');
const srcTauri = join(projectRoot, 'src-tauri');

// 监听的目录（MCP 相关 + 共享核心）
const WATCH_DIRS = [
  join(srcTauri, 'src', 'mcp'),
  join(srcTauri, 'src', 'dangerous_commands.rs'),
  join(srcTauri, 'src', 'bin'),
  join(srcTauri, 'src', 'lib.rs'),
  join(srcTauri, 'src', 'ssh.rs'),
  join(projectRoot, 'crates', 'myshelltool-core', 'src'),
  join(srcTauri, 'Cargo.toml'),
];

const WATCH_EXTS = new Set(['.rs', '.toml']);

// 防抖：短时间多次保存只触发一次构建
let buildTimer = null;
let building = false;
const DEBOUNCE_MS = 800;

const GREEN = '\x1b[32m';
const YELLOW = '\x1b[33m';
const RED = '\x1b[31m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';
const DIM = '\x1b[2m';

function log(tag, msg, color = RESET) {
  const time = new Date().toLocaleTimeString('zh-CN', { hour12: false });
  console.log(`${DIM}[${time}]${RESET} ${color}[${tag}]${RESET} ${msg}`);
}

function collectFilePaths(paths) {
  const result = [];
  for (const p of paths) {
    try {
      const stat = statSync(p);
      if (stat.isFile()) {
        result.push(p);
      } else if (stat.isDirectory()) {
        walkDir(p, result);
      }
    } catch {
      // 路径不存在，跳过
    }
  }
  return result;
}

function walkDir(dir, result) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      walkDir(full, result);
    } else if (WATCH_EXTS.has(extname(entry.name))) {
      result.push(full);
    }
  }
}

function buildMcp() {
  if (building) {
    log('skip', '上一次构建仍在进行，跳过', YELLOW);
    return;
  }
  building = true;
  const startTime = Date.now();
  log('build', '开始重建 myshelltool-mcp...', CYAN);

  const child = spawn('cargo', ['build', '--manifest-path', join(srcTauri, 'Cargo.toml'), '--bin', 'myshelltool-mcp'], {
    stdio: ['ignore', 'pipe', 'pipe'],
    shell: true,
  });

  let stderr = '';
  child.stderr.on('data', (data) => {
    stderr += data.toString();
  });

  // 实时透传 cargo 输出（让用户看到编译进度）
  child.stdout.pipe(process.stdout);
  child.stderr.pipe(process.stderr);

  child.on('close', (code) => {
    building = false;
    const elapsed = ((Date.now() - startTime) / 1000).toFixed(1);

    if (code === 0) {
      log('ok', `重建成功（${elapsed}s）`, GREEN);
    } else {
      // 检测 Windows 文件锁（GUI 进程占用 cdylib）
      const isLocked = stderr.includes('os error 5') || stderr.includes('Access is denied') || stderr.includes('拒绝访问');
      if (isLocked) {
        log('warn', '构建失败：输出文件被锁定（GUI 进程正在运行）', YELLOW);
        console.log(`         ${DIM}解决方法：重启 GUI（关闭后自动重试，或手动重新保存触发）${RESET}`);
        console.log(`         ${DIM}或先停 GUI 再改 MCP 代码${RESET}`);
      } else {
        log('error', `构建失败（exit ${code}，${elapsed}s）`, RED);
      }
    }
  });
}

function scheduleBuild(reason) {
  log('change', reason, DIM);
  if (buildTimer) clearTimeout(buildTimer);
  buildTimer = setTimeout(() => {
    buildTimer = null;
    buildMcp();
  }, DEBOUNCE_MS);
}

function watchPath(p) {
  try {
    const stat = statSync(p);
    if (stat.isFile()) {
      watch(p, { persistent: true }, (eventType) => {
        if (eventType === 'change') {
          scheduleBuild(`文件变化：${p.replace(projectRoot, '.')}`);
        }
      });
    } else if (stat.isDirectory()) {
      // 递归监听目录（Node 15+ 支持 recursive）
      watch(p, { persistent: true, recursive: true }, (eventType, filename) => {
        if (!filename) return;
        const ext = extname(filename);
        if (WATCH_EXTS.has(ext)) {
          scheduleBuild(`${filename}`);
        }
      });
    }
  } catch (e) {
    log('warn', `无法监听 ${p}：${e.message}`, YELLOW);
  }
}

// ─── 主流程 ───

const noInitial = process.argv.includes('--no-initial');
const withGui = process.argv.includes('--with-gui');

console.log(`${CYAN}╔══════════════════════════════════════════════╗${RESET}`);
console.log(`${CYAN}║  myshelltool MCP 开发监听                    ║${RESET}`);
console.log(`${CYAN}╚══════════════════════════════════════════════╝${RESET}`);
console.log();

const files = collectFilePaths(WATCH_DIRS);
log('info', `监听 ${files.length} 个文件/目录：`, DIM);
WATCH_DIRS.forEach((d) => {
  console.log(`  ${DIM}${d.replace(projectRoot, '.')}${RESET}`);
});
console.log();

// 可选：同时启动 GUI（tauri dev）
let guiProcess = null;
if (withGui) {
  log('info', '启动 GUI（tauri dev）...', CYAN);
  guiProcess = spawn('npx', ['tauri', 'dev'], {
    stdio: 'inherit',
    shell: true,
    cwd: projectRoot,
  });
  guiProcess.on('close', (code) => {
    log('info', `GUI 进程退出（exit ${code}）`, DIM);
    process.exit(code ?? 0);
  });
  console.log();
}

// 首次构建
if (!noInitial) {
  log('info', '首次构建 MCP...', CYAN);
  buildMcp();
} else {
  log('info', '跳过首次构建（--no-initial）', DIM);
}

// 启动监听
WATCH_DIRS.forEach(watchPath);
log('watch', `监听已启动（防抖 ${DEBOUNCE_MS}ms）`, GREEN);
console.log();
if (withGui) {
  console.log(`${DIM}GUI + MCP watch 同时运行。改 MCP 源码自动重建。${RESET}`);
  console.log(`${DIM}若 GUI 锁住 cdylib，重启 GUI 后改一次文件即重建。${RESET}`);
} else {
  console.log(`${DIM}提示：改 MCP 相关源码后会自动重建。${RESET}`);
  console.log(`${DIM}      若 GUI 运行导致文件锁，重启 GUI 即可。${RESET}`);
}
console.log(`${DIM}      Ctrl+C 退出（会同时关闭 GUI）。${RESET}`);

// 退出时清理 GUI 子进程
process.on('SIGINT', () => {
  if (guiProcess) {
    log('info', '关闭 GUI 进程...', YELLOW);
    guiProcess.kill();
  }
  process.exit(0);
});
