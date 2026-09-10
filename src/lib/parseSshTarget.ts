// ssh 快速连接目标解析 — `ssh user@host[:port]`（全局搜索 + 侧栏快速连接共用）。
// 两处解析逻辑的单一权威实现，避免 ui.ts 与 ConnectionSidebar.vue 各持一份正则漂移。

/** `ssh user@host[:port]` 解析结果。 */
export interface SshTarget {
  username: string;
  host: string;
  port: number;
}

// host 段支持两种形式：IPv6 括号形式 `[2001:db8::1]`（可带 :port）或普通
// hostname/IPv4（不含冒号，也不含 @——`ssh a@b@c` 的 host 出现 @ 必是非法输入，
// 放过去会在 IPC 层炸出难懂错误）。裸 IPv6（如 ssh root@2001:db8::1）与 port
// 分隔符存在歧义，按现有行为显式解析失败，不猜测。
const SSH_TARGET_RE = /^ssh\s+([^\s@]+)@(\[[^\]]+\]|[^\s:@]+)(?::(\d+))?$/;

/**
 * 解析 ssh 连接串。解析失败返回 null（调用方就地提示格式错误）。
 * 括号形式返回去方括号的纯 IPv6 地址（russh 连接需要裸地址）。
 */
export function parseSshTarget(input: string): SshTarget | null {
  const match = input.trim().match(SSH_TARGET_RE);
  if (!match) return null;
  const rawHost = match[2];
  const host = rawHost.startsWith('[') && rawHost.endsWith(']')
    ? rawHost.slice(1, -1)
    : rawHost;
  // 端口值域校验：`:0` / `:99999` / 超长数字串在 IPC 层才炸，这里显式失败
  // 走既有「格式错误」提示路径。
  const port = match[3] !== undefined ? Number(match[3]) : 22;
  if (!Number.isInteger(port) || port < 1 || port > 65535) return null;
  return {
    username: match[1],
    host,
    port
  };
}
