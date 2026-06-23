import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import { invokeBackend, isTauriRuntime } from '../services/backend.js';

// ─── 配置引导：给外部 LLM 宿主的注册 JSON 模板 ───
//
// v1.4：MCP 内嵌 GUI，用 Streamable HTTP transport。配置从「stdio command」
// 改为「HTTP url」。server key 固定为 "myshelltool"（与 rmcp get_info 的
// server_info.name 对齐）。url 由 refresh() 从后端 mcp_status.endpoint 拿到。

/**
 * 生成单家宿主的配置 JSON 字符串（Streamable HTTP transport）。
 * @param {string} url  MCP HTTP endpoint（如 http://127.0.0.1:41235/mcp）
 */
function buildConfigSnippet(url) {
  const server = { url: url || '<MCP_HTTP_URL>' };
  return JSON.stringify({ mcpServers: { myshelltool: server } }, null, 2);
}

/**
 * useMcpStore — v1.4 MCP 服务可观测与配置引导。
 *
 * 信号源：Rust 端 HTTP 健康检查（probe.rs::probe_endpoint）。每次 refresh()
 * 向 GUI 自己的 MCP HTTP endpoint 发 initialize 握手，回答「MCP 能否正常工作」。
 *
 * v1.4 变化：不再 spawn 子进程（v1.2 的一次性 spawn 探测已废弃），从源头
 * 消除僵尸进程 + os error 32。MCP server 内嵌 GUI 进程，Streamable HTTP transport。
 *
 * 无事件监听：探测是按需调用，不需要 Rust → 前端的实时推送。
 */
export const useMcpStore = defineStore('mcp', () => {
  // ============================================================
  // State
  // ============================================================
  // mcp_status 命令返回的完整 payload。null = 尚未拉取过。
  const status = ref(null);
  // 是否正在拉取（防重复 + 按钮态）。
  const loading = ref(false);

  // ============================================================
  // Computed
  // ============================================================
  // 完整探测结果（含 ok/reason/detail/exePath/serverInfo/probedAt）。
  // exePath 语义 v1.4 改为 HTTP endpoint URL（字段名保留兼容）。
  const probe = computed(() => status.value?.probe ?? null);
  // MCP 能否正常工作（HTTP 健康检查握手成功）。
  // 名称保留 clientConnected 以避免 workbench/AppStatusBar/App.vue 连锁改名——
  // 语义已从「连接计数/心跳」收敛为「探测握手是否成功」。
  const clientConnected = computed(() => Boolean(probe.value?.ok));
  // 服务端能力声明（tools/resources/prompts），拉取前为空数组。
  const tools = computed(() => status.value?.tools ?? []);
  const resources = computed(() => status.value?.resources ?? []);
  const prompts = computed(() => status.value?.prompts ?? []);
  const dataDir = computed(() => status.value?.dataDir ?? '');
  const serverVersion = computed(() => status.value?.serverVersion ?? '');
  // MCP HTTP endpoint URL（供配置引导 + 用户复制）。
  const endpoint = computed(() => status.value?.endpoint ?? '');

  // ============================================================
  // Actions
  // ============================================================

  /**
   * 拉取 mcp_status（触发一次 HTTP 健康检查 + 聚合能力清单）。
   * 浏览器预览模式（非 Tauri runtime）静默跳过——npm run dev 无 IPC 能力，
   * 调了也只会报错。用显式 isTauriRuntime() 判断，不靠错误文案字符串匹配
   *（那会耦合 backend.js 的报错文案，脆弱）。
   */
  async function refresh() {
    if (loading.value) return;
    // 浏览器预览模式直接跳过，不发探测、不 warn。
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
   * v1.4：基于 HTTP endpoint URL（不再需要 exe 路径）。
   * @param {object} [opts]
   * @param {string} [opts.url]  覆盖 endpoint URL（默认用后端返回的 status.endpoint）
   */
  function buildConfig(opts = {}) {
    const url = opts.url || endpoint.value;
    return buildConfigSnippet(url);
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
    endpoint,
    // actions
    refresh,
    buildConfig
  };
});
