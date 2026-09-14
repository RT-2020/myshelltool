/**
 * errorMessage — 从任意 catch 到的值安全提取可展示的错误文本
 *
 * 为什么需要它：Tauri 后端 `#[tauri::command]` 返回 `Err(String)` 时，前端
 * `invoke` 以**字符串**（不是 Error 实例）reject，而 `services/backend.ts` 的
 * invokeBackend 原样透传该 Promise、不做规范化。因此 `(error as Error).message`
 * 在真实的后端失败上取到 `undefined` —— UI 显示「undefined」、错误卡片显示
 * 「undefined」，甚至把 `Error: undefined` 写进终端，真实故障原因被吞掉。
 *
 * 同一排查教训此前以三行内联三元式散落在调用点（见 files.ts 的 remoteError
 * 赋值处），现主体调用点已收敛到本文件。需要**区分错误类型**而非单纯取展示文本
 * 的位置仍保留内联判断，属有意为之：如 files.ts 的 probeRemoteTarget（按错误
 * 文本判「明确不存在」与「无法确认」）与 lib/sessionHandoff.ts 的 announce。
 */

/**
 * 提取可展示的错误文本。
 *
 * - `Error` 实例 → `.message`（为空串时回落到 fallback）
 * - 非空字符串 → 原样返回（Tauri `Err(String)` 的主路径）
 * - 其他值 → `String(value)` 兜底
 * - `null` / `undefined` / 空串 → 返回 fallback
 *
 * @param error  catch 到的任意值
 * @param fallback 无可用信息时的文案（默认为中文「未知错误」，与项目 UI 文案一致）
 */
export function errorMessage(error: unknown, fallback = '未知错误'): string {
  if (error instanceof Error) return error.message || fallback;
  if (typeof error === 'string') return error || fallback;
  const text = String(error);
  return text && text !== 'null' && text !== 'undefined' ? text : fallback;
}
