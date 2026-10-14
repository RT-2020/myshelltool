//! Agent named pipe early eof 复现探针（v0.19.1 真机验收发现的缺陷定位）。
//! 用法：cargo run --example agent_probe（需 ssh-agent 服务运行 + 至少一把 key）
//! 对照组：
//!   A. russh AgentClient::connect_named_pipe + request_identities（复现缺陷路径）
//!   B. tokio NamedPipeClient 手写 agent 协议（4B 长度 + 0x0B，读响应）——绕开 russh
//!
//! 仅 Windows：named pipe 与 connect_named_pipe 均为 Windows 机制
//! （portability 基线：非 Windows 平台编译为占位 main）。
#[cfg(windows)]
use russh::keys::agent::client::AgentClient;

#[cfg(windows)]
#[tokio::main]
async fn main() {
    let pipe = r"\\.\pipe\openssh-ssh-agent";

    // —— 对照组 B：手写协议（先跑，证明 pipe 本身可用）——
    match manual_pipe_probe(pipe).await {
        Ok(n) => println!("[B-manual] pipe OK, identities answer carries {n} bytes payload"),
        Err(e) => println!("[B-manual] FAILED: {e}"),
    }

    // —— 对照组 A：russh 路径 ——
    match AgentClient::connect_named_pipe(pipe).await {
        Ok(mut client) => {
            println!("[A-russh] connected");
            match client.request_identities().await {
                Ok(keys) => println!("[A-russh] request_identities OK, {} keys", keys.len()),
                Err(e) => println!("[A-russh] request_identities FAILED: {e}"),
            }
        }
        Err(e) => println!("[A-russh] connect FAILED: {e}"),
    }
}

/// 非 Windows 占位：named pipe 探针无意义，编译占位保持 example 目标存在。
#[cfg(not(windows))]
fn main() {
    println!("agent_probe 仅 Windows（named pipe / connect_named_pipe）");
}

/// 手写 agent 协议：连 pipe → 写 [00 00 00 01 0B] → 读 4B 长度 → 读载荷。
#[cfg(windows)]
async fn manual_pipe_probe(path: &str) -> Result<usize, String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut client = tokio::net::windows::named_pipe::ClientOptions::new()
        .open(path)
        .map_err(|e| format!("open: {e}"))?;
    client
        .write_all(&[0, 0, 0, 1, 0x0B])
        .await
        .map_err(|e| format!("write: {e}"))?;
    client.flush().await.map_err(|e| format!("flush: {e}"))?;
    let mut len_buf = [0u8; 4];
    client
        .read_exact(&mut len_buf)
        .await
        .map_err(|e| format!("read len: {e}"))?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    client
        .read_exact(&mut payload)
        .await
        .map_err(|e| format!("read payload({len}): {e}"))?;
    if payload.first() != Some(&12) {
        return Err(format!("unexpected type {}", payload.first().unwrap_or(&0)));
    }
    Ok(len)
}
