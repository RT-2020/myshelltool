/**
 * useModalView — GlobalModals 的退场动画视图层。
 *
 * 背景：.modal-layer 常驻 DOM、靠 .open 切 display（legacy 测试选择器依赖该结构，
 * 不能改成 v-if + <Transition>）。打开动画由 display 切换重播，但关闭此前是硬切。
 * 关闭时 store.modal.type 立即变 null（业务状态、Promise resolve 不允许被动画拖慢），
 * 本 composable 让**渲染层**滞留最后一份非空载荷，退场动画播完再撤 —— 状态即时、
 * 视图滞后，两者解耦。
 *
 * 退出时机：优先监听遮罩层自身的 animationend（reduced-motion 下 _base.scss 把时长
 * 压到 0.01ms，事件立即到 → 立即隐藏，不残留「可见但已关闭」的窗口期）；面板的
 * 退场动画会冒泡上来，按 event.target 过滤。定时器只作兜底——万一未来样式丢了
 * 动画，也不至于把弹窗卡在可见态。
 */
import { computed, onBeforeUnmount, ref, watch, type Ref } from 'vue';

export function useModalView<T extends { type: string | null }>(modal: Ref<T>) {
  // 最近一次非空载荷：退场期间模板分支/标题仍按它渲染
  const lastOpen = ref(modal.value) as Ref<T>;
  const closing = ref(false);
  let fallbackTimer: ReturnType<typeof setTimeout> | null = null;

  function finishClose() {
    if (fallbackTimer) { clearTimeout(fallbackTimer); fallbackTimer = null; }
    closing.value = false;
  }

  watch(() => modal.value.type, type => {
    if (type) {
      // 关闭途中又开了新弹窗：撤掉退场态，动画属性回到进场定义会自然重播
      finishClose();
      lastOpen.value = modal.value;
      return;
    }
    if (!lastOpen.value.type || closing.value) return;
    closing.value = true;
    fallbackTimer = setTimeout(finishClose, 300); // 略大于 --motion-fast，仅兜底
  });

  function onExitAnimationEnd(event: AnimationEvent) {
    if (closing.value && event.target === event.currentTarget) finishClose();
  }

  onBeforeUnmount(() => { if (fallbackTimer) clearTimeout(fallbackTimer); });

  // 打开态 = 真实 store 载荷（零延迟）；关闭退场中 = 滞留载荷；退场结束 = null
  const view = computed<T>(() =>
    modal.value.type ? modal.value : (closing.value ? lastOpen.value : modal.value)
  );

  return { view, closing, onExitAnimationEnd };
}
