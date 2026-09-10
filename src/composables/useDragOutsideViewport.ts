/**
 * useDragOutsideViewport — 「拖拽释放点在视口外」判定（提炼自 ConnectionSidebar）。
 *
 * dragend 的 clientX/Y 在窗外释放时是最后已知的视口内位置（浏览器不再更新窗外
 * 坐标），坐标判定单独不可靠；document 级 dragleave 且 relatedTarget === null
 * 表示指针离开整个 webview 视口（而非子元素间移动），才是窗外释放的可靠信号。
 * 两者取或，dragenter 回到视口时清除标记。
 *
 * 用法：dragstart 时 attach()，dragend 时 isOutside(event) 判定 + detach() 卸载。
 * isOutside 取值后自动复位标记（detach 由调用方负责，对称防泄漏）。
 */
import { isPointOutsideViewport } from '../lib/assetWindows';

export function useDragOutsideViewport() {
  let leftViewport = false;
  let detachFn: (() => void) | null = null;

  function onDragLeave(event: DragEvent) {
    if (event.relatedTarget === null) leftViewport = true;
  }
  function onDragEnter() {
    leftViewport = false;
  }

  return {
    // dragstart 时调用：防御性先 detach 旧监听，防上次 dragend 异常路径残留重复监听
    attach() {
      detachFn?.();
      leftViewport = false;
      document.addEventListener('dragleave', onDragLeave);
      document.addEventListener('dragenter', onDragEnter);
      detachFn = () => {
        document.removeEventListener('dragleave', onDragLeave);
        document.removeEventListener('dragenter', onDragEnter);
        detachFn = null;
      };
    },
    // dragend 后调用：卸载监听并复位标记，不残留毒化下一次拖拽
    detach() {
      detachFn?.();
      leftViewport = false;
    },
    // dragend 时调用：坐标判定 ∥ 离开过视口标记，取值后自动复位标记
    isOutside(event: DragEvent): boolean {
      const outside = isPointOutsideViewport(event.clientX, event.clientY) || leftViewport;
      leftViewport = false;
      return outside;
    }
  };
}
