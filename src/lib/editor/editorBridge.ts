/**
 * editorBridge — editor store 的 workbench 桥接接线（v0.18）。
 *
 * 从 stores/workbench.ts 抽出（行数红线：workbench ≤500）。桥接成员与
 * files/tunnels 等子 store 的 attachWorkbench 同构：announce/modal/资产与
 * 面板路径。会话解析在 editor 侧复用 lib/filePanel（不经此桥）。
 */
import { bindEditorStoreBridge } from '@/stores/editor';
import type { useUiStore } from '@/stores/ui';
import type { useFilesStore } from '@/stores/files';
import type { useAssetsStore } from '@/stores/assets';
import type { NormalizedConnectionAsset, NotifyOptions } from '@/types/domain';

interface EditorBridgeDeps {
  announce(message: string, opts?: NotifyOptions): void;
  uiStore: ReturnType<typeof useUiStore>;
  assetsStore: ReturnType<typeof useAssetsStore>;
  filesStore: ReturnType<typeof useFilesStore>;
}

export function attachEditorBridge(deps: EditorBridgeDeps): void {
  const { announce, uiStore, assetsStore, filesStore } = deps;
  bindEditorStoreBridge({
    announce,
    get modal() { return uiStore.modal; },
    set modal(v) { uiStore.modal = v; },
    assets: () => assetsStore.assets as NormalizedConnectionAsset[],
    get selectedAsset() { return assetsStore.selectedAsset; },
    get localPath() { return filesStore.localPath; },
    get remotePath() { return filesStore.remotePath; },
    refreshRemoteFiles: (path) => filesStore.refreshRemoteFiles(path),
    refreshLocalFiles: (path) => filesStore.refreshLocalFiles(path)
  });
}
