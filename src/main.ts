import { createApp } from 'vue';
import { createPinia } from 'pinia';
import App from './App.vue';
import './styles/main.scss';
// xterm.css 必须通过 JS import 让 Vite 打包。之前用 index.html 的 <link href="../node_modules/...">
// 会解析成 /node_modules/...，但 Vite dev server 不直接服务该路径，fallback 到 index.html，
// 导致浏览器拿到 HTML 当 CSS 加载 → 类型不匹配 → xterm 样式完全不生效 → 终端元素层级错乱、
// canvas 渲染了内容但视觉不可见。这是"终端空白"的真正根因。
import '@xterm/xterm/css/xterm.css';

createApp(App).use(createPinia()).mount('#app');

// 【v2.8】DEV-only 测试钩子：tests/ui-ipc-flows.mjs 经 window.__myshelltool 驱动真实
// store 流（连接/上传/重连），配合脚本化的 window.__TAURI__ mock 后端——覆盖浏览器
// 预览模式测不到的 Tauri IPC 路径。生产构建（import.meta.env.DEV=false）整段剔除。
if (import.meta.env.DEV) {
  const { useWorkbenchStore } = await import('./stores/workbench');
  const { useSessionsStore } = await import('./stores/sessions');
  const { useFilesStore } = await import('./stores/files');
  (window as unknown as Record<string, unknown>).__myshelltool = {
    workbench: useWorkbenchStore(),
    sessions: useSessionsStore(),
    files: useFilesStore()
  };
}
