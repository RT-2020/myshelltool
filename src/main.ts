import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import { useUiStore } from './stores/ui';
import './styles/main.scss';
// xterm.css 必须通过 JS import 让 Vite 打包。之前用 index.html 的 <link href="../node_modules/...">
// 会解析成 /node_modules/...，但 Vite dev server 不直接服务该路径，fallback 到 index.html，
// 导致浏览器拿到 HTML 当 CSS 加载 → 类型不匹配 → xterm 样式完全不生效 → 终端元素层级错乱、
// canvas 渲染了内容但视觉不可见。这是"终端空白"的真正根因。
import '@xterm/xterm/css/xterm.css';

createApp(App).use(createPinia()).mount('#app');

// 生产构建屏蔽 WebView2 默认右键菜单：其「共享」项会调起 Windows 系统共享浮窗
// （在桌面壳里是个无意义入口，且面板故障态会把焦点劫走），「打印/另存为」等浏览器
// 入口同理不该出现。dev 构建不屏蔽，保留「检查」开 DevTools。
// 两类放行：组件已自行处理的（终端/文件行/Monaco 等，defaultPrevented 为真）不重复
// 干预；文本输入框保留原生编辑菜单（右键粘贴/复制是表单唯一右键入口）。
if (import.meta.env.PROD) {
  document.addEventListener('contextmenu', (event) => {
    if (event.defaultPrevented) return;
    const target = event.target as HTMLElement | null;
    if (target?.closest('input, textarea, [contenteditable="true"]')) return;
    event.preventDefault();
  });
}

// 中键自动滚动（Chromium Windows 内建的浏览器式 autoscroll：中键按下出滚动原点、
// 移动鼠标持续滚动）默认抑制——与右键菜单同类的浏览器行为泄漏。设置面板
// 「外观 → 鼠标」可开启；监听器在事件时读 store 当前值，改动即时生效。
// 标签页中键关闭等组件级处理在目标阶段先执行且自带 preventDefault，不受影响。
document.addEventListener('mousedown', (event) => {
  if (event.button !== 1) return;
  if (useUiStore().middleClickAutoscroll) return;
  event.preventDefault();
});

// 【v2.8】DEV-only 测试钩子：tests/ui-ipc-flows.mjs 经 window.__myshelltool 驱动真实
// store 流（连接/上传/重连），配合脚本化的 window.__TAURI__ mock 后端——覆盖浏览器
// 预览模式测不到的 Tauri IPC 路径。生产构建（import.meta.env.DEV=false）整段剔除。
if (import.meta.env.DEV) {
  const { useWorkbenchStore } = await import('./stores/workbench');
  const { useSessionsStore } = await import('./stores/sessions');
  const { useFilesStore } = await import('./stores/files');
  // v0.18：编辑器 store（tests/ui-editor.mjs 驱动 打开→编辑→保存→冲突 链路）
  const { useEditorStore } = await import('./stores/editor');
  (window as unknown as Record<string, unknown>).__myshelltool = {
    workbench: useWorkbenchStore(),
    sessions: useSessionsStore(),
    files: useFilesStore(),
    editor: useEditorStore()
  };
}
