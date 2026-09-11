<script setup lang="ts">
/**
 * SyncPanelContent — 「设置 → 同步」页（v2.7 重设计）。
 *
 * 信息架构（这一版的核心决定）：**状态优先、动作其次、配置折叠**。
 *   ┌ 状态管线（SyncStatusRail）  本机 ─▶ 加密 ─▶ Gist + 状态词 + 上次同步
 *   ├ 冲突（有冲突时独占，SyncConflictResolver）
 *   ├ 动作（SyncActionBar）       推送到云端 / 从云端拉取（主次由状态决定）
 *   └ 设置与高级（SyncAdvancedSettings，默认收起）
 *      免密 · 密码与密钥 · GitHub 账号 · 重置主密码 · 换机 · 清空
 *
 * 上一版把「主密码输入框」放在日常视图正中央、把 5 个配置卡片平铺在下半屏，
 * 结果是：每次同步前都要先判断"要不要输密码"，而"现在该推还是该拉"没有任何信号。
 * 这一版把两件事分别解决：密码行只在未免密时出现（并说明"输一次就够"），
 * 「该推还是该拉」由后端新增的 `local_has_changes` + 远端探测共同驱动按钮主次。
 *
 * 未配置账号/同步时只展示对应的单一步骤（登录卡 / 创建表单），不展示动作与设置。
 */
import { onMounted } from 'vue';
import { storeToRefs } from 'pinia';
import { useWorkbenchStore } from '@/stores/workbench';
import SyncStatusRail from '@/components/shell/SyncStatusRail.vue';
import SyncActionBar from '@/components/shell/SyncActionBar.vue';
import SyncAdvancedSettings from '@/components/shell/SyncAdvancedSettings.vue';
import SyncConflictResolver from '@/components/shell/SyncConflictResolver.vue';
import SyncSetupForm from '@/components/shell/SyncSetupForm.vue';
import PatConfigCard from '@/components/shell/PatConfigCard.vue';

const store = useWorkbenchStore();
const { syncConfigured, syncConflict, githubPatConfigured } = storeToRefs(store);

// 打开本页时刷一次真实状态 + 探一次远端：面板上的每个字都该是当前值，
// 而不是启动那一刻的快照（上次同步时间、是否待推送、云端有没有新版本）。
onMounted(() => {
  store.syncRefreshStatus();
  store.syncCheckRemoteUpdates();
});
</script>

<template>
  <div class="sync-panel">
    <!-- 读数 + 操作同属一块「同步面板」：同一个对象的两个面（状态 / 控制）。
         未配置时 SyncActionBar 自行隐藏，这里只剩状态读数 —— 结构一致，不出现两张卡。 -->
    <section class="sync-surface">
      <SyncStatusRail />
      <SyncActionBar />
    </section>

    <!-- 冲突优先：它是唯一需要用户在两个数据版本之间做决定的状态 -->
    <SyncConflictResolver v-if="syncConflict" />

    <!-- 已配置：配置折叠在一个入口里 -->
    <SyncAdvancedSettings v-if="syncConfigured" />

    <!-- 未配置：只给当下这一步（登录 → 创建/恢复），不提前展示配置 -->
    <template v-else>
      <PatConfigCard v-if="!githubPatConfigured" />
      <SyncSetupForm v-else />
    </template>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.sync-panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  font-size: var(--text-sm);
  // 不自带滚动：AppModal body 是唯一滚动容器，避免滚动条嵌套
}

/* 读数 + 操作合成一块面板：外框/圆角只在这里出现一次，内部用 hairline 分隔两个面。
   之前它们是两张卡，视觉上像两件无关的事，而用户的心智是"这是同一个同步开关"。 */
.sync-surface {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  background: var(--app-panel);
  overflow: hidden; // 让左状态色条贴合圆角
}
.sync-surface > * + * {
  border-top: 1px solid var(--app-border-soft);
}
</style>
