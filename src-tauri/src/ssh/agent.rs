//! SSH agent 认证桥（v0.20，SSH P1 收官件）。
//!
//! ## PoC 结论（russh 0.49.2 实测 API 形态）
//!
//! - `russh::keys::agent::client::AgentClient` 实现了 `russh::client::auth::Signer`
//!   trait，可直接传给 `handle.authenticate_publickey_with(user, pubkey, &mut agent)`；
//! - Windows 双通道：`connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")`
//!   （Windows 内置 OpenSSH agent 服务）→ `connect_pageant()`（Pageant 窗口
//!   消息协议，russh-keys 自带 PageantStream）；
//! - `request_identities()` 列出 agent 持有的公钥。
//!
//! ## 认证策略
//!
//! 列出全部 agent 公钥，**逐个尝试**——agent 里可能有多个 key，服务器接受
//! 哪个我们无法预知（公钥与 authorized_keys 的匹配在服务器侧）。任何一个
//! 成功即通过；全部失败才报错（错误文案如实说「逐个尝试了 N 个」）。
//!
//! ## 安全语义
//!
//! 私钥永远留在 agent 进程——本应用只收公钥与签名，不触私钥字节。
//! 这是 agent 模式的根本价值：密钥零复制（§8 精神的自然延伸）。

use russh::keys::agent::client::AgentClient;
use russh::keys::ssh_key::PublicKey;

/// Windows OpenSSH agent 的默认 named pipe 路径。
const OPENSSH_AGENT_PIPE: &str = r"\\.\pipe\openssh-ssh-agent";

/// Agent 认证结果。
pub enum AgentAuthOutcome {
    /// 成功。
    Authenticated,
    /// agent 不可连接 / 无 key / 全部 key 被拒 / 签名失败。
    Failed(String),
}

/// 用 agent 完成公钥认证：named pipe 优先，Pageant 兜底，逐 key 尝试。
#[allow(clippy::type_complexity)]
pub async fn authenticate_with_agent<U, H>(
    handle: &mut russh::client::Handle<H>,
    username: U,
) -> AgentAuthOutcome
where
    U: Into<String> + Clone,
    H: russh::client::Handler<Error = russh::Error> + Send + 'static,
{
    let username_pipe = username.clone();
    let username_pageant = username;

    // 通道①：OpenSSH named pipe（Windows 内置 agent 服务）
    #[cfg(windows)]
    {
        match AgentClient::connect_named_pipe(OPENSSH_AGENT_PIPE).await {
            Ok(mut client) => {
                log::info!("SSH agent: connected via OpenSSH named pipe");
                if let Some(outcome) =
                    try_all_keys(handle, username_pipe.clone(), &mut client).await
                {
                    return outcome;
                }
                // named pipe 的 key 全被拒——继续试 Pageant（可能加载了不同 key 集合）
                log::info!("SSH agent: named pipe keys 未通过，尝试 Pageant");
            }
            Err(e) => {
                log::info!("SSH agent: named pipe 不可用（{e}），尝试 Pageant");
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = username_pipe;
    }

    // 通道②：Pageant
    let mut pageant = AgentClient::connect_pageant().await;
    if let Some(outcome) = try_all_keys(handle, username_pageant, &mut pageant).await {
        return outcome;
    }
    AgentAuthOutcome::Failed(
        "SSH agent 认证失败：已逐个尝试 agent 中的全部公钥，均被服务器拒绝。请确认 agent 已加载对应私钥、服务器 authorized_keys 含其公钥".to_string(),
    )
}

/// 逐 key 尝试认证。
/// Some(outcome) = 有确定结果（成功 / agent 故障）；None = key 全被拒（可换通道再试）。
/// 泛型 S：两种通道的流类型不同（NamedPipeClient / PageantStream），AgentClient<S>
/// 对满足 AgentStream+Send+Unpin+'static 的 S 均实现 Signer。
async fn try_all_keys<S, H>(
    handle: &mut russh::client::Handle<H>,
    username: impl Into<String> + Clone,
    agent: &mut AgentClient<S>,
) -> Option<AgentAuthOutcome>
where
    S: russh::keys::agent::client::AgentStream + Send + Unpin + 'static,
    H: russh::client::Handler<Error = russh::Error> + Send + 'static,
{
    let identities: Vec<PublicKey> = match agent.request_identities().await {
        Ok(keys) => keys,
        Err(e) => {
            return Some(AgentAuthOutcome::Failed(format!(
                "SSH agent 无法列出密钥（{e}）——agent 未运行或协议不兼容"
            )))
        }
    };
    if identities.is_empty() {
        log::info!("SSH agent: 无密钥（agent 在运行但未加载任何私钥）");
        return None;
    }
    log::info!("SSH agent: 尝试 {} 个密钥", identities.len());
    for key in identities {
        match handle
            .authenticate_publickey_with(username.clone(), key, agent)
            .await
        {
            Ok(true) => {
                log::info!("SSH agent: 公钥认证成功");
                return Some(AgentAuthOutcome::Authenticated);
            }
            Ok(false) => continue, // 此 key 被拒，试下一个
            Err(e) => {
                return Some(AgentAuthOutcome::Failed(format!("SSH agent 签名失败: {e}")))
            }
        }
    }
    None
}
