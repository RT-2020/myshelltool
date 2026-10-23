/**
 * platformCaps — 平台视觉能力探测（当前仅一项：窗口圆角）。
 *
 * Windows 11 起 DWM 自动给顶层窗口切圆角（含无边框窗口）；Win10 无此能力也无 API
 * （DWMWA_WINDOW_CORNER_PREFERENCE 为 Win11 独有），且 decorations:false 的无边框
 * 窗口在 Win10 连系统投影都没有。探测结果驱动 :root[data-os-round='false'] 的
 * 全局 radius token 直角化降级 + 窗口自绘描边（挂载点见 main.ts，样式见
 * styles/_tokens.scss 尾部）。
 *
 * 判据（UA-CH platformVersion，Chromium 官方映射）：主版本 ≥13 = Win11
 * （13.0 对应 build 22000）；1–10 = Win10；更小 = 更早版本。navigator.userAgent
 * 报的 NT 版本 Win10/11 同为 10.0 无法区分，故必须用 high-entropy 值。
 * 失败保守：探测不到（API 缺失/超时/非 Windows）不挂降级类——保持圆角现状；
 * 误方向的代价只是「极旧 WebView 跑在 Win10 上仍圆角」= 维持现状，无回归。
 */

const PROBE_TIMEOUT_MS = 150;

type UaData = {
  platform: string;
  getHighEntropyValues: (hints: string[]) => Promise<{ platformVersion?: string }>;
};

/** 探测窗口圆角能力；仅在「确认是 Windows 且主版本 <13」时返回 false。 */
export async function detectWindowRoundness(): Promise<boolean> {
  const uad = (navigator as Navigator & { userAgentData?: UaData }).userAgentData;
  if (!uad || uad.platform !== 'Windows' || typeof uad.getHighEntropyValues !== 'function') {
    return true;
  }
  try {
    const info = await Promise.race([
      uad.getHighEntropyValues(['platformVersion']),
      new Promise<{ platformVersion?: string }>((_, reject) =>
        setTimeout(() => reject(new Error('ua-ch probe timeout')), PROBE_TIMEOUT_MS)
      )
    ]);
    const major = Number.parseInt((info.platformVersion ?? '').split('.')[0] ?? '', 10);
    if (!Number.isFinite(major)) return true;
    return major >= 13;
  } catch {
    return true;
  }
}
