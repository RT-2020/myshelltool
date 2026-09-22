//! GUI 弹窗式 keyboard-interactive 认证循环（v0.20 自 session.rs 拆出，守 800 行
//! Rust 硬限）。行为原样迁移：密码类 prompt 自动应答（存了密码时），非密码类
//! prompt 经 GUI 弹窗让用户输入（ssh-keyboard-interactive 事件链）。

use super::*;



pub(crate) async fn keyboard_interactive_loop(
    mut handle: client::Handle<SshClient>,
    mut resp: client::KeyboardInteractiveAuthResponse,
    pending_keyboard: &PendingKeyboardResponses,
    app: &AppHandle,
    auto_password: Option<&str>,
) -> Result<(client::Handle<SshClient>, bool), String> {
    loop {
        match resp {
            client::KeyboardInteractiveAuthResponse::Success => return Ok((handle, true)),
            client::KeyboardInteractiveAuthResponse::Failure => return Ok((handle, false)),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                // 判断是否所有 prompts 都是密码类（单 prompt + 文本含 password/passphrase/密码）
                // 且 auto_password 可用 → 自动响应，不弹 modal
                let all_password_like = !prompts.is_empty()
                    && prompts.iter().all(|p| {
                        let lower = p.prompt.to_lowercase();
                        lower.contains("password") || lower.contains("passphrase") || lower.contains("密码")
                    });
                let responses = if all_password_like && auto_password.is_some() {
                    let pwd = auto_password.unwrap().to_string();
                    info!(
                        "keyboard-interactive: auto-responding {} password-like prompt(s) with saved credential",
                        prompts.len()
                    );
                    prompts.iter().map(|_| pwd.clone()).collect::<Vec<String>>()
                } else {
                    // 弹 modal 让用户手动输入（MFA、非密码 prompt 等）
                    let request_id = uuid::Uuid::new_v4().to_string();
                    let (tx, rx) = oneshot::channel();
                    {
                        let mut map = pending_keyboard.lock().await;
                        map.insert(request_id.clone(), tx);
                    }
                    let prompt_texts: Vec<String> = prompts.iter().map(|p| p.prompt.clone()).collect();
                    let event = KeyboardInteractiveEvent {
                        request_id,
                        name,
                        instructions,
                        prompts: prompt_texts,
                    };
                    if app.emit("ssh-keyboard-interactive", event).is_err() {
                        return Ok((handle, false));
                    }
                    match rx.await {
                        Ok(r) => r,
                        Err(_) => return Ok((handle, false)),
                    }
                };
                resp = handle
                    .authenticate_keyboard_interactive_respond(responses)
                    .await
                    .map_err(|e| format!("Keyboard-interactive respond failed: {e}"))?;
            }
        }
    }
}
