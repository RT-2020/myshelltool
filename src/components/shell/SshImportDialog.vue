<script setup lang="ts">
/**
 * SshImportDialog — OpenSSH config 导入（v0.20，SSH P0-1）。
 *
 * 定位：替换 FinalShell 的第一公里——用户手里几十台的 ~/.ssh/config 一键入库。
 * 流程：打开即预览（默认 ~/.ssh/config）→ 冲突标记（同 host:port:username 已
 * 存在，默认不勾选）→ 勾选 + 选目标分组 → 逐条复用 saveAsset（与手工建资产
 * 同链路）→ 汇报结果。
 *
 * 挂载形态：App.vue 根级 overlay（AppModal），不走 GlobalModals/modal 系统
 * ——GlobalModals 贴 500 行硬上限（RATCHET），新弹窗不往里塞；入口在侧栏
 * 资产右键菜单（useSidebarContextMenu）。
 */
import { computed, onMounted, ref } from 'vue';
import { storeToRefs } from 'pinia';
import { FolderOpen, RefreshCw } from 'lucide-vue-next';
import { useAssetsStore } from '@/stores/assets';
import { useWorkbenchStore } from '@/stores/workbench';
import { invokeBackend } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import {
  importCandidates,
  previewSshConfig
} from '@/lib/sshImport';
import type { SshImportPreview } from '@/types/domain';
import AppModal from '@/components/ui/AppModal.vue';
import AppButton from '@/components/ui/AppButton.vue';
import AppSelect from '@/components/ui/AppSelect.vue';

const store = useWorkbenchStore();
const assetsStore = useAssetsStore();
const { declaredGroups, assets } = storeToRefs(assetsStore);

const preview = ref<SshImportPreview | null>(null);
const loading = ref(false);
const importing = ref(false);
const errorText = ref('');
const selected = ref<Set<number>>(new Set());

async function runPreview(path?: string) {
  loading.value = true;
  errorText.value = '';
  preview.value = null;
  try {
    const result = await previewSshConfig(path);
    preview.value = result;
    // 默认勾选：无冲突项全选（多数用户一次全导入）
    selected.value = new Set(
      result.candidates
        .map((c, idx) => (c.conflictExistingId ? -1 : idx))
        .filter(i => i >= 0)
    );
  } catch (error) {
    errorText.value = errorMessage(error);
  } finally {
    loading.value = false;
  }
}

async function onPickFile() {
  // config 文件常无扩展名：过滤器给「所有文件」+ 常见命名
  const picked = await invokeBackend<string | null>('plugin:dialog|open', {
    options: {
      title: '选择 OpenSSH config 文件',
      multiple: false,
      directory: false,
      filters: [
        { name: 'SSH config', extensions: ['config', 'conf'] },
        { name: '所有文件', extensions: ['*'] }
      ]
    }
  });
  if (picked) await runPreview(picked);
}

function toggle(idx: number) {
  const next = new Set(selected.value);
  if (next.has(idx)) next.delete(idx);
  else next.add(idx);
  selected.value = next;
}

// 目标分组：默认「导入」；选项 = 显式分组 ∪ 现有资产分组 ∪「导入」（新组，
// 导入即声明——首次导入的用户还没有任何分组）。
const importGroup = ref('导入');
const groupOptions = computed(() => {
  const set = new Set(['导入']);
  for (const g of declaredGroups.value || []) set.add(g);
  for (const a of assets.value || []) if (a.group && a.group !== '未分组') set.add(a.group);
  return [...set].map(g => ({ label: g, value: g }));
});

async function onImport() {
  if (!preview.value || selected.value.size === 0 || importing.value) return;
  importing.value = true;
  try {
    const picks = [...selected.value].map(i => preview.value!.candidates[i]);
    const outcome = await importCandidates(
      { saveAsset: input => assetsStore.saveAsset(input) },
      picks,
      importGroup.value
    );
    if (outcome.failed.length === 0) {
      store.announce(`已导入 ${outcome.ok} 台主机`, { level: 'success' });
      assetsStore.closeImportDialog();
    } else {
      store.announce(
        `导入完成：成功 ${outcome.ok} 台，失败 ${outcome.failed.length} 台（${outcome.failed[0].alias}: ${outcome.failed[0].error}）`,
        { level: 'error' }
      );
      // 失败不清空对话框：用户可重试或换文件
    }
  } catch (error) {
    store.announce('导入失败：' + errorMessage(error), { level: 'error' });
  } finally {
    importing.value = false;
  }
}

onMounted(() => {
  if (preview.value === null && !loading.value) void runPreview();
});
</script>

