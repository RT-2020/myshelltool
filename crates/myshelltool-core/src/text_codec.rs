//! 内置编辑器的文本编码内核：本地编码白名单解码/编码 + EOL 三态 + UTF-8 BOM。
//!
//! 设计纪律（继承 [`crate::remote_text`] 的「宁严不猜」）：
//! - **白名单制**：仅接受 8 个常用编码标签（`SUPPORTED_ENCODING_LABELS`），
//!   未知标签一律 `Err`，绝不静默回落到别的编码；
//! - **绝不 lossy**：encoding_rs 的 `decode`/`encode` 遇非法序列会写入替换字符并置
//!   `had_errors`——本模块把它当**硬错误**返回。调用方只能显式换编码重试，
//!   拿不到「看起来正常、实则损坏」的内容（与 remote_text 拒绝 U+FFFD 同根）；
//! - **EOL 三态**：`Lf`/`Crlf`/`Mixed`。检测只报告事实；写回时由调用方显式指定
//!   目标（`apply_eol`），`Mixed` 作为目标是 no-op（保内容不动）。
//!
//! 注意与 [`crate::remote_text::decode_remote_text`] 的分工：那边做**无先验**的
//! 三态判定（二进制？UTF-8？非 UTF-8？），本模块做**用户已选定编码**后的严格
//! 转码——编辑器读取链路先用那边判 UTF-8，非 UTF-8 时用户选编码后走这边。

use encoding_rs::Encoding;

/// 允许在编辑器读写下使用的编码标签（encoding_rs 规范名，全小写）。
/// `encoding_for_label` 同时接受常见别名（如 shift-jis / sjis），解析后按规范名比对。
pub const SUPPORTED_ENCODING_LABELS: &[&str] = &[
    "utf-8",
    "gbk",
    "gb18030",
    "big5",
    "shift_jis",
    "euc-jp",
    "euc-kr",
    "windows-1252",
];

/// 把用户提供的编码标签解析为白名单内的 encoding_rs 编码。
/// 未知/不在白名单 → `Err`（fail-closed，不回落）。
pub fn encoding_for_label(label: &str) -> Result<&'static Encoding, String> {
    let normalized = label.trim().to_ascii_lowercase();
    let Some(encoding) = Encoding::for_label(normalized.as_bytes()) else {
        return Err(format!(
            "不支持的编码「{label}」（可选：{}）",
            SUPPORTED_ENCODING_LABELS.join(" / ")
        ));
    };
    let canonical = encoding.name().to_ascii_lowercase();
    if !SUPPORTED_ENCODING_LABELS.contains(&canonical.as_str()) {
        return Err(format!(
            "不支持的编码「{label}」（可选：{}）",
            SUPPORTED_ENCODING_LABELS.join(" / ")
        ));
    }
    Ok(encoding)
}

pub fn is_supported_encoding(label: &str) -> bool {
    encoding_for_label(label).is_ok()
}

/// 按指定编码把字节严格解码为文本。非法序列 → `Err`（含编码名，便于用户换选）。
pub fn decode_bytes(bytes: &[u8], label: &str) -> Result<String, String> {
    let encoding = encoding_for_label(label)?;
    if encoding == encoding_rs::UTF_8 {
        // UTF-8 走 from_utf8 严格路径：错误信息带首个非法偏移，
        // 与 remote_text::decode_remote_text 的 NotUtf8 口径一致
        return match std::str::from_utf8(bytes) {
            Ok(text) => Ok(text.to_string()),
            Err(e) => Err(format!(
                "第 {} 字节处不是合法 UTF-8（可能是 GBK/Big5 等本地编码，请切换编码重试）",
                e.valid_up_to()
            )),
        };
    }
    let (cow, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return Err(format!(
            "内容含「{}」编码无法解码的字节序列（可能选错了编码）",
            encoding.name()
        ));
    }
    Ok(cow.into_owned())
}

