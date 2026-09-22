#!/usr/bin/env node
/**
 * ipc-contract-check —— IPC 契约清单级一致性门禁
 *
 * 背景：78 个 IPC 命令 + 12 类事件的两侧契约靠手工镜像（src/types/domain.ts、
 * 各处 listenBackendEvent 字面量），重命名/漏注册类错误全部延迟到运行时。
 * 本脚本做三个事实源的对账（清单级，不校验字段形状——字段级 codegen 是
 * 另一项独立评估，见架构评审 S4）：
 *
 *   ① Rust 命令注册表（lib.rs generate_handler!）× 前端 invokeBackend 调用面
 *      - 前端调了没注册的命令 → 失败（typo / 漏注册，运行时必报 command not found）
 *      - 注册了但前端从未调用 → 失败（死命令；确属保留的登记到 REGISTERED_NOT_INVOKED_OK）
 *   ② Rust emit 事件面 × 前端监听面（listenBackendEvent / 包装器登记的常量表）
 *      - 前端监听了后端从未 emit 的事件 → 失败（拼写错误 = 永远等不到）
 *      - 后端 emit 了前端无人监听的事件 → 失败（死事件；存量缺口登记 UNLISTENED_OK）
 *   ③ 前端内部广播（emitBackendEvent）必须有监听者（否则是死广播）
 *
 * 动态事件名按前缀归一：Rust `format!("ssh-output-{id}")` 与前端
 * `'ssh-output-' + id` / 模板串都归一为 `ssh-output-*` 后比对。
 *
 * 随 `npm run build` 执行 → CI 与发版自动生效（与 fact-guards / size-guard 同链路）。
 * 用法：node scripts/ipc-contract-check.mjs [--self-test]
 */
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));

/** 后端 emit 了但前端刻意不监听的事件（存量缺口，每条必须给出去向）。 */
const UNLISTENED_OK = {
  'tunnel-traffic-*': '隧道流量事件：AGENTS.md §9 已文档化的存量缺口，异步失败靠 tunnel_list 的 active/error 字段呈现；接线或删除是独立任务',
  'tunnel-error-*': '同上（tunnel.rs 隧道错误事件）',
  'resource-monitor-stopped': '后端清理会话后的停止信号，前端当前靠快照超时自然呈现；接线或删除 emit 待评估'
};

/** 注册了但前端不直接调用的命令（每条必须给出理由；新命令一律应有前端调用点）。 */
const REGISTERED_NOT_INVOKED_OK = {
  sftp_read_file: '裸读命令：GUI 编辑器链路走 sftp_read_text（编码白名单），裸读保留给 MCP/未来预览；疑似死命令，待下个迭代确认后删除或接线',
  sftp_write_file: '裸写命令：同上（GUI 走 sftp_write_text 的 expected 守护 + 原子写）',
  sftp_upload_cancel: 'transferQueueOps 的 cancelCmd 按方向字面量分发（三元给 grep 两形态）',
  sftp_download_cancel: '同上（S9 下载取消，共用 transfer_cancels 旗标通道）'
};

/** 用变量包装 listenBackendEvent 的文件：该文件内声明的事件形态常量（kebab-case 字符串）全部视为已监听。 */
const WRAPPER_LISTENER_FILES = ['src/lib/sessionHandoff.ts'];
// transferQueueOps 的 cancelCmd 按方向选命令（上传/下载共用旗标通道 S9）——
// 静态字面量在该文件 grep 两形态后豁免动态判定

function toRel(abs) {
  return path.relative(ROOT, abs).split(path.sep).join('/');
}

function walk(dir, exts, out) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, exts, out);
    else if (exts.has(path.extname(entry.name))) out.push(full);
  }
}

/** 动态名归一：`format!("ssh-output-{id}")` 的模板段 / 前端拼接与模板串 → `ssh-output-*`。 */
export function toPattern(raw) {
  const brace = raw.indexOf('{');
  if (brace >= 0) return raw.slice(0, brace) + '*';
  return raw;
}

export function eventMatches(a, b) {
  if (a === b) return true;
  if (a.endsWith('*') && b.startsWith(a.slice(0, -1))) return true;
  if (b.endsWith('*') && a.startsWith(b.slice(0, -1))) return true;
  return false;
}

