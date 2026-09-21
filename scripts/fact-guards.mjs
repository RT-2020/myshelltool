#!/usr/bin/env node
/**
 * fact-guards —— 「靠猜测代替事实 / 硬编码」机械门禁（docs/llm-engineering-guidelines.md §7）
 *
 * 文档是提醒，门禁才是约束：每条规则都对应一次真实事故，规则即该事故的回归防线。
 * 随 `npm run build` 执行 → CI（.github/workflows/ci.yml 跑 npm run build）与发版
 * （tauri beforeBuildCommand = npm run build）自动生效，违规无法合入。
 *
 * 用法：
 *   node scripts/fact-guards.mjs              扫描 src / src-tauri / core / tests / scripts
 *   node scripts/fact-guards.mjs --self-test  只跑规则自检（正反样例）
 *
 * 豁免语法：行尾注释 `// fact-guard:allow <rule-id> <理由≥8字符>`。rule-id 必须与
 * 该行命中的规则一致、理由去空白后 ≥8 字符、且豁免标记必须写在注释里（字符串
 * 字面量内的 allow 标记无效）。每条规则的豁免数量会在输出中统计，供 review 留意。
 * 增长策略（规则先行）：每出现一次新的同类事故，先在本文件加规则 + 正反样例，
 * 再改代码修复——保证同类错误第二次出现会被机器拦下。
 */
import fs from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const TARGETS = {
  app: {
    roots: ['src'],
    exts: new Set(['.ts', '.tsx', '.vue', '.js', '.jsx', '.mjs', '.mts', '.cts'])
  },
  rust: {
    roots: ['src-tauri/src', 'crates/myshelltool-core/src'],
    exts: new Set(['.rs'])
  },
  tests: { roots: ['tests'], exts: new Set(['.mjs', '.js', '.ts']) },
  // 样式文件单独一个 target：现有规则多为代码形态，不想因扩展名扩容让旧规则意外
  // 扫到样式文本；样式专属规则（no-double-easing-animation-shorthand）按需挂 styles。
  styles: { roots: ['src'], exts: new Set(['.css', '.scss']) },
  // 门禁自身也守规矩。唯一例外是本文件：正反样例字符串本身就是「故意违规」的
  // 代码文本，扫自己必然自伤；样例的正确性由 --self-test 保障，故排除自身。
  scripts: {
    roots: ['scripts'],
    exts: new Set(['.mjs', '.js', '.ts']),
    exclude: ['scripts/fact-guards.mjs']
  },
  // build.rs 位于 src-tauri 根目录（不在 src-tauri/src 下），单文件 target。
  buildRs: { files: ['src-tauri/build.rs'] }
};

/**
 * 豁免判定（收紧版）：`// fact-guard:allow <rule-id> <理由>`。
 * - 必须出现在注释内（// # /* <!-- 之后）——行内字符串里的 allow 标记不算豁免；
 * - rule-id 必须与该行命中的规则一致（防止从别处复制豁免后静默放行错规则）；
 * - 理由去空白后长度 ≥8（单字符/空理由不算）。
 */
