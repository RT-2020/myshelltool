// 共享领域类型：由 services/backend.ts 的 normalize 函数提炼。
// 字段保持 snake_case，与 Rust 端（src-tauri / myshelltool-core）的 IPC 序列化字段一致，勿改 camelCase。

/** normalizeAsset 的返回契约（资产状态/认证方式宽松为 string：normalize 只做默认值兜底，不校验枚举）。 */
export interface NormalizedConnectionAsset {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  auth_method: string;
  private_key_path: string | null;
  group: string;
  tags: string[];
  status: string;
  last_connected: string;
  credential_id: string | null;
  passphrase_credential_id: string | null;
  private_key_credential_id: string | null;
}

export type TunnelKind = 'local' | 'remote' | 'dynamic';

/** normalizeTunnelConfig 的返回契约。 */
export interface NormalizedTunnelConfig {
  id: string;
  name: string;
  kind: TunnelKind;
  local_addr: string;
  local_port: number;
  remote_addr: string;
  remote_port: number;
  session_id: string;
  auto_start: boolean;
}

/** normalizeTunnelStatus 的返回契约。 */
export interface NormalizedTunnelStatus {
  id: string;
  config: NormalizedTunnelConfig;
  active: boolean;
  error: string | null;
}

// ============================================================
// 资产持久化命令返回（lib.rs）
// ============================================================

/** list/save/delete_connection_asset 与分组批量命令的返回（assets 为 IPC 原始记录，经 normalizeAsset 规整）。 */
export interface AssetListResult {
  source: string;
  /** 兼容回退键（assetSourceText 的 source || mode 兜底链）。 */
  mode?: string;
  count?: number | null;
  assets?: Record<string, unknown>[];
  groups?: string[];
}

/** get_credential_status / delete_credential 的状态返回。 */
export interface CredentialStatusResult {
  exists: boolean;
}

/** saveAsset 的入参（编辑器表单的部分资产字段；id 缺省时由 uniqueAssetId 生成）。 */
export type ConnectionAssetInput = Partial<NormalizedConnectionAsset>;

/** saveAsset 的凭据入参：三种凭据各自「提供新值」或「清除已存」。 */
export interface AssetSaveCredentials {
  password?: string;
  clearPassword?: boolean;
  passphrase?: string;
  clearPassphrase?: boolean;
  privateKey?: string;
  clearPrivateKey?: boolean;
}

/** 分组树节点（assets store buildGroupTree 的产物，AssetGroupNode 递归消费）。 */
export interface GroupTreeNode {
  name: string;
  path: string;
  parent: string;
  children: GroupTreeNode[];
  items: NormalizedConnectionAsset[];
}

// ============================================================
// 文件 / SFTP / 本地文件（ssh.rs RemoteFileEntry + fs_local.rs）
// ============================================================

/** 远程/本地目录条目（Rust RemoteFileEntry 的宽松契约：permissions/user/group 后端可缺省）。 */
export interface RemoteFileEntry {
  name: string;
  path: string;
  /** 'file' | 'directory' | 'symlink'（Rust 端字符串，宽松 string）。 */
  kind: string;
  size?: number | null;
  modified?: string | null;
  permissions?: string | null;
  user?: string | null;
  group?: string | null;
}

/** sftp_list_dir / ssh_list_directory 返回。 */
export interface RemoteDirectoryListResult {
  host?: string;
  path: string;
  entries?: RemoteFileEntry[];
}

/** fs_local_list_dir 返回（parent 供 navigateLocalUp 用）。 */
export interface LocalDirectoryListResult {
  path: string;
  parent?: string;
  entries?: RemoteFileEntry[];
}

// ============================================================
// SSH 会话（ssh.rs）
// ============================================================

/** ssh_connect 返回。 */
export interface SshConnectResult {
  session_id: string;
  connected: boolean;
  error?: string | null;
}

/** ssh-host-key-verify 事件 payload（HostKeyVerifyEvent，serde 默认 snake_case）。 */
export interface HostKeyVerifyPayload {
  request_id: string;
  host_port?: string | null;
  key_type?: string | null;
  fingerprint?: string | null;
  is_changed?: boolean | null;
}

/** ssh-keyboard-interactive 事件 payload（宽松：前端只透传展示 + 按 request_id 回传）。 */
export interface KeyboardInteractivePayload {
  request_id: string;
  name?: string | null;
  instruction?: string | null;
  prompts?: Array<{ text?: string; echo?: boolean }> | null;
}

/** sftp-transfer-progress 事件 payload。 */
export interface TransferProgressPayload {
  transfer_id: string;
  bytes_transferred: number;
  total_bytes: number;
}

// ============================================================
// 资源监控（resource_monitor.rs，rename_all=camelCase）
// ============================================================

export interface ResourceSnapshot {
  sessionId: string;
  timestamp: number;
  cpuUsage: number;
  cpuCores?: number;
  memTotal: number;
  memUsed: number;
  netRxBytes: number;
  netTxBytes: number;
  diskReadBytes: number;
  diskWriteBytes: number;
  diskTotal?: number;
  diskUsed?: number;
}

