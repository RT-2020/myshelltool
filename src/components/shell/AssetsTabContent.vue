<script setup lang="ts">
/**
 * AssetsTabContent — 设置·资产 tab（v0.20，SSH P0-1）。
 *
 * 当前内容：OpenSSH config 导入入口（预览/勾选/导入流程在 SshImportDialog
 * 的 AppModal 里，App.vue 根级挂载——不走 GlobalModals）。后续批量执行等
 * 资产域设置在此追加。
 */
import { HardDriveDownload, Server } from 'lucide-vue-next';
import { useAssetsStore } from '@/stores/assets';
import AppButton from '@/components/ui/AppButton.vue';

const assetsStore = useAssetsStore();
</script>

<template>
  <div class="stack">
    <section class="block">
      <header class="block-head"><HardDriveDownload :size="12" />从 OpenSSH config 导入</header>
      <p class="muted">
        解析 <code>~/.ssh/config</code>（或指定文件）的 Host 块，一键批量创建资产：
        预览全部主机、标记与现有资产冲突项、自选导入分组。IdentityFile 映射为
        私钥路径认证；密码类主机导入后首次连接时输入并保存。
      </p>
      <AppButton variant="primary" size="sm" @click="assetsStore.openImportDialog()">
        <Server :size="12" />打开导入向导…
      </AppButton>
    </section>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.block {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md, 8px);
  background: var(--bg-elevated, var(--app-surface));
}
.block-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-secondary, var(--app-muted));
}
</style>
