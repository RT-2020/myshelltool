<script setup lang="ts">
/**
 * SecurityPromptContent — 安全审批类弹窗体（hostKeyVerify / mcpApproval）。
 *
 * 视觉层级遵循 docs/设计/10-弹窗体系与全局反馈.md §3.2：
 * - hostKeyVerify 发生密钥变更（is_changed）时，danger-note 警示条置顶；
 * - 主机 / 指纹 / 命令等判断依据使用 mono 等宽网格对齐，避免截断。
 */
import type { HostKeyVerifyPayload, McpApprovalPrompt } from '@/types/domain';

defineProps<{
  kind: 'hostKeyVerify' | 'mcpApproval';
  hostKeyPrompt: HostKeyVerifyPayload | null;
  mcpApprovalPrompt: McpApprovalPrompt | null;
}>();
</script>

<template>
  <div v-if="kind === 'hostKeyVerify'" class="stack">
    <p v-if="hostKeyPrompt?.is_changed" class="danger-note">
      警告：远程主机密钥与本地 known_hosts 记录不一致，可能存在中间人攻击或重装系统风险！
    </p>
    <p v-else class="muted">首次连接该主机，请逐字核对指纹后决定是否信任。</p>
    <dl class="context-grid">
      <dt>主机</dt><dd class="num break-all">{{ hostKeyPrompt?.host_port }}</dd>
      <dt>密钥类型</dt><dd class="num">{{ hostKeyPrompt?.key_type }}</dd>
      <dt>指纹</dt><dd class="num break-all">{{ hostKeyPrompt?.fingerprint }}</dd>
      <dt>状态</dt><dd>{{ hostKeyPrompt?.is_changed ? '密钥已变更' : '首次连接' }}</dd>
    </dl>
    <p class="muted">确认后将会保存到本地 known_hosts，下次连接不再提示。</p>
  </div>

  <div v-else class="stack">
    <p class="muted">外部 AI 客户端请求执行高危命令，请确认三段信息是否一致：</p>
    <dl class="context-grid">
      <dt>AI 声明意图</dt><dd>{{ mcpApprovalPrompt?.intent || '(未声明)' }}</dd>
      <dt>真实命令</dt><dd class="num break-all">{{ mcpApprovalPrompt?.command }}</dd>
      <dt>后果预测</dt><dd>{{ mcpApprovalPrompt?.consequence }}</dd>
    </dl>
    <p class="muted">核对意图与命令是否相符后再确认。拒绝或关闭都会阻止执行。</p>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_form-feedback' as *;

.danger-note {
  @include danger-note-box;
}
.context-grid {
  display: grid;
  grid-template-columns: auto 1fr;
  gap: var(--space-1) var(--space-3);
  margin: var(--space-2) 0;
}
.context-grid dt {
  color: var(--app-muted);
  font-size: var(--text-xs);
}
.break-all {
  word-break: break-all;
}
</style>
