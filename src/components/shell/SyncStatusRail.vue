<script setup lang="ts">
/**
 * SyncStatusRail — 同步状态管线（面板的签名元素）。
 *
 * 为什么是"管线"而不是状态徽章：用户此前反复问的就是「我现在该按哪个按钮」。
 * 备份是**有方向**的：本机 → 加密 → Gist。把方向画出来，并在**需要动的那一段**上挂状态
 * （本机有未推送改动 / 云端有新版本），按钮的主次随之改变 —— 结构本身就在回答那个问题。
 * 三段也正好把此前最让人困惑的两层模型讲开：账号层（Gist 是谁的）与加密层（内容怎么保护）。
 *
 * 设计取舍（与本项目设计系统一致）：
 * - 只用既有 token（颜色/间距/圆角/动效），不引入新色值或新字体族；
 * - 读数（状态词/数量/rev 尾号/时间）用 `--font-mono`：这是终端工具的"仪器字体"，
 *   且让数量与时间在竖直方向对齐 —— 不是为了装饰；
 * - 唯一动效留给"数据正在走"这件事（推送/拉取进行中箭头行进），
 *   `prefers-reduced-motion` 下不动、只改文案；
 * - 节点不可点（不制造额外 tab 陷阱），整条 rail 是只读读数。
 */
import { computed } from 'vue';
import { storeToRefs } from 'pinia';
import { ArrowRight, Cloud, HardDrive, Lock, TriangleAlert } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useSyncStore } from '@/stores/sync';

const store = useWorkbenchStore();
const { syncConfigured, syncLoading, syncCredentialsEnabled } = storeToRefs(store);
// 直接 use 子 store（先例：resourceMonitor panel / useGithubDeviceLogin）——
// 避免为多一个字段就去动 517 行、已超硬上限的 workbench 编排壳。
const syncStore = useSyncStore();
const { localHasChanges, autoSyncEnabled, remoteHasUpdates, lastSyncedAt, gistIdMasked, status } =
  storeToRefs(syncStore);

/** 资产台数（读数用；拿不到就只显示"本机"）。 */
const assetCount = computed(() => {
  const n = store.assets?.length;
  return typeof n === 'number' && n >= 0 ? n : null;
});

/** 上次同步：本地时区的短格式（08-14 11:30）。 */
const lastSyncShort = computed(() => {
  const iso = lastSyncedAt.value;
  if (!iso) return '从未同步';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
});

/** Gist 尾号（已是 `...abc123` 形态，直接去掉前导点更利落）。 */
const gistTail = computed(() => (gistIdMasked.value || '').replace(/^\.+/, ''));

type Tone = 'ok' | 'pending' | 'danger' | 'idle';

/**
 * 状态词与色调。**优先级刻意如此**：异常 > 待办方向 > 稳定。
 * 每个分支都必须能被用户翻译成"下一步做什么"，否则不该出现在这里。
 *
 * `hint` 只在"按钮本身说不清下一步"时给（未配置/冲突/进行中）；
 * 已同步、有待推送这类状态，状态词 + 主按钮已经说完了 —— 再解释一遍是噪音。
 */
const state = computed<{ tone: Tone; label: string; hint: string | null }>(() => {
  if (syncLoading.value) {
    return { tone: 'pending', label: '同步中', hint: '正在与 GitHub 通信，完成后这里会更新。' };
  }
  if (!syncConfigured.value) {
    return status.value?.pat_configured
      ? { tone: 'idle', label: '未开启同步', hint: '还差一步：设置主密码，创建加密备份。' }
      : { tone: 'idle', label: '未连接账号', hint: '先登录 GitHub，备份会存进你的私有 Gist。' };
  }
  if (localHasChanges.value && remoteHasUpdates.value) {
    return { tone: 'danger', label: '两边都有改动', hint: '先拉取再推送，或让应用帮你比较两份改动。' };
  }
  if (localHasChanges.value) {
    return { tone: 'pending', label: '有改动待推送', hint: null };
  }
  if (remoteHasUpdates.value) {
    return { tone: 'pending', label: '云端有新版本', hint: null };
  }
  return { tone: 'ok', label: '已同步', hint: null };
});

/** 进行中的方向（驱动唯一动效）：推送=向外，拉取=向内。 */
const movingDirection = computed<'out' | 'in' | null>(() => {
  const op = syncStore.activeOp;
  if (!syncLoading.value || !op) return null;
  return op === 'push' ? 'out' : 'in';
});
</script>