/// 按指定编码把文本严格编码为字节。目标编码无法表示的字符（如 emoji 进 GBK）→ `Err`。
pub fn encode_text(text: &str, label: &str) -> Result<Vec<u8>, String> {
    let encoding = encoding_for_label(label)?;
    if encoding == encoding_rs::UTF_8 {
        return Ok(text.as_bytes().to_vec());
    }
    let (cow, _, had_errors) = encoding.encode(text);
    if had_errors {
        return Err(format!(
            "文本含「{}」编码无法表示的字符（如 emoji/生僻字进 GBK），保存会被损坏，已拒绝",
            encoding.name()
        ));
    }
    Ok(cow.into_owned())
}

/// 换行风格三态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EolKind {
    /// 全部（或不含换行）为 LF。
    Lf,
    /// 全部（或不含换行）为 CRLF。
    Crlf,
    /// CRLF 与孤立 LF/CR 混杂——只报告事实，不替用户归一。
    Mixed,
}

/// 检测文本的换行风格（不含换行的文本返回 Lf，视为无 CRLF 证据）。
pub fn detect_eol(text: &str) -> EolKind {
    let has_crlf = text.contains("\r\n");
    // 孤立 \n 或孤立 \r（老 Mac 风格）都算与 CRLF 混杂的证据；
    // 先按 \r\n 切开，剩下的段里再出现 \r/\n 即为孤立
    let has_lone = text
        .split("\r\n")
        .any(|seg| seg.contains('\n') || seg.contains('\r'));
    match (has_crlf, has_lone) {
        (true, true) => EolKind::Mixed,
        (true, false) => EolKind::Crlf,
        // 无 CRLF 时：残留孤立 \r（无任何 \n）既不是 Lf 也不是 Crlf，如实报 Mixed
        (false, _) if text.contains('\r') => EolKind::Mixed,
        (false, _) => EolKind::Lf,
    }
}

/// 把文本换行统一为目标风格。先归一到 \n 再展开，保证幂等
/// （CRLF→CRLF 不会变 \r\r\n；孤立 \r 也归一为普通换行）。
/// `Mixed` 作为目标是 no-op（调用方不应传，防御性保留原样）。
pub fn apply_eol(text: &str, eol: EolKind) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    match eol {
        EolKind::Lf | EolKind::Mixed => normalized,
        EolKind::Crlf => normalized.replace('\n', "\r\n"),
    }
}

/// UTF-8 BOM（U+FEFF）。GBK 等本地编码无 BOM 概念，不做处理。
pub const UTF8_BOM: &str = "\u{FEFF}";

/// 剥离文本头部的 UTF-8 BOM，返回 (正文, 是否带 BOM)。
/// BOM 会污染下游 JSON/YAML 解析，编辑器读入时剥离、写回时按需还原。
pub fn strip_utf8_bom(text: &str) -> (&str, bool) {
    match text.strip_prefix(UTF8_BOM) {
        Some(rest) => (rest, true),
        None => (text, false),
    }
}