/** 从 emit/监听调用的首个参数片段中提取事件名（字面量 / format! 模板 / 标识符）。 */
export function parseEventArg(chunk) {
  const fmt = chunk.match(/format!\(\s*"([^"]*)"/);
  if (fmt) return { kind: 'value', value: toPattern(fmt[1]) };
  const tpl = chunk.match(/`([^`]*)`/);
  if (tpl) return { kind: 'value', value: toPattern(tpl[1].replace(/\$\{[^}]*\}/g, '{').replace(/\{+/g, '{')) };
  const lit = chunk.match(/^[\s&]*['"]([^'"]+)['"]\s*(\+)?/);
  if (lit) return { kind: 'value', value: lit[2] ? lit[1] + '*' : lit[1] };
  const ident = chunk.match(/^[\s&]*(?:\w+\(\s*)?([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)?)/);
  if (ident) return { kind: 'ident', value: ident[1] };
  return { kind: 'unknown', value: chunk.trim().slice(0, 40) };
}

// ─── Rust 侧提取 ───

function rustSources() {
  const files = [];
  walk(path.join(ROOT, 'src-tauri/src'), new Set(['.rs']), files);
  return files.map(abs => ({ rel: toRel(abs), content: fs.readFileSync(abs, 'utf8') }));
}

function rustCommands(libRs) {
  const m = libRs.match(/generate_handler!\[([\s\S]*?)\]\s*\)/);
  if (!m) return null;
  return m[1]
    .split(/\r?\n/) // CRLF 兼容：先行尾 \r 会让 /\/\/.*$/ 的 $ 锚点失效（注释剥离静默不生效）
    .map(l => l.replace(/\/\/[^\r\n]*/, '')) // 去行注释（注释里的逗号/标识符不执行语义）
    .join('\n')
    .split(',')
    .map(s => s.trim())
    .filter(s => /^[a-z_][a-z0-9_:]*$/.test(s)) // 允许 ssh::ssh_connect 路径形态
    .map(s => s.split('::').pop());
}

function rustEvents(files) {
  // 全局常量表：const X_EVENT: &str = "...";（含 pub）
  const consts = {};
  for (const { content } of files) {
    for (const m of content.matchAll(/(?:pub\s+)?const\s+([A-Za-z_]\w*)\s*:\s*&str\s*=\s*"([^"]+)"/g)) {
      consts[m[1]] = m[2];
    }
  }
  const events = new Map(); // event → 出处
  for (const { rel, content } of files) {
    // 文件内 let 绑定：let x = format!("ssh-output-{id}");
    const lets = {};
    for (const m of content.matchAll(/let\s+(\w+)\s*=\s*format!\(\s*"([^"]*)"/g)) {
      lets[m[1]] = toPattern(m[2]);
    }
    // 去行注释后的文本用于 emit 扫描（事件名字面量不含 //，误伤可接受）；
    // [^\r\n]* 而非 .*$ —— CRLF 行尾下 $ 锚点不匹配 \r 前位置，注释会剥离失败
    const stripped = content.split(/\r?\n/).map(l => l.replace(/\/\/[^\r\n]*/, '')).join('\n');
    for (const m of stripped.matchAll(/\.emit(?:_all)?\(\s*([\s\S]{0,160}?)[,)]/g)) {
      const parsed = parseEventArg(m[1]);
      let name = null;
      if (parsed.kind === 'value') name = parsed.value;
      else if (parsed.kind === 'ident') name = lets[parsed.value] ?? consts[parsed.value] ?? null;
      if (name) events.set(name, rel);
      else events.set(`<未解析:${parsed.value}@${rel}>`, rel);
    }
  }
  return events;
}

// ─── 前端侧提取 ───

function frontendSources() {
  const files = [];
  walk(path.join(ROOT, 'src'), new Set(['.ts', '.vue']), files);
  return files
    .map(abs => ({ rel: toRel(abs), content: fs.readFileSync(abs, 'utf8') }))
    .filter(f => !f.rel.startsWith('src/generated/'));
}

// 泛型前缀可嵌套一层（invokeBackend<Record<string, unknown> | null>(...)），[^>]* 会在内层 > 处截断
const GENERIC = String.raw`(?:<(?:[^<>]|<[^<>]*>)*>)?`;

function frontendInvocations(files) {
  const invoked = new Map();
  const dynamic = [];
  for (const { rel, content } of files) {
    // 字面量命令名（含无参调用，不含 backend.ts 里的函数定义——定义的首参是 `command: string`，无引号不匹配）
    const literalRe = new RegExp('invokeBackend' + GENERIC + '\\(\\s*[\'"`]([a-z_][a-z0-9_:|]*)[\'"`]', 'g');
    for (const m of content.matchAll(literalRe)) {
      invoked.set(m[1], rel);
    }
    // 动态命令名（首参不是引号开头，且不是函数定义的形参声明）
    const dynamicRe = new RegExp('invokeBackend' + GENERIC + '\\(\\s*([^\'"`\\s)][^,)]{0,60})', 'g');
    for (const m of content.matchAll(dynamicRe)) {
      const arg = m[1].trim();
      if (/^\w+\s*:/.test(arg)) continue; // backend.ts 的函数定义（command: string）
      if (/cancelCmd/.test(arg)) continue; // S9：方向字面量分发的取消命令
    }
  }
  return { invoked, dynamic };
}

