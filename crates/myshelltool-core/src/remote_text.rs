//! 远端文件内容的编码判定（不许猜「它一定是 UTF-8 文本」）。
//!
//! 事故：`sftp_read_file` 用 `String::from_utf8_lossy` 把任意字节「解码」成字符串
//! 并**返回成功**——GBK/GB18030/CP1252 写出的配置与日志（无 NUL 字节）每个非 ASCII
//! 字节变成 U+FFFD，交给 AI 宿主与审计日志的是乱码；前 512 字节无 NUL 的二进制
//! （部分 PNG/.so/.zip）同样被当文本「成功」读出。用户拿到的是一份看起来正常、
//! 实则损坏的内容——比直接报错危险得多。
//!
//! 判据（三态，宁严不猜）：
//! - **二进制**：前若干字节命中 NUL 或常见二进制魔数 → 拒绝（引导改用下载工具）；
//! - **非 UTF-8 文本**：非法字节序列 → 拒绝并报出首个非法位置，不替换成 U+FFFD；
//! - **UTF-8 文本**：原样返回，并标注是否带 BOM（BOM 会污染下游解析，调用方需知情）。
//!
//! 同主题的**文件名**侧事实见 [`is_lossy_remote_path`]：russh-sftp 原版对非 UTF-8
//! 文件名做 lossy 解码（U+FFFD，不可逆）——按它反查服务器必然失配，且可能命中
//! **名字字面含 U+FFFD 的另一个真实文件**（跨文件误删风险）。
//! **v2.8 已根治**：本仓库 vendored fork（third-party/russh-sftp，[patch.crates-io]）
//! 把非法字节可逆编码为 U+E000+b（PUA-A），回发时还原原始字节——文件名字节
//! 全链路无损往返，普通操作不再产生 U+FFFD 名。本守卫保留为纵深防御
//!（防 fork 失效/其它来源的失真名进入危险操作）。

/// 探测二进制用的前缀长度（与常见实现一致的 512 字节启发式）。
const SNIFF_LEN: usize = 512;

/// 常见二进制文件魔数（前几字节）。
const BINARY_MAGIC: &[&[u8]] = &[
    &[0x89, b'P', b'N', b'G'],        // PNG
    b"\x7fELF",                       // ELF（可执行/共享库）
    b"PK\x03\x04",                    // ZIP / JAR / DOCX
    b"\x1f\x8b",                      // gzip
    b"BZh",                           // bzip2
    b"\xfd7zXZ",                      // xz
    b"\xd0\xcf\x11\xe0",              // OLE2（旧 Office）
    b"%PDF",                          // PDF
    b"SQLite format 3",               // SQLite
    b"\x00asm",                       // WebAssembly
];

/// 内容判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteTextOutcome {
    /// 合法 UTF-8 文本。`has_bom` 为真时正文首部仍带 U+FEFF（调用方决定是否剥离）。
    Text { text: String, has_bom: bool },
    /// 判定为二进制内容（NUL 或魔数命中），附原因。
    Binary(String),
    /// 非 UTF-8 文本（字节序列非法），附首个非法字节偏移。
    NotUtf8(String),
}

/// 把远端文件字节判定并解码为文本。**绝不做 lossy 替换**。
pub fn decode_remote_text(bytes: &[u8]) -> RemoteTextOutcome {
    let sniff_len = bytes.len().min(SNIFF_LEN);
    let head = &bytes[..sniff_len];

    if let Some(pos) = head.iter().position(|b| *b == 0) {
        return RemoteTextOutcome::Binary(format!("前 {sniff_len} 字节内第 {pos} 字节为 NUL（空字节）"));
    }
    for magic in BINARY_MAGIC {
        if head.starts_with(magic) {
            return RemoteTextOutcome::Binary(format!(
                "文件头匹配二进制魔数 {:?}",
                String::from_utf8_lossy(magic)
            ));
        }
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let has_bom = text.starts_with('\u{feff}');
            RemoteTextOutcome::Text {
                text: text.to_string(),
                has_bom,
            }
        }
        Err(e) => RemoteTextOutcome::NotUtf8(format!(
            "第 {} 字节处不是合法 UTF-8（可能是 GBK/GB18030/CP1252 等本地编码）",
            e.valid_up_to()
        )),
    }
}

/// 远端路径是否含 U+FFFD 替换符——russh-sftp 对非 UTF-8 文件名 lossy 解码的产物
///（上游 `buf.rs`，暂无 raw bytes 接口，v2.6 审计 backlog #2）。
///
/// 这样的名字**无法忠实寻址**：拿它发 remove/rename/read/download，服务器按
/// 字节比对，真实文件名（GBK 等）对不上 → "No such file"；更危险的是，若服务器
/// 上**恰好存在**一个名字字面含 U+FFFD 的真实文件，操作会命中它——跨文件误删/
/// 误覆盖（write/upload 的 create 是 O_CREAT|O_TRUNC 覆盖语义，分不清新建与
/// 覆盖；MCP 场景 AI 宿主会把 sftp_list 的失真名回灌成目标路径）。因此所有
/// 「按名寻址远端文件」的操作入口（GUI 的 read/write/upload/download/rename/
/// remove 与 MCP 的 read/write/upload/download/remove）都必须先过本守卫，命中
/// 即拒绝（fail-closed），文案引导用户在终端按真实文件名处理。
///
/// 唯一不拦的是「创建全新路径且名字由用户显式输入」的 mkdir（目录新建无覆盖
/// 语义，名字失真立即自见）。
pub fn is_lossy_remote_path(path: &str) -> bool {
    path.contains('\u{FFFD}')
}

