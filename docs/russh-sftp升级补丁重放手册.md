# russh-sftp 升级补丁重放手册（runbook）

> 2026-09-22 在 russh-sftp 2.4.0 上完成重放演练（2.3.0 → 2.4.0 实测通过）。
> 本手册是 AGENTS.md「vendored fork」红线的操作细则：每次升级 russh-sftp 前照此执行。

## 补丁清单（2.3.0 fork 相对上游的三处）

| # | 文件 | 位置 | 内容 | 升级处置 |
|---|---|---|---|---|
| ① | src/buf.rs | try_get_string + 文件尾两个函数 + myshelltool_tests | 非 UTF-8 文件名可逆解码：非法字节 b → U+E000+b（PUA-A）；encode_back_to_wire 反向 | **必须重放**（2.4.0 的 try_get_string 仍是 from_utf8_lossy） |
| ② | src/ser.rs | serialize_str | 回发时 U+E000+b 解码回原始字节（与①配对） | **必须重放**（2.4.0 仍是 v.as_bytes()） |
| ③ | src/client/session.rs | set_metadata | setstat 透传（chmod 用） | **2.4.0 已原生提供同名同签名方法——升级时直接丢弃本补丁**（重放会 duplicate definition 编译错） |

## 重放步骤

1. 拷贝新版 russh-sftp 源码到临时目录（如 cargo registry 缓存）；
2. **补丁①**：buf.rs 的 try_get_string 中 from_utf8_lossy 行替换为
   decode_lossy_reversible(&bytes)，然后把 fork 里「/// 非法 UTF-8 字节」doc 起
   到文件尾（含 myshelltool_tests 模块）整块追加；
3. **补丁②**：ser.rs 的 serialize_str 体首行加 encode_back_to_wire（照 fork 注释块）；
4. **补丁③**：先 grep 新版 session.rs 是否已有 set_metadata——有则跳过（2.4.0+ 已原生）；
5. Cargo.toml 摘除 [[example]]/[[bench]]/[dev-dependencies] 段（本机 gcc 坏环境
   编译不过 aws-lc-sys，lib 测试不需要它们；正式升级在 CI 验证全量）；
6. 验收：cargo test --lib **3 个 myshelltool_tests 全过**（GBK 往返/合法 UTF-8 透传/trybuf 线上往返）——
   这是 fork 的核心契约，任何 russh-sftp 版本升级前后都必须绿；
7. 替换 third-party/russh-sftp 后跑 myshelltool 全量（npm run build + cargo check --tests + UI 5 套）。

## 演练记录（2026-09-22，2.3.0 → 2.4.0）

- ①②重放一次通过；③发现上游已原生——删除后编译过（这说明 vendored 补丁有「被上游
  吸收」的自然演化路径，升级时要先查再放）；
- 3/3 往返单测全过；
- 2.3→2.4 的 src 差异集中在 client/fs/file.rs、rawsession.rs、session.rs、extensions.rs、
  protocol/{file,file_attrs}.rs——与补丁①②的接触面（buf.rs/ser.rs）零交叠，重放无冲突。