function frontendEvents(files) {
  // 常量表只收事件形态标识符（名字含 EVENT）——全量收会把状态标签/颜色映射表
  // 等任意对象常量误当事件名（事故：'已连接'/'#f5c518' 曾被判成监听事件）。
  const singles = {}; // 全局，供跨文件标识符解析
  const objects = {}; // 全局，供 HANDOFF_EVENTS.X 形态解析
  const localEventNames = {}; // 每文件自己声明的事件名集合（包装器回退用）
  for (const { rel, content } of files) {
    const local = new Set();
    for (const m of content.matchAll(/const\s+([A-Za-z_]\w*EVENT\w*)\s*=\s*'([^']+)'/g)) {
      singles[m[1]] = m[2];
      local.add(m[2]);
    }
    for (const m of content.matchAll(/const\s+([A-Za-z_]\w*EVENTS\w*)\s*=\s*\{([\s\S]*?)\};/g)) {
      const values = [...m[2].matchAll(/:\s*'([^']+)'/g)].map(v => v[1]);
      objects[m[1]] = values;
      for (const v of values) local.add(v);
    }
    localEventNames[rel] = [...local];
  }
  const resolveIdent = ident => {
    if (ident.includes('.')) return objects[ident.split('.')[0]] ?? null;
    return singles[ident] ?? null;
  };

  const listened = new Map();
  const emitted = new Map();
  const unresolved = [];
  for (const { rel, content } of files) {
    const collect = (regex, into, label) => {
      for (const m of content.matchAll(regex)) {
        const chunk = m[1];
        if (/^\s*\w+\s*:/.test(chunk)) continue; // backend.ts 的函数定义（eventName: string）
        const parsed = parseEventArg(chunk);
        let names = [];
        if (parsed.kind === 'value') names = [parsed.value];
        else if (parsed.kind === 'ident') {
          const r = resolveIdent(parsed.value);
          if (typeof r === 'string') names = [r];
          else if (Array.isArray(r)) names = r;
        }
        if (names.length === 0) {
          if (WRAPPER_LISTENER_FILES.includes(rel)) {
            // 包装器（sessionHandoff 的 on() 间接层）：该文件自己声明的事件常量全部计入
            names = localEventNames[rel] ?? [];
          }
          if (names.length === 0) {
            unresolved.push(`${rel}: ${label}(${parsed.value}) 无法静态解析——改字面量/常量，或登记 WRAPPER_LISTENER_FILES`);
          }
        }
        for (const n of names) into.set(n, rel);
      }
    };
    collect(/listenBackendEvent(?:<[^>]*>)?\(\s*([\s\S]{0,100}?),/g, listened, 'listenBackendEvent');
    collect(/emitBackendEvent(?:<[^>]*>)?\(\s*([\s\S]{0,100}?),/g, emitted, 'emitBackendEvent');
  }
  return { listened, emitted, unresolved };
}

// ─── 对账 ───

function diff(mine, theirs, okTable) {
  const missing = [];
  for (const [name, origin] of mine) {
    if ([...theirs.keys()].some(t => eventMatches(name, t))) continue;
    if (Object.keys(okTable).some(p => eventMatches(name, p))) continue;
    missing.push({ name, origin });
  }
  return missing;
}

function scan() {
  const rustFiles = rustSources();
  const libRs = fs.readFileSync(path.join(ROOT, 'src-tauri/src/lib.rs'), 'utf8');
  const feFiles = frontendSources();
  if (rustFiles.length === 0 || feFiles.length === 0) {
    console.error('✗ ipc-contract-check：源码目录扫描为空——目录改名？门禁拒绝在空集合上宣称通过。');
    return 1;
  }

  const commands = rustCommands(libRs);
  if (!commands || commands.length === 0) {
    console.error('✗ ipc-contract-check：未能从 lib.rs 提取 generate_handler! 命令清单——注册宏形态变了？');
    return 1;
  }
  const registered = new Map(commands.map(c => [c, 'src-tauri/src/lib.rs']));
  const { invoked, dynamic } = frontendInvocations(feFiles);
  const rustEmitted = rustEvents(rustFiles);
  const { listened, emitted: feEmitted, unresolved } = frontendEvents(feFiles);

  const problems = [];
  // ① 命令
  for (const { name, origin } of diff(
    new Map([...invoked].filter(([n]) => !n.startsWith('plugin:'))),
    registered,
    {}
  )) problems.push(`前端调用了未注册的命令 "${name}"（${origin}）——typo 或漏注册 generate_handler!`);
  for (const { name } of diff(registered, new Map([...invoked].filter(([n]) => !n.startsWith('plugin:'))), REGISTERED_NOT_INVOKED_OK)) {
    problems.push(`命令 "${name}" 已注册但前端从未调用——死命令，或登记 REGISTERED_NOT_INVOKED_OK 并写明理由`);
  }
  // ② 后端事件
  for (const { name, origin } of diff(rustEmitted, listened, UNLISTENED_OK)) {
    problems.push(`后端 emit 的事件 "${name}"（${origin}）前端无人监听——死事件，或登记 UNLISTENED_OK 写明去向`);
  }
  for (const { name, origin } of diff(listened, rustEmitted, {})) {
    if ([...feEmitted.keys()].some(f => eventMatches(name, f))) continue; // 前端内部广播
    problems.push(`前端监听的事件 "${name}"（${origin}）后端从未 emit——拼写错误 = 永远等不到`);
  }
  // ③ 前端内部广播
  for (const { name, origin } of diff(feEmitted, listened, {})) {
    problems.push(`前端 emitBackendEvent("${name}")（${origin}）无人监听——死广播`);
  }
  for (const d of dynamic) problems.push(`动态命令名调用（清单级门禁无法覆盖，请改字面量）：${d}`);
  for (const u of unresolved) problems.push(u);

  console.log(`· 命令：注册 ${registered.size} / 前端调用 ${invoked.size}（含 plugin:* ${[...invoked.keys()].filter(n => n.startsWith('plugin:')).length} 个）`);
  console.log(`· 事件：后端 emit ${rustEmitted.size} / 前端监听 ${listened.size} / 前端内部广播 ${feEmitted.size}`);
  const okNotes = [
    ...Object.entries(UNLISTENED_OK).map(([p, why]) => `  事件 ${p} —— ${why}`),
    ...Object.entries(REGISTERED_NOT_INVOKED_OK).map(([p, why]) => `  命令 ${p} —— ${why}`)
  ];
  if (okNotes.length > 0) console.log(`ℹ 登记豁免（review 请留意）：\n${okNotes.join('\n')}`);

  if (problems.length === 0) {
    console.log('✔ ipc-contract-check：命令与事件两侧一致');
    return 0;
  }
  console.log(`\n✗ ipc-contract-check：${problems.length} 处不一致\n`);
  for (const p of problems) console.log(`  ${p}`);
  return 1;
}

function selfTest() {
  let failed = 0;
  const check = (desc, actual, expected) => {
    if (JSON.stringify(actual) !== JSON.stringify(expected)) {
      console.log(`  ✗ 自检失败：${desc}——期望 ${JSON.stringify(expected)}，实际 ${JSON.stringify(actual)}`);
      failed += 1;
    }
  };
  check('format! 模板归一', toPattern('ssh-output-{session_id}'), 'ssh-output-*');
  check('字面量保持', toPattern('mcp-tool-approval'), 'mcp-tool-approval');
  check('模式匹配字面量', eventMatches('ssh-output-*', 'ssh-output-abc'), true);
  check('字面量匹配模式', eventMatches('ssh-output-abc', 'ssh-output-*'), true);
  check('不同事件不匹配', eventMatches('ssh-output-*', 'ssh-closed-abc'), false);
  check('emit 字面量参数', parseEventArg('"mcp-tool-approval", payload'), { kind: 'value', value: 'mcp-tool-approval' });
  check('emit format! 参数', parseEventArg('&format!("tunnel-traffic-{tid}"), x'), { kind: 'value', value: 'tunnel-traffic-*' });
  check('前端拼接参数', parseEventArg("'ssh-output-' + realSessionId"), { kind: 'value', value: 'ssh-output-*' });
  check('常量标识符参数', parseEventArg('SESSION_STATUS_EVENT, event'), { kind: 'ident', value: 'SESSION_STATUS_EVENT' });
  return failed;
}

console.log('ipc-contract-check —— IPC 契约清单级一致性门禁\n');
const failures = selfTest();
if (failures > 0) {
  console.log(`\n提取/比对逻辑自检失败 ${failures} 项——请先修 scripts/ipc-contract-check.mjs`);
  process.exit(1);
}
console.log('✔ 提取/比对逻辑自检通过\n');
if (!process.argv.includes('--self-test')) process.exit(scan());
