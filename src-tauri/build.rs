fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let target = std::env::var("TARGET").unwrap_or_default();
    let needs_windres = target.ends_with("-windows-gnu");
    if needs_windres {
        println!("cargo:rustc-cdylib-link-arg=-Wl,--exclude-libs,ALL");
    }
    let mut attrs = tauri_build::Attributes::new();

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
            let tmp_dir = std::path::PathBuf::from(std::env::var("TEMP").unwrap_or_else(|_| {
                std::path::PathBuf::from("C:\\tmp")
                    .to_str()
                    .unwrap()
                    .to_string()
            }));
            let tmp_icon = tmp_dir.join("myshelltool-icon.ico");
            std::fs::create_dir_all(&tmp_dir).ok();
            std::fs::copy(&icon_src, &tmp_icon).expect("failed to copy icon to temp");
            let win_attrs = tauri_build::WindowsAttributes::new().window_icon_path(&tmp_icon);
            attrs = attrs.windows_attributes(win_attrs);
        }
    }

    tauri_build::try_build(attrs).expect("failed to run build script");
}