function allowFor(line, ruleId) {
  const m = line.match(/(?:\/\/|#|\/\*|<!--)\s*fact-guard:allow\s+(\S+)\s*(.*)$/);
  if (!m || m[1] !== ruleId) return false;
  return m[2].trim().replace(/\s+/g, '').length >= 8;
}

const RULES = [
  {
    id: 'no-guessed-home-path',
    title: '远程家目录必须由服务器给出，禁止拼接猜测路径',
    fix: '空 path 交给后端 SFTP canonicalize(".") 解析（指南 §7 形态 A）。事故：/home/root 不存在导致目录加载失败。',
    targets: ['app'],
    pattern: /["'`]\/home\//,
    samples: {
      bad: [
        "const p = '/home/' + asset.username;",
        'const p = `/home/${asset.username}`;'
      ],
      good: ['const p = await invokeBackend("sftp_list_dir", { path: "" });']
    }
  },
  {
    id: 'no-drive-hardcoded-system-path',
    title: '禁止硬编码盘符系统路径（黑名单必须覆盖任意盘符）',
    fix: '按 <盘符>:\\windows\\ 等前缀规则匹配任意盘符，而非字面 c:\\windows。事故：系统装 D 盘时保护失守。',
    targets: ['app', 'rust'],
    pattern: /["'`][a-zA-Z]:[\\/]{1,2}(windows|program files|programdata)/i,
    // Rust 单测里的反例路径（assert!(is_sensitive_path("D:/Windows/..."))）是合法用例
    skipLine: /\bassert/,
    samples: {
      bad: [
        'if (p.startsWith("C:\\\\Windows")) return true;',
        'if (p.startsWith(`D:\\\\ProgramData`)) return true;'
      ],
      good: [
        'const SYSTEM_DIRS = ["windows", "program files", "programdata"];',
        'assert!(is_sensitive_path(Path::new("D:/Windows/System32")));'
      ]
    }
  },
  {
    id: 'no-gnu-only-flags',
    title: '禁止 GNU 专有命令选项（BusyBox/BSD 上必挂）',
    fix: 'find -printf → 走 SFTP read_dir；df -B1 → df -k。事故：Alpine/BusyBox 上目录列表整条失败。',
    targets: ['rust'],
    // 引号锚定：解释性注释（含反引号示例）不误报
    pattern: /["'][^"'\n]*(?:-printf|-B1)\b/,
    samples: {
      bad: ['let cmd = format!("find {} -maxdepth 1 -printf %f", q);'],
      good: ['// 曾用 find -printf（GNU-only）已弃用', 'let cmd = "LC_ALL=C df -P -k /";']
    }
  },
  {
    id: 'no-blocking-sleep',
    title: '禁止固定 sleep 充当就绪信号/重试等待（时序猜测）',
    fix: '用协议信号（握手/输出到达）或带超时的真实探测；重试用退避调度器（指南 §7 形态 D）。tokio 的固定 sleep 与 thread::sleep 同罪。',
    targets: ['rust'],
    pattern: /\b(?:thread|tokio::time)::sleep\s*\(/,
    samples: {
      bad: [
        'std::thread::sleep(Duration::from_millis(300));',
        'tokio::time::sleep(Duration::from_millis(300)).await;'
      ],
      good: ['tokio::time::timeout(Duration::from_secs(2), fut).await']
    }
  },
  {
    id: 'locale-pinned-df',
    title: '解析 df 输出必须同行锁 LC_ALL=C（表头随 locale 变化）',
    fix: '命令串加 LC_ALL=C 前缀（如 "LC_ALL=C df -P -k /" 或 "export LC_ALL=C; ..."）。事故：中文 locale 下按英文表头切分，磁盘统计静默归零。',
    targets: ['rust'],
    // 命令起始锚定（行首/引号后/分号管道后）：说明文案（"…（执行 df -h）"）与
    // 注释（`// … (df -P 首行)`）不误报；单测样例数据用 fact-guard:allow 豁免。
    pattern: /(?:^|["'`;|&])\s*df\s+-[a-zA-Z0-9]/,
    unless: /LC_ALL=C/,
    skipLine: /\bassert/,
    samples: {
      bad: [
        'pub const CMD_DISK_USAGE: &str = "df -h";',
        '"uptime; free -m; df -h",'
      ],
      good: [
        'let cmd = "LC_ALL=C df -P -k /";',
        '"查询指定资产的磁盘使用情况（执行 df -h）",',
        '// df anchor: the POSIX header "Filesystem" (df -P 输出首行).'
      ]
    }
  },
  {
    id: 'no-host-only-identity',
    title: '实体身份必须用完整键（username+host+port），部分键=碰撞',
    fix: 'slugify(`${username}-${host}`)（端口非 22 时再入 id）。事故：同主机不同用户名互相覆盖资产且沿用错误凭据。',
    targets: ['app'],
    pattern: /slugify\(\s*[A-Za-z_.]*(?:\?\.)?[A-Za-z_.]*host\s*\)/,
    samples: {
      bad: [
        'const tempId = slugify(suggestion.host);',
        'const tempId = slugify(asset?.host);'
      ],
      good: [
        "const base = slugify(name || host || 'asset');",
        "const id = slugify(item?.name || item?.host || 'asset');"
      ]
    }
  },
  {
    id: 'no-fixed-wait-in-tests',
    title: 'UI 测试禁止固定毫秒等待（flaky 根源：快机器断言早于渲染、慢机器超时）',
    fix: '用 page.waitForFunction / waitForSelector 等待可观测状态到达（DOM 属性、store 公开状态、mock 调用计数），而非赌固定毫秒。事故：waitForTimeout(500) 后断言 invoke 次数，慢 CI 上必 flaky。',
    targets: ['tests'],
    pattern: /waitForTimeout\s*\(/,
    samples: {
      bad: ['await page.waitForTimeout(500);'],
      good: [
        "await page.waitForFunction(() => document.documentElement.dataset.theme === 'dark');"
      ]
    }
  },
  {
    id: 'no-fixed-wait-frontend',
    title: '前端禁止裸固定延迟充当就绪信号（时序猜测，快机器白等、慢机器不够）',
    fix: '优先等可观测信号（事件监听 / store 状态轮询带 deadline / rAF）；确属「探测-等待-再探测」节流（每次等待前后都校验真实状态）的存量写法，行尾加 `fact-guard:allow no-fixed-wait-frontend 理由` 显式辩护。与 no-blocking-sleep（Rust）/ no-fixed-wait-in-tests（测试）补齐三端覆盖。',
    targets: ['app'],
    pattern: /await\s+new\s+Promise\(\s*\w+\s*=>\s*setTimeout\s*\(/,
    samples: {
      bad: ['await new Promise(r => setTimeout(r, 500)); // 赌 500ms 后弹窗已挂载'],
      good: [
        "await listenBackendEvent('ssh-session-status', handler); // 等事件信号",
        'await new Promise(resolve => { emitter.once("ready", resolve); });'
      ]
    }
  },
  {
    id: 'no-csh-hostile-env-prefix',
    title: '命令串禁止 `VAR=值 命令` / `export VAR=值` 形态（csh/tcsh/fish 下不是赋值，整条命令失败）',
    fix: '改用 `env LC_ALL=C <命令>`（POSIX，BusyBox/BSD 自带，两种 shell 语义一致）。事故：FreeBSD（root 默认 /bin/csh）上 `LC_ALL=C df -h` 报 "LC_ALL=C: Command not found."，df 根本没运行、工具完全取不到数据。',
    targets: ['rust'],
    // 段起始锚定（行首/引号后/分号管道后）：同段已有 `env LC_ALL=C` 时不命中；
    // 注释行由 judgeLine 统一跳过（文档里引用旧写法不误报）。
    pattern: /(?:^|["'`;|&])\s*(?:(?:LC_ALL|LANG|LC_NUMERIC|TZ)=|export\s+(?:LC_ALL|LANG|LC_NUMERIC|TZ)=)/,
    unless: /env\s+(?:LC_ALL|LANG|LC_NUMERIC|TZ)=/,
    skipLine: /\bassert/,
    samples: {
      bad: [
        'pub const CMD_DISK_USAGE: &str = "LC_ALL=C df -h; echo rc_df=$?";',
        'let x = "export LC_ALL=C; uptime";'
      ],
      good: [
        'pub const CMD_DISK_USAGE: &str = "env LC_ALL=C df -h; echo rc_df=$?";',
        '/// Parse `LC_ALL=C df -P -k /` output (comment)'
      ]
    }
  },
  {
    id: 'no-pipeline-rc-echo',
    title: '管道后紧跟 echo rc_*=$? 的「逐段退出码」是假的（rc 恒为管道末端命令的码）',
    fix: '先把该段落到临时文件再截断：`__t=$(mktemp); cmd >"$__t" 2>&1; echo rc_x=$?; head -N "$__t"`（POSIX 无 PIPESTATUS，pipefail 又是 bash 专有）。事故：`top -bn1 | head -20; echo rc_top=$?` 在无 procps 的容器上 rc_top=0，把「top 不存在」伪装成该段成功。',
    targets: ['rust'],
    pattern: /["'`][^"'`\n]{0,200}?\|[^"'`\n]{0,200}?echo\s+rc_\w+=\$\?/,
    samples: {
      bad: ['pub const CMD: &str = "top -bn1 | head -20; echo rc_top=$?";'],
      good: [
        'pub const CMD: &str = "uptime; echo rc_uptime=$?";',
        'let c = "__t=$(mktemp); top -bn1 >\\"$__t\\"; echo rc_top=$?; head -20 \\"$__t\\"";'
      ]
    }
  },
  {
    id: 'no-fake-type-cast-call',
    title: '禁止「把对象断言成它没有的形状再调用方法」（类型系统被谎报绕过，运行时必炸）',
    fix: '调用真实存在的方法：查 store/模块是否已导出，没有就在归属层补上（跨 store 走 lazy bridge，见 AGENTS.md §4.2）。事故：三处 SyncPanel 组件写 `(store as unknown as { listAssets }).listAssets()`，而 workbench 从未导出 listAssets——「刷新资产列表」这条唯一路径每次必抛 TypeError 且被异步处理器吞掉，pull 成功后列表永不更新。',
    targets: ['app'],
    // 只拦「断言后调用方法」；属性读取（(x as unknown as { value?: string }).value）是合法收窄，不拦
    pattern: /as\s+unknown\s+as\s*\{[^}]*\}\s*\)?\s*\.\s*\w+\s*\(/,
    samples: {
      bad: [
        'await (store as unknown as { listAssets: () => Promise<unknown> }).listAssets();'
      ],
      good: [
        'const v = (effectiveTheme() as unknown as { value?: string }).value;',
        'await store.reloadAssets();'
      ]
    }
  },
  {
    id: 'no-empty-catch-block',
    title: '空 catch 块必须写明「为何可忽略」（否则错误被折叠成「什么也没发生」）',
    fix: '补一行理由注释：`catch { /* xterm 销毁阶段 runtime 可能已不可用 */ }`，或改为可见反馈（announce / 错误态 / 日志）。事故：跨窗口 ACK、传输失败、持久化失败都曾长在「静默 catch」这个习惯上。',
    targets: ['app'],
    // 行内空块；跨行空块（catch { 换行 + 注释 + }）不误伤——带注释的写法是本规则要求的形态
    pattern: /catch\s*(?:\([^)]*\))?\s*\{\s*\}\s*$/,
    samples: {
      bad: ['try { session.term.dispose(); } catch {}'],
      good: [
        'try { session.term.dispose(); } catch { /* 销毁阶段 runtime 可能已不可用，无可补救动作 */ }',
        "} catch (error) { announce('取消失败：' + String(error)); }"
      ]
    }
  },
  {
    id: 'no-drive-root-fallback',
    title: '禁止把字面盘符根（`C:\\tmp` 之类）当回退/临时目录（系统盘未必是 C:，根目录通常不可写）',
    fix: '用 `std::env::temp_dir()` / app_data_dir / 用户目录等**运行时解析**的位置；确需固定路径时先探测可用性并给出显式报错。事故：build.rs 在 TEMP 缺失时回退 `C:\\tmp`，标准用户对 C:\\ 无写权限 → `create_dir_all().ok()` 吞掉失败 → 后续 `expect` 抛在图标拷贝处，报错与真实原因（权限/TEMP）无关。',
    targets: ['rust'],
    pattern: /PathBuf::from\(\s*r?["'][A-Za-z]:[\\/]/,
    skipLine: /\bassert/,
    samples: {
      bad: ['let tmp = PathBuf::from("C:\\\\tmp");', 'let p = PathBuf::from(r"D:/cache");'],
      good: [
        'let tmp = std::env::temp_dir().join(format!("myshelltool-icon-{}.ico", std::process::id()));',
        'let p = app_data_dir.join("cache");'
      ]
    }
  },
  {
    id: 'no-env-path-without-absolute-check',
    title: '环境变量当路径根时必须区分「缺失/空串」并校验绝对性（空值与相对值都是猜测）',
    fix: '用 `var_os` + 空值过滤 + `Path::is_absolute()` 校验，不满足即显式回退并留痕。事故：`MYSHELLTOOL_DATA_DIR=""`（变量存在但为空）或相对值 → 配置/日志落到进程 CWD，重启后路径漂移、mcp-config.json 找不到 → 拦截等级静默回落默认档。',
    targets: ['rust'],
    pattern: /env::var\(\s*"[A-Z_]*(?:DIR|PATH|HOME|TEMP|APPDATA)[A-Z_]*"\s*\)/,
    unless: /var_os|is_empty|is_absolute/,
    skipLine: /\bassert/,
    samples: {
      bad: ['if let Ok(dir) = std::env::var("MYSHELLTOOL_DATA_DIR") {'],
      good: [
        'let dir = std::env::var_os("MYSHELLTOOL_DATA_DIR").filter(|v| !v.is_empty());',
        'let ok = std::env::var("DATA_DIR").ok().filter(|d| std::path::Path::new(d).is_absolute());'
      ]
    }
  },
  {
    id: 'no-unredacted-command-in-log',
    title: '命令文本落日志前必须过 `redact_command`（`mysql -pP@ss` 明文进日志 = 凭据红线）',
    fix: '日志参数里用 `myshelltool_core::redact_command(command)`；整参数表用 `redacted_args_summary(&args)`。事故：审计日志 `mcp-execution-log.json`（30 天/1000 条长期保留）与应用日志 `myshelltool.log` 曾记录命令原文，AI 宿主执行的 `mysql -uroot -pP@ss`、`curl -u user:pass` 于是以明文落盘并回显。',
    targets: ['rust'],
    // 形态：整条日志语句里出现 command/cmd/args 标识符（行尾收束），且**没有**脱敏调用。
    // 已知边界：日志文案本身含英文单词 "command"/"args" 时会命中——这是有意的「宁可误报」，
    // 用行尾 `// fact-guard:allow no-unredacted-command-in-log <理由>` 豁免（本项目现有 1 处）。
    pattern: /\blog::(?:info|warn|error|debug)!\([^\n]*\b(?:command|cmd|args)\b[^\n]*\),?\s*;?\s*$/,
    unless: /redact_command|redacted_args_summary|\b(?:redacted|masked|safe)\w*(?:command|cmd|args)/,
    samples: {
      bad: [
        'log::info!("ssh_exec: command={:?}", command);',
        'log::warn!("exec_on_asset: cmd={}", cmd);'
      ],
      good: [
        'log::info!("ssh_exec: command={:?}", myshelltool_core::redact_command(command));',
        'log::info!("MCP call_tool: {} args={}", name, redacted_args_summary(&arguments));',
        'log::info!("approval: command allowed under minimal level: {:?}", myshelltool_core::redact_command(command));'
      ]
    }
  },
  {
    id: 'no-double-easing-animation-shorthand',
    title: 'animation/transition 简写禁止 var(--motion-*) 再叠 var(--ease-*)（--motion-* 是「时长+缓动」合体 token，再叠一个缓动 = 出现两个缓动函数，违反语法、整条声明解析期被静默丢弃——动画/过渡不跑且零报错）',
    fix: '需要自定义缓动用纯时长 token：`animation: <name> var(--dur-base) var(--ease-emphasized);` / `transition: opacity var(--dur-base) var(--ease-standard);`；只用合体 token 自带缓动则写 `animation: <name> var(--motion-base);` / `transition: opacity var(--motion-base);`。事故：① animation——GlobalModals 打开动画、设置面板 pane-in、连接点 dot-pulse 三处同一坏模式从未运行（靠运行时探针定位）；② transition——全仓 58 处 `transition: X var(--motion-*) var(--ease-*)`（含 .workbench-shell 网格列宽、AppButton/AppInput hover 等），computed transition-duration=0s 全被丢弃，用户感知所有交互硬切。',
    targets: ['styles', 'app'],
    pattern: /(?:animation|transition)(?:-[a-z]+)?\s*:[^;{}]*var\(--motion-[^;{}]*var\(--ease-/,
    samples: {
      bad: [
        'animation: modal-pop var(--motion-base) var(--ease-emphasized);',
        'animation: settings-pane-in var(--motion-base) var(--ease-standard);',
        'transition: grid-template-columns var(--motion-base) var(--ease-emphasized);',
        'transition: opacity var(--motion-fast) var(--ease-standard);'
      ],
      good: [
        'animation: modal-pop var(--dur-base) var(--ease-emphasized);',
        'animation: modal-layer-fade var(--motion-fast);',
        'transition: grid-template-columns var(--dur-base) var(--ease-emphasized);',
        'transition: opacity var(--motion-fast);',
        'transition: opacity var(--dur-base) var(--ease-standard);'
      ]
    }
  },
  {
    id: 'no-box-model-in-hover',
    title: ':hover 块内禁止盒模型/文档流属性（hover 改尺寸/边距/显隐 = 重排，页面抖动）',
    fix: '交互态只改绘制属性（color/background/border-color/opacity/box-shadow/outline-color/filter/transform）；悬停显隐用 opacity/visibility 常驻占位（禁 display:none↔flex 切换），位移只用 transform。见 AGENTS.md §4.1「hover/focus 反馈铁律」与 docs/已知边界与修复史.md「hover 抖动排查」。',
    targets: ['styles', 'app'],
    // 块级规则（2026-09-20 引擎升级）：selector 命中链（含 SCSS 嵌套 &:hover、
    // 后代选择器 .a:hover .b）内出现 declaration 属性即违规。已知边界：
    // ① border 简写带宽度字面值（border: 2px …）不拦（按属性名判定，不解析值）；
    // ② 跨行块注释（/* … */）里的花括号会干扰选择器栈（仓内无此形态，自检覆盖
    //    同行注释剥离）；③ :not(:hover) 会被视同 :hover（基态布局写在里面的情况
    //    仓内不存在，误报时用豁免语法）。
    block: {
      selector: /:hover\b/,
      // AGENTS.md §4.1 禁改清单：padding/margin 全家、border-*-width、宽高与
      // min/max、font-size/weight、line-height、letter-spacing、gap、flex、
      // grid-template、display、position、overflow、top/left/right/bottom/inset
      declaration: /^(?:padding(?:-\w+)?|margin(?:-\w+)?|border(?:-\w+)?-width|width|height|min-\w+|max-\w+|font-size|font-weight|line-height|letter-spacing|(?:row-|column-)?gap|flex(?:-\w+)?|grid-template(?:-\w+)?|display|position|overflow(?:-\w+)?|top|left|right|bottom|inset(?:-\w+)?)$/,
      samples: {
        bad: [
          '.btn:hover {\n  padding: 4px 8px;\n}',
          '.item {\n  color: red;\n  &:hover {\n    height: 32px;\n  }\n}',
          '.menu li:hover .icon { display: flex; }',
          '.tab:hover { border-bottom-width: 2px; }'
        ],
        good: [
          '.btn:hover {\n  background: var(--bg-hover);\n  color: var(--fg);\n  border-color: var(--accent);\n  opacity: 0.85;\n}',
          '.btn:hover { transform: translateY(-1px); box-shadow: var(--shadow-1); }',
          '.btn .icon { opacity: 0; }\n.btn:hover .icon { opacity: 1; }',
          '.a:hover { padding: 4px; } /* fact-guard:allow no-box-model-in-hover 刻意的宽度呼吸动效 */'
        ]
      }
    }
  }
];

/**
 * 单行判定。返回 'clean' | 'comment' | 'skip' | 'unless' | 'allowed' | 'violation'。
 * 顺序：注释行 > 跳过 > 命中 > unless 反证 > 豁免 > 违规。
 */
function judgeLine(rule, line) {
  // 注释行不执行任何逻辑，一律不判（规则只约束代码）
  if (/^\s*(\/\/|\/\*|\*)/.test(line)) return 'comment';
  if (rule.skipLine && rule.skipLine.test(line)) return 'skip';
  if (!rule.pattern.test(line)) return 'clean';
  if (rule.unless && rule.unless.test(line)) return 'unless';
  if (allowFor(line, rule.id)) return 'allowed';
  return 'violation';
}

// ============================================================
// 块级扫描（2026-09-20 引擎升级）：为「选择器块 + 声明」形态的规则服务
// （首条：no-box-model-in-hover）。行级正则无法把声明行关联到其上方的
// :hover 选择器（会全仓误报），因此升级为轻量选择器栈跟踪：
//   - 逐字符按 { } 分段：「{」前的段 = 选择器（入栈），「}」出栈；
//   - 块内文本按 ; 切声明，取属性名（[a-zA-Z][a-zA-Z-]*，$scss 变量与
//     --自定义属性天然不匹配）交给规则判定；
//   - 命中链上**任一**帧含 :hover 即视为 hover 块（覆盖 SCSS 嵌套 &:hover
//     与后代选择器 .a:hover .b 两种形态）；
//   - .vue 只扫 <style> 段（script/template 的花括号会干扰栈）；
//     .css/.scss 全文；其它扩展名不适用块规则。
// ============================================================

/** 提取可扫描的样式区间：返回 [{ startLine, text }]，startLine 为 text 首行在原文件的 0 基行号。 */
function styleRegions(file, content) {
  if (/\.(?:css|scss)$/.test(file)) return [{ startLine: 0, text: content }];
  if (file.endsWith('.vue')) {
    const regions = [];
    const openRe = /<style\b[^>]*>/g;
    let m;
    while ((m = openRe.exec(content))) {
      const openEnd = m.index + m[0].length;
      const close = content.indexOf('</style>', openEnd);
      if (close === -1) break;
      regions.push({
        startLine: content.slice(0, openEnd).split(/\r?\n/).length - 1,
        text: content.slice(openEnd, close)
      });
    }
    return regions;
  }
  return [];
}

/**
 * 用块级规则扫描一个文件。violations/allowCount 语义与行级路径一致；
 * 行号是原文件的 1 基行号（region.startLine + 区内部行号 + 1）。
 */
function scanStyleBlocks(rule, file, content, violations, allowCount) {
  const rel = path.relative(ROOT, file);
  for (const region of styleRegions(file, content)) {
    const lines = region.text.split(/\r?\n/);
    const selectorStack = [];
    for (let i = 0; i < lines.length; i += 1) {
      const rawLine = lines[i];
      // 剥离同行注释后再入栈跟踪（注释里的花括号/属性文本不执行语义）
      const code = rawLine.replace(/\/\/.*$/, '').replace(/\/\*.*?\*\//g, '');
      let seg = '';
      const checkSegment = text => {
        if (!selectorStack.some(sel => rule.block.selector.test(sel))) return;
        for (const decl of text.split(';')) {
          const m = decl.match(/^\s*([a-zA-Z][a-zA-Z-]*)\s*:/);
          if (!m) continue;
          if (!rule.block.declaration.test(m[1].toLowerCase())) continue;
          if (allowFor(rawLine, rule.id)) {
            allowCount[rule.id] = (allowCount[rule.id] ?? 0) + 1;
          } else {
            violations.push({ rule, file: rel, line: region.startLine + i + 1, text: rawLine.trim() });
          }
        }
      };
      for (const ch of code) {
        if (ch === '{') {
          selectorStack.push(seg.trim());
          seg = '';
        } else if (ch === '}') {
          checkSegment(seg); // 单行的「选择器 { 声明 }」形态
          seg = '';
          selectorStack.pop();
        } else {
          seg += ch;
        }
      }
      checkSegment(seg); // 多行块内的普通声明行（无花括号）
    }
  }
}

function walk(dir, exts, out) {
  let entries;
  try {
    entries = fs.readdirSync(dir, { withFileTypes: true });
  } catch (e) {
    // 不静默吞错：打印后由调用方的「root 非空断言」兜底（目录缺失 = 0 文件 = exit 1）。
    console.error(`⚠ 无法读取目录 ${dir}：${e.message}`);
    return;
  }
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) walk(full, exts, out);
    else if (exts.has(path.extname(entry.name))) out.push(full);
  }
}

/** 收集各 target 的文件列表。A1 防线：root 扫到 0 个文件 = 目录改名/拼错/过滤全灭，
 *  必须当场失败——否则门禁会在空集合上宣称「全部通过」而静默失效。 */
function collectFiles() {
  const filesByTarget = {};
  for (const [name, t] of Object.entries(TARGETS)) {
    const files = [];
    if (t.roots) {
      for (const root of t.roots) {
        const before = files.length;
        walk(path.join(ROOT, root), t.exts, files);
        if (files.length === before) {
          console.error(
            `✗ fact-guards：target [${name}] 的 root "${root}" 扫描到 0 个文件——` +
              '目录不存在/已改名，或扩展名过滤全灭。门禁拒绝在空集合上宣称通过。'
          );
          process.exit(1);
        }
      }
    }
    if (t.files) {
      for (const rel of t.files) {
        const abs = path.join(ROOT, rel);
        if (!fs.existsSync(abs)) {
          console.error(`✗ fact-guards：target [${name}] 的单文件 "${rel}" 不存在——路径拼错？`);
          process.exit(1);
        }
        files.push(abs);
      }
    }
    if (t.exclude) {
      const excl = new Set(t.exclude.map(rel => path.join(ROOT, rel)));
      filesByTarget[name] = files.filter(f => !excl.has(f));
    } else {
      filesByTarget[name] = files;
    }
  }
  return filesByTarget;
}

function selfTest() {
  let failed = 0;
  for (const rule of RULES) {
    if (rule.block) {
      // 块级规则：样例是多行样式文本，经虚拟文件走完整扫描路径判定
      const probe = text => {
        const violations = [];
        scanStyleBlocks(rule, path.join(ROOT, 'selftest.scss'), text, violations, {});
        return violations.length > 0;
      };
      for (const text of rule.block.samples.bad) {
        if (!probe(text)) {
          console.log(`  ✗ 自检失败 [${rule.id}] 应命中但未命中: ${JSON.stringify(text)}`);
          failed += 1;
        }
      }
      for (const text of rule.block.samples.good) {
        if (probe(text)) {
          console.log(`  ✗ 自检失败 [${rule.id}] 不应命中但命中: ${JSON.stringify(text)}`);
          failed += 1;
        }
      }
      continue;
    }
    for (const line of rule.samples.bad) {
      if (judgeLine(rule, line) !== 'violation') {
        console.log(`  ✗ 自检失败 [${rule.id}] 应命中但未命中: ${line}`);
        failed += 1;
      }
    }
    for (const line of rule.samples.good) {
      if (judgeLine(rule, line) === 'violation') {
        console.log(`  ✗ 自检失败 [${rule.id}] 不应命中但命中: ${line}`);
        failed += 1;
      }
    }
  }
  return failed;
}

function scan() {
  const filesByTarget = collectFiles();
  // 每个 target 的文件计数随结果打印——「扫描面缩小」必须在输出里可见
  for (const [name, files] of Object.entries(filesByTarget)) {
    console.log(`· target ${name}：${files.length} 个文件`);
  }

  const violations = [];
  const allowCount = {};
  for (const rule of RULES) {
    for (const target of rule.targets) {
      for (const file of filesByTarget[target]) {
        const content = fs.readFileSync(file, 'utf8');
        if (rule.block) {
          // 块级规则：按文件扩展名自取样式区间（.vue 的 <style> / .css/.scss 全文），
          // 其余扩展名在 styleRegions 返回空后自然跳过
          scanStyleBlocks(rule, file, content, violations, allowCount);
          continue;
        }
        const lines = content.split(/\r?\n/);
        for (let i = 0; i < lines.length; i += 1) {
          const verdict = judgeLine(rule, lines[i]);
          if (verdict === 'allowed') {
            allowCount[rule.id] = (allowCount[rule.id] ?? 0) + 1;
          } else if (verdict === 'violation') {
            violations.push({ rule, file: path.relative(ROOT, file), line: i + 1, text: lines[i].trim() });
          }
        }
      }
    }
  }

  const exemptions = Object.entries(allowCount).filter(([, n]) => n > 0);
  if (exemptions.length > 0) {
    console.log(`ℹ 豁免统计（review 请留意）：${exemptions.map(([id, n]) => `${id} ${n} 处`).join('；')}`);
  }

  const totalFiles = new Set(Object.values(filesByTarget).flat()).size;
  if (violations.length === 0) {
    console.log(`✔ fact-guards：${RULES.length} 条规则全部通过（扫描 ${totalFiles} 个文件）`);
    return 0;
  }

  console.log(`✗ fact-guards：发现 ${violations.length} 处违规\n`);
  for (const { rule, file, line, text } of violations) {
    console.log(`[${rule.id}] ${rule.title}`);
    console.log(`  ${file}:${line}`);
    console.log(`    ${text}`);
    console.log(`  修法：${rule.fix}\n`);
  }
  console.log('豁免：确属误报/合法用例，可在该行行尾注释加 `// fact-guard:allow <rule-id> <理由≥8字符>`（rule-id 须与命中规则一致）。');
  return 1;
}

const selfTestOnly = process.argv.includes('--self-test');
console.log('fact-guards —— 靠猜测代替事实门禁（指南 §7）\n');
const selfTestFailures = selfTest();
if (selfTestFailures > 0) {
  console.log(`\n规则自检失败 ${selfTestFailures} 项——规则已腐烂，请先修 scripts/fact-guards.mjs`);
  process.exit(1);
}
console.log(`✔ 规则自检通过（${RULES.length} 条规则，正反样例共 ${RULES.reduce((n, r) => { const s = r.block ? r.block.samples : r.samples; return n + s.bad.length + s.good.length; }, 0)} 项）\n`);
if (!selfTestOnly) process.exit(scan());
