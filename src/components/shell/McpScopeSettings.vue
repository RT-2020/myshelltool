<script setup lang="ts">
/**
 * McpScopeSettings — MCP 授权范围编辑（v0.20，B1 的 GUI 面）。
 *
 * 后端就绪：core::mcp_scope 判定（组件级分组匹配/优先级/fail-closed）+
 * mcp_set_scope 保存（deny_all 不收 UI 通道——它是读盘损坏的收敛态）。
 * 本组件只做编辑面：分组勾选 / 标签逗号输入 / 排除资产勾选 / 本机 FS 开关。
 *
 * 语义提示（与后端一致的诚实文案）：
 * - 分组与标签是**并集**（命中任一即允许）；两者都空 = 不按组/标签限制；
 * - 完全清空（含排除列表）= 回到「不限制」（scope 未启用，老用户行为不变）；
 * - 本机 FS 开关只在配置了任一限制后生效（未启用 scope 时上传/下载不受影响）。
 */
import { computed, onMounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { ShieldCheck } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { useMcpStore } from '@/stores/mcp';
import { useAssetsStore } from '@/stores/assets';
import type { McpConfigResult } from '@/types/domain';

interface ScopeForm {
  allowedGroups: string[];
  allowedTags: string;
  deniedAssetIds: string[];
  allowLocalFs: boolean;
}

const store = useWorkbenchStore();
const mcpStore = useMcpStore();
const assetsStore = useAssetsStore();
const { assets, declaredGroups } = storeToRefs(assetsStore);

const editing = ref(false);
const saving = ref(false);
const form = ref<ScopeForm>({ allowedGroups: [], allowedTags: '', deniedAssetIds: [], allowLocalFs: false });

/** 后端权威 scope（loadMcpConfig 拉取；scope 为空对象 = 旧配置/未配置）。 */
const scope = computed(() => mcpStore.config?.scope ?? null);
const restricted = computed(() =>
  Boolean(scope.value && (scope.value.allowedGroups?.length || scope.value.allowedTags?.length || scope.value.deniedAssetIds?.length)));
const denyAll = computed(() => Boolean(scope.value?.denyAll));

const summary = computed(() => {
  if (denyAll.value) return '全部拒绝（配置曾损坏，保存一次新范围即可恢复）';
  if (!restricted.value) {
    return scope.value?.allowLocalFs
      ? '未限制资产访问；本机文件开关已开（无实际约束）'
      : '未限制（MCP 可访问全部资产）';
  }
  const parts: string[] = [];
  if (scope.value!.allowedGroups?.length) parts.push(`${scope.value!.allowedGroups.length} 个分组`);
  if (scope.value!.allowedTags?.length) parts.push(`${scope.value!.allowedTags.length} 个标签`);
  if (scope.value!.deniedAssetIds?.length) parts.push(`排除 ${scope.value!.deniedAssetIds.length} 台`);
  parts.push(scope.value!.allowLocalFs ? '本机文件已开放' : '本机文件未开放');
  return parts.join(' · ');
});

/** 可选分组：显式分组 ∪ 资产实际分组（去重；「未分组」也是合法路径）。 */
const groupOptions = computed(() => {
  const set = new Set<string>();
  for (const g of declaredGroups.value || []) set.add(g);
  for (const a of assets.value || []) if (a.group) set.add(a.group);
  return [...set].sort();
});

/** 现有标签提示（资产 tags 去重排序）。 */
const knownTags = computed(() => {
  const set = new Set<string>();
  for (const a of assets.value || []) for (const t of a.tags || []) set.add(t);
  return [...set].sort();
});

const assetOptions = computed(() =>
  (assets.value || []).map(a => ({ id: a.id, label: `${a.name}（${a.host}）` }))
);

function toggleIn(list: string[], value: string): string[] {
  return list.includes(value) ? list.filter(v => v !== value) : [...list, value];
}

function startEdit() {
  form.value = {
    allowedGroups: [...(scope.value?.allowedGroups ?? [])],
    allowedTags: (scope.value?.allowedTags ?? []).join(', '),
    deniedAssetIds: [...(scope.value?.deniedAssetIds ?? [])],
    allowLocalFs: Boolean(scope.value?.allowLocalFs)
  };
  editing.value = true;
}

async function save(clear = false) {
  if (saving.value) return;
  saving.value = true;
  try {
    const payload = clear
      ? { allowedGroups: [], allowedTags: [], deniedAssetIds: [], allowLocalFs: false }
      : {
          allowedGroups: form.value.allowedGroups,
          allowedTags: form.value.allowedTags.split(',').map(s => s.trim()).filter(Boolean),
          deniedAssetIds: form.value.deniedAssetIds,
          allowLocalFs: form.value.allowLocalFs
        };
    const config: unknown = await mcpStore.setScope(payload);
    if (config) {
      editing.value = false;
      store.announce(clear ? '已清空 MCP 授权范围（不限制）' : 'MCP 授权范围已保存（已建会话下次调用生效）', { level: 'success' });
    }
  } finally {
    saving.value = false;
  }
}

onMounted(() => {
  if (!mcpStore.config) void mcpStore.loadMcpConfig();
});
</script>

<template>
  <!-- 授权范围行：状态 + 编辑入口（对齐 McpPanelContent 的 s-cell 视觉语言） -->
  <div class="s-cell">
    <div class="s-row">
      <ShieldCheck :size="13" class="chev" />
      <span class="s-label">授权范围</span>
      <span class="s-spacer" />
      <span class="scope-summary" :class="{ 'is-deny': denyAll }">{{ summary }}</span>
      <button v-if="!editing" type="button" class="icon-act" title="编辑授权范围" @click="startEdit">
        {{ restricted ? '编辑' : '设置…' }}
      </button>
    </div>

    <div v-if="editing" class="scope-editor">
      <p class="muted scope-hint">
        分组与标签为并集（命中任一即允许）；两者都留空 = 不按组/标签限制。完全清空所有项 = 不限制任何资产。
      </p>

      <!-- 允许的分组 -->
      <div class="field">
        <span class="field-label">允许的分组（{{ form.allowedGroups.length }}/{{ groupOptions.length }}）</span>
        <div v-if="groupOptions.length" class="check-list">
          <label v-for="g in groupOptions" :key="g" class="check-item">
            <input type="checkbox" :checked="form.allowedGroups.includes(g)" @change="form.allowedGroups = toggleIn(form.allowedGroups, g)" />
            <span>{{ g }}</span>
          </label>
        </div>
        <p v-else class="muted field-empty">尚无分组（先在侧栏创建分组或给资产分组）</p>
      </div>

      <!-- 允许的标签 -->
      <div class="field">
        <span class="field-label">允许的标签（逗号分隔）</span>
        <input
          v-model="form.allowedTags"
          type="text"
          class="tag-input"
          placeholder="如 prod, 运维"
        />
        <p v-if="knownTags.length" class="muted field-empty">现有标签：{{ knownTags.join('、') }}</p>
      </div>

      <!-- 排除资产 -->
      <div class="field">
        <span class="field-label">显式排除的资产（优先级最高，{{ form.deniedAssetIds.length }} 台）</span>
        <div v-if="assetOptions.length" class="check-list">
          <label v-for="a in assetOptions" :key="a.id" class="check-item">
            <input type="checkbox" :checked="form.deniedAssetIds.includes(a.id)" @change="form.deniedAssetIds = toggleIn(form.deniedAssetIds, a.id)" />
            <span>{{ a.label }}</span>
          </label>
        </div>
      </div>

      <!-- 本机 FS -->
      <label class="check-item fs-toggle">
        <input type="checkbox" v-model="form.allowLocalFs" />
        <span>允许 MCP 触及本机文件系统（上传读本机 / 下载写本机）。仅在配置了上方任一限制时生效——未限制时上传/下载本就不受限。</span>
      </label>

      <div class="editor-actions">
        <button type="button" class="text-btn" :disabled="saving" @click="save(true)">清空全部限制</button>
        <span class="s-spacer" />
        <button type="button" class="text-btn" :disabled="saving" @click="editing = false">取消</button>
        <button type="button" class="primary-btn" :disabled="saving" @click="save(false)">
          {{ saving ? '保存中…' : '保存' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.s-cell {
  display: flex;
  flex-direction: column;
}
.s-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
}
.s-label {
  flex-shrink: 0;
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--app-strong);
}
.s-spacer { flex: 1 1 0; }
.chev { flex-shrink: 0; color: var(--app-muted); }
.scope-summary {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: var(--text-xs);
  color: var(--app-muted);
}
.scope-summary.is-deny { color: var(--danger); }
.icon-act {
  flex-shrink: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--accent);
  font: inherit;
  font-size: var(--text-xs);
  padding: 2px 8px;
  cursor: pointer;
}
.icon-act:hover { background: var(--app-hover); }

