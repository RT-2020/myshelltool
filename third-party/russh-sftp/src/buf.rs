use bytes::Buf;

use crate::error::Error;

pub trait TryBuf: Buf {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error>;
    fn try_get_string(&mut self) -> Result<String, Error>;
}

impl<T: Buf> TryBuf for T {
    fn try_get_bytes(&mut self) -> Result<Vec<u8>, Error> {
        let len = self
            .try_get_u32()
            .map_err(|e| Error::UnexpectedBehavior(e.to_string()))? as usize;
        if self.remaining() < len {
            return Err(Error::BadMessage("no remaining for vec".to_owned()));
        }

        Ok(self.copy_to_bytes(len).to_vec())
    }

    fn try_get_string(&mut self) -> Result<String, Error> {
        let bytes = self.try_get_bytes()?;
        //String::from_utf8(bytes).map_err(|_| Error::BadMessage("unable to parse str".to_owned()))
        // 【myshelltool fork 补丁】非 UTF-8 文件名的可逆解码：原版 from_utf8_lossy
        // 把非法字节换成 U+FFFD（不可逆），客户端拿失真名回发 remove/rename 时
        // 服务器按字节比对必不命中，且可能误中「字面含 U+FFFD 的另一真实文件」。
        // 此处把每个非法字节 b 编码为 U+E000+b（PUA-A），与 ser.rs serialize_str
        // 的反向解码配对，使文件名字节在「列出→显示→回发」全链路无损往返。
        Ok(decode_lossy_reversible(&bytes))
    }
}

/// 非法 UTF-8 字节 → U+E000+b（PUA-A）的可逆映射；合法序列原样保留。
pub(crate) fn decode_lossy_reversible(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(err) => {
            let mut out = String::with_capacity(bytes.len());
            let (valid, rest) = bytes.split_at(err.valid_up_to());
            out.push_str(unsafe { std::str::from_utf8_unchecked(valid) });
            for &b in rest {
                // PUA-A：U+E000..=U+E0FF 与单字节一一对应
                out.push(char::from_u32(0xE000 + b as u32).unwrap_or('\u{FFFD}'));
            }
            out
        }
    }
}

/// serialize_str 的反向解码：U+E000+b → 字节 b；其余字符按 UTF-8 原样。
pub(crate) fn encode_back_to_wire(s: &str) -> std::borrow::Cow<'_, [u8]> {
    if !s.chars().any(|c| ('\u{E000}'..='\u{E0FF}').contains(&c)) {
        return std::borrow::Cow::Borrowed(s.as_bytes());
    }
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        if let cp @ '\u{E000}'..='\u{E0FF}' = c {
            out.push(cp as u32 as u8);
        } else {
            let mut buf = [0u8; 4];
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
    std::borrow::Cow::Owned(out)
}

#[cfg(test)]
mod myshelltool_tests {
    use super::*;
    use bytes::BytesMut;

    /// GBK 名字（非法 UTF-8）→ 解码为 PUA-A 串 → 回编码 → 字节一致（fork 核心契约）。
    #[test]
    fn gbk_filename_roundtrips_byte_exact() {
        // “测试.txt” 的 GBK 编码（0xB2 0xE2 0xCA 0xD4）+ ASCII 后缀
        let raw: Vec<u8> = vec![0xB2, 0xE2, 0xCA, 0xD4, b'.', b't', b'x', b't'];
        let decoded = decode_lossy_reversible(&raw);
        // 解码结果不含 U+FFFD（可逆，不是 lossy）
        assert!(!decoded.contains('\u{FFFD}'), "must be reversible, got {decoded:?}");
        // 混合场景：合法 UTF-8 中文 + 非法字节交错
        let mixed: Vec<u8> = "配置".as_bytes().iter().copied().chain([0xFF, 0xFE, b'=', b'1']).collect();
        let d2 = decode_lossy_reversible(&mixed);
        assert!(d2.starts_with("配置"));
        assert!(!d2.contains('\u{FFFD}'));
        // 回编码字节级一致
        assert_eq!(encode_back_to_wire(&decoded).as_ref(), raw.as_slice(), "decode/encode must be inverse");
        assert_eq!(encode_back_to_wire(&d2).as_ref(), mixed.as_slice());
    }

    /// 合法 UTF-8 全程零拷贝（Cow::Borrowed 路径）。
    #[test]
    fn valid_utf8_passes_through_unchanged() {
        let s = "正常文件名.conf";
        assert_eq!(decode_lossy_reversible(s.as_bytes()), s);
        assert!(matches!(encode_back_to_wire(s), std::borrow::Cow::Borrowed(_)));
    }

    /// wire 往返：try_get_string 读出的串经 serialize_str 等价路径写回 → 字节一致。
    #[test]
    fn wire_string_roundtrip_via_trybuf() {
        let raw: Vec<u8> = vec![0xD0, b'a', 0x80, b'b'];
        // 手工构造 length-prefixed buffer（与协议同形）
        let mut buf = BytesMut::new();
        buf.extend_from_slice(&(raw.len() as u32).to_be_bytes());
        buf.extend_from_slice(&raw);
        let got: String = buf.try_get_string().expect("parse");
        assert!(!got.contains('\u{FFFD}'));
        assert_eq!(encode_back_to_wire(&got).as_ref(), raw.as_slice());
    }
}
