import { defineStore } from 'pinia';
import { computed, ref, watch } from 'vue';
import { invokeBackend, isTauriRuntime, listenBackendEvent } from '../services/backend.js';

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

// v1.5：GUI 弹窗审批事件 channel。server.rs 在客户端不支持 elicitation 时
//（如 ZCode）emit 此事件，前端 GlobalModals 弹窗让用户确认。
const MCP_APPROVAL_EVENT = 'mcp-tool-approval';

/**
 * useMcpStore — MCP 服务可观测、配置引导、高危工具 GUI 审批。
 *
 * 信号源（两部分）：
 * - 探测（probe）：Rust 端 HTTP 健康检查（probe.rs::probe_endpoint）。refresh()
 *   向 GUI 自己的 MCP HTTP endpoint 发 initialize 握手，回答「MCP 能否正常工作」。
 * - 审批（approval）：v1.5 新增。当 MCP host 客户端不支持 elicitation 时，
 *   server.rs emit mcp-tool-approval 事件，本 store 监听后弹 GlobalModals。
 *
 * v1.4：不再 spawn 子进程（v1.2 的一次性 spawn 探测已废弃），从源头消除僵尸
 * 进程 + os error 32。MCP server 内嵌 GUI 进程，Streamable HTTP transport。
 *
 * v1.5：本 store 从纯按需查询升级为带事件监听/dispose 的 store（approvalPrompt
 * 事件链路）。监听/超时/dispose 模式照 sessions.js 的 hostKeyPrompt（L130-234）。
 */
export const useMcpStore = defineStore('mcp', () => {
  // ============================================================
  // State
  // ============================================================
  // mcp_status 命令返回的完整 payload。null = 尚未拉取过。
  const status = ref(null);
  // 是否正在拉取（防重复 + 按钮态）。
  const loading = ref(false);

  // v1.5：GUI 审批 prompt（由 mcp-tool-approval 事件填充）。
  // null = 无待审批。非 null 时结构 = { request_id, intent, command, consequence }。
  const approvalPrompt = ref(null);
  // 监听句柄（dispose 时调用）。模块作用域而非 store state——它不是响应式数据。
  let approvalUnlisten = null;
  // 65s 超时句柄（与后端 60s 对齐 + 5s 缓冲）。
  let approvalTimeout = null;

  // workbench bridge（attachWorkbench 注入）。延迟绑定，避免循环 import。
  let workbenchBridge = null;
  function wb() {
    if (!workbenchBridge) throw new Error('mcp store: workbench bridge not attached');
    return workbenchBridge;
  }

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
  // Actions：探测 + 配置引导
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

  // ============================================================
  // Actions：v1.5 GUI 弹窗审批
  // ============================================================
  //
  // 链路：server.rs emit mcp-tool-approval → 本 store 监听 → set approvalPrompt
  // + open modal → GlobalModals 渲染 → 用户点确认/拒绝 → resolveMcpApproval
  // → invokeBackend('mcp_confirm_tool') → 后端 resolve_approval 取 sender。
  // 模式照 sessions.js 的 hostKeyPrompt（L130-234）。

  /**
   * 注册 mcp-tool-approval 事件监听（幂等：已注册则跳过）。
   * 由 setupEventListeners 调用。浏览器预览模式静默跳过。
   */
  async function ensureApprovalListener() {
    if (!isTauriRuntime() || approvalUnlisten) return;
    try {
      approvalUnlisten = await listenBackendEvent(MCP_APPROVAL_EVENT, event => {
        approvalPrompt.value = event.payload;
        wb().modal = { type: 'mcpApproval' };
      });
    } catch (error) {
      // eslint-disable-next-line no-console
      console.warn('[mcp] approval listener registration deferred:', error?.message);
    }
  }

  /**
   * 用户在 GUI 弹窗中确认/拒绝后回传后端。
   * 调 mcp_confirm_tool 命令 → 后端 resolve_approval 取 oneshot::Sender。
   * @param {string} requestId  审批请求 id（来自 approvalPrompt.request_id）
   * @param {boolean} accepted  用户是否确认执行
   */
  async function resolveMcpApproval(requestId, accepted) {
    try {
      await invokeBackend('mcp_confirm_tool', { requestId, accepted });
    } catch (error) {
      wb().announce?.('MCP 审批响应失败：' + (error?.message || error));
    }
    approvalPrompt.value = null;
    if (!accepted) wb().modal = { type: null };
  }

  // approvalPrompt 65s 自动清理（与后端 60s 超时对齐 + 5s 缓冲）。
  // 后端超时后会自己清 pending 表，前端这层是为避免 modal 卡死。
  watch(approvalPrompt, prompt => {
    if (approvalTimeout) {
      clearTimeout(approvalTimeout);
      approvalTimeout = null;
    }
    if (prompt) {
      approvalTimeout = setTimeout(() => {
        if (approvalPrompt.value) {
          approvalPrompt.value = null;
          wb().modal = { type: null };
          wb().announce?.('MCP 高危操作审批超时（65秒未响应），已自动关闭');
        }
      }, 65000);
    }
  });

  // ============================================================
  // 生命周期：workbench 编排（照 sessions.js 三件套模式）
  // ============================================================

  /** workbench 注入跨 store 桥（modal setter / announce）。 */
  function attachWorkbench(bridge) {
    workbenchBridge = bridge;
  }

  /** 由 workbench.initialize() 调用，注册所有事件监听。 */
  async function setupEventListeners() {
    if (!isTauriRuntime()) return;
    await ensureApprovalListener();
  }

  /** 由 workbench 销毁时调用，解绑监听防内存泄漏。 */
  async function disposeEventListeners() {
    if (approvalTimeout) {
      clearTimeout(approvalTimeout);
      approvalTimeout = null;
    }
    if (approvalUnlisten) {
      try {
        await approvalUnlisten();
      } catch {
        // 忽略：销毁阶段，Tauri runtime 可能已不可用
      }
      approvalUnlisten = null;
    }
  }

  return {
    // state
    status,
    loading,
    approvalPrompt,
    // computed
    probe,
    clientConnected,
    tools,
    resources,
    prompts,
    dataDir,
    serverVersion,
    endpoint,
    // actions：探测 + 配置
    refresh,
    buildConfig,
    // actions：v1.5 GUI 弹窗审批
    resolveMcpApproval,
    // 生命周期
    attachWorkbench,
    setupEventListeners,
    disposeEventListeners
  };
});
