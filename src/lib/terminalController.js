// TerminalController: 封装 xterm Terminal + addon 生命周期。
// 所有销毁相关方法用 _disposed 标志做幂等，防止 close-tab 后 resize 回调引用已销毁实例。
// 注意：term/fit/search 实例由 store 在 connectSession 时构造并传入，controller 仅接管 addon 加载与观察器。

import { WebLinksAddon } from '@xterm/addon-web-links';
import { WebglAddon } from '@xterm/addon-webgl';

export class TerminalController {
  constructor({ term, fit, search, sessionId, onResize, onData }) {
    this.term = term;
    this.fit = fit;
    this.search = search;
    this.sessionId = sessionId;
    this.onResize = onResize || (() => {});
    this.onData = onData || (() => {});
    this.weblinks = null;
    this.webgl = null;
    this.resizeObserver = null;
    this._disposed = false;
    this._attached = false;
  }

  attach(domElement) {
    if (this._disposed || this._attached) return;
    this.term.open(domElement);
    this._loadOptionalAddons();
    this._startResizeObserver(domElement);
    this._attached = true;
  }

  _loadOptionalAddons() {
    // WebLinksAddon：让 URL 可点击。失败时静默跳过。
    try {
      this.weblinks = new WebLinksAddon();
      this.term.loadAddon(this.weblinks);
    } catch (e) {
      this.weblinks = null;
    }
    // WebglAddon：Canvas 渲染，大输出更顺滑。显卡失败时回退到默认 canvas renderer。
    try {
      const webgl = new WebglAddon();
      webgl.onContextLoss(() => {
        try { webgl.dispose(); } catch (_) { /* noop */ }
      });
      this.term.loadAddon(webgl);
      this.webgl = webgl;
    } catch (e) {
      this.webgl = null;
    }
  }

  _startResizeObserver(domElement) {
    if (typeof ResizeObserver === 'undefined') return;
    this.resizeObserver = new ResizeObserver(() => {
      if (this._disposed) return;
      try {
        this.fit.fit();
      } catch (_) { /* noop */ }
    });
    this.resizeObserver.observe(domElement);
  }

  applyTheme(theme) {
    if (this._disposed) return;
    try { this.term.options.theme = theme; } catch (_) { /* noop */ }
  }

  applyFontSize(size) {
    if (this._disposed) return;
    try { this.term.options.fontSize = size; } catch (_) { /* noop */ }
  }

  doFit() {
    if (this._disposed) return;
    try { this.fit.fit(); } catch (_) { /* noop */ }
  }

  write(text) {
    if (this._disposed) return;
    try { this.term.write(text); } catch (_) { /* noop */ }
  }

  writeln(text) {
    if (this._disposed) return;
    try { this.term.writeln(text); } catch (_) { /* noop */ }
  }

  focus() {
    if (this._disposed) return;
    try { this.term.focus(); } catch (_) { /* noop */ }
  }

  clear() {
    if (this._disposed) return;
    try { this.term.clear(); } catch (_) { /* noop */ }
  }

  findNext(query, opts) {
    if (this._disposed) return;
    try { this.search.findNext(query, opts); } catch (_) { /* noop */ }
  }

  findPrevious(query, opts) {
    if (this._disposed) return;
    try { this.search.findPrevious(query, opts); } catch (_) { /* noop */ }
  }

  detachObservers() {
    if (this.resizeObserver) {
      try { this.resizeObserver.disconnect(); } catch (_) { /* noop */ }
      this.resizeObserver = null;
    }
  }

  safeDispose() {
    if (this._disposed) return;
    this._disposed = true;
    this.detachObservers();
    // addon dispose 在 term.dispose 内部级联，但 weblinks/webgl 显式 dispose 更稳妥
    [this.weblinks, this.webgl].forEach(addon => {
      if (addon) {
        try { addon.dispose(); } catch (_) { /* noop */ }
      }
    });
    try { this.term.dispose(); } catch (_) { /* noop */ }
    this.weblinks = null;
    this.webgl = null;
    this.term = null;
    this.fit = null;
    this.search = null;
  }
}
