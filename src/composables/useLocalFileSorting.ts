/**
 * useLocalFileSorting — 文件面板本地侧的过滤/排序（v0.20 自 FileColumn 拆出，
 * S2 的一刀；逻辑原样迁移）。files store 只跟踪远程排序，本地等价状态留在
 * 列内——本 composable 承载该状态与排序算法，保持 store 不被本地等价物污染。
 */
import { computed, ref, type Ref } from 'vue';
import type { RemoteFileEntry } from '@/types/domain';
import { inferFileEntryType } from '@/components/files/fileColumnUtils';

function typeOfEntry(e: RemoteFileEntry) {
  return inferFileEntryType(e);
}

export function useLocalFileSorting(localEntries: Ref<RemoteFileEntry[]>) {
  const localFilterQuery = ref('');
  const localSortKey = ref('name');
  const localSortDir = ref('asc');

  function setLocalSort(key: string) {
    if (localSortKey.value === key) {
      localSortDir.value = localSortDir.value === 'asc' ? 'desc' : 'asc';
    } else {
      localSortKey.value = key;
      localSortDir.value = 'asc';
    }
  }

  // Apply local sort (files store doesn't expose local sort, so re-sort here).
  const sortedLocalEntries = computed(() => {
    // entries() already filtered; but localEntries sort needs to happen before filter for stability.
    const q = (localFilterQuery.value || '').trim().toLowerCase();
    const list = q
      ? localEntries.value.filter(e => e.name.toLowerCase().includes(q))
      : localEntries.value.slice();
    const key = localSortKey.value;
    const dir = localSortDir.value === 'asc' ? 1 : -1;
    const ownerOf = (e: RemoteFileEntry) => [e.user || '', e.group || ''].join(':');
    return list.sort((a, b) => {
      const aDir = a.kind === 'directory' ? 0 : 1;
      const bDir = b.kind === 'directory' ? 0 : 1;
      if (aDir !== bDir) return aDir - bDir;
      let av: string | number, bv: string | number;
      if (key === 'size') { av = a.size || 0; bv = b.size || 0; }
      else if (key === 'modified') { av = Number(a.modified) || 0; bv = Number(b.modified) || 0; }
      else if (key === 'type') { av = typeOfEntry(a); bv = typeOfEntry(b); }
      else if (key === 'permissions') {
        av = a.permissions ? parseInt(a.permissions, 8) || 0 : 0;
        bv = b.permissions ? parseInt(b.permissions, 8) || 0 : 0;
      }
      else if (key === 'owner') { av = ownerOf(a); bv = ownerOf(b); }
      else { av = a.name.toLowerCase(); bv = b.name.toLowerCase(); }
      if (av < bv) return -1 * dir;
      if (av > bv) return 1 * dir;
      return 0;
    });
  });

  return { localFilterQuery, localSortKey, localSortDir, setLocalSort, sortedLocalEntries };
}