/// 给文本加回 UTF-8 BOM（读时剥离过、用户未取消 BOM 的写回场景）。
pub fn prepend_utf8_bom(text: &str) -> String {
    format!("{UTF8_BOM}{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_allowlist_accepts_canonical_and_aliases() {
        // 规范名 + 大小写/首尾空白
        assert!(encoding_for_label("utf-8").is_ok());
        assert!(encoding_for_label(" UTF-8 ").is_ok());
        assert!(encoding_for_label("Shift_JIS").is_ok());
        assert!(encoding_for_label("windows-1252").is_ok());
        // 常见别名经 for_label 归一到白名单内规范名（cp936 不在 WHATWG 标签表，
        // 前端编码下拉只给规范名，不依赖别名）
        assert!(encoding_for_label("sjis").is_ok());
        // 白名单外拒绝（fail-closed，不回落）
        assert!(encoding_for_label("utf-9").is_err());
        assert!(!is_supported_encoding("utf-16"));
        // WHATWG 把 latin1 归一到 windows-1252，属白名单内 → 应通过
        assert!(encoding_for_label("latin1").is_ok());
    }

    #[test]
    fn gbk_round_trip() {
        let text = "中文配置\nport=3306\n";
        let bytes = encode_text(text, "gbk").unwrap();
        assert_eq!(decode_bytes(&bytes, "gbk").unwrap(), text);
        // 「中文」的 GBK 编码确实不是合法 UTF-8（remote_text 用例同款字节）
        assert_eq!(&bytes[..4], &[0xD6, 0xD0, 0xCE, 0xC4]);
    }

    #[test]
    fn shift_jis_and_big5_and_cp1252_round_trip() {
        for (label, text) in [
            ("shift_jis", "設定ファイル\n"),
            ("big5", "系統設定\n"),
            ("windows-1252", "café — naïve\n"),
            ("euc-kr", "환경 설정\n"),
            ("gb18030", "配置文件\n"),
        ] {
            let bytes = encode_text(text, label).unwrap();
            assert_eq!(decode_bytes(&bytes, label).unwrap(), text, "{label} 往返失真");
        }
    }

    #[test]
    fn invalid_gbk_sequence_is_rejected_not_replaced() {
        // 0xFF 不是 GBK 合法首字节 → encoding_rs 会替换并置 had_errors → 必须 Err
        let bad = [b'a', 0xFF, 0xFF, b'b'];
        let err = decode_bytes(&bad, "gbk").unwrap_err();
        assert!(err.contains("GBK"), "{err}");
    }

    #[test]
    fn unrepresentable_char_rejected_on_encode() {
        // emoji 无法用 GBK 表示——编码必须拒绝而不是替换成问号
        let err = encode_text("配置 😀", "gbk").unwrap_err();
        assert!(err.contains("无法表示"), "{err}");
    }

    #[test]
    fn utf8_strict_path_reports_offset() {
        let gbk_bytes = [0xD6u8, 0xD0, 0xCE, 0xC4];
        let err = decode_bytes(&gbk_bytes, "utf-8").unwrap_err();
        assert!(err.contains("第 0 字节"), "{err}");
    }

    #[test]
    fn eol_detection_and_apply() {
        assert_eq!(detect_eol("a\nb\nc"), EolKind::Lf);
        assert_eq!(detect_eol("a\r\nb\r\nc"), EolKind::Crlf);
        assert_eq!(detect_eol("a\r\nb\nc"), EolKind::Mixed);
        assert_eq!(detect_eol("a\rb"), EolKind::Mixed, "孤立 \\r 与 LF 侧混杂");
        assert_eq!(detect_eol("没有换行"), EolKind::Lf);

        // 应用幂等：CRLF→CRLF 不产生 \r\r\n
        let crlf = "a\r\nb\r\n";
        assert_eq!(apply_eol(crlf, EolKind::Crlf), crlf);
        // LF→CRLF→LF 往返
        let lf = "a\nb";
        let to_crlf = apply_eol(lf, EolKind::Crlf);
        assert_eq!(to_crlf, "a\r\nb");
        assert_eq!(apply_eol(&to_crlf, EolKind::Lf), lf);
        // 混合输入统一为 CRLF
        assert_eq!(apply_eol("a\r\nb\nc", EolKind::Crlf), "a\r\nb\r\nc");
    }

    #[test]
    fn utf8_bom_strip_and_prepend_round_trip() {
        let (body, has_bom) = strip_utf8_bom("\u{FEFF}{\"a\":1}");
        assert!(has_bom);
        assert_eq!(body, "{\"a\":1}");
        let (again, has_again) = strip_utf8_bom(body);
        assert!(!has_again && again == body);
        assert_eq!(prepend_utf8_bom(body), "\u{FEFF}{\"a\":1}");
    }
}
