/**
 * editorFlows — 编辑器保存/冲突/草稿/备份/编码切换的流程域（v0.18）。
 *
 * 从 stores/editor.ts 按域拆出（绑定式 context，先例 lib/fileTransfers）：
 * store 保留状态/内容通道/tab 管理，本模块承载「写盘链路 + 弹窗编排」。
 * 单向依赖：store → 本模块（bind 注入内部件）；本模块不 import store。
 */
import type {
  EditorClassifiedError,
  EditorDialogButton,
  EditorDialogPayload,
  EditorTab,
  EditorTarget
} from '@/types/editor';
import {
  deleteDraft,
  getDraft,
  listBackups,
  readBackup,
  readText,
  saveDraft,
  toClassifiedError,
  writeText
} from '@/lib/editor/editorIo';
import { basenameOf } from '@/lib/editor/editorPaths';

/** 流程域需要的 store 内部件（bind 于 workbench 挂载时）。 */
export interface EditorFlowContext {
  tabs(): EditorTab[];
  savingTabIds(): Record<string, boolean>;
  announce(message: string, opts?: { level?: string; action?: { label: string; run: () => void } }): void;
  currentContent(tabId: string): string;
  replaceContent(tabId: string, text: string): void;
  markDirty(tabId: string, dirty: boolean): void;
  openDialog(payload: EditorDialogPayload): void;
  loadTab(tab: EditorTab): Promise<void>;
  makeTab(target: EditorTarget, encodingOverride: string | null): EditorTab;
  resolveRemoteSession(target: EditorTarget): { sessionId: string } | { error: EditorClassifiedError };
  setActiveTab(tabId: string): void;
  showSurface(): void;
  /** 选中资产 id（上传到远端/远端探测的会话归属）。 */
  selectedAssetId(): string | null;
  selectedAssetName(): string | null;
  remotePathPrefix(): string;
  isBackupEnabled(): boolean;
  /** 保存成功后刷新对应文件面板（大小/mtime 已变）。 */
  refreshPanels(kind: 'remote' | 'local'): void;
}

let bound: EditorFlowContext | null = null;

import { bindEditorTargets, openSaveAsDialog, uploadToRemote } from '@/lib/editor/editorRemoteTargets';

export function bindEditorFlows(ctx: EditorFlowContext): void {
  bound = ctx;
  // v0.20（S2 刀）：另存为/上传流程级联注入依赖（同一 ctx 域的 helper）
  bindEditorTargets({ fc, findTab, probeRemoteExists, performWrite });
}

function fc(): EditorFlowContext {
  if (!bound) throw new Error('editorFlows 未绑定 context（editor store 未初始化）');
  return bound;
}

function findTab(tabId: string): EditorTab | undefined {
  return fc().tabs().find(t => t.id === tabId);
}

/** 用户显式切换过的 EOL（未切过沿用基线推断：crlf→crlf，其余→lf）。 */
const eolOverrides = new Map<string, 'lf' | 'crlf'>();

export function tabEolChoice(tab: EditorTab): 'lf' | 'crlf' {
  const override = eolOverrides.get(tab.id);
  if (override) return override;
  return tab.baseline?.eol === 'crlf' ? 'crlf' : 'lf';
}

export function setEol(tabId: string, eol: 'lf' | 'crlf'): void {
  const tab = findTab(tabId);
  if (!tab) return;
  eolOverrides.set(tabId, eol);
  fc().markDirty(tabId, true);
}

export function dropEolOverride(tabId: string): void {
  eolOverrides.delete(tabId);
}

/** 远端存在性探测：'yes' | 'no' | 'error'（fail-closed；读得出 too-large/binary/
 *  not-utf8 类错误也证明目标存在）。 */
export async function probeRemoteExists(path: string, assetId: string | null): Promise<'yes' | 'no' | 'error'> {
  try {
    await readText({ kind: 'remote', assetId, path }, { sessionId: requireRemoteSession(assetId) });
    return 'yes';
  } catch (error) {
    const classified = toClassifiedError(error);
    if (classified.kind === 'not-found') return 'no';
    if (classified.kind === 'too-large' || classified.kind === 'binary' || classified.kind === 'not-utf8') return 'yes';
    return 'error';
  }
}

