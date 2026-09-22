//! 应用日志 FileLogger（v0.20/S7 起含滚动：5 MiB × 3 份）。
//! 自 lib.rs 拆出（守 800 行 Rust 硬上限）；滚动失败不致命（AV 扫描瞬间占用
//! 归档名等场景保留旧句柄继续写，下次再试）。

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
/// 单个日志文件大小上限（v0.20/S7 滚动阈值）。app 日志是排查辅助而非审计
/// （审计在 mcp-execution-log.json，自有 30 天/1000 条清理），滚动只防无界增长。
const LOG_ROTATE_BYTES: u64 = 5 * 1024 * 1024;
/// 滚动保留份数（.1 最新归档 → .3 最旧，超龄删除）。
const LOG_ROTATE_KEEP: u32 = 3;

pub struct FileLogger {
    file: Mutex<std::fs::File>,
    path: std::path::PathBuf,
    /// 自上次滚动检查以来写入的近似字节数（每 256 KiB 查一次实际大小，
    /// 避免每条日志一次 metadata 系统调用）。
    written_since_check: std::sync::atomic::AtomicU64,
}

impl FileLogger {
    pub fn new(path: &std::path::Path) -> Result<Self, std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
            path: path.to_path_buf(),
            written_since_check: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// 滚动：当前文件超上限 → .2→.3、.1→.2、当前→.1，重开新文件。
    /// 滚动失败不致命（AV 扫描瞬间占用 .1 等）：保留旧句柄继续写，下次再试。
    fn rotate(&self, file: &mut std::fs::File) {
        let shift = |from: u32, to: u32| {
            let from_path = self.path.with_extension(format!("log.{from}"));
            let to_path = self.path.with_extension(format!("log.{to}"));
            if from_path.exists() {
                let _ = std::fs::rename(&from_path, &to_path);
            }
        };
        let shift_down = |n: u32| {
            // 从最旧端往新端挪（.2→.3 先行，腾出 .1 的位置）
            let mut k = LOG_ROTATE_KEEP;
            while k > n {
                shift(k - 1, k);
                k -= 1;
            }
        };
        shift_down(1);
        let archived = self.path.with_extension("log.1");
        if std::fs::rename(&self.path, &archived).is_ok() {
            if let Ok(new_file) = OpenOptions::new().create(true).append(true).open(&self.path) {
                *file = new_file;
            } else {
                // 重开失败：把归档挪回来继续写旧文件（尽力而为）
                let _ = std::fs::rename(&archived, &self.path);
            }
        }
    }
}

impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            // 本地时间前缀（便于按时间排查问题；chrono 处理时区/闰秒等）。
            let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
            let msg = format!(
                "[{} {} {}] {}\n",
                now,
                record.level(),
                record.target(),
                record.args()
            );
            eprint!("{}", msg);
            if let Ok(mut file) = self.file.lock() {
                let _ = file.write_all(msg.as_bytes());
                // 近似计数攒到 256 KiB 再查实际大小（metadata 是系统调用，
                // 每条日志查一次不划算）
                use std::sync::atomic::Ordering;
                let written = self
                    .written_since_check
                    .fetch_add(msg.len() as u64, Ordering::Relaxed)
                    + msg.len() as u64;
                if written >= 256 * 1024 {
                    self.written_since_check.store(0, Ordering::Relaxed);
                    if file
                        .metadata()
                        .map(|m| m.len() >= LOG_ROTATE_BYTES)
                        .unwrap_or(false)
                    {
                        self.rotate(&mut file);
                    }
                }
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}
