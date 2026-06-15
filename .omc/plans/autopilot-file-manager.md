# 文件管理界面核心重构 - 规格 + 实施计划（Autopilot）

> Phase 0 (Spec) + Phase 1 (Plan) 合并文档。
> 用户决策：核心重构 + 可切换本地栏。
> 参考标杆：FileZilla / WinSCP / VS Code Explorer / MobaXterm SFTP。

## 1. 现状（一句话）

`src/App.vue:640-708` 的文件 panel 只是单栏远程列表 + 假本地栏（队列占位）；
每行 3 个按钮（下载/重命名/删除）挤占空间；缺多选/右键/快捷键/过滤/排序/面包屑；
对比 FileZilla/WinSCP 明显落后。

## 2. 不做（明确范围外，避免镀金）

- 拖拽上传/下载（保留 P1 follow-up）
- 目录递归传输（保留 P1 follow-up）
- 书签/快速跳转、同步浏览（P2）
- 完整属性面板（保留现有 sftp_stat 调用即可）

## 3. 后端新增（Wave A）

新建 `src-tauri/src/fs_local.rs`，5 个 Tauri 命令（参考 `ssh.rs` 中 sftp_* 实现样式）：

| 命令 | 签名 | 说明 |
|---|---|---|
| `fs_local_list_dir(path)` | `-> LocalDirectoryList` | 列出本地目录（系统无关：Windows 用 `D:\` 起点） |
| `fs_local_mkdir(path)` | `-> ()` | 新建本地目录 |
| `fs_local_delete(path, kind)` | `-> ()` | 删除本地文件/目录 |
| `fs_local_rename(old_path, new_path)` | `-> ()` | 重命名/移动 |
| `fs_local_home_dir()` | `-> String` | 返回用户主目录，作为本地栏默认路径 |

`LocalFileEntry` 复用 `RemoteFileEntry` 结构（name/path/kind/size/modified）。
`LocalDirectoryList` = `{ path, entries, parent }`。

错误模型：与 sftp_* 一致，`Result<T, String>`。

在 `lib.rs` 注册 5 个命令到 `invoke_handler`。
新增 `mod fs_local;`。

## 4. 前端 Store 升级（Wave B）- `src/stores/workbench.js`

### 新增 state
- `localPath: ref('')`、`localEntries: ref([])`、`localViewMode: ref('browser')`（'queue' | 'browser'）
- `selectedRemotePaths: ref(new Set())`、`selectedLocalPaths: ref(new Set())`
- `remoteSortKey: ref('name')`、`remoteSortDir: ref('asc')`
- `remoteFilter: ref('')`、`remoteListMode: ref('detailed')`（'compact' | 'detailed'）
- `contextMenu: ref({ visible: false, x: 0, y: 0, side: 'remote', entry: null })`
- `transferDrawerOpen: ref(true)`

### 新增 getters（computed）
- `filteredRemoteEntries`：按 `remoteFilter` 模糊过滤（name 小写包含）
- `sortedRemoteEntries`：按 `remoteSortKey`+`remoteSortDir` 排序（directory 优先）
- `selectedRemoteEntries`：基于 `selectedRemotePaths` 反查 `remoteEntries`

### 新增 actions
本地栏：
- `refreshLocalFiles(path?)`：调 `fs_local_list_dir`，fallback 浏览器预览模式（announce 提示）
- `navigateLocalPath(path)`、`navigateLocalUp()`
- `localMkdir(name)`、`localDelete(paths)`、`localRename(oldPath, newPath)`
- `setLocalViewMode(mode)`

远程选择 / 排序 / 过滤：
- `toggleRemoteSelection(path, additive)`：additive=false 清除其他，true 则 Ctrl 多选
- `toggleRemoteRangeSelection(path)`：Shift 范围多选（基于当前 sortedRemoteEntries 索引）
- `selectAllRemote()`、`clearRemoteSelection()`
- `setRemoteSort(key)`：点击同列名切换 asc/desc
- `setRemoteFilter(query)`、`setRemoteListMode(mode)`

右键菜单：
- `openContextMenu({ x, y, side, entry })`、`closeContextMenu()`

批量操作：
- `batchRemoteDelete()`：confirm 后 sftp_remove 所有选中
- `batchRemoteDownload()`：所有选中文件循环下载
- `batchLocalUpload(files)`：现有 uploadFiles 已支持多文件
- `copyRemotePath(entry)`：navigator.clipboard.writeText

传输抽屉：
- `openTransferDrawer()`、`closeTransferDrawer()`、`toggleTransferDrawer()`

## 5. 前端组件重构（Wave C）- `src/App.vue`

`activeTab === 'files'` 的 `<section>` 整体重写。新结构：

```
<section data-panel="files">
  <header class="screen-header">
    <h1>...文件管理</h1>
    <toolbar>
      上传 | 新建目录 | 上级 | 刷新 | 列模式切换 | 排序下拉 | 模式（队列/浏览）切换
    </toolbar>
  </header>

  <!-- 远程工具栏：路径栏（可输入）+ 面包屑 + 过滤框 + 选中计数 -->
  <div class="file-toolbar">
    <input class="path-input" v-model="manualRemotePath" @keydown.enter="goToManualPath" />
    <nav class="breadcrumb">...</nav>
    <input class="filter-input" v-model="remoteFilter" placeholder="过滤..." />
    <span class="selection-count" v-if="selectedRemotePaths.size">{{ n }} 项已选</span>
  </div>

  <div class="dual-pane">
    <!-- 左：本地栏（模式切换） -->
    <div class="card file-pane" :class="{ 'drag-over': dragOverLocal }"
         @drop="..." @dragover="..." @dragleave="...">
      <pane-toolbar>
        <strong>本地</strong>
        <mode-switch :value="localViewMode" />
      </pane-toolbar>
      <div v-if="localViewMode === 'browser'" class="file-list">
        <div v-for="entry in localEntries" class="file-entry"
             :class="{ selected: selectedLocalPaths.has(entry.path) }"
             @click="onLocalRowClick($event, entry)"
             @dblclick="onLocalRowDblClick(entry)"
             @contextmenu.prevent="onLocalContextMenu($event, entry)">
          <span class="file-icon" :class="iconClassFor(entry)"></span>
          <span class="file-name">{{ entry.name }}</span>
          <span class="file-meta">{{ formatBytes(entry.size) }}</span>
          <span class="file-time">{{ entry.modified }}</span>
        </div>
      </div>
      <div v-else class="file-list">
        <!-- 队列视图：保留现有 transferQueue 列表 -->
      </div>
    </div>

    <!-- 右：远程栏 -->
    <div class="card file-pane">
      <pane-toolbar>
        <strong>远程</strong>
        <span class="path-bar">{{ remotePath }}</span>
      </pane-toolbar>
      <div class="file-list" @click.self="clearRemoteSelection()">
        <div v-for="entry in sortedRemoteEntries" class="file-entry"
             :class="{ selected: selectedRemotePaths.has(entry.path) }"
             @click="onRemoteRowClick($event, entry)"
             @dblclick="onRemoteRowDblClick(entry)"
             @contextmenu.prevent="onRemoteContextMenu($event, entry)">
          <span class="file-icon" :class="iconClassFor(entry)"></span>
          <span class="file-name">{{ entry.name }}</span>
          <span class="file-meta">{{ formatBytes(entry.size) }}</span>
          <span class="file-time">{{ entry.modified }}</span>
        </div>
      </div>
    </div>
  </div>

  <!-- 底部抽屉：传输队列 -->
  <div class="transfer-drawer" :class="{ open: transferDrawerOpen }">
    <header @click="toggleTransferDrawer">
      <strong>传输队列</strong>
      <span>{{ activeTransfers.length }} 进行中 · {{ completedTransfers.length }} 完成</span>
      <button>{{ transferDrawerOpen ? '▼' : '▲' }}</button>
    </header>
    <div class="drawer-body" v-if="transferDrawerOpen">
      <!-- 现有 transferQueue 列表 + 进度条 + 速度/剩余时间 -->
    </div>
  </div>

  <!-- 右键菜单浮层 -->
  <div v-if="contextMenu.visible" class="context-menu"
       :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
       @click.self="closeContextMenu">
    <ul>
      <li v-for="item in contextMenuItems" @click="item.action">{{ item.label }}</li>
    </ul>
  </div>