function requireRemoteSession(assetId: string | null): string {
  const resolved = fc().resolveRemoteSession({ kind: 'remote', assetId, path: '' });
  if ('error' in resolved) throw new Error(resolved.error.message);
  return resolved.sessionId;
}

function openSaveConflictDialog(tab: EditorTab): void {
  fc().openDialog({
    title: '文件已被修改',
    message: `「${tab.name}」在打开后被其他人/进程修改（${tab.conflict?.note ?? ''}）。`,
    detail: '覆盖保存会丢失对方的改动；重新加载会丢弃你本地的修改；另存为会保留两边。',
    buttons: [
      { label: '取消' },
      {
        label: '重新加载',
        action: () => {
          void reloadTab(tab.id, { skipConfirm: true });
        }
      },
      {
        label: '另存为…',
        action: () => {
          openSaveAsDialog(tab);
        }
      },
      {
        label: '覆盖保存',
        danger: true,
        primary: true,
        action: () => {
          void performWrite(tab, { force: true });
        }
      }
    ]
  });
}

interface PerformWriteOptions {
  force?: boolean;
  newPath?: string;
  targetKind?: 'remote' | 'local';
  /** 上传到远端（本地 tab）时的目标资产；缺省沿用 tab 自身资产。 */
  assetId?: string | null;
  announceSuccess?: string;
}

async function performWrite(tab: EditorTab, opts: PerformWriteOptions = {}): Promise<boolean> {
  let sessionId: string | undefined;
  const effectiveTarget: EditorTarget = opts.newPath
    ? {
        kind: opts.targetKind ?? tab.target.kind,
        assetId: opts.assetId !== undefined
          ? opts.assetId
          : (opts.targetKind === 'local' ? null : tab.target.assetId),
        path: opts.newPath
      }
    : tab.target;
  if (effectiveTarget.kind === 'remote') {
    const resolved = fc().resolveRemoteSession(effectiveTarget);
    if ('error' in resolved) {
      fc().announce(resolved.error.message, { level: 'warn' });
      return false;
    }
    sessionId = resolved.sessionId;
  }
  const content = fc().currentContent(tab.id);
  const baseline = tab.baseline;
  const eolChoice = tabEolChoice(tab);
  try {
    const result = await writeText(effectiveTarget, {
      sessionId,
      content,
      encoding: tab.encodingOverride ?? (baseline?.encoding && baseline.encoding !== 'utf-8' ? baseline.encoding : null),
      eol: eolChoice,
      keepBom: Boolean(baseline?.hasBom),
      expectedSize: opts.force || opts.newPath ? null : baseline?.size ?? null,
      expectedModified: opts.force || opts.newPath ? null : baseline?.modified ?? null,
      backup: fc().isBackupEnabled()
    });
    if (result.status === 'conflict') {
      tab.conflict = { note: result.conflictNote || '远端内容已变化', size: result.size, modified: result.modified };
      openSaveConflictDialog(tab);
      return false;
    }
    // 成功：更新基线与 tab 状态
    tab.baseline = {
      size: result.size,
      modified: result.modified,
      encoding: tab.encodingOverride ?? baseline?.encoding ?? 'utf-8',
      eol: eolChoice === 'crlf' ? 'crlf' : 'lf',
      hasBom: Boolean(baseline?.hasBom)
    };
    tab.dirty = false;
    tab.conflict = null;
    if (opts.newPath) {
      void deleteDraft(tab.target).catch(() => undefined);
      tab.target = effectiveTarget;
      tab.name = basenameOf(effectiveTarget.path);
    }
    void deleteDraft(tab.target).catch(() => undefined);
    // 刷新对应文件面板（大小/mtime 已变）
    fc().refreshPanels(effectiveTarget.kind);
    fc().announce(opts.announceSuccess ?? `已保存：${tab.name}`, { level: 'success' });
    return true;
  } catch (error) {
    const classified = toClassifiedError(error);
    fc().announce(`保存失败：${classified.message}`, {
      level: 'error',
      action: { label: '重试', run: () => { void saveTab(tab.id); } }
    });
    return false;
  }
}

