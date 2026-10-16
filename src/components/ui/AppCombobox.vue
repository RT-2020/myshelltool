<script setup lang="ts">
/**
 * AppCombobox — 可输入的下拉选择（AppSelect 的「可选可输」姊妹件）。
 *
 * 与 AppSelect 的契约分界（不复用实现的原因）：AppSelect 是严格 listbox
 * （值必属选项集，键盘 typeahead 仅高亮）；本组件允许手输任意文本，
 * 无精确匹配时在菜单末尾给「新建 "xx"」提示行。首个消费面是资产分组
 * 字段（直输新分组路径），值 = 选项 value 或手输文本，由调用方赋予语义。
 *
 * 提交时机：选选项 / 点「新建」行 / Enter / blur 各提交一次（无逐键 emit，
 * 避免 prop 回写打断输入）；Escape 回滚到 focus 时的已提交值。
 * 选项行 @mousedown.prevent：防止点击选项先触发 input blur 收起菜单。
 */
import { ref, computed, watch, onMounted, onBeforeUnmount } from 'vue';
import { ChevronDown, Plus } from 'lucide-vue-next';

/** 选项契约（与 AppSelect 同形：label 宽松可空）。 */
interface ComboboxOption {
  label?: string;
  value: string | number;
}

// 同页多实例时 aria-controls/id 不能撞：模块级自增后缀
let instanceSeq = 0;

const props = withDefaults(
  defineProps<{
    modelValue?: string;
    options?: ComboboxOption[];
    placeholder?: string;
  }>(),
  {
    modelValue: '',
    options: () => [],
    placeholder: ''
  }
);
const emit = defineEmits<{ 'update:modelValue': [value: string] }>();

const menuId = `app-combobox-menu-${++instanceSeq}`;

const open = ref(false);
const text = ref(''); // 输入框实时文本（含未提交的中间态）
const rootRef = ref<HTMLElement | null>(null);
const menuRef = ref<HTMLElement | null>(null);
const inputRef = ref<HTMLInputElement | null>(null);
const activeIndex = ref(-1); // 菜单高亮行（-1 = 无；含末尾「新建」行）
let focusSnapshot = ''; // focus 时的已提交值，Escape 回滚基准

function labelOf(value: string): string {
  const found = props.options.find(o => String(o.value) === value);
  return found && found.label ? found.label : value;
}

const trimmed = computed(() => text.value.trim());
// 子串过滤（大小写不敏感，label/value 都参与）；空输入显示全部
const filtered = computed(() => {
  const q = trimmed.value.toLowerCase();
  if (!q) return props.options;
  return props.options.filter(o => String(o.label ?? o.value).toLowerCase().includes(q));
});
// 手输文本非空且选项集无精确匹配 → 菜单末尾出现「新建 "q"」行
const showCreateRow = computed(() => {
  if (!trimmed.value) return false;
  return !props.options.some(o => String(o.value) === trimmed.value || o.label === trimmed.value);
});
const rowCount = computed(() => filtered.value.length + (showCreateRow.value ? 1 : 0));

// 外部值变化（弹窗预填/清空）同步进输入框；自身提交的回显因值相同不触发重写
watch(
  () => props.modelValue,
  v => {
    text.value = labelOf(String(v));
  },
  { immediate: true }
);

function openMenu() {
  open.value = true;
  activeIndex.value = -1;
}
function closeMenu() {
  open.value = false;
  activeIndex.value = -1;
}
function commit(value: string) {
  const v = value.trim();
  text.value = v;
  emit('update:modelValue', v);
}
function selectOption(value: string | number) {
  commit(String(value));
  closeMenu();
}
function commitCreateRow() {
  commit(trimmed.value);
  closeMenu();
}

function scrollActiveIntoView() {
  if (activeIndex.value < 0 || !menuRef.value) return;
  const el = menuRef.value.querySelectorAll('.app-combobox-option')[activeIndex.value];
  if (el) el.scrollIntoView({ block: 'nearest' });
}
function moveHighlight(dir: number) {
  const len = rowCount.value;
  if (!len) return;
  let next = activeIndex.value + dir;
  if (next < 0) next = len - 1;
  if (next >= len) next = 0;
  activeIndex.value = next;
  scrollActiveIntoView();
}
function commitActive() {
  if (activeIndex.value < 0) {
    commit(text.value);
    closeMenu();
    return;
  }
  const opt = filtered.value[activeIndex.value];
  if (opt) selectOption(opt.value);
  else if (showCreateRow.value) commitCreateRow();
}

function onInput(e: Event) {
  text.value = (e.target as HTMLInputElement).value;
  if (!open.value) openMenu();
  activeIndex.value = -1;
}
function onFocus() {
  focusSnapshot = props.modelValue;
  if (!open.value) openMenu();
}
function onBlur() {
  closeMenu();
  // blur 提交（点保存按钮/Tab 切走前值已落袋）；Escape 已回滚则值相同、无副作用
  if (trimmed.value !== props.modelValue) commit(text.value);
}
function onKeydown(e: KeyboardEvent) {
  if (e.key === 'ArrowDown') {
    e.preventDefault();
    if (!open.value) openMenu();
    else moveHighlight(1);
    return;
  }
  if (e.key === 'ArrowUp') {
    e.preventDefault();
    if (open.value) moveHighlight(-1);
    return;
  }
  if (e.key === 'Enter') {
    if (open.value) {
      e.preventDefault();
      commitActive();
    }
    return;
  }
  if (e.key === 'Escape') {
    if (open.value) {
      e.preventDefault();
      text.value = labelOf(focusSnapshot);
      emit('update:modelValue', focusSnapshot);
      closeMenu();
    }
  }
}

