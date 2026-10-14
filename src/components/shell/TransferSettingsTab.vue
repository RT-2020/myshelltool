<script setup lang="ts">
/**
 * TransferSettingsTab — 设置·传输 tab 内容。
 *
 * 职责：配置全局并发传输数（1 ~ 5，默认 3）及传输偏好说明。
 * 纯前端响应式 + localStorage 持久化，即时生效。
 */
import { computed } from 'vue';
import { ArrowUpDown, Gauge, ShieldCheck } from 'lucide-vue-next';
import {
  transferConcurrency,
  setTransferConcurrency,
  DEFAULT_CONCURRENCY,
  MIN_CONCURRENCY,
  MAX_CONCURRENCY
} from '@/lib/transferConcurrency';

const CONCURRENCY_LEVELS = [1, 2, 3, 4, 5] as const;

const hintText = computed(() => {
  const c = transferConcurrency.value;
  if (c === 1) {
    return '单文件串行传输：资源开销最小、稳定性最高。适合弱网、高丢包网络或 CPU/Channel 受限的嵌入式设备。';
  }
  if (c <= 3) {
    return '平衡并发模式（推荐）：在保证连接稳定的前提下，显著消除海量小文件的网络往返（RTT）等待。';
  }
  return '高速并发模式：适合局域网或低延迟高速网络的大批小文件传输。请确保服务器 sshd 配置允许较高并发。';
});
</script>

<template>
  <div class="stack tab-pane">
    <header class="page-title"><ArrowUpDown :size="15" />文件传输设置</header>
    <p class="muted">配置目录上传与多文件传输的并发行为。更改即时生效并持久化保存。</p>

    <section class="block">
      <header class="block-head"><Gauge :size="12" />最大并发传输数</header>
      <div class="concurrency-grid">
        <button
          v-for="lvl in CONCURRENCY_LEVELS"
          :key="lvl"
          type="button"
          class="concurrency-card"
          :class="{ active: transferConcurrency === lvl, isDefault: lvl === DEFAULT_CONCURRENCY }"
          @click="setTransferConcurrency(lvl)"
        >
          <span class="level-num">{{ lvl }}</span>
          <span class="level-label">
            {{ lvl === 1 ? '串行' : lvl === DEFAULT_CONCURRENCY ? '推荐' : '并发' }}
          </span>
        </button>
      </div>
      <p class="muted concurrency-hint">{{ hintText }}</p>
    </section>

    <section class="block">
      <header class="block-head"><ShieldCheck :size="12" />传输保障机制</header>
      <ul class="feature-notes">
        <li>
          <span class="bullet">•</span>
          <span><strong>目录预建</strong>：目录整传前预先递归完成远端目录树创建，并发传输时免除竞态锁。</span>
        </li>
        <li>
          <span class="bullet">•</span>
          <span><strong>冲突收敛</strong>：同名冲突弹窗支持「应用到本批剩余冲突」，并发传输不触发弹窗风暴。</span>
        </li>
        <li>
          <span class="bullet">•</span>
          <span><strong>整批取消</strong>：随时中止批次，在途文件自动截断并清理远端半截脏文件。</span>
        </li>
      </ul>
    </section>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.concurrency-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 8px;
}

.concurrency-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 12px 6px;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
  color: var(--app-muted);
  cursor: pointer;
  font: inherit;
  transition: border-color var(--dur-fast) var(--ease-standard),
    background-color var(--dur-fast) var(--ease-standard),
    color var(--dur-fast) var(--ease-standard);

  &:hover {
    border-color: var(--app-border-hover);
    color: var(--app-fg);
  }

  &.active {
    border-color: var(--color-primary);
    background: var(--color-primary-subtle);
    color: var(--color-primary);
    font-weight: 500;
  }
}

.level-num {
  font-size: 16px;
  line-height: 1;
  font-family: var(--font-mono, monospace);
}

.level-label {
  font-size: 11px;
}

.concurrency-hint {
  margin-top: 8px;
  line-height: 1.5;
}

.feature-notes {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  font-size: 12px;
  color: var(--app-muted);

  li {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    line-height: 1.5;

    strong {
      color: var(--app-fg);
    }
  }

  .bullet {
    color: var(--color-primary);
  }
}
</style>
