fn main() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let mut attrs = tauri_build::Attributes::new();

    let has_windres = std::process::Command::new("windres")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());

    if has_windres {
        if !manifest_dir.is_ascii() {
            let icon_src = std::path::Path::new(&manifest_dir).join("icons/icon.ico");
            if icon_src.exists() {
                let tmp_dir = std::path::PathBuf::from(std::env::var("TEMP").unwrap_or_else(|_| {
                    std::path::PathBuf::from("C:\\tmp").to_str().unwrap().to_string()
                }));
                let tmp_icon = tmp_dir.join("myshelltool-icon.ico");
                std::fs::create_dir_all(&tmp_dir).ok();
                std::fs::copy(&icon_src, &tmp_icon).expect("failed to copy icon to temp");
                let win_attrs =
                    tauri_build::WindowsAttributes::new().window_icon_path(&tmp_icon);
                attrs = attrs.windows_attributes(win_attrs);
            }
        }
    } else {
        attrs = attrs.windows_attributes(
            tauri_build::WindowsAttributes::new()
        );
    }

    let result = std::panic::catch_unwind(|| {
        tauri_build::try_build(attrs).expect("failed to run build script");
    });

    if result.is_err() && !has_windres {
        println!("cargo:warning=tauri build: windres not found, windows resources skipped");
    }
}