// 菜单外点击收起（右键走 mousedown，与 AppSelect 同款）；input blur 覆盖左键路径
function onDocMouseDown(e: MouseEvent) {
  if (open.value && rootRef.value && !rootRef.value.contains(e.target as Node)) closeMenu();
}
function onWindowBlur() {
  if (open.value) closeMenu();
}

onMounted(() => {
  document.addEventListener('mousedown', onDocMouseDown);
  window.addEventListener('blur', onWindowBlur);
});
onBeforeUnmount(() => {
  document.removeEventListener('mousedown', onDocMouseDown);
  window.removeEventListener('blur', onWindowBlur);
});
</script>

<template>
  <div class="app-combobox" ref="rootRef">
    <input
      ref="inputRef"
      class="app-combobox-input"
      type="text"
      role="combobox"
      :value="text"
      :placeholder="placeholder"
      :aria-expanded="open"
      aria-autocomplete="list"
      :aria-controls="menuId"
      :aria-activedescendant="
        open && activeIndex >= 0 ? `${menuId}-option-${activeIndex}` : undefined
      "
      autocomplete="off"
      spellcheck="false"
      @input="onInput"
      @focus="onFocus"
      @blur="onBlur"
      @keydown="onKeydown"
    />
    <ChevronDown :size="14" class="app-combobox-caret" :class="{ 'is-open': open }" />
    <Transition name="app-combobox-menu">
      <!-- 与 AppSelect 同因：label 包裹下的选项点击须 preventDefault + stop，
           否则 label 转发 click 会把菜单重新弹开（选完不收起）。 -->
      <ul
        v-if="open"
        ref="menuRef"
        :id="menuId"
        class="app-combobox-menu"
        role="listbox"
        @click.prevent.stop
      >
        <li
          v-for="(opt, i) in filtered"
          :key="opt.value"
          :id="`${menuId}-option-${i}`"
          class="app-combobox-option"
          :class="{ 'is-selected': String(opt.value) === modelValue, 'is-active': activeIndex === i }"
          role="option"
          :aria-selected="String(opt.value) === modelValue"
          tabindex="-1"
          @mousedown.prevent
          @click="selectOption(opt.value)"
        >
          <span class="app-combobox-option-label">{{ opt.label }}</span>
        </li>
        <li
          v-if="showCreateRow"
          :id="`${menuId}-option-${filtered.length}`"
          class="app-combobox-option app-combobox-create"
          :class="{ 'is-active': activeIndex === filtered.length }"
          role="option"
          :aria-selected="false"
          tabindex="-1"
          @mousedown.prevent
          @click="commitCreateRow"
        >
          <Plus :size="13" class="app-combobox-create-icon" />
          <span class="app-combobox-option-label">新建「{{ trimmed }}」</span>
        </li>
      </ul>
    </Transition>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.app-combobox {
  position: relative;
  display: inline-block;
  width: 100%;
}

.app-combobox-input {
  width: 100%;
  padding: 6px 28px 6px 10px;
  background: var(--app-control);
  color: var(--app-text);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  font-family: var(--font-body);
  text-align: left;
  transition: border-color var(--dur-fast) var(--ease-standard),
    box-shadow var(--dur-fast) var(--ease-standard);
}
.app-combobox-input::placeholder {
  color: var(--app-subtle);
}
.app-combobox-input:focus {
  outline: none;
  border-color: var(--accent);
  box-shadow: var(--focus-ring);
}

.app-combobox-caret {
  position: absolute;
  right: 8px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--app-muted);
  pointer-events: none;
  transition: transform var(--dur-fast) var(--ease-standard);
}
.app-combobox-caret.is-open {
  transform: translateY(-50%) rotate(180deg);
}

.app-combobox-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: var(--z-dropdown);
  list-style: none;
  margin: 0;
  padding: 4px;
  background: var(--app-panel);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-md);
  box-shadow: var(--app-shadow);
  max-height: 240px;
  overflow: auto;
}

.app-combobox-option {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 6px 10px;
  border-radius: var(--radius-sm);
  font-size: var(--text-sm);
  color: var(--app-text);
  cursor: pointer;
}
.app-combobox-option:hover,
.app-combobox-option.is-active {
  background: var(--app-hover);
}
.app-combobox-option.is-selected {
  color: var(--accent);
}
.app-combobox-option-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.app-combobox-create {
  color: var(--accent);
  border-top: 1px solid var(--app-border-soft);
  margin-top: 2px;
  border-radius: 0 0 var(--radius-sm) var(--radius-sm);
}
.app-combobox-create-icon {
  flex: 0 0 auto;
}

.app-combobox-menu-enter-active,
.app-combobox-menu-leave-active {
  transition: opacity var(--dur-fast) var(--ease-standard),
    transform var(--dur-fast) var(--ease-standard);
}
.app-combobox-menu-enter-from,
.app-combobox-menu-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
