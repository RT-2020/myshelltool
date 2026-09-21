/**
 * editor — 内置文本/配置文件编辑器的域 store（v0.18）。
 *
 * 形态：主窗口/资产窗口中央区的覆盖面板（EditorSurface），多文件 tab。
 * 内容策略：编辑器正文**不进响应式**（2 MiB 字符串高频触发 reactive 的代价
 * 不可接受）——CodeMirror 实例归组件持有，经 registerContentProvider 注册
 * get/set 通道；store 只管 tab 元数据/dirty/基线/冲突等轻状态。
 *
 * 分层（行数红线）：写盘/冲突/草稿/备份/编码切换等流程域在
 * lib/editor/editorFlows.ts（绑定式 context，先例 lib/fileTransfers），
 * 本文件保留状态、内容通道、tab 管理与打开链路，并在 return 处 re-export
 * 流程函数（组件/测试经 store 单入口使用）。
 *
 * 与后端的契约见 lib/editor/editorIo.ts（错误前缀协议、expected 守护写、
 * conflict 三选）。弹窗走 GlobalModals 的单一通用 type `editorDialog`。
 */
import { defineStore } from 'pinia';
import { computed, ref } from 'vue';
import type { ModalState, NormalizedConnectionAsset, NotifyOptions } from '@/types/domain';
import type {
  EditorClassifiedError,
  EditorDialogPayload,
  EditorTab,
  EditorTarget
} from '@/types/editor';
import { isErrorKindRetryable, readText, toClassifiedError, deleteDraft } from '@/lib/editor/editorIo';
import { resolveSessionForAsset } from '@/lib/filePanel';
import { basenameOf } from '@/lib/editor/editorPaths';
import {
  bindEditorFlows,
  dropEolOverride,
  listBackupsAction,
  offerDraftRecovery,
  reloadTab,
  retryTab,
  saveAllEditorTabs,
  discardAllDirty,
  saveDraftAction,
  saveTab,
  setEncoding,
  setEol,
  tabEolChoice,
  uploadToRemote,
  openSaveAsDialog
} from '@/lib/editor/editorFlows';

export type { EditorDialogPayload } from '@/types/editor';

export interface EditorWorkbenchBridge {
  announce(message: string, opts?: NotifyOptions): void;
  get modal(): ModalState;
  set modal(v: ModalState);
  assets(): NormalizedConnectionAsset[];
  /** 当前选中资产（上传到远端/远端探测的会话归属）。 */
  get selectedAsset(): NormalizedConnectionAsset | null;
  get localPath(): string;
  get remotePath(): string;
  refreshRemoteFiles(path?: string | null): Promise<void>;
  refreshLocalFiles(path?: string | null): Promise<void>;
}

let bridge: EditorWorkbenchBridge | null = null;

export function bindEditorStoreBridge(b: EditorWorkbenchBridge): void {
  bridge = b;
}

function wb(): EditorWorkbenchBridge {
  if (!bridge) throw new Error('editor store 未绑定 bridge（workbench 未初始化）');
  return bridge;
}

let tabSeq = 0;

