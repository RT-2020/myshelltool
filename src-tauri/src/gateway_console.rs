//! 网关模式的 Windows 控制台支持（计划 §5 决策 11）。
//!
//! release 构建是 `windows_subsystem = "windows"`（无控制台），`--gateway` /
//! `--import` 从终端/PowerShell 运行时靠 `AttachConsole(ATTACH_PARENT_PROCESS)`
//! 附加到调用方控制台回显结果；双击运行等无控制台场景退化为「日志 + 退出码」。
//! 口令读取用 CONIN$ + 关回显（不落 shell 历史，也不在共享终端明文显示）。
//!
//! FFI 手写 extern 声明（kernel32）而不引 windows-sys：只 6 个函数，
//! 引 crate 反而多一个依赖面。常量值来自 WinCon.h / FileAPI.h，逐个注释出处。

#[cfg(windows)]
mod ffi {
    type HANDLE = isize;
    type BOOL = i32;
    type DWORD = u32;

    /// ATTACH_PARENT_PROCESS（WinCon.h：((DWORD)-1)）
    pub const ATTACH_PARENT_PROCESS: DWORD = 0xFFFF_FFFF;
    pub const GENERIC_READ: DWORD = 0x8000_0000;
    pub const GENERIC_WRITE: DWORD = 0x4000_0000;
    /// FILE_SHARE_READ | FILE_SHARE_WRITE（FileAPI.h：0x00000001 | 0x00000002）
    pub const FILE_SHARE_RW: DWORD = 0x3;
    pub const OPEN_EXISTING: DWORD = 3;
    pub const INVALID_HANDLE_VALUE: HANDLE = -1;
    // WinCon.h：读口令时仅去掉 ENABLE_ECHO_INPUT（0x0004）关回显；
    // 行缓冲（ENABLE_LINE_INPUT 0x0002）保留——回车提交、退格可编辑
    pub const ENABLE_ECHO_INPUT: DWORD = 0x0004;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn AttachConsole(dwProcessId: DWORD) -> BOOL;
        pub fn CreateFileW(
            lpFileName: *const u16,
            dwDesiredAccess: DWORD,
            dwShareMode: DWORD,
            lpSecurityAttributes: *mut core::ffi::c_void,
            dwCreationDisposition: DWORD,
            dwFlagsAndAttributes: DWORD,
            hTemplateFile: HANDLE,
        ) -> HANDLE;
        pub fn WriteConsoleW(
            hConsoleOutput: HANDLE,
            lpBuffer: *const u16,
            nCharsToWrite: DWORD,
            lpNumberOfCharsWritten: *mut DWORD,
            lpReserved: *mut core::ffi::c_void,
        ) -> BOOL;
        pub fn ReadConsoleW(
            hConsoleInput: HANDLE,
            lpBuffer: *mut u16,
            nCharsToRead: DWORD,
            lpNumberOfCharsRead: *mut DWORD,
            pInputControl: *mut core::ffi::c_void,
        ) -> BOOL;
        pub fn GetConsoleMode(hConsoleHandle: HANDLE, lpMode: *mut DWORD) -> BOOL;
        pub fn SetConsoleMode(hConsoleHandle: HANDLE, dwMode: DWORD) -> BOOL;
        pub fn CloseHandle(hObject: HANDLE) -> BOOL;
    }
}

#[cfg(windows)]
use std::io;

/// 尝试附加到父进程控制台。成功返回 true（此后 console_out / read_password 可用）。
#[cfg(windows)]
pub fn attach_parent_console() -> bool {
    unsafe { ffi::AttachConsole(ffi::ATTACH_PARENT_PROCESS) != 0 }
}

#[cfg(windows)]
fn open_console(name: &str, access: u32) -> io::Result<isize> {
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);
    let handle = unsafe {
        ffi::CreateFileW(
            wide.as_ptr(),
            access,
            ffi::FILE_SHARE_RW,
            core::ptr::null_mut(),
            ffi::OPEN_EXISTING,
            0,
            0,
        )
    };
    if handle == ffi::INVALID_HANDLE_VALUE {
        Err(io::Error::last_os_error())
    } else {
        Ok(handle)
    }
}

/// 向控制台写一行（无控制台时静默——信息同时进 gateway.log）。
#[cfg(windows)]
pub fn console_out(line: &str) {
    let Ok(handle) = open_console("CONOUT$", ffi::GENERIC_WRITE) else {
        return;
    };
    let mut wide: Vec<u16> = line.encode_utf16().collect();
    wide.push(0x0A); // '\n' 的 UTF-16 编码
    let mut written = 0u32;
    unsafe {
        ffi::WriteConsoleW(handle, wide.as_ptr(), wide.len() as u32, &mut written, core::ptr::null_mut());
        ffi::CloseHandle(handle);
    }
}

/// 从控制台读一行口令（读前关回显、读后恢复模式）。
/// 无控制台（双击运行）或读失败 → Err（调用方提示改在有终端的环境运行或用环境变量）。
#[cfg(windows)]
pub fn read_password(prompt: &str) -> io::Result<String> {
    console_out(prompt);
    let handle = open_console("CONIN$", ffi::GENERIC_READ)?;
    let mut old_mode = 0u32;
    let mode_ok = unsafe { ffi::GetConsoleMode(handle, &mut old_mode) } != 0;
    if mode_ok {
        // 行缓冲保留（回车提交），仅去回显
        let new_mode = old_mode & !ffi::ENABLE_ECHO_INPUT;
        unsafe { ffi::SetConsoleMode(handle, new_mode) };
    }
    let mut buf = [0u16; 512];
    let mut read = 0u32;
    let result = unsafe {
        ffi::ReadConsoleW(handle, buf.as_mut_ptr(), buf.len() as u32, &mut read, core::ptr::null_mut())
    };
    if mode_ok {
        unsafe { ffi::SetConsoleMode(handle, old_mode) };
    }
    console_out(""); // 关回显后的手动换行
    unsafe { ffi::CloseHandle(handle) };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    let text = String::from_utf16_lossy(&buf[..read as usize])
        .trim_end_matches(['\r', '\n'])
        .trim()
        .to_string();
    Ok(text)
}

// ─── 非 Windows：网关 CLI 本产品面向 Windows（AGENTS.md §1），其余平台明确报错 ───

#[cfg(not(windows))]
pub fn attach_parent_console() -> bool {
    false
}

#[cfg(not(windows))]
pub fn console_out(_line: &str) {}

#[cfg(not(windows))]
pub fn read_password(_prompt: &str) -> std::io::Result<String> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "网关模式仅支持 Windows",
    ))
}
