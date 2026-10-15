#!/usr/bin/env node
/**
 * component-resolve-guard —— SFC 模板组件解析门禁
 *
 * 背景（2026-10 架构审查 P1）：v0.20 S2 拆分把 <TitleBarSearch /> 留在
 * WorkbenchShell 模板里但 import 落在了拆出文件——vue-tsc / vite build /
 * lint:size / check:ipc 四道门禁全绿，组件被 tree-shake，标题栏全局搜索
 * 自 v0.20 起静默消失并随版本发布（Vue 对未解析组件编译为运行时
 * resolveComponent 回退 + console 告警，实际渲染为空）。
 *
 * 判据：本项目无全局组件注册（main.ts 仅 createApp + use(Pinia)）、无
 * 自动导入插件（vite.config 仅 plugin-vue）——<script setup> SFC 的模板
 * 编译产物中出现 _resolveComponent("X") 即未解析组件（setup 作用域解析
 * 成功的标签会被编译为直接绑定引用，不走 resolveComponent）。
 *
 * 全量编译 src 下所有 .vue（@vue/compiler-sfc 是 vue 包自带依赖，零新增），
 * 任一命中即 fail。随 npm run build 执行 → CI 与发版自动生效（与
 * fact-guards / size-guard / ipc-contract-check 同链路）。
 */
import { parse, compileScript, compileTemplate } from '@vue/compiler-sfc';
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const SRC = path.join(ROOT, 'src');

/** 合法的全局注册组件名（app.component）。当前为零；未来引入时逐名登记。 */
const GLOBAL_COMPONENTS = [];

function* walkVueFiles(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) yield* walkVueFiles(full);
    else if (entry.isFile() && entry.name.endsWith('.vue')) yield full;
  }
}

const failures = [];
let scanned = 0;
let skipped = 0;

for (const file of walkVueFiles(SRC)) {
  const rel = path.relative(ROOT, file).replaceAll('\\', '/');
  const source = fs.readFileSync(file, 'utf8');
  const { descriptor, errors } = parse(source, { filename: file });
  if (errors.length) {
    failures.push(`${rel}: SFC 解析失败 ${errors[0].message}`);
    continue;
  }
  if (!descriptor.template) { scanned++; continue; }
  // 无 <script setup> 的 SFC：组件经 components 选项注册是 Vue 合法解析路径，
  // 本门禁不覆盖（项目规约禁新增 Options API，存量应为零——出现即打印提示）。
  if (!descriptor.scriptSetup) {
    skipped++;
    console.warn(`[component-resolve-guard] 跳过无 script setup 的 SFC：${rel}`);
    continue;
  }
  let bindingMetadata;
  try {
    bindingMetadata = compileScript(descriptor, { id: rel }).bindings;
  } catch (error) {
    failures.push(`${rel}: compileScript 失败 ${error.message}`);
    continue;
  }
  const { code, errors: templateErrors } = compileTemplate({
    id: rel,
    filename: file,
    source: descriptor.template.content,
    compilerOptions: { bindingMetadata }
  });
  if (templateErrors && templateErrors.length) {
    failures.push(`${rel}: 模板编译失败 ${templateErrors[0].message}`);
    continue;
  }
  const unresolved = [...code.matchAll(/_resolveComponent\(\s*"([^"]+)"\s*\)/g)]
    .map(match => match[1])
    .filter(name => !GLOBAL_COMPONENTS.includes(name));
  if (unresolved.length) {
    failures.push(`${rel}: 模板引用未解析组件 ${unresolved.map(n => `<${n} />`).join('、')} —— script setup 缺 import？`);
  }
  scanned++;
}

if (failures.length) {
  console.error(`\n[component-resolve-guard] ${failures.length} 个 SFC 存在未解析组件（类型/构建门禁不查此项，运行时渲染为空）：\n`);
  for (const failure of failures) console.error(`  ✗ ${failure}`);
  console.error('\n修复：在对应 SFC 的 <script setup> 补 import，或（确属全局注册场景）登记进本脚本 GLOBAL_COMPONENTS。');
  process.exit(1);
}

console.log(`[component-resolve-guard] 通过：${scanned} 个 SFC 模板组件全部可解析${skipped ? `（跳过 ${skipped} 个非 script-setup SFC）` : ''}`);