export const useEditorStore = defineStore('editor', () => {
  const tabs = ref<EditorTab[]>([]);
  const activeTabId = ref<string | null>(null);
  const surfaceVisible = ref(false);
  /** tabId → 保存进行中（禁重复触发）。 */
  const savingTabIds = ref<Record<string, boolean>>({});
  /** tabId → 正文快照（host 未挂载时的兜底真相；挂载后以 CodeMirror doc 为准）。 */
  const tabContent = new Map<string, string>();
  /** tabId → 活动实例访问通道（get=读 doc；set=同步替换 doc，格式化/恢复草稿用）。 */
  const contentAccess = new Map<string, { get: () => string; set?: (text: string) => void }>();

  const activeTab = computed(() => tabs.value.find(t => t.id === activeTabId.value) || null);
  const dirtyCount = computed(() => tabs.value.filter(t => t.dirty).length);

  // 备份开关（设置面板写入 localStorage；flows.performWrite 经 ctx 读取）
  const backupEnabled = ref(true);
  const editorBackupEnabled = computed(() => backupEnabled.value);
  function setBackupEnabled(v: boolean): void {
    backupEnabled.value = v;
    try {
      localStorage.setItem('myshelltool-editor-backup', v ? '1' : '0');
    } catch {
      // localStorage 不可用（隐私模式等）：仅本次会话生效
    }
  }
  try {
    if (localStorage.getItem('myshelltool-editor-backup') === '0') backupEnabled.value = false;
  } catch {
    // 读取失败维持默认开（fail-safe：宁可多备份）
  }

  // ============================================================
  // 流程域绑定（lib/editor/editorFlows.ts）
  // ============================================================
  bindEditorFlows({
    tabs: () => tabs.value,
    savingTabIds: () => savingTabIds.value,
    announce: (message, opts) => wb().announce(message, opts),
    currentContent,
    replaceContent,
    markDirty,
    openDialog,
    loadTab,
    makeTab,
    resolveRemoteSession,
    setActiveTab: setActive,
    showSurface: () => { surfaceVisible.value = true; },
    selectedAssetId: () => wb().selectedAsset?.id ?? null,
    selectedAssetName: () => wb().selectedAsset?.name ?? null,
    remotePathPrefix: () => wb().remotePath,
    isBackupEnabled: () => backupEnabled.value,
    refreshPanels: kind => {
      if (kind === 'remote') wb().refreshRemoteFiles().catch(() => undefined);
      else wb().refreshLocalFiles().catch(() => undefined);
    }
  });

  // ============================================================
  // 内容通道（组件侧）
  // ============================================================
  function currentContent(tabId: string): string {
    const access = contentAccess.get(tabId);
    if (access) return access.get();
    return tabContent.get(tabId) ?? '';
  }

  /** 最近一次读取/替换写入的快照（host 的版本号 watch 用它对比，避免与活动 doc 自比较）。 */
  function contentSnapshot(tabId: string): string {
    return tabContent.get(tabId) ?? '';
  }

  function registerContentProvider(
    tabId: string,
    access: { get: () => string; set?: (text: string) => void } | null
  ): void {
    if (access) contentAccess.set(tabId, access);
    else contentAccess.delete(tabId);
  }

  /** 程序化替换正文（格式化/压缩/恢复草稿）：挂载中同步落 doc，否则写快照+版本号。 */
  function replaceContent(tabId: string, text: string): void {
    const access = contentAccess.get(tabId);
    if (access?.set) {
      access.set(text);
    } else {
      tabContent.set(tabId, text);
      const tab = tabs.value.find(t => t.id === tabId);
      if (tab) tab.contentVersion++;
    }
    markDirty(tabId, true);
  }

  function markDirty(tabId: string, dirty: boolean): void {
    const tab = tabs.value.find(t => t.id === tabId);
    if (tab && tab.dirty !== dirty) tab.dirty = dirty;
  }

  function setActive(tabId: string): void {
    activeTabId.value = tabId;
  }

  /** 通用编辑器弹窗（GlobalModals 单一 type `editorDialog` 的数据源与回调分发）。 */
  function openDialog(payload: EditorDialogPayload): void {
    wb().modal = {
      type: 'editorDialog',
      payload: payload as unknown as Record<string, unknown>
    };
  }

  /** GlobalModals 按钮回调入口：返回错误串则弹窗保持打开并内联展示。 */
  async function resolveEditorDialog(index: number, inputValue?: string): Promise<string | null> {
    const modal = wb().modal;
    const payload = modal?.payload as unknown as EditorDialogPayload | undefined;
    if (modal?.type !== 'editorDialog' || !payload?.buttons?.[index]) return '弹窗状态已失效';
    const result = await payload.buttons[index]!.action?.(inputValue);
    return typeof result === 'string' ? result : null;
  }

  // ============================================================
  // tab 管理
  // ============================================================
  function makeTab(target: EditorTarget, encodingOverride: string | null): EditorTab {
    const id = `ed-${Date.now()}-${++tabSeq}`;
    return {
      id,
      target,
      name: basenameOf(target.path),
      status: 'loading',
      error: null,
      encodingOverride,
      baseline: null,
      readOnly: false,
      readOnlyReason: null,
      dirty: false,
      conflict: null,
      contentVersion: 0
    };
  }

  function resolveRemoteSession(target: EditorTarget): { sessionId: string } | { error: EditorClassifiedError } {
    const asset = wb().assets().find(a => a.id === target.assetId) || null;
    if (!asset) {
      return {
        error: {
          kind: 'not-found',
          message: `找不到资产（id=${target.assetId}），可能已被删除`,
          raw: ''
        }
      };
    }
    const session = resolveSessionForAsset(asset);
    if (!session) {
      return {
        error: {
          kind: 'network',
          message: `资产「${asset.name}」尚未建立 SSH 会话，请先连接再编辑远端文件`,
          raw: ''
        }
      };
    }
    return { sessionId: session.sessionId };
  }

  /** 读取并落 tab（打开/重读共用）。 */
  async function loadTab(tab: EditorTab): Promise<void> {
    tab.status = 'loading';
    tab.error = null;
    tab.conflict = null;
    let sessionId: string | undefined;
    if (tab.target.kind === 'remote') {
      const resolved = resolveRemoteSession(tab.target);
      if ('error' in resolved) {
        tab.status = 'error';
        tab.error = resolved.error;
        return;
      }
      sessionId = resolved.sessionId;
    }
    try {
      const result = await readText(tab.target, {
        sessionId,
        encoding: tab.encodingOverride
      });
      tab.baseline = {
        size: result.size,
        modified: result.modified,
        encoding: result.encoding,
        eol: result.eol,
        hasBom: result.hasBom
      };
      tab.readOnly = Boolean(result.readOnly);
      tab.readOnlyReason = result.readOnly
        ? tab.target.kind === 'remote'
          ? '远端文件权限位不含写权限，按只读打开'
          : '本地文件是只读属性，按只读打开'
        : null;
      tabContent.set(tab.id, result.content);
      contentAccess.delete(tab.id); // 挂载中的 host 经版本号 watch 重载 doc
      tab.contentVersion++;
      tab.dirty = false;
      tab.status = 'ready';
      // 草稿恢复：草稿比文件新 → 让用户选（恢复进编辑器 / 丢弃）
      await offerDraftRecovery(tab, result.modified);
    } catch (error) {
      tab.status = 'error';
      tab.error = toClassifiedError(error);
    }
  }

  /** 打开目标（去重：同 target 已开则激活，可带新编码重读）。 */
  async function openTarget(target: EditorTarget, opts: { encoding?: string | null } = {}): Promise<void> {
    const existing = tabs.value.find(
      t => t.target.kind === target.kind
        && t.target.path === target.path
        && (t.target.assetId ?? null) === (target.assetId ?? null)
    );
    if (existing) {
      if (opts.encoding !== undefined && opts.encoding !== existing.encodingOverride) {
        existing.encodingOverride = opts.encoding;
        await loadTab(existing);
      }
      setActive(existing.id);
      surfaceVisible.value = true;
      return;
    }
    const tab = makeTab(target, opts.encoding ?? null);
    tabs.value.push(tab);
    setActive(tab.id);
    surfaceVisible.value = true;
    await loadTab(tab);
  }

  function closeTabInternal(tabId: string): void {
    const idx = tabs.value.findIndex(t => t.id === tabId);
    if (idx === -1) return;
    tabs.value.splice(idx, 1);
    tabContent.delete(tabId);
    contentAccess.delete(tabId);
    dropEolOverride(tabId);
    if (activeTabId.value === tabId) {
      const next = tabs.value[Math.max(0, idx - 1)];
      activeTabId.value = next?.id ?? null;
    }
    if (!tabs.value.length) surfaceVisible.value = false;
  }

  /** 关 tab：dirty 先走三选弹窗（保存并关闭 / 放弃更改 / 取消）。 */
  function requestCloseTab(tabId: string): void {
    const tab = tabs.value.find(t => t.id === tabId);
    if (!tab) return;
    if (!tab.dirty || tab.status !== 'ready') {
      closeTabInternal(tabId);
      return;
    }
    openDialog({
      title: '未保存的修改',
      message: `「${tab.name}」有未保存的修改。`,
      buttons: [
        { label: '取消' },
        {
          label: '放弃更改',
          danger: true,
          action: () => {
            void deleteDraft(tab.target);
            closeTabInternal(tabId);
          }
        },
        {
          label: '保存并关闭',
          primary: true,
          action: () => {
            // 异步保存成功后再关；失败保持弹窗关闭、toast 已呈因
            void saveTab(tabId).then(ok => {
              if (ok) closeTabInternal(tabId);
            });
          }
        }
      ]
    });
  }

  /** 「打开」入口：手动输入本地路径 / 远端路径（远端挂当前选中资产的会话）。 */
  function openFromPathPrompt(kind: 'local' | 'remote'): void {
    if (kind === 'remote' && !wb().selectedAsset) {
      wb().announce('打开远端文件需要先在侧栏选中并连接一个资产', { level: 'warn' });
      return;
    }
    const isRemote = kind === 'remote';
    const preset = isRemote ? `${wb().remotePath || ''}/`.replace(/\/+/g, '/') : '';
    openDialog({
      title: isRemote ? '打开远端文件' : '打开本地文件',
      message: isRemote
        ? '输入远端文件的绝对路径（会话为当前选中资产）。'
        : '输入本地文件的完整路径（支持 ~ 开头，家目录展开）。',
      input: {
        label: '路径',
        placeholder: isRemote ? '/etc/nginx/nginx.conf' : 'C:\\Users\\me\\notes.txt',
        value: preset
      },
      buttons: [
        { label: '取消' },
        {
          label: '打开',
          primary: true,
          action: async inputValue => {
            const path = (inputValue || '').trim();
            if (!path) return '请输入路径';
            await openTarget(
              { kind, assetId: isRemote ? (wb().selectedAsset?.id ?? null) : null, path },
              {}
            );
          }
        }
      ]
    });
  }

  function hideSurface(): void {
    surfaceVisible.value = false;
  }

  function errorRetryable(tab: EditorTab): boolean {
    return tab.error ? isErrorKindRetryable(tab.error.kind) : false;
  }

  return {
    tabs,
    activeTabId,
    activeTab,
    surfaceVisible,
    savingTabIds,
    dirtyCount,
    editorBackupEnabled,
    // 内容通道（组件侧）
    registerContentProvider,
    currentContent,
    contentSnapshot,
    replaceContent,
    markDirty,
    // tab 操作
    setActive,
    openTarget,
    openFromPathPrompt,
    requestCloseTab,
    hideSurface,
    errorRetryable,
    setBackupEnabled,
    // 弹窗
    openDialog,
    resolveEditorDialog,
    // 流程域 re-export（lib/editor/editorFlows.ts）
    saveTab,
    saveAllEditorTabs,
    discardAllDirty,
    openSaveAsDialog,
    uploadToRemote,
    saveDraftAction,
    reloadTab,
    retryTab,
    setEncoding,
    setEol,
    tabEolChoice,
    listBackupsAction
  };
});