/// [`is_lossy_remote_path`] 命中时的统一拒绝文案（GUI 命令与 MCP 工具共用，
/// 保证两边给用户的解释一致）。
pub fn lossy_remote_path_error(path: &str) -> String {
    format!(
        "路径「{path}」含无法识别的字符（U+FFFD：远端文件名不是 UTF-8，客户端拿到的名字已失真）。\
         按这个名字操作可能找不到真实文件、甚至误伤同名文件；请在终端里用真实文件名处理，\
         或先把文件改成 UTF-8 文件名"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lossy_remote_path_detection() {
        // russh-sftp lossy 解码产物（GBK 名字常见）、以及字面含 U+FFFD 的名字
        assert!(is_lossy_remote_path("/tmp/\u{FFFD}\u{FFFD}.conf"));
        assert!(is_lossy_remote_path("./\u{FFFD}"));
        // 正常路径（含中文 UTF-8 文件名）不受影响
        assert!(!is_lossy_remote_path("/etc/nginx/nginx.conf"));
        assert!(!is_lossy_remote_path("/home/user/日志.log"));
        // 拒绝文案点名路径并给出可操作指引
        let msg = lossy_remote_path_error("/tmp/\u{FFFD}.sh");
        assert!(msg.contains("/tmp/\u{FFFD}.sh"));
        assert!(msg.contains("终端"));
    }

    #[test]
    fn ascii_and_chinese_utf8_are_text() {
        match decode_remote_text("hello world\n".as_bytes()) {
            RemoteTextOutcome::Text { text, has_bom } => {
                assert_eq!(text, "hello world\n");
                assert!(!has_bom);
            }
            other => panic!("应为文本，得到 {other:?}"),
        }
        match decode_remote_text("中文配置\n".as_bytes()) {
            RemoteTextOutcome::Text { text, .. } => assert_eq!(text, "中文配置\n"),
            other => panic!("应为文本，得到 {other:?}"),
        }
    }

    #[test]
    fn gbk_bytes_are_rejected_not_replaced_by_ufffd() {
        // 「中文」的 GBK 编码（0xD6 0xD0 0xCE 0xC4）不是合法 UTF-8——
        // 旧实现会 lossy 成 4 个 U+FFFD 并返回成功（乱码交给用户）
        let gbk = [0xD6u8, 0xD0, 0xCE, 0xC4];
        match decode_remote_text(&gbk) {
            RemoteTextOutcome::NotUtf8(reason) => assert!(reason.contains("UTF-8"), "{reason}"),
            other => panic!("GBK 内容必须被拒绝，得到 {other:?}"),
        }
    }

    #[test]
    fn nul_byte_means_binary() {
        let data = b"some text\x00more";
        match decode_remote_text(data) {
            RemoteTextOutcome::Binary(reason) => assert!(reason.contains("NUL"), "{reason}"),
            other => panic!("含 NUL 必须判二进制，得到 {other:?}"),
        }
    }

    #[test]
    fn known_magic_means_binary_even_without_nul_in_head() {
        // PNG 头 + 可打印正文（模拟前 512 字节无 NUL 的二进制）
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png.extend_from_slice(b"printable-looking payload without nul");
        match decode_remote_text(&png) {
            RemoteTextOutcome::Binary(reason) => assert!(reason.contains("魔数"), "{reason}"),
            other => panic!("魔数命中必须判二进制，得到 {other:?}"),
        }
    }

    #[test]
    fn nul_late_in_binary_still_detected_within_sniff_window() {
        // NUL 出现在前 512 字节内即可判二进制（不依赖文件整体扫描）
        let mut data = b"text".to_vec();
        data.resize(400, b'a');
        data.push(0);
        match decode_remote_text(&data) {
            RemoteTextOutcome::Binary(_) => {}
            other => panic!("应为二进制，得到 {other:?}"),
        }
    }

    #[test]
    fn bom_is_reported_but_content_kept() {
        let mut data = vec![0xEF, 0xBB, 0xBF];
        data.extend_from_slice("{\"a\":1}".as_bytes());
        match decode_remote_text(&data) {
            RemoteTextOutcome::Text { text, has_bom } => {
                assert!(has_bom, "BOM 必须被标注（下游 JSON 解析会被它破坏）");
                assert!(text.contains("\"a\":1"));
            }
            other => panic!("应为带 BOM 的文本，得到 {other:?}"),
        }
    }
}
