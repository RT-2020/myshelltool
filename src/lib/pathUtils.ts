/**
 * pathUtils — 远程/本地路径拼接与父目录解析（从 workbench store 抽出，
 * v2.8 第五轮；workbench re-export 保持既有 import 路径不变）。
 */
import type { NormalizedConnectionAsset } from '@/types/domain';

export function remotePathForAsset(_asset?: NormalizedConnectionAsset | null) {
  // 空串 = 服务器默认：后端 sftp_list_dir 对空 path 做 canonicalize(".") 解析
  // 登录用户真实家目录。旧版猜 /home/<username>（root 家在 /root 必错）与按 tag
  // 硬编码（/backup、/var/lib/redis、/srv/app/releases——不存在即 No such file）已废弃。
  return '';
}

export function parentPath(path: string) {
  if (!path || path === '/' || path === '') return '/';
  const trimmed = path.replace(/\/+$/, '');
  const idx = trimmed.lastIndexOf('/');
  if (idx <= 0) return '/';
  return trimmed.slice(0, idx);
}

export function joinPath(base: string, name: string) {
  if (!base || base === '/') return '/' + name;
  return base.replace(/\/+$/, '') + '/' + name;
}

// 本地路径 helper：处理 Windows 反斜杠和 Unix 正斜杠。
// 前端只做拼接/解析，所有真实 IO 走 Rust fs_local_* 命令。
export function joinLocalPath(base: string, name: string) {
  if (!base) return name;
  const sep = base.includes('\\') && !base.includes('/') ? '\\' : '/';
  const trimmed = base.replace(/[\\/]+$/, '');
  return trimmed + sep + name;
}

export function parentLocalPath(path: string) {
  if (!path) return '';
  // 同时支持 Windows 和 Unix 风格
  const match = path.match(/^(.*?)[\\/]+[^\\/]+[\\/]*$/);
  if (!match) return path;
  const parent = match[1];
  if (!parent) return path; // 根目录
  // Windows 盘符根：D: -> D:\
  if (/^[a-zA-Z]:$/.test(parent)) return parent + '\\';
  return parent || path;
}
