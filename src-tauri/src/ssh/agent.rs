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
//! 成功即通过。
//!
//! ## 通道结果聚合（v0.20 修复，真机验收 2026-09-23 发现）
//!
//! russh 的 `connect_pageant()` 不返回连接错误（无 Pageant 进程时返回一个
//! 读即 EOF 的流），其 `request_identities` 报 early eof——若让它直接冒泡为
//! 最终错误，会掩盖 named pipe 通道的真实结论（key 被拒）。真机曾表现为
//! 「named pipe 的 1 个密钥被拒（authorized_keys 不含该公钥）」却报成
//! 「无法列出密钥（early eof）」。故 eof 类错误归类为「通道不可用」，最终
//! Failed 文案聚合两通道各自的真实结果，把用户指向 authorized_keys 排查。
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
    /// 两通道都未通过；文案聚合各自真实结论。
    Failed(String),
}

/// 单通道尝试的结论（供聚合最终文案）。
enum ChannelResult {
    /// 该通道认证成功（唯一终止成功的路径）。
    Authenticated,
    /// 该通道有确定性故障（签名失败等，不再试其他通道——环境性问题）。
    Fatal(String),
    /// 该通道列出 N 个 key 且全部被服务器拒绝（应查 authorized_keys）。
    AllRejected(usize),
    /// agent 在运行但未加载任何私钥。
    NoKeys,
    /// 通道不可用：连不上 / 列不出 key（eof = 未运行的常见形态）。
    Unavailable(String),
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
    let pipe_result = match AgentClient::connect_named_pipe(OPENSSH_AGENT_PIPE).await {
        Ok(mut client) => {
            log::info!("SSH agent: connected via OpenSSH named pipe");
            try_all_keys(handle, username_pipe.clone(), &mut client).await
        }
        Err(e) => {
            log::info!("SSH agent: named pipe 不可用（{e}），尝试 Pageant");
            ChannelResult::Unavailable(format!("named pipe 连接失败: {e}"))
        }
    };
    #[cfg(windows)]
    {
        if let ChannelResult::Authenticated = pipe_result {
            return AgentAuthOutcome::Authenticated;
        }
        if let ChannelResult::Fatal(msg) = pipe_result {
            return AgentAuthOutcome::Failed(msg);
        }
        log::info!("SSH agent: named pipe keys 未通过，尝试 Pageant（可能持有不同 key 集合）");
    }

    // 通道②：Pageant
    let mut pageant = AgentClient::connect_pageant().await;
    let pageant_result = try_all_keys(handle, username_pageant, &mut pageant).await;
    match pageant_result {
        ChannelResult::Authenticated => return AgentAuthOutcome::Authenticated,
        ChannelResult::Fatal(msg) => return AgentAuthOutcome::Failed(msg),
        _ => {}
    }

    // 聚合两通道的真实结论（走到这里 = 全部未通过且无确定性故障）
    #[cfg(windows)]
    {
        return AgentAuthOutcome::Failed(summarize(&pipe_result, &pageant_result));
    }
    #[allow(unreachable_code)]
    AgentAuthOutcome::Failed(summarize(
        &ChannelResult::Unavailable("非 Windows 无 named pipe 通道".into()),
        &pageant_result,
    ))
}

/// 聚合两通道结论为最终报错文案——如实区分「key 被拒」与「通道不可用」。
fn summarize(pipe: &ChannelResult, pageant: &ChannelResult) -> String {
    let part = |r: &ChannelResult, name: &str| -> String {
        match r {
            ChannelResult::AllRejected(n) => format!("{name}的 {n} 个密钥均被服务器拒绝"),
            ChannelResult::NoKeys => format!("{name}在运行但未加载任何私钥"),
            ChannelResult::Unavailable(e) => format!("{name}不可用（{e}）"),
            // Authenticated/Fatal 在调用方已提前返回，此处不可达；兜底如实
            ChannelResult::Authenticated => format!("{name}已认证"),
            ChannelResult::Fatal(m) => format!("{name}故障: {m}"),
        }
    };
    let mut msg = format!(
        "SSH agent 认证失败：{}；{}。请确认 agent 已加载对应私钥、服务器 authorized_keys 含其公钥",
        part(pipe, "named pipe 通道"),
        part(pageant, "Pageant 通道")
    );
    if matches!(pipe, ChannelResult::AllRejected(_)) || matches!(pageant, ChannelResult::AllRejected(_)) {
        msg.push_str("（有密钥被拒：优先核对 authorized_keys 与 agent 加载的私钥是否匹配）");
    }
    msg
}

/// 逐 key 尝试认证。
/// 泛型 S：两种通道的流类型不同（NamedPipeClient / PageantStream），AgentClient<S>
/// 对满足 AgentStream+Send+Unpin+'static 的 S 均实现 Signer。
async fn try_all_keys<S, H>(
    handle: &mut russh::client::Handle<H>,
    username: impl Into<String> + Clone,
    agent: &mut AgentClient<S>,
) -> ChannelResult
where
    S: russh::keys::agent::client::AgentStream + Send + Unpin + 'static,
    H: russh::client::Handler<Error = russh::Error> + Send + 'static,
{
    let identities: Vec<PublicKey> = match agent.request_identities().await {
        Ok(keys) => keys,
        Err(e) => {
            // Pageant 无进程时 russh 的 connect_pageant 给出读即 EOF 的流，
            // 此处 eof 是「未运行」的常见形态——归入通道不可用而非协议错误。
            return ChannelResult::Unavailable(format!("无法列出密钥: {e}"));
        }
    };
    if identities.is_empty() {
        log::info!("SSH agent: 无密钥（agent 在运行但未加载任何私钥）");
        return ChannelResult::NoKeys;
    }
    log::info!("SSH agent: 尝试 {} 个密钥", identities.len());
    let count = identities.len();
    for key in identities {
        match handle
            .authenticate_publickey_with(username.clone(), key, agent)
            .await
        {
            Ok(true) => {
                log::info!("SSH agent: 公钥认证成功");
                return ChannelResult::Authenticated;
            }
            Ok(false) => continue, // 此 key 被拒，试下一个
            Err(e) => {
                return ChannelResult::Fatal(format!("SSH agent 签名失败: {e}"));
            }
        }
    }
    ChannelResult::AllRejected(count)
}
