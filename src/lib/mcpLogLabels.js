/**
 * MCP 执行日志的标签映射（单一事实源）。
 *
 * McpExecutionLogList（行展示）与 McpLogFilterBar（过滤下拉）共用，
 * 避免两处定义漂移。键与 Rust execution_log.rs 的 decision/outcome 常量镜像。
 */

// 工具 → 中文名
export const TOOL_LABELS = {
  ssh_exec: '远程命令',
  disk_usage: '磁盘情况',
  system_status: '系统状态',
  service_status: '服务状态',
  sftp_list: '列出目录',
  sftp_read_file: '读取文件',
  sftp_write_file: '写入文件',
  sftp_upload: '上传文件',
  sftp_download: '下载文件',
  sftp_remove: '删除文件',
  resource_monitor_snapshot: '资源快照'
};

// 决策 → 全称（title 悬浮用）
export const DECISION_LABELS = {
  not_required: '无需审批',
  auto_approved: '自动放行',
  minimal_allowed: '低拦截放行',
  elicitation_accepted: '客户端确认执行',
  elicitation_declined: '客户端拒绝',
  gui_accepted: 'GUI 确认执行',
  gui_declined: 'GUI 拒绝',
  rejected: '已拒绝',
  hard_blocked: '毁灭性拦截',
  timeout: '审批超时'
};

// 决策 → 行内短标签（窄容器扫描用）；语义分组：放行 / 已确认 / 已拒绝 / 硬拦截 / 超时
export const DECISION_SHORT = {
  not_required: '放行',
  auto_approved: '放行',
  minimal_allowed: '放行',
  elicitation_accepted: '已确认',
  gui_accepted: '已确认',
  elicitation_declined: '已拒绝',
  gui_declined: '已拒绝',
  rejected: '已拒绝',
  hard_blocked: '硬拦截',
  timeout: '超时'
};

// 结果 → 中文
export const OUTCOME_LABELS = {
  ok: '成功',
  error: '失败',
  skipped: '未执行'
};

// 结果 tone class：成功/失败/未执行
export function outcomeClass(outcome) {
  if (outcome === 'ok') return 'tone-ok';
  if (outcome === 'error') return 'tone-err';
  return 'tone-muted';
}