<template>
  <section class="rail-card" :class="`tone-${state.tone}`">
    <!-- 管线：本机 ─▶ 加密 ─▶ Gist（节点标签 11px uppercase，读数 mono） -->
    <div class="rail" role="group" aria-label="同步链路">
      <span class="node" :class="{ 'is-hot': state.tone === 'pending' && localHasChanges && !remoteHasUpdates }">
        <HardDrive :size="12" />
        <span class="node-label">本机</span>
        <span v-if="assetCount !== null" class="node-read num">{{ assetCount }} 台</span>
      </span>

      <span class="link" aria-hidden="true">
        <span class="link-line" />
        <ArrowRight :size="12" class="link-arrow" :class="{ moving: movingDirection === 'out' }" />
      </span>

      <span class="node">
        <Lock :size="12" />
        <span class="node-label">加密</span>
        <span class="node-read num">{{ syncCredentialsEnabled ? '含密码' : '仅资产' }}</span>
      </span>

      <span class="link" aria-hidden="true">
        <span class="link-line" />
        <ArrowRight :size="12" class="link-arrow" :class="{ moving: movingDirection === 'in' }" />
      </span>

      <span class="node" :class="{ 'is-hot': remoteHasUpdates }">
        <Cloud :size="12" />
        <span class="node-label">Gist</span>
        <span class="node-read num">{{ syncConfigured ? `…${gistTail}` : '未连接' }}</span>
      </span>
    </div>

    <!-- 状态行：色点 + 状态词（mono）+ 右侧上次同步（未建立备份时不显示，rail 已说明状态） -->
    <div class="readout">
      <span class="state" :class="`tone-${state.tone}`">
        <span class="dot" aria-hidden="true" />
        <span class="state-label num">{{ state.label }}</span>
      </span>
      <span v-if="syncConfigured" class="last num">
        <TriangleAlert v-if="state.tone === 'danger'" :size="11" />
        上次同步 {{ lastSyncShort }}
      </span>
    </div>
    <p v-if="state.hint" class="hint">{{ state.hint }}</p>
  </section>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

// 读数块：不再自带外框（外层 .sync-surface 提供唯一的卡片），
// 只保留"状态色左边线"这一个强信号 + padding。
.rail-card {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-left: 3px solid var(--app-border-strong);
  background: var(--app-panel-2);
}
.rail-card.tone-ok { border-left-color: var(--success); }
.rail-card.tone-pending { border-left-color: var(--warn); }
.rail-card.tone-danger { border-left-color: var(--danger); }

// ─── 管线 ───
.rail {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-1) var(--space-2);
  row-gap: var(--space-2);
}
.node {
  display: inline-flex;
  align-items: baseline;
  gap: 5px;
  min-width: 0;
  color: var(--app-muted);
}
.node :deep(svg) { flex-shrink: 0; align-self: center; }
.node-label {
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.06em;
  // 不做 text-transform：产品名 "Gist" 是混合大小写，全大写会读成缩写 GIST
  color: var(--app-text);
}
.node-read {
  font-size: 11px;
  color: var(--app-muted);
  white-space: nowrap;
}
// "该动的那一段"：标签转 warn 色，其余保持安静
.node.is-hot .node-label,
.node.is-hot :deep(svg) { color: var(--warn); }
.node.is-hot .node-read { color: var(--warn); }

.link {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  flex: 1 1 20px;
  min-width: 24px;
  color: var(--app-border-strong);
}
.link-line {
  height: 1px;
  flex: 1 1 auto;
  background: currentColor;
}
.link-arrow { flex-shrink: 0; }
// 唯一动效：数据正在走（推送=向外，拉取=向内）
.link-arrow.moving { animation: travel 900ms var(--ease-standard) infinite; }
@keyframes travel {
  0% { transform: translateX(-3px); opacity: 0.35; }
  50% { transform: translateX(1px); opacity: 1; }
  100% { transform: translateX(3px); opacity: 0.35; }
}

// ─── 状态读数 ───
.readout {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}
.state {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.dot {
  width: 7px;
  height: 7px;
  border-radius: var(--radius-pill);
  background: var(--app-subtle);
  flex-shrink: 0;
}
.state.tone-ok .dot { background: var(--success); }
.state.tone-pending .dot { background: var(--warn); }
.state.tone-danger .dot { background: var(--danger); }
.state-label {
  font-size: var(--text-sm);
  font-weight: 600;
  // 状态色只由「左边线 + 圆点」承担：同一个状态不上三遍色（而且彩色小字对比度更差）。
  color: var(--app-strong);
}

.last {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11px;
  color: var(--app-muted);
  white-space: nowrap;
  flex-shrink: 0;
}
.last :deep(svg) { color: var(--danger); }

.hint {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.5;
  color: var(--app-muted);
}

// 无障碍：尊重系统减弱动效设置（此时动画不做，状态词本身已说明进行中）
@media (prefers-reduced-motion: reduce) {
  .link-arrow.moving { animation: none; }
}
</style>
