#!/usr/bin/env node
/**
 * mcp-tools-snapshot —— MCP 工具面快照比对（A2 注册表迁移的零行为变更验收）。
 *
 * 用法：
 *   node scripts/mcp-tools-snapshot.mjs --capture HEAD   # 从 git 版本提取基线
 *   node scripts/mcp-tools-snapshot.mjs --compare        # 从工作树 registry.rs + 两侧提取比对
 *
 * 比对维度：工具名集合、description 逐字、input schema 逐字段（json! 字面量解析为 JSON 后深比）。
 */
import { execSync } from 'node:child_process';
import fs from 'node:fs';
import process from 'node:process';

const args = process.argv.slice(2);
const mode = args[0];

/** 从源文本提取 Tool::new("name", "desc", <schema>) 三元组（括号配对扫描 schema 段）。 */
function extractToolNews(src) {
  const out = [];
  const re = /Tool::new\(\s*"([^"]+)"\s*,\s*"((?:[^"\\]|\\.)*)"\s*,/g;
  let m;
  while ((m = re.exec(src))) {
    let i = re.lastIndex;
    let depth = 0;
    let end = -1;
    for (; i < src.length; i++) {
      const c = src[i];
      if (c === '(' || c === '{') depth++;
      else if (c === ')') {
        if (depth === 0) { end = i; break; }
        depth--;
      } else if (c === '}') depth--;
    }
    const schemaText = src.slice(re.lastIndex, end).trim();
    out.push({ name: m[1], description: m[2], schemaText });
  }
  return out;
}

/** 从 schema 段中提取 json!({...}) 的字面对象并解析（schema_obj(...) 包装也兼容）。 */
function parseSchema(schemaText) {
  const j = schemaText.indexOf('json!(');
  if (j === -1) return { kind: 'expr', text: schemaText }; // empty_object_schema() 等调用
  let i = j + 'json!('.length;
  let depth = 0, end = -1;
  for (; i < schemaText.length; i++) {
    const c = schemaText[i];
    if (c === '{') depth++;
    else if (c === '}') { depth--; if (depth === 0) { end = i; break; } }
  }
  const lit = schemaText.slice(j + 'json!('.length, end + 1);
  try {
    return { kind: 'json', value: JSON.parse(lit) };
  } catch (e) {
    return { kind: 'parse-error', text: lit.slice(0, 120), error: String(e) };
  }
}

/** registry.rs 形态：ToolSpec { name: "...", description: "...", schema: <fn 路径>, ... } */
function extractRegistry(src) {
  const out = [];
  const re = /name:\s*"([^"]+)"\s*,\s*description:\s*"((?:[^"\\]|\\.)*)"\s*,\s*schema:\s*([\w:]+)/g;
  let m;
  while ((m = re.exec(src))) out.push({ name: m[1], description: m[2], schemaFn: m[3] });
  return out;
}

/** 共享 schema helper 的解析结果（tools.rs 的两个既有函数，内容稳定）。 */
const KNOWN_SCHEMA_HELPERS = {
  'empty_object_schema': { type: 'object', properties: {} },
  'schema_with_required_session': {
    type: 'object',
    properties: { asset_id: { type: 'string', description: '资产 ID（先用 list_assets 查看）' } },
    required: ['asset_id']
  }
};

/** 解析 schema 引用为可比较的 JSON：自定义 schema_* fn 取函数体里的 json! 字面量；共享 helper 查表。 */
function resolveSchema(schemaFn, schemaFns) {
  const leaf = schemaFn.split('::').pop();
  if (KNOWN_SCHEMA_HELPERS[leaf]) return { kind: 'json', value: KNOWN_SCHEMA_HELPERS[leaf] };
  if (schemaFns[schemaFn]) return parseSchema(schemaFns[schemaFn]);
  return { kind: 'missing-fn', text: schemaFn };
}

/** registry.rs 里的 schema 函数体：fn schema_xxx() -> ... { json!({...})... } */
function extractSchemaFns(src) {
  const fns = {};
  const re = /fn\s+(schema_\w+)\s*\(\s*\)\s*->\s*Map\s*<\s*String\s*,\s*Value\s*>\s*\{/g;
  let m;
  while ((m = re.exec(src))) {
    let i = re.lastIndex;
    let depth = 1, end = -1;
    for (; i < src.length; i++) {
      if (src[i] === '{') depth++;
      else if (src[i] === '}') { depth--; if (depth === 0) { end = i; break; } }
    }
    fns[m[1]] = src.slice(re.lastIndex, end);
  }
  return fns;
}

const ESCAPES = { '\\"': '"', '\\\\': '\\', '\\n': '\n' };
function unescapeRust(s) {
  return s.replace(/\\["\\n]/g, x => ESCAPES[x] ?? x);
}

if (mode === '--capture') {
  const ref = args[1] || 'HEAD';
  // `worktree` 伪引用：从当前工作树提取（git ref 提取不到未提交的演进）。
  // 用途：一次有意演进完成后刷新基线，作为后续比对的起点。
  const read = p => (ref === 'worktree')
    ? fs.readFileSync(p, 'utf8')
    : execSync(`git show ${ref}:${p}`, { encoding: 'utf8' });
  const toolsRs = read('src-tauri/src/mcp/tools.rs');
  const fileToolsRs = read('src-tauri/src/mcp/file_tools.rs');
  const registrySrc = read('src-tauri/src/mcp/registry.rs');
  // v0.20（3-6 月段刀）：schema fn 拆至 registry_schemas.rs——与 compare 同口径合并
  const schemasSrc = read('src-tauri/src/mcp/registry_schemas.rs');
  const schemasMerged = registrySrc + '\n' + schemasSrc;
  // registry 形态（迁移后）：优先从注册表提取；tools/file_tools 的 Tool::new 仅迁移前存在。
  const specs = extractRegistry(registrySrc);
  let all;
  if (specs.length > 0) {
    const schemaFns = extractSchemaFns(schemasMerged);
    all = specs.map(s => ({
      name: s.name,
      description: s.description,
      schemaText: `${s.schemaFn}()`
    }));
    // schema 解析延用 --compare 的 resolveSchema：写入前先解析
    const resolved = specs.map(s => ({
      name: s.name,
      description: s.description,
      schema: resolveSchema(s.schemaFn, schemaFns)
    }));
    fs.writeFileSync('scripts/.mcp-tools-baseline.json', JSON.stringify(resolved, null, 2));
    console.log(`✔ 基线已写入 scripts/.mcp-tools-baseline.json（${resolved.length} 个工具，来源 registry@${ref}）`);
    resolved.forEach(t => console.log(`  - ${t.name} (${t.schema.kind})`));
    process.exit(0);
  }
  const legacy = [...extractToolNews(toolsRs), ...extractToolNews(fileToolsRs)];
  const snap = legacy.map(t => ({
    name: t.name,
    description: unescapeRust(t.description),
    schema: parseSchema(t.schemaText)
  }));
  fs.writeFileSync('scripts/.mcp-tools-baseline.json', JSON.stringify(snap, null, 2));
  console.log(`✔ 基线已写入 scripts/.mcp-tools-baseline.json（${snap.length} 个工具，来源 tools/file_tools@${ref}）`);
  snap.forEach(t => console.log(`  - ${t.name} (${t.schema.kind})`));
  process.exit(0);
}

if (mode === '--compare') {
  const baseline = JSON.parse(fs.readFileSync('scripts/.mcp-tools-baseline.json', 'utf8'));
  const registrySrc = fs.readFileSync('src-tauri/src/mcp/registry.rs', 'utf8');
  // v0.20（3-6 月段刀）：schema fn 已拆 registry_schemas.rs——两文件合并提取
  const schemasSrc = fs.existsSync('src-tauri/src/mcp/registry_schemas.rs')
    ? registrySrc + '\n' + fs.readFileSync('src-tauri/src/mcp/registry_schemas.rs', 'utf8')
    : registrySrc;
  const specs = extractRegistry(registrySrc);
  const schemaFns = extractSchemaFns(schemasSrc);
  // 两侧都归一为可比较 JSON：基线的 expr 形态（empty_object_schema() 等）查同一表
  const resolveBaseline = b => b.schema.kind === 'json'
    ? b.schema.value
    : KNOWN_SCHEMA_HELPERS[b.schema.text.replace(/\(\)\s*,?$/, '')] ?? { kind: b.schema.kind, text: b.schema.text };
  const current = specs.map(s => ({
    name: s.name,
    description: unescapeRust(s.description),
    schema: resolveSchema(s.schemaFn, schemaFns)
  }));

  const problems = [];
  const beforeNames = baseline.map(t => t.name);
  const afterNames = current.map(t => t.name);
  for (const n of beforeNames) if (!afterNames.includes(n)) problems.push(`工具缺失: ${n}`);
  for (const n of afterNames) if (!beforeNames.includes(n)) problems.push(`新增工具（迁移期不应有）: ${n}`);
  for (const b of baseline) {
    const a = current.find(t => t.name === b.name);
    if (!a) continue;
    if (a.description !== b.description) problems.push(`${b.name}: description 漂移`);
    const bJson = resolveBaseline(b);
    if (JSON.stringify(a.schema.value ?? a.schema) !== JSON.stringify(bJson)) {
      problems.push(`${b.name}: schema 漂移`);
    }
  }

  if (problems.length === 0) {
    console.log(`✔ 快照一致：${current.length} 个工具的名称/描述/schema 与基线逐字相同`);
    process.exit(0);
  }
  console.log(`✗ 快照不一致（${problems.length} 项）:`);
  problems.forEach(p => console.log(`  ${p}`));
  process.exit(1);
}

console.error('用法：--capture [ref] | --compare');
process.exit(2);
