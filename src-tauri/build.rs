fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let target = std::env::var("TARGET").unwrap_or_default();
    let needs_windres = target.ends_with("-windows-gnu");
    if needs_windres {
        println!("cargo:rustc-cdylib-link-arg=-Wl,--exclude-libs,ALL");
    }
    let mut attrs = tauri_build::Attributes::new();
    // winres 绕道用到的临时图标副本；try_build 之后（windres 已读完）清理。
    let mut staged_icon: Option<std::path::PathBuf> = None;

    let has_windres = std::process::Command::new("windres")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());

    if needs_windres && !has_windres {
        panic!("windres is required to embed Windows resources/manifest; install MSYS2 mingw64 windres or ensure it is on PATH");
    }

    if needs_windres && !manifest_dir.is_ascii() {
        let icon_src = std::path::Path::new(&manifest_dir).join("icons/icon.ico");
        if icon_src.exists() {
            // 临时目录来源：std::env::temp_dir()，**不读 TEMP 环境变量、不硬编码盘符**。
            // 旧实现读 TEMP（std::env::var 对非 UTF-8 值直接 Err，会被当成「缺失」）
            // 并在缺失时回退字面 C:\tmp：标准用户对系统盘根没有建目录权限 →
            // create_dir_all 失败被 .ok() 吞掉 → 下一行 copy 的 expect panic，
            // 报错指向图标而不是 TEMP/权限。temp_dir() 按 TMP → TEMP → USERPROFILE
            // → 系统目录顺序探测，返回 PathBuf（非 UTF-8 安全）且必是用户可写位置。
            let tmp_dir = std::env::temp_dir();
            // 文件名带进程 id + 纳秒时间戳：%TEMP% 是机器全局命名空间，多份 checkout
            // （或 CI 并发 job、cargo build 与 cargo test 并行）若共用固定名
            // myshelltool-icon.ico 会互相覆盖，可能把**另一份副本/另一版本的图标**
            // 静默嵌进资源——错误产物且全程无报错。
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let tmp_icon = tmp_dir.join(format!(
                "myshelltool-icon-{}-{stamp}.ico",
                std::process::id()
            ));
            // 建目录失败带路径与原因显式报错：静默跳过只会让下面 copy 失败，
            // 而那条错误信息指向图标路径，掩盖真正的 TEMP/权限问题。
            std::fs::create_dir_all(&tmp_dir).unwrap_or_else(|e| {
                panic!(
                    "无法创建临时目录 {}（TEMP/TMP 不可用或无写权限？）: {e}",
                    tmp_dir.display()
                )
            });
            std::fs::copy(&icon_src, &tmp_icon).unwrap_or_else(|e| {
                panic!(
                    "无法把图标 {} 拷到临时路径 {}（TEMP/TMP 不可用或无写权限？）: {e}",
                    icon_src.display(),
                    tmp_icon.display()
                )
            });
            // 这段绕道的目的是给 windres 一个纯 ASCII 路径；若 TEMP 本身含非 ASCII
            // （如 C:\Users\张三\AppData\Local\Temp），拷贝解决不了问题。这里不猜
            // 也不回退字面盘符，只把原因和出路讲清楚，让 windres 的真实报错可归因。
            if !tmp_dir.as_os_str().is_ascii() {
                println!(
                    "cargo:warning=临时目录 {} 含非 ASCII 字符，windres 可能仍无法处理该路径；若构建失败，请把 TEMP/TMP 指向纯 ASCII 目录后重试",
                    tmp_dir.display()
                );
            }
            let win_attrs = tauri_build::WindowsAttributes::new().window_icon_path(&tmp_icon);
            attrs = attrs.windows_attributes(win_attrs);
            staged_icon = Some(tmp_icon);
        }
    }

    tauri_build::try_build(attrs).expect("failed to run build script");

    // best-effort 清理：windres 已在上面的 try_build 内同步读完图标（tauri-build 的
    // WindowsResource::compile），此处删除临时副本。删除失败不影响本次构建产物
    //（文件在 %TEMP%，系统会回收），故不 panic，但打印原因——不静默吞错。
    if let Some(icon) = staged_icon {
        if let Err(e) = std::fs::remove_file(&icon) {
            println!("cargo:warning=无法清理临时图标 {}: {e}", icon.display());
        }
    }
}