<template>
  <AppModal
    :open="assetsStore.importDialogOpen"
    title="从 OpenSSH config 导入"
    width="640px"
    @close="assetsStore.closeImportDialog()"
  >
    <div class="import-stack">
      <p class="muted intro">
        解析 <code>~/.ssh/config</code>（或指定文件）的 Host 块生成候选资产；
        IdentityFile 映射为私钥路径认证，ProxyJump 保留待跳板功能启用。
      </p>

      <div class="toolbar-row">
        <AppButton variant="ghost" size="sm" :loading="loading" @click="runPreview()">
          <RefreshCw v-if="!loading" :size="12" />重新解析
        </AppButton>
        <AppButton variant="ghost" size="sm" @click="onPickFile">
          <FolderOpen :size="12" />选择文件…
        </AppButton>
        <span v-if="preview" class="muted num source-path" :title="preview.sourcePath">{{ preview.sourcePath }}</span>
      </div>

      <p v-if="errorText" class="error-text">{{ errorText }}</p>
      <p v-else-if="loading" class="muted">正在解析…</p>
      <p v-else-if="preview && preview.candidates.length === 0" class="muted">
        未解析到可导入的 Host 块。
      </p>

      <template v-if="preview && preview.candidates.length > 0">
        <div class="list-head">
          <span class="muted">共 {{ preview.candidates.length }} 台 · 已选 {{ selected.size }} 台（冲突项默认跳过）</span>
          <label class="setting-field group-field">
            <span class="muted field-label">导入到分组</span>
            <AppSelect
              :model-value="importGroup"
              :options="groupOptions"
              class="group-select"
              @update:model-value="v => (importGroup = String(v))"
            />
          </label>
        </div>
        <ul class="cand-list">
          <li
            v-for="(c, idx) in preview.candidates"
            :key="c.alias + idx"
            :class="{ 'is-conflict': c.conflictExistingId, 'is-checked': selected.has(idx) }"
          >
            <label class="cand-row">
              <input
                type="checkbox"
                :checked="selected.has(idx)"
                @change="toggle(idx)"
              />
              <span class="cand-name" :title="c.alias">{{ c.alias }}</span>
              <span class="cand-host num">{{ c.host }}:{{ c.port }}</span>
              <span class="cand-user muted">{{ c.username || '(本机用户名)' }}</span>
              <span class="cand-auth muted">{{ c.identityFile ? '私钥' : '密码' }}</span>
              <span v-if="c.proxyJump" class="muted" :title="`ProxyJump: ${c.proxyJump}`">跳板</span>
              <span v-if="c.conflictExistingId" class="conflict-badge" :title="`与现有资产「${c.conflictExistingName}」同主机`">已存在</span>
            </label>
          </li>
        </ul>
      </template>
    </div>

    <template #footer>
      <AppButton variant="ghost" @click="assetsStore.closeImportDialog()">取消</AppButton>
      <AppButton
        variant="primary"
        :disabled="selected.size === 0 || importing || loading"
        :loading="importing"
        @click="onImport"
      >
        导入 {{ selected.size > 0 ? `${selected.size} 台` : '' }}
      </AppButton>
    </template>
  </AppModal>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.import-stack {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  font-size: var(--text-sm);
}
.intro {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.5;
}
.toolbar-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.source-path {
  flex: 1 1 0;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 11px;
}
.error-text {
  margin: 0;
  color: var(--danger);
  font-size: var(--text-xs);
  line-height: 1.5;
  word-break: break-all;
}

.list-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-3);
  font-size: var(--text-xs);
}
.group-field {
  display: flex;
  align-items: center;
  gap: var(--space-2);
}
.field-label { flex-shrink: 0; }
.group-select { min-width: 160px; }

.cand-list {
  margin: 0;
  padding: 0;
  list-style: none;
  max-height: 320px;
  overflow: auto;
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
}
.cand-list li + li { border-top: 1px solid var(--app-border-soft); }
.cand-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: 4px var(--space-2);
  cursor: pointer;
}
.cand-row:hover { background: var(--app-hover); }
.is-checked { background: var(--app-panel-2); }
.cand-name {
  flex: 0 0 30%;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 600;
  color: var(--app-strong);
}
.cand-host { flex: 0 0 auto; font-size: var(--text-xs); }
.cand-user, .cand-auth { flex: 0 0 auto; font-size: var(--text-xs); }
.conflict-badge {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: var(--radius-pill);
  background: var(--app-hover);
  color: var(--warn);
}
.is-conflict .cand-name { color: var(--app-muted); }
</style>
