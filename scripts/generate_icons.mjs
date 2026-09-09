import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { chromium } from 'playwright';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, '..');

// 优化的现代终端与SSH运维客户端标志设计 (1024x1024)
export function getAppIconSvg() {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="1024" height="1024">
  <defs>
    <!-- 背景极夜深邃渐变 -->
    <linearGradient id="bgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#101726"/>
      <stop offset="45%" stop-color="#0B101D"/>
      <stop offset="100%" stop-color="#040711"/>
    </linearGradient>

    <!-- 边框拟态高光渐变 (上高亮，下幽光) -->
    <linearGradient id="borderGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#38BDF8" stop-opacity="0.85"/>
      <stop offset="30%" stop-color="#60A5FA" stop-opacity="0.45"/>
      <stop offset="70%" stop-color="#1E293B" stop-opacity="0.15"/>
      <stop offset="100%" stop-color="#2563EB" stop-opacity="0.5"/>
    </linearGradient>

    <!-- 核心主体高光渐变 (科技青 -> 极光蓝) -->
    <linearGradient id="brightCyan" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#BAE6FD"/>
      <stop offset="25%" stop-color="#38BDF8"/>
      <stop offset="75%" stop-color="#0284C7"/>
      <stop offset="100%" stop-color="#0369A1"/>
    </linearGradient>

    <!-- 节点网络渐变 -->
    <linearGradient id="nodeGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#7DD3FC"/>
      <stop offset="100%" stop-color="#0284C7"/>
    </linearGradient>

    <!-- 中心网关节点光芒 -->
    <linearGradient id="gatewayGrad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#FFFFFF"/>
      <stop offset="40%" stop-color="#38BDF8"/>
      <stop offset="100%" stop-color="#0284C7"/>
    </linearGradient>

    <!-- 柔和中心环境光 -->
    <radialGradient id="ambientGlow" cx="50%" cy="48%" r="48%">
      <stop offset="0%" stop-color="#0284C7" stop-opacity="0.32"/>
      <stop offset="50%" stop-color="#2563EB" stop-opacity="0.12"/>
      <stop offset="100%" stop-color="#030712" stop-opacity="0"/>
    </radialGradient>

    <!-- 立体投影 (针对 Windows 桌面与应用窗口) -->
    <filter id="chassisShadow" x="-10%" y="-8%" width="120%" height="124%">
      <feDropShadow dx="0" dy="28" stdDeviation="36" flood-color="#000000" flood-opacity="0.6"/>
    </filter>

    <filter id="neonGlow" x="-15%" y="-15%" width="130%" height="130%">
      <feGaussianBlur stdDeviation="12" result="blur"/>
      <feMerge>
        <feMergeNode in="blur"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </defs>

  <!-- 1. 底盘容器 (Squircle 840x840, 留有呼吸感外边距 92px) -->
  <g filter="url(#chassisShadow)">
    <rect x="92" y="92" width="840" height="840" rx="210" ry="210" fill="url(#bgGrad)"/>
    <!-- 环境内光晕 -->
    <rect x="92" y="92" width="840" height="840" rx="210" ry="210" fill="url(#ambientGlow)"/>
    <!-- 质感高光边缘 (2.5px 极精细描边) -->
    <rect x="92" y="92" width="840" height="840" rx="210" ry="210" fill="none" stroke="url(#borderGrad)" stroke-width="5"/>
  </g>

  <!-- 2. 精密技术网格 (隐约可辨的背景，增加深度) -->
  <g opacity="0.04" stroke="#FFFFFF" stroke-width="2">
    <line x1="92" y1="302" x2="932" y2="302"/>
    <line x1="92" y1="512" x2="932" y2="512"/>
    <line x1="92" y1="722" x2="932" y2="722"/>
    <line x1="302" y1="92" x2="302" y2="932"/>
    <line x1="512" y1="92" x2="512" y2="932"/>
    <line x1="722" y1="92" x2="722" y2="932"/>
  </g>

  <!-- 3. 核心视觉主体 (Shell 提示符 + 拓扑连接网络 + 终端就绪光标) -->
  <g filter="url(#neonGlow)">
    <!-- 3.1 终端 Shell 提示符 '>' (强有力、清晰饱满，低分辨率下依然锐利) -->
    <path d="M 270 326 L 474 512 L 270 698" 
          fill="none" 
          stroke="url(#brightCyan)" 
          stroke-width="72" 
          stroke-linecap="round" 
          stroke-linejoin="round"/>

    <!-- 3.2 SSH 拓扑网络连线 -->
    <g fill="none" stroke="url(#nodeGrad)" stroke-width="28" stroke-linecap="round" opacity="0.95">
      <!-- 节点1(上) 连向 主调度网关 -->
      <line x1="596" y1="360" x2="712" y2="446"/>
      <!-- 主调度网关 连向 节点2(下左) -->
      <line x1="712" y1="446" x2="634" y2="596"/>
      <!-- 主调度网关 连向 节点3(下右) -->
      <line x1="712" y1="446" x2="806" y2="606"/>
    </g>

    <!-- 3.3 SSH 拓扑节点 (层次分明的主从架构) -->
    <!-- 节点 1 (上方从属主机节点) -->
    <circle cx="596" cy="360" r="36" fill="#0A0F1D" stroke="url(#nodeGrad)" stroke-width="20"/>
    <circle cx="596" cy="360" r="14" fill="#38BDF8"/>

    <!-- 核心网关节点 (高亮发光中心) -->
    <circle cx="712" cy="446" r="54" fill="url(#gatewayGrad)" stroke="#FFFFFF" stroke-width="10"/>
    <circle cx="712" cy="446" r="22" fill="#0369A1"/>

    <!-- 节点 2 (下方左侧从属主机) -->
    <circle cx="634" cy="596" r="32" fill="#0A0F1D" stroke="url(#nodeGrad)" stroke-width="18"/>
    <circle cx="634" cy="596" r="12" fill="#38BDF8"/>

    <!-- 节点 3 (下方右侧从属主机) -->
    <circle cx="806" cy="606" r="36" fill="#0A0F1D" stroke="url(#nodeGrad)" stroke-width="20"/>
    <circle cx="806" cy="606" r="14" fill="#38BDF8"/>

    <!-- 3.4 终端就绪光标 '_' (与提示符底部呼应) -->
    <rect x="500" y="718" width="240" height="60" rx="30" ry="30" fill="url(#brightCyan)"/>
  </g>
</svg>`;
}

async function main() {
  console.log('Generating high-res source icon...');
  const svg = getAppIconSvg();

  // 保存 SVG 到 scripts
  const svgPath = path.join(projectRoot, 'scripts', 'app-icon.svg');
  fs.writeFileSync(svgPath, svg, 'utf-8');

  // 保存给前端使用的 SVG（如关于面板与favicon）
  const frontendSvgPath = path.join(projectRoot, 'src', 'app-logo.svg');
  fs.writeFileSync(frontendSvgPath, svg, 'utf-8');

  // 使用 Playwright 渲染 1024x1024 高清透明底 PNG
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1024, height: 1024 }, deviceScaleFactor: 1 });
  
  const html = `<!DOCTYPE html>
  <html>
  <head>
    <meta charset="utf-8">
    <style>
      * { margin: 0; padding: 0; box-sizing: border-box; }
      body {
        width: 1024px;
        height: 1024px;
        background: transparent;
        display: flex;
        align-items: center;
        justify-content: center;
        overflow: hidden;
      }
      svg {
        width: 1024px;
        height: 1024px;
      }
    </style>
  </head>
  <body>
    ${svg}
  </body>
  </html>`;

  await page.setContent(html);

  // 1024x1024 完整源图（供 tauri icon 生成工具使用）
  const sourcePngPath = path.join(projectRoot, 'app-icon.png');
  await page.screenshot({ path: sourcePngPath, omitBackground: true });
  console.log('Saved source icon PNG to:', sourcePngPath);

  // 同时复制一份到 scratch 目录供查看与评估
  const scratchDir = 'C:\\Users\\lenovo\\.gemini\\antigravity\\brain\\37f9748e-ea1d-42c4-912b-5c276ca78c55';
  const artifactPngPath = path.join(scratchDir, 'myshelltool_app_icon_v2.png');
  fs.copyFileSync(sourcePngPath, artifactPngPath);

  await browser.close();
  console.log('Icon rendering complete!');
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