/** 保存（含保存前按类型校验的「仍要保存？」确认）。返回是否成功落盘。 */
export async function saveTab(tabId: string): Promise<boolean> {
  const tab = findTab(tabId);
  if (!tab || tab.status !== 'ready') return false;
  if (tab.readOnly) {
    fc().announce(tab.readOnlyReason || '文件为只读，无法保存', { level: 'warn' });
    return false;
  }
  if (fc().savingTabIds()[tabId]) return false;
  fc().savingTabIds()[tabId] = true;
  try {
    const { validateForLanguage } = await import('@/lib/editor/editorValidation');
    const { languageIdForName } = await import('@/lib/editor/editorLanguages');
    const issue = await validateForLanguage(languageIdForName(tab.name), fc().currentContent(tabId));
    if (!issue.ok) {
      return await new Promise<boolean>(resolve => {
        fc().openDialog({
          title: '语法检查未通过',
          message: issue.message,
          detail: '仍要保存可能让这份配置在运行时解析失败。',
          buttons: [
            { label: '取消', action: () => resolve(false) },
            {
              label: '仍要保存',
              danger: true,
              primary: true,
              action: () => {
                void performWrite(tab).then(resolve);
              }
            }
          ]
        });
      });
    }
    return await performWrite(tab);
  } finally {
    delete fc().savingTabIds()[tabId];
  }
}

/** 关窗前「全部保存」：任一失败（冲突/校验被取消）返回 false 中止关窗。 */
export async function saveAllEditorTabs(): Promise<boolean> {
  for (const dirtyTab of fc().tabs().filter(t => t.dirty && t.status === 'ready')) {
    const ok = await saveTab(dirtyTab.id);
    if (!ok) return false;
  }
  return true;
}

/** 关窗前「放弃全部」：清 dirty（不删草稿）供放行关闭流程。 */
export function discardAllDirty(): void {
  for (const t of fc().tabs()) t.dirty = false;
}

/** 保存草稿（显式动作；内容按 UTF-8 暂存，不落目标文件）。 */
export async function saveDraftAction(tabId: string): Promise<void> {
  const tab = findTab(tabId);
  if (!tab) return;
  try {
    await saveDraft(tab.target, fc().currentContent(tabId), tab.encodingOverride ?? 'utf-8', tabEolChoice(tab));
    fc().announce(`草稿已保存：${tab.name}`, { level: 'info' });
  } catch (error) {
    fc().announce(`保存草稿失败：${toClassifiedError(error).message}`, { level: 'error' });
  }
}

/** 重新加载（丢弃本地改动重读原文件）。 */
export async function reloadTab(tabId: string, opts: { skipConfirm?: boolean } = {}): Promise<void> {
  const tab = findTab(tabId);
  if (!tab) return;
  if (tab.dirty && !opts.skipConfirm) {
    fc().openDialog({
      title: '重新加载',
      message: `重新加载会丢弃「${tab.name}」的全部未保存修改。`,
      buttons: [
        { label: '取消' },
        {
          label: '重新加载',
          danger: true,
          primary: true,
          action: () => {
            void reloadTab(tabId, { skipConfirm: true });
          }
        }
      ]
    });
    return;
  }
  await fc().loadTab(tab);
}

/** 切换编码：按新编码重读原文件（dirty 时先确认丢弃）。 */
export function setEncoding(tabId: string, label: string | null): void {
  const tab = findTab(tabId);
  if (!tab) return;
  const apply = () => {
    tab.encodingOverride = label;
    void fc().loadTab(tab);
  };
  if (tab.dirty) {
    fc().openDialog({
      title: '切换编码',
      message: `切换编码将按「${label ?? 'UTF-8'}」重新加载原文件，未保存的修改会丢弃。`,
      detail: '如需保留修改，请先保存或保存草稿。',
      buttons: [
        { label: '取消' },
        { label: '切换并重载', danger: true, primary: true, action: apply }
      ]
    });
    return;
  }
  apply();
}