.scope-editor {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  margin: 0 var(--space-3) var(--space-3);
  padding: var(--space-2) var(--space-3);
  border-radius: var(--radius-sm);
  background: var(--app-panel-2);
}
.scope-hint {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.5;
}
.field {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.field-label {
  font-size: var(--text-xs);
  font-weight: 600;
  color: var(--app-strong);
}
.field-empty {
  margin: 0;
  font-size: 11px;
}
.check-list {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-1) var(--space-3);
  max-height: 120px;
  overflow: auto;
  border: 1px solid var(--app-border-soft);
  border-radius: var(--radius-sm);
  padding: var(--space-2);
}
.check-item {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: var(--text-xs);
  cursor: pointer;
  user-select: none;
  white-space: nowrap;
}
.fs-toggle {
  white-space: normal;
  line-height: 1.5;
  align-items: flex-start;
  color: var(--app-muted);
}
.tag-input {
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  background: var(--app-panel);
  color: inherit;
  font: inherit;
  font-size: var(--text-xs);
  padding: 4px 8px;
}
.tag-input:focus-visible { outline: none; box-shadow: var(--focus-ring); }

.editor-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.text-btn {
  border: none;
  background: transparent;
  color: var(--app-muted);
  font: inherit;
  font-size: var(--text-xs);
  padding: 4px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.text-btn:hover:not(:disabled) { color: var(--app-strong); background: var(--app-hover); }
.text-btn:disabled { opacity: 0.5; cursor: default; }
.primary-btn {
  border: none;
  background: var(--accent);
  color: var(--accent-contrast, #fff);
  font: inherit;
  font-size: var(--text-xs);
  padding: 4px 14px;
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.primary-btn:hover:not(:disabled) { background: var(--accent-hover); }
.primary-btn:disabled { opacity: 0.6; cursor: default; }
.muted { color: var(--app-muted); }
</style>