</section>
```

### 关键交互
- **路径栏输入回车**：调用 `navigateRemotePath(manualRemotePath)`
- **面包屑**：每段可点击，跳转该级
- **过滤**：实时输入，clear 按钮
- **排序**：列头点击切换 asc/desc（不显示列头时通过下拉菜单切换）
- **单击选中**：清除其他；Ctrl+单击追加；Shift+单击范围选
- **双击**：directory→进入、file→下载
- **右键**：弹出菜单（打开/下载/上传/重命名/删除/复制路径/属性/全选）
- **键盘**：见下表

### 键盘快捷键（仅 activeTab === 'files' 时绑定）
| 键 | 行为 |
|---|---|
| Enter | 进入目录或下载文件 |
| Backspace | 返回上级 |
| F2 | 重命名当前选中 |
| Delete | 删除选中 |
| F5 | 上传（本地→远程，浏览器文件选择） |
| Ctrl+A | 全选 |
| Ctrl+R / F5 (alternative) | 刷新 |
| Esc | 清选 / 关菜单 |

注意：F2/F5 浏览器默认行为要 preventDefault。

## 6. CSS 新增（Wave D）- `src/styles.css`

复用现有 token（--accent, --space-*, --radius-*, --text-*）。新增类：

- `.file-entry`（grid: icon / name / meta / time，可点击、可选中）
- `.file-entry.selected`（高亮：accent 5% 背景 + accent 左边框）
- `.file-icon`（基础占位）+ `.file-icon.dir` / `.file-icon.file` / `.file-icon.symlink`
  + `.file-icon.ext-img` / `.ext-txt` / `.ext-zip` / `.ext-doc` 等（unicode 字符）
- `.breadcrumb`（已有，增强：行内间距、悬停）
- `.path-input`（mono 字体，回车提示）
- `.filter-input`（左侧搜索图标）
- `.selection-count`（chip 样式）
- `.context-menu`（fixed 浮层，accent-border，shadow，z-index 高）
- `.context-menu ul li`（hover bg）
- `.context-menu .separator`（分隔线）
- `.transfer-drawer`（底部固定 / 抽屉动画）
- `.transfer-drawer header`（可点击）
- `.file-toolbar`（grid 三段：路径/面包屑、过滤、选中计数）
- `.pane-toolbar .mode-switch`（两按钮分段控件）

## 7. 浏览器预览模式兼容（关键）

非 Tauri runtime（`isTauriRuntime() === false`）：
- 调用 fs_local_* 会抛错，前端捕获并显示"桌面端才支持"
- 远程栏保持空，显示"未连接"
- UI 完整渲染（验证 npm run dev 模式下不崩）

## 8. 验收清单

- [ ] `npm run build` 通过
- [ ] `npm run test:ui` 通过（smoke test 不变）
- [ ] 浏览器预览模式打开文件 panel，渲染正常，无报错
- [ ] 现有 ui-smoke.mjs 测试通过
- [ ] 类型/Lint 检查通过
- [ ] Phase 4 architect + security + code-reviewer 全部 APPROVE 或合理解释

## 9. 实施顺序（增量 commit）

1. **Wave A**：fs_local.rs + lib.rs 注册（后端可独立编译验证 `cargo check`）
2. **Wave B**：workbench.js store 扩展（向后兼容，不破坏现有功能）
3. **Wave C**：App.vue 文件 panel 重写
4. **Wave D**：styles.css 样式新增
5. **Wave E**：QA 验证
