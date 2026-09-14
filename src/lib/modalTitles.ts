/**
 * modalTitles — 弹窗类型 → 人类可读标题的单一映射（GlobalModals 标题栏）。
 *
 * 从 GlobalModals.vue 抽出（拆分第一轮）：纯映射无组件依赖；
 * assetEditor 的标题取决于编辑态/新建态，由调用方传布尔。
 */
import type { ModalState } from '@/types/domain';


export function modalTitleFor(
  type: ModalState['type'],
  opts: { editingExistingAsset: boolean }
): string {
  switch (type) {
    case 'assetEditor': return opts.editingExistingAsset ? '编辑连接资产' : '新增连接资产';
    case 'reauthPassword': return '重新输入密码';
    case 'tunnelCreate': return '新增隧道';
    case 'tokenConfig': return '配置 / 更新 GitHub token';
    case 'hostKeyVerify': return '主机密钥验证';
    case 'keyboardInteractive': return '键盘交互认证';
    case 'mcpApproval': return 'MCP 高危操作审批';
    case 'mcpPanel': return 'MCP 服务管理';
    case 'syncPanel': return '资产同步（Gist）';
    case 'settings': return '设置';
    case 'mkdir': return '新建远程目录';
    case 'rename': return '重命名远程条目';
    case 'localMkdir': return '新建本地目录';
    case 'localRename': return '重命名本地条目';
    case 'terminalSearch': return '终端搜索';
    case 'confirmDelete': return '删除连接资产';
    case 'confirmFileDelete': return '删除文件确认';
    case 'confirmFileOverwrite': return '覆盖同名文件？';
    case 'renameGroup': return '重命名分组';
    case 'createGroup': return '新建分组';
    case 'moveAsset': return '移动到分组';
    case 'confirmCloseAssetWindow': return '关闭独立窗口';
    default: return '提示';
  }
}
