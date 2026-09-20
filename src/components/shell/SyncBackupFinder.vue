<script setup lang="ts">
/**
 * SyncBackupFinder — 恢复模式下定位云端备份（免填 Gist ID）。
 *
 * 登录 GitHub 后自动列出此账号下的备份候选（sync_discover_gists：按备份文件名
 * 识别，只读元数据不触碰加密内容），默认选中最新一份；找不到时提示「可能登错
 * 账号」；手填 Gist ID 折叠为高级兜底（发现逻辑失效时的逃生口）。
 *
 * 选择结果经 select 事件上抛——SyncSetupForm 持有 gistId 并据此决定能否提交。
 */
import { onMounted, ref, watch } from 'vue';
import { storeToRefs } from 'pinia';
import { Search } from 'lucide-vue-next';
import { useWorkbenchStore } from '@/stores/workbench';
import { invokeBackend } from '@/services/backend';
import { errorMessage } from '@/lib/errorMessage';
import type { SyncGistCandidate } from '@/types/domain';
import AppButton from '@/components/ui/AppButton.vue';
import AppInput from '@/components/ui/AppInput.vue';

const emit = defineEmits<{ select: [gistId: string] }>();

const store = useWorkbenchStore();
const { githubPatConfigured } = storeToRefs(store);

const finding = ref(false);
const candidates = ref<SyncGistCandidate[]>([]);
const selectedId = ref('');
/** 已查找过：区分「还没找」与「找了但没有」（后者要给登错账号提示）。 */
const searched = ref(false);
const errorText = ref('');
// 手填兜底（高级）：默认折叠，发现不可用时展开仍可恢复
const showManual = ref(false);
const manualId = ref('');

/** 更新时间降序；未知/非法时间排尾部并保持相对顺序（不按字符串序猜时间先后）。 */
function sortByUpdatedDesc(list: SyncGistCandidate[]): SyncGistCandidate[] {
  const timeOf = (c: SyncGistCandidate) => (c.updated_at ? Date.parse(c.updated_at) : Number.NaN);
  return [...list].sort((a, b) => {
    const ta = timeOf(a);
    const tb = timeOf(b);
    if (Number.isNaN(ta) || Number.isNaN(tb)) return Number.isNaN(ta) ? 1 : -1;
    return tb - ta;
  });
}

async function find() {
  if (finding.value) return;
  finding.value = true;
  errorText.value = '';
  try {
    const list = await invokeBackend<SyncGistCandidate[]>('sync_discover_gists');
    candidates.value = sortByUpdatedDesc(list);
    searched.value = true;
    if (candidates.value.length) {
      selectedId.value = candidates.value[0].gist_id; // 默认最新一份
      emit('select', selectedId.value);
    }
  } catch (error) {
    errorText.value = errorMessage(error);
  } finally {
    finding.value = false;
  }
}

// 进入恢复模式时通常已登录：直接自动查找，不让用户多点一步；
// 未登录则等 Device Flow 完成（githubPatConfigured 翻真）后补找
onMounted(() => {
  if (githubPatConfigured.value) void find();
});
watch(githubPatConfigured, v => {
  if (v && !searched.value) void find();
});

function onPick(id: string) {
  selectedId.value = id;
  emit('select', id);
}

function onManualInput(v: string) {
  manualId.value = v;
  selectedId.value = ''; // 手填与候选二选一：手填时取消列表高亮
  emit('select', v.trim());
}

/** 候选时间：MM-dd HH:mm（同 SyncStatusRail 短格式），非法值原样展示。 */
function shortTime(iso: string | null | undefined): string {
  if (!iso) return '未知时间';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function idTail(id: string): string {
  return id.length <= 6 ? id : `…${id.slice(-6)}`;
}
</script>

<template>
  <div class="backup-finder">
    <p v-if="!githubPatConfigured" class="note">
      先在下方「账号」中登录 GitHub，即可自动找到你的备份，无需手动填写 Gist ID。
    </p>
    <template v-else>
      <div class="finder-row">
        <AppButton variant="subtle" size="sm" :disabled="finding" @click="find">
          <Search :size="12" />{{ finding ? '查找中…' : searched ? '重新查找' : '查找我的备份' }}
        </AppButton>
        <span v-if="finding" class="muted">正在列出此账号下的备份…</span>
      </div>

      <p v-if="errorText" class="error-inline">{{ errorText }}</p>
      <p v-else-if="searched && !candidates.length" class="note">
        此 GitHub 账号下未找到备份。请确认登录的是<strong>当初创建备份的那个账号</strong>，
        或展开下方「手动填 Gist ID」。
      </p>

      <div v-else-if="candidates.length" class="candidate-list">
        <label
          v-for="c in candidates"
          :key="c.gist_id"
          class="candidate"
          :class="{ 'is-active': selectedId === c.gist_id }"
        >
          <input
            type="radio"
            name="sync-backup-candidate"
            :value="c.gist_id"
            :checked="selectedId === c.gist_id"
            @change="onPick(c.gist_id)"
          />
          <span class="c-main">更新于 {{ shortTime(c.updated_at) }}</span>
          <span class="c-id">{{ idTail(c.gist_id) }}</span>
        </label>
        <p class="tiny muted">列出了此账号下的全部备份，默认选中最新一份；在多台电脑上用过会有多份。</p>
      </div>
    </template>

    <button type="button" class="link-btn is-quiet" @click="showManual = !showManual">
      {{ showManual ? '收起手动填写' : '手动填 Gist ID（高级）' }}
    </button>
    <label v-if="showManual" class="field">
      <span class="field-label">Gist ID</span>
      <AppInput
        :model-value="manualId"
        placeholder="在 GitHub 上那份 Gist 的 ID"
        @update:model-value="onManualInput"
      />
    </label>
  </div>
</template>

<style scoped lang="scss">
@use '@/styles/_tokens' as *;

.backup-finder {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
}
.finder-row {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  flex-wrap: wrap;
}
.finder-row :deep(svg) { flex-shrink: 0; }

.note {
  margin: 0;
  font-size: var(--text-xs);
  line-height: 1.6;
  color: var(--app-muted);
}
.note strong { color: var(--app-strong); }

.candidate-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.candidate {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--app-border);
  border-radius: var(--radius-sm);
  cursor: pointer;
  font-size: var(--text-xs);
}
.candidate:hover { background: var(--app-hover); }
.candidate.is-active {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.c-main { color: var(--app-strong); }
.c-id {
  margin-left: auto;
  font-family: var(--font-mono);
  color: var(--app-muted);
}

.field { display: flex; flex-direction: column; gap: var(--space-1); }
.field-label { font-size: var(--text-xs); color: var(--app-muted); }
.error-inline { margin: 0; font-size: var(--text-xs); color: var(--danger); word-break: break-word; }
.tiny { margin: 0; font-size: 11px; }

.link-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: none;
  border: none;
  padding: 0;
  color: var(--accent);
  font: inherit;
  font-size: var(--text-xs);
  cursor: pointer;
}
.link-btn:hover { color: var(--accent-hover); text-decoration: underline; }
.link-btn:focus-visible { outline: none; box-shadow: var(--focus-ring); border-radius: var(--radius-sm); }
.link-btn.is-quiet { color: var(--app-muted); align-self: flex-start; }
.link-btn.is-quiet:hover { color: var(--app-strong); }
</style>
