import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { invokeBackend, isTauriRuntime } from '../services/backend.js';

// ─── 配置引导：给外部 LLM 宿主的注册 JSON 模板 ───
//
// 三家（Claude Desktop / Cursor / Cline）的权威写法取自 docs/mcp-setup.md §3/§4。
// server key 固定为 "myshelltool"（与 rmcp get_info 的 server_info.name 对齐）。
//
// command 用占位符 <INSTALL_DIR>，因为 MCP exe 当前不随 NSIS 自动打包
// （见 docs/mcp-setup.md:24），用户需手动填绝对路径。dataDir 由 refresh()
// 从后端拿到后，可在面板里展示并填入 env 字段。
const MCP_EXE_PLACEHOLDER = '<INSTALL_DIR>\\myshelltool-mcp.exe';

/**
 * 生成单家宿主的配置 JSON 字符串。
 * @param {string} exePath  MCP exe 绝对路径（默认占位符）
 * @param {string} [dataDir]  可选 MYSHELLTOOL_DATA_DIR（为空则不输出 env 字段）
 */
function buildConfigSnippet(exePath, dataDir) {
  const server = { command: exePath || MCP_EXE_PLACEHOLDER, args: [] };
  if (dataDir) {
    server.env = { MYSHELLTOOL_DATA_DIR: dataDir };
  }
  return JSON.stringify({ mcpServers: { myshelltool: server } }, null, 2);
}

/**
 * useMcpStore — v1.2 MCP 服务可观测与配置引导。
 *
 * 信号源：Rust 端无状态按需探测（probe.rs）。每次 refresh() 触发 GUI 主动
 * spawn myshelltool-mcp.exe + initialize 握手，回答「MCP 能否正常工作」。
 *
 * 这是 v1.2 的最终形态：不做运行时状态机（心跳/计数/事件），纯函数查询，
 * 打开程序或点刷新即可见结果。详见 probe.rs 头注释的设计取舍。
 *
 * 无事件监听：探测是按需调用，不需要 Rust → 前端的实时推送。
 */
export const useMcpStore = defineStore('mcp', () => {
  // ============================================================
  // State
  // ============================================================
  // mcp_status 命令返回的完整 payload。null = 尚未拉取过。
  const status = ref(null);
  // 是否正在拉取（防重复 + 按钮态）。探测会 spawn 子进程，不宜并发。
  const loading = ref(false);

  // ============================================================
  // Computed
  // ============================================================
  // 完整探测结果（含 ok/reason/detail/exePath/serverInfo/probedAt）。
  const probe = computed(() => status.value?.probe ?? null);
  // MCP 能否正常工作（探测握手成功）。
  // 名称保留 clientConnected 以避免 workbench/AppStatusBar/App.vue 连锁改名——
  // 语义已从「连接计数/心跳」收敛为「探测握手是否成功」。
  const clientConnected = computed(() => Boolean(probe.value?.ok));
  // 服务端能力声明（tools/resources/prompts），拉取前为空数组。
  const tools = computed(() => status.value?.tools ?? []);
  const resources = computed(() => status.value?.resources ?? []);
  const prompts = computed(() => status.value?.prompts ?? []);
  const dataDir = computed(() => status.value?.dataDir ?? '');
  const serverVersion = computed(() => status.value?.serverVersion ?? '');

  // ============================================================
  // Actions
  // ============================================================

  /**
   * 拉取 mcp_status（触发一次无状态探测 + 聚合能力清单）。
   * 浏览器预览模式（非 Tauri runtime）静默跳过——npm run dev 无 IPC 能力，
   * 调了也只会报错。用显式 isTauriRuntime() 判断，不靠错误文案字符串匹配
   *（那会耦合 backend.js 的报错文案，脆弱）。
   */
  async function refresh() {
    if (loading.value) return;
    // 浏览器预览模式直接跳过，不 spawn 探测、不 warn。
    if (!isTauriRuntime()) return;
    loading.value = true;
    try {
      status.value = await invokeBackend('mcp_status');
    } catch (error) {
      // 真 runtime 下的失败才记日志——探测本身的失败已内化成 probe.ok=false，
      // 这里 catch 的只有 IPC 层异常（命令未注册等），值得 warn。
      // eslint-disable-next-line no-console
      console.warn('[mcp] refresh failed:', error?.message || error);
    } finally {
      loading.value = false;
    }
  }

  /**
   * 生成指定宿主的配置 JSON 字符串（复制即用）。
   * @param {object} [opts]
   * @param {string} [opts.exePath]  用户填的 exe 绝对路径
   * @param {string} [opts.dataDir]  覆盖默认数据目录（默认用后端返回的）
   */
  function buildConfig(opts = {}) {
    const exePath = opts.exePath || MCP_EXE_PLACEHOLDER;
    const dir = opts.dataDir ?? dataDir.value;
    return buildConfigSnippet(exePath, dir);
  }

  return {
    // state
    status,
    loading,
    // computed
    probe,
    clientConnected,
    tools,
    resources,
    prompts,
    dataDir,
    serverVersion,
    // actions
    refresh,
    buildConfig
  };
});

// 导出占位符常量供面板组件直接引用（输入框 placeholder）。
export { MCP_EXE_PLACEHOLDER };
