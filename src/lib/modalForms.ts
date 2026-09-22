/**
 * modalForms — 弹窗表单的空值工厂与克隆（从 GlobalModals.vue 抽出，拆分第二刀）。
 * 纯函数：只产新值，不触 store / 组件。
 */
import type {
  AssetEditorForm,
  CredentialEditorForm,
  ModalState,
  TunnelEditorForm
} from '@/types/domain';

export function emptyAsset(): AssetEditorForm {
  return {
    id: '',
    name: '',
    host: '',
    port: 22,
    username: '',
    auth_method: 'Password',
    private_key_path: '',
    jump_host: '',
    connect_timeout_secs: '',
    keepalive_interval_secs: '',
    group: '未分组',
    tags: '',
    status: 'Idle',
    credential_id: null,
    passphrase_credential_id: null,
    private_key_credential_id: null
  };
}

export function emptyCredential(): CredentialEditorForm {
  // clearPassword / clearPassphrase / clearPrivateKey：编辑器「清除」按钮的标记（保存时删除凭据）。
  // 与重新输入互斥：输入框有值会重置对应标记。
  return {
    password: '',
    passphrase: '',
    privateKey: '',
    clearPassword: false,
    clearPassphrase: false,
    clearPrivateKey: false
  };
}

export function emptyTunnelForm(): TunnelEditorForm {
  return {
    name: '',
    kind: 'local',
    local_addr: '127.0.0.1',
    local_port: '',
    remote_addr: '',
    remote_port: '',
    auto_start: false
  };
}

export function cloneAsset(asset: NonNullable<ModalState['asset']>): AssetEditorForm {
  return {
    ...asset,
    tags: asset.tags.join(', '),
    private_key_path: asset.private_key_path || '',
    jump_host: asset.jump_host || '',
    connect_timeout_secs: asset.connect_timeout_secs ?? '',
    keepalive_interval_secs: asset.keepalive_interval_secs ?? '',
  };
}
