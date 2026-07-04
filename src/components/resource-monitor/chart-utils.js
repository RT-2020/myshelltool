// Shared formatting + SVG path helpers for resource-monitor charts.
// Extracted from CpuChart/MemoryChart/NetworkChart/DiskChart to remove
// per-file duplication. Behaviour-preserving.

export const CHART_W = 240;
export const CHART_H = 60;
export const CHART_PAD = 4;
export const MAX_POINTS = 60;

export function formatBytes(b) {
  if (!b || b <= 0) return '0B';
  if (b < 1024) return `${b}B`;
  if (b < 1024 * 1024) return `${(b / 1024).toFixed(1)}KB`;
  if (b < 1024 * 1024 * 1024) return `${(b / 1024 / 1024).toFixed(1)}MB`;
  return `${(b / 1024 / 1024 / 1024).toFixed(2)}GB`;
}

export function formatRate(bytesPerSec) {
  if (!bytesPerSec || bytesPerSec <= 0) return '0B/s';
  if (bytesPerSec < 1024) return `${bytesPerSec}B/s`;
  if (bytesPerSec < 1024 * 1024) return `${(bytesPerSec / 1024).toFixed(1)}KB/s`;
  if (bytesPerSec < 1024 * 1024 * 1024) return `${(bytesPerSec / 1024 / 1024).toFixed(1)}MB/s`;
  return `${(bytesPerSec / 1024 / 1024 / 1024).toFixed(2)}GB/s`;
}

export function formatCompactRate(bytesPerSec) {
  if (!bytesPerSec || bytesPerSec <= 0) return '0';
  if (bytesPerSec < 1024) return `${Math.round(bytesPerSec)}B`;

  const units = [
    { unit: 'K', value: bytesPerSec / 1024 },
    { unit: 'M', value: bytesPerSec / 1024 / 1024 },
    { unit: 'G', value: bytesPerSec / 1024 / 1024 / 1024 }
  ];
  const match = units.find((item) => item.value < 1024) ?? units[units.length - 1];
  const displayValue = Math.min(match.value, 999);
  const digits = displayValue < 10 ? 1 : 0;
  const suffix = match.value > 999 ? '+' : '';
  return `${displayValue.toFixed(digits).replace(/\.0$/, '')}${match.unit}${suffix}`;
}

export function buildLinePath(pts, max) {
  if (!pts.length) return '';
  const stepX = (CHART_W - CHART_PAD * 2) / Math.max(1, MAX_POINTS - 1);
  const len = Math.min(pts.length, MAX_POINTS);
  const offset = MAX_POINTS - len;
  // 守卫：后端 snapshot 任何字段为 undefined/NaN 时（例如首次采样、解析边缘情况），
  // 单点会变 NaN，Math.min/max 对 NaN 返回 NaN → path "M236.0,NaN" 让整个 SVG 报错。
  // 这里强制把非有限值归零，保证 path 永远是合法数值。
  return pts.slice(-MAX_POINTS).map((raw, i) => {
    const v = Number.isFinite(raw) ? raw : 0;
    const x = CHART_PAD + (offset + i) * stepX;
    const y = CHART_H - CHART_PAD - (Math.min(max, Math.max(0, v)) / Math.max(1, max)) * (CHART_H - CHART_PAD * 2);
    return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`;
  }).join(' ');
}

export function buildAreaPath(linePath) {
  if (!linePath) return '';
  return `${linePath} L${CHART_W - CHART_PAD},${CHART_H - CHART_PAD} L${CHART_PAD},${CHART_H - CHART_PAD} Z`;
}