/** 出错 tab 的重试入口。 */
export function retryTab(tabId: string): void {
  const tab = findTab(tabId);
  if (!tab) return;
  void fc().loadTab(tab);
}

/** 草稿恢复（loadTab 成功后调用）：草稿比文件新 → 恢复/丢弃。 */
export async function offerDraftRecovery(tab: EditorTab, fileModifiedSec: string): Promise<void> {
  try {
    const draft = await getDraft(tab.target);
    const fileMtimeMs = Number(fileModifiedSec) * 1000;
    if (!draft) return;
    if (Number.isFinite(fileMtimeMs) && Number(draft.savedAt) <= fileMtimeMs) return;
    fc().openDialog({
      title: '发现较新的草稿',
      message: `「${tab.name}」有一份 ${new Date(Number(draft.savedAt)).toLocaleString()} 保存的草稿，比文件本体新。`,
      detail: '恢复草稿会把草稿内容载入编辑器（未保存状态）；丢弃将删除这份草稿。',
      buttons: [
        { label: '丢弃草稿', action: () => { void deleteDraft(tab.target); } },
        {
          label: '恢复草稿',
          primary: true,
          action: () => {
            fc().replaceContent(tab.id, draft.content);
            tab.encodingOverride = draft.encoding !== 'utf-8' ? draft.encoding : tab.encodingOverride;
          }
        }
      ]
    });
  } catch {
    // 草稿探测失败不阻断打开（正文已就绪；草稿是可再生便利数据）
  }
}

/** 查看备份（列表 → 选择载入为只读新 tab）。 */
export async function listBackupsAction(tabId: string): Promise<void> {
  const tab = findTab(tabId);
  if (!tab || !tab.baseline) return;
  try {
    const versions = await listBackups(tab.target);
    if (!versions.length) {
      fc().announce('该文件还没有备份（保存前备份默认开启，保留最近 3 份）', { level: 'info' });
      return;
    }
    const buttons: EditorDialogButton[] = versions.slice(0, 3).map(v => ({
      label: `${new Date(Number(v.at)).toLocaleString()}（${v.size} 字节）`,
      action: () => {
        void loadBackupView(tab, v.id);
      }
    }));
    buttons.push({ label: '关闭' });
    fc().openDialog({
      title: `「${tab.name}」的备份（最近 ${versions.length} 份）`,
      message: '选择一份备份载入为新 tab（只读查看，可另存为新文件）。',
      buttons
    });
  } catch (error) {
    fc().announce(`读取备份列表失败：${toClassifiedError(error).message}`, { level: 'error' });
  }
}

async function loadBackupView(tab: EditorTab, backupId: string): Promise<void> {
  try {
    const result = await readBackup(tab.target, backupId, tab.encodingOverride);
    const viewTab = fc().makeTab(
      { kind: tab.target.kind, assetId: tab.target.assetId, path: `${tab.target.path}（备份 ${new Date(Number(backupId)).toLocaleString()}）` },
      tab.encodingOverride
    );
    viewTab.status = 'ready';
    viewTab.readOnly = true;
    viewTab.readOnlyReason = '备份快照为只读视图，可另存为新文件';
    viewTab.baseline = {
      size: result.size,
      modified: '',
      encoding: result.encoding,
      eol: result.eol,
      hasBom: result.hasBom
    };
    fc().tabs().push(viewTab);
    fc().setActiveTab(viewTab.id);
    fc().showSurface();
    // 内容经 store 的 replaceContent 通道落快照（新 tab 未挂载 host）
    fc().replaceContent(viewTab.id, result.content);
    viewTab.dirty = false;
  } catch (error) {
    fc().announce(`读取备份失败：${toClassifiedError(error).message}`, { level: 'error' });
  }
}

// v0.20（S2 刀）：另存为/上传流程拆至 editorRemoteTargets.ts（调用方路径不变）
export { openSaveAsDialog, uploadToRemote };
