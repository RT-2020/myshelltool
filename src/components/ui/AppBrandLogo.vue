<script setup lang="ts">
import { computed } from 'vue';

/**
 * AppBrandLogo.vue
 * myshelltool 官方矢量品牌徽标组件。
 * 结合了 Squircle 钛晶底座、发光终端提示符 `>`、SSH 主从拓扑网络与就绪光标。
 *
 * 尺寸自适应：< 40px（标题栏等小尺寸场景）渲染加粗提亮的简化版——
 * 1024 viewBox 缩到 20-26px 时细描边（底座 6px/节点 20px）只剩亚像素，
 * 网格线完全消失，只有粗线条实心形还认得出来；≥ 40px（关于页）保留完整细节。
 */
const props = withDefaults(defineProps<{ size?: number | string; glow?: boolean }>(), {
  size: 20,
  glow: true
});

const detailed = computed(() => {
  const n = typeof props.size === 'number' ? props.size : Number.parseFloat(props.size);
  return Number.isFinite(n) ? n >= 40 : true;
});
</script>

<template>
  <svg
    class="app-brand-logo"
    :class="{ 'has-glow': glow }"
    :width="size"
    :height="size"
    viewBox="0 0 1024 1024"
    fill="none"
    xmlns="http://www.w3.org/2000/svg"
    aria-hidden="true"
  >
    <defs>
      <linearGradient id="ablBgGrad" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#101726"/>
        <stop offset="45%" stop-color="#0B101D"/>
        <stop offset="100%" stop-color="#040711"/>
      </linearGradient>

      <linearGradient id="ablBorderGrad" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#38BDF8" stop-opacity="0.85"/>
        <stop offset="30%" stop-color="#60A5FA" stop-opacity="0.45"/>
        <stop offset="70%" stop-color="#1E293B" stop-opacity="0.15"/>
        <stop offset="100%" stop-color="#2563EB" stop-opacity="0.5"/>
      </linearGradient>

      <linearGradient id="ablBrightCyan" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#BAE6FD"/>
        <stop offset="25%" stop-color="#38BDF8"/>
        <stop offset="75%" stop-color="#0284C7"/>
        <stop offset="100%" stop-color="#0369A1"/>
      </linearGradient>

      <linearGradient id="ablNodeGrad" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#7DD3FC"/>
        <stop offset="100%" stop-color="#0284C7"/>
      </linearGradient>

      <linearGradient id="ablGatewayGrad" x1="0%" y1="0%" x2="100%" y2="100%">
        <stop offset="0%" stop-color="#FFFFFF"/>
        <stop offset="40%" stop-color="#38BDF8"/>
        <stop offset="100%" stop-color="#0284C7"/>
      </linearGradient>

      <radialGradient id="ablAmbientGlow" cx="50%" cy="48%" r="48%">
        <stop offset="0%" stop-color="#0284C7" stop-opacity="0.32"/>
        <stop offset="50%" stop-color="#2563EB" stop-opacity="0.12"/>
        <stop offset="100%" stop-color="#030712" stop-opacity="0"/>
      </radialGradient>
    </defs>

    <!-- 底盘容器 -->
    <rect x="92" y="92" width="840" height="840" rx="210" ry="210" fill="url(#ablBgGrad)"/>
    <rect x="92" y="92" width="840" height="840" rx="210" ry="210" fill="url(#ablAmbientGlow)"/>
    <rect
      x="92" y="92" width="840" height="840" rx="210" ry="210"
      fill="none"
      :stroke="detailed ? 'url(#ablBorderGrad)' : '#38BDF8'"
      :stroke-opacity="detailed ? 1 : 0.9"
      :stroke-width="detailed ? 6 : 30"
    />

    <template v-if="detailed">
      <!-- 精密技术网格（仅大尺寸可见，小尺寸亚像素不可辨） -->
      <g opacity="0.05" stroke="#FFFFFF" stroke-width="2">
        <line x1="92" y1="302" x2="932" y2="302"/>
        <line x1="92" y1="512" x2="932" y2="512"/>
        <line x1="92" y1="722" x2="932" y2="722"/>
        <line x1="302" y1="92" x2="302" y2="932"/>
        <line x1="512" y1="92" x2="512" y2="932"/>
        <line x1="722" y1="92" x2="722" y2="932"/>
      </g>

      <!-- 提示符 '>' -->
      <path d="M 270 326 L 474 512 L 270 698" fill="none" stroke="url(#ablBrightCyan)" stroke-width="72" stroke-linecap="round" stroke-linejoin="round"/>

      <!-- 拓扑连接线 -->
      <g fill="none" stroke="url(#ablNodeGrad)" stroke-width="28" stroke-linecap="round" opacity="0.95">
        <line x1="596" y1="360" x2="712" y2="446"/>
        <line x1="712" y1="446" x2="634" y2="596"/>
        <line x1="712" y1="446" x2="806" y2="606"/>
      </g>

      <!-- 节点 -->
      <circle cx="596" cy="360" r="36" fill="#0A0F1D" stroke="url(#ablNodeGrad)" stroke-width="20"/>
      <circle cx="596" cy="360" r="14" fill="#38BDF8"/>

      <!-- 网关枢纽 -->
      <circle cx="712" cy="446" r="54" fill="url(#ablGatewayGrad)" stroke="#FFFFFF" stroke-width="10"/>
      <circle cx="712" cy="446" r="22" fill="#0369A1"/>

      <!-- 节点 2 -->
      <circle cx="634" cy="596" r="32" fill="#0A0F1D" stroke="url(#ablNodeGrad)" stroke-width="18"/>
      <circle cx="634" cy="596" r="12" fill="#38BDF8"/>

      <!-- 节点 3 -->
      <circle cx="806" cy="606" r="36" fill="#0A0F1D" stroke="url(#ablNodeGrad)" stroke-width="20"/>
      <circle cx="806" cy="606" r="14" fill="#38BDF8"/>

      <!-- 光标 '_' -->
      <rect x="500" y="718" width="240" height="60" rx="30" ry="30" fill="url(#ablBrightCyan)"/>
    </template>

    <template v-else>
      <!-- 小尺寸简化版：同一构图，线条加粗、节点实心提亮，保证 20-28px 下可辨识 -->
      <path d="M 270 326 L 474 512 L 270 698" fill="none" stroke="url(#ablBrightCyan)" stroke-width="108" stroke-linecap="round" stroke-linejoin="round"/>

      <g fill="none" stroke="url(#ablNodeGrad)" stroke-width="54" stroke-linecap="round">
        <line x1="596" y1="360" x2="712" y2="446"/>
        <line x1="712" y1="446" x2="634" y2="596"/>
        <line x1="712" y1="446" x2="806" y2="606"/>
      </g>

      <circle cx="596" cy="360" r="54" fill="url(#ablNodeGrad)"/>
      <circle cx="712" cy="446" r="66" fill="url(#ablGatewayGrad)"/>
      <circle cx="634" cy="596" r="48" fill="url(#ablNodeGrad)"/>
      <circle cx="806" cy="606" r="54" fill="url(#ablNodeGrad)"/>

      <rect x="500" y="712" width="240" height="84" rx="42" ry="42" fill="url(#ablBrightCyan)"/>
    </template>
  </svg>
</template>

<style scoped lang="scss">
.app-brand-logo {
  display: inline-block;
  vertical-align: middle;
  flex-shrink: 0;
  transition: transform var(--dur-fast, 120ms) var(--ease-standard, ease);

  &.has-glow:hover {
    transform: scale(1.05);
    filter: drop-shadow(0 0 6px rgba(56, 189, 248, 0.45));
  }
}
</style>
