/**
 * sshImport — OpenSSH config 导入流程域（v0.20，SSH P0-1）。
 *
 * 后端链路：`import_ssh_config_preview`（core::ssh_config 解析 + 冲突比对）
 * → 本模块组装 ConnectionAssetInput → 复用 assets store 的 saveAsset
 * （与手工建资产完全相同的 ID 生成/校验/凭据链路，不另写第二条保存路径）。
 *
 * store 只挂薄壳（importOpen 状态），重活在本模块——assets.ts 贴近 500 行
 * 硬上限，照 editorFlows/fileTransfers 的「store 持状态、lib 干重活」先例。
 */
import type {
  ConnectionAssetInput,
  SshImportCandidate,
  SshImportPreview
} from '@/types/domain';
import { invokeBackend } from '@/services/backend';

/** 拉取预览（path 缺省 = 后端解析 ~/.ssh/config）。 */
export async function previewSshConfig(path?: string): Promise<SshImportPreview> {
  return invokeBackend<SshImportPreview>('import_ssh_config_preview', path ? { path } : {});
}

/** 候选 → 资产输入。auth_method 取 PrivateKey（有 IdentityFile 时）否则 Password。 */
export function candidateToAssetInput(
  candidate: SshImportCandidate,
  group: string
): ConnectionAssetInput {
  const hasKey = Boolean(candidate.identityFile);
  return {
    id: '',
    name: candidate.alias,
    host: candidate.host,
    port: candidate.port,
    username: candidate.username || '',
    auth_method: hasKey ? 'PrivateKey' : 'Password',
    private_key_path: candidate.identityFile || undefined,
    jump_host: candidate.proxyJump || undefined,
    group,
    tags: []
  };
}

/** 导入上下文（assets store 注入 saveAsset + reload，保持本模块 store-agnostic）。 */
export interface SshImportContext {
  saveAsset(input: ConnectionAssetInput): Promise<unknown>;
}

export interface SshImportOutcome {
  ok: number;
  failed: { alias: string; error: string }[];
}

/** 逐条导入（顺序执行：saveAsset 内部有磁盘查重对账，并发会互相看到中间态）。 */
export async function importCandidates(
  ctx: SshImportContext,
  candidates: SshImportCandidate[],
  group: string
): Promise<SshImportOutcome> {
  const outcome: SshImportOutcome = { ok: 0, failed: [] };
  for (const candidate of candidates) {
    try {
      await ctx.saveAsset(candidateToAssetInput(candidate, group));
      outcome.ok += 1;
    } catch (error) {
      outcome.failed.push({
        alias: candidate.alias,
        error: String(error instanceof Error ? error.message : error)
      });
    }
  }
  return outcome;
}
