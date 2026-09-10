## v0.11.0

_区间: v0.10.0 → HEAD_

### ✨ 前端全量迁移 TypeScript

- `src/` 下 29 个 `.js` 全部转为 `.ts`、52 个 `.vue` 全部 `<script setup lang="ts">`（git mv 保留历史），一次到位、不做渐进式
- 新增 tsconfig 三件套（composite/strict/bundler resolution）+ `src/types/`（`tauri.d.ts` 声明 `window.__TAURI__`、`domain.ts` 沉淀 25+ 共享领域类型）
- `npm run build` 现在先跑 `vue-tsc` 类型检查再 vite 构建——发版 CI 自动获得类型门禁；tsconfig 收紧后 `.js` 无法再进入 `src/`
- 行为零变化：backend/services/stores 与迁移前逐行比对等价（唯一例外：修复 `McpExecutionLogList` 缺失的 `expandedId` 声明，原代码点击展开即 ReferenceError）

### 🐛 消除「靠猜测代替事实」——全仓审计修复 18 项

一次真实故障（root 服务器远程目录加载失败）牵出全仓审计，同类缺陷一次清干净：

- **家目录不再猜**：SFTP 列目录空路径改用 `canonicalize(".")` 向服务器求证真实家目录（旧版猜 `/home/<username>`，root 的家在 `/root`，必错）
- **目录列表不再依赖远端工具**：`ssh_list_directory` 弃用 GNU-only 的 `find -printf`（BusyBox/Alpine/BSD 上不存在），改走 SFTP 子系统——零远端用户态依赖
- **资源监控可移植**：`df` 加 `LC_ALL=C` 并改 POSIX `-k`（非英文 locale 磁盘统计静默归零、BusyBox 无 `-B1`）；`/proc/diskstats` 父设备与分区去重（磁盘 IO 不再虚高一倍）
- **同步时钟域统一**：`last_synced_at` 记本地完成时刻（原记远端 `updated_at`，pull 后永远误判「本地更新」）
- **上传不再静默截断**：循环以短读为 EOF（原信任目录列表时刻的文件大小，上传增长中的文件会被截断且标记成功）；大小变化时告警
- **覆盖确认不再被绕过**：上传前目标探测三态化，stat 失败按「可能存在」走确认
- **快速连接身份**：资产 id 改 `username-host[-port]`（原 `slugify(host)` 导致同主机不同用户名互相覆盖且沿用错误凭据）
- **安全兜底**：本地系统目录黑名单泛化到任意盘符（原仅 C 盘）；MCP `find -delete`/`-exec` 不再白名单免审批；SFTP 原子写 rename 加覆盖兜底
- **其他**：ssh 连接串支持 IPv6 括号形式（`src/lib/parseSshTarget.ts` 单一实现）；UNC 路径面包屑；布局恢复 clamp；TEAROFF 迁移失败防孤儿连接；错误串显式透出（不再恒显「未知错误」）+ SFTP 失败落日志

### 🐛 终端注入无痕化

- pty 从建连起以 `ECHO=0` 打开（RFC 4254 终端模式）：OSC 7 cwd 上报注入行全程不可见，根治慢 `.bashrc` 机器的双重回显（此前用 MOTD 横幅当就绪信号是错的——它是 sshd 在 shell 启动前打印的）
- 单行注入：payload `eval` 包裹（fish 拒解析也不丢回显恢复）+ bash 专属 ANSI 擦除多余提示符行；路径编码先 `%25` 再 `%20` 保证往返

### 📚 工程建设

- `docs/llm-engineering-guidelines.md` 新增 §7「靠猜测代替事实」：四种猜测形态分类（环境假设/格式猜测/静默兜底/时序猜测）+ 6 条硬规则 + 自检清单项，AGENTS.md 质量红线同步

### 📦 其他

- Release v0.11.0（版本号 bump）