// ============================================================
// MCP（lib.rs McpStatus + mcp/config.rs + mcp/execution_log.rs）
// ============================================================

export interface McpToolInfo {
  name: string;
  description: string;
  /** "readonly" | "dangerous"。 */
  tag: string;
}

export interface McpResourceInfo {
  uri: string;
  name: string;
  /** 序列化名按实际到达形状宽松声明（camel/snake 均可选）。 */
  isTemplate?: boolean;
  is_template?: boolean;
}

export interface McpPromptInfo {
  name: string;
  description: string;
  arguments?: string[];
}

export interface McpProbeResult {
  ok: boolean;
  reason?: string | null;
  detail?: string | null;
  exePath?: string;
  serverInfo?: { name?: string; version?: string } | null;
  probedAt?: string;
}

/** mcp_status 返回（前端按 camelCase 读取，字段宽松可选）。 */
export interface McpStatusResult {
  serverName?: string;
  serverVersion?: string;
  endpoint?: string;
  dataDir?: string;
  probe?: McpProbeResult | null;
  tools?: McpToolInfo[];
  resources?: McpResourceInfo[];
  prompts?: McpPromptInfo[];
}

/** mcp-tool-approval 事件 payload（v1.5 GUI 弹窗审批）。 */
export interface McpApprovalPrompt {
  request_id: string;
  intent?: string | null;
  command?: string | null;
  consequence?: string | null;
}

/** mcp_get_config / mcp_set_config 返回。 */
export interface McpConfigResult {
  level?: string | null;
}

/** mcp_list_execution_logs 返回条目（execution_log.rs，rename_all=camelCase）。 */
export interface McpExecutionLogEntry {
  id: string;
  timestampMs: number;
  level: string;
  tool: string;
  assetId: string;
  assetName: string;
  host: string;
  port: number;
  username: string;
  command: string;
  intent: string;
  decision: string;
  outcome: string;
  outputSummary: string;
}

// ============================================================
// Gist 同步（sync.rs）
// ============================================================

/** sync_status 返回。 */
export interface SyncStatusResult {
  configured: boolean;
  last_synced_at?: string | null;
  gist_id_masked?: string | null;
  pat_configured?: boolean;
  auto_sync_enabled?: boolean;
  sync_credentials?: boolean;
}

/** sync_setup 返回（serde tag="kind"）。 */
export type SyncSetupResult =
  | { kind: 'Created'; gist_id_masked?: string }
  | { kind: 'PulledRemote'; assets_json?: string }
  | { kind: 'AlreadyConfigured'; gist_id_masked?: string };

/** sync_push 返回。 */
export interface SyncPushResult {
  success?: boolean;
  message: string;
  new_rev?: number | null;
}

/** sync_pull 返回（serde tag="decision"）。 */
export type SyncPullResult =
  | { decision: 'NoChange' }
  | { decision: 'Pulled'; assets_json?: string; new_rev?: number }
  | { decision: 'LocalNewer' }
  | { decision: 'Conflict'; local_json?: string; remote_json?: string; remote_rev?: number };

/** sync_check_remote_updates 返回。 */
export interface RemoteUpdateStatus {
  has_updates: boolean;
  local_rev?: number | null;
  remote_rev?: number | null;
}

/** sync store 的冲突暂存（pull 返回 Conflict 时存双方 JSON，字段宽松可选：透传给 IPC）。 */
export interface SyncConflictStash {
  localJson?: string;
  remoteJson?: string;
  remoteRev?: number;
}

// ============================================================
// UI（ui store）
// ============================================================

/**
 * ui.modal 的 payload。type 按字符串分支（GlobalModals 的 modalTitle/submitModal），
 * Phase 2A 不枚举全部类型字面量，宽松为 string | null。
 */
export interface ModalState {
  type: string | null;
  asset?: NormalizedConnectionAsset | null;
  payload?: Record<string, unknown> | null;
}

export type ToastLevel = 'info' | 'success' | 'warn' | 'error';

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface ToastItem {
  id: number;
  level: string;
  message: string;
  action: ToastAction | null;
}

/** notify 的可选参数（announce 不带 level，保持仅状态栏的旧行为）。 */
export interface NotifyOptions {
  level?: string;
  duration?: number;
  action?: ToastAction | null;
}

/** backend_status 命令返回（lib.rs BackendStatus）。 */
export interface BackendStatusState {
  ready: boolean;
  mode: string;
}

/** 全局搜索建议（ui store searchState.suggestions 元素）。 */
export type SearchSuggestion =
  | { kind: 'quick-connect'; username: string; host: string; port: number; label: string }
  | { kind: 'asset'; asset: NormalizedConnectionAsset };

/** 全局搜索状态（ui store searchState）。 */
export interface SearchState {
  open: boolean;
  query: string;
  suggestions: SearchSuggestion[];
}

/** 终端内联搜索状态（sessions store terminalSearch）。 */
export interface TerminalSearchState {
  open: boolean;
  query: string;
  direction: string;
  result: string | null;
}
