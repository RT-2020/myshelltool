//! Gist API 客户端（create/get/update，从 sync.rs 按域拆出，v2.8 第四轮）。
//! HTTP 走 crate::http 单例（connect/整体超时 + 连接池复用）。
use super::*;

// ─── Gist API 客户端 ───

#[derive(Serialize)]
pub struct CreateGistRequest<'a> {
    description: &'a str,
    #[serde(rename = "public")]
    _public: bool,
    files: std::collections::HashMap<&'a str, GistFileContent<'a>>,
}

#[derive(Serialize)]
pub struct GistFileContent<'a> {
    content: &'a str,
}

#[derive(Deserialize, Debug)]
pub struct GistResponse {
    id: String,
    updated_at: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct GistGetResponse {
    updated_at: Option<String>,
    files: Option<std::collections::HashMap<String, GistFileMeta>>,
}

#[derive(Deserialize, Debug)]
pub struct GistFileMeta {
    content: Option<String>,
}

/// 创建 Gist（首次推送）。返回 (gist_id, updated_at)。
pub async fn gist_create(pat: &str, content: &str) -> Result<(String, Option<String>), String> {
    let mut files = std::collections::HashMap::new();
    files.insert(
        GIST_FILENAME,
        GistFileContent { content },
    );
    let body = CreateGistRequest {
        description: "myshelltool connection assets sync (encrypted)",
        _public: false, // 私有 Gist
        files,
    };
    let client = crate::http::shared_client()?;
    let resp = client
        .post(format!("{GITHUB_API_BASE}/gists"))
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gist create 请求失败: {}", crate::http::error_chain(&e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gist create 失败 (HTTP {status}): {text}"));
    }
    let gist: GistResponse = resp
        .json()
        .await
        .map_err(|e| format!("Gist create 响应解析失败 (HTTP {status}): {e}"))?;
    Ok((gist.id, gist.updated_at))
}

/// 获取 Gist 内容 + updated_at。Gist 不存在返回 Ok(None)。
pub async fn gist_get(pat: &str, gist_id: &str) -> Result<Option<(String, Option<String>)>, String> {
    let client = crate::http::shared_client()?;
    let resp = client
        .get(format!("{GITHUB_API_BASE}/gists/{gist_id}"))
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await
        .map_err(|e| format!("Gist get 请求失败: {}", crate::http::error_chain(&e)))?;

    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None); // Gist 不存在（可能被手动删了）
    }
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gist get 失败 (HTTP {status}): {text}"));
    }
    let gist: GistGetResponse = resp
        .json()
        .await
        .map_err(|e| format!("Gist get 响应解析失败: {e}"))?;
    let content = gist
        .files
        .and_then(|f| f.get(GIST_FILENAME).and_then(|m| m.content.clone()))
        .ok_or_else(|| "Gist 中无 sync 文件".to_string())?;
    Ok(Some((content, gist.updated_at)))
}

/// 更新 Gist 内容。返回 updated_at。
pub async fn gist_update(pat: &str, gist_id: &str, content: &str) -> Result<Option<String>, String> {
    let mut files = std::collections::HashMap::new();
    files.insert(GIST_FILENAME, GistFileContent { content });
    let body = CreateGistRequest {
        description: "myshelltool connection assets sync (encrypted)",
        _public: false,
        files,
    };
    let client = crate::http::shared_client()?;
    let resp = client
        .patch(format!("{GITHUB_API_BASE}/gists/{gist_id}"))
        .header("Authorization", format!("Bearer {pat}"))
        .header("Accept", "application/vnd.github+json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Gist update 请求失败: {}", crate::http::error_chain(&e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Gist update 失败 (HTTP {status}): {text}"));
    }
    let gist: GistResponse = resp
        .json()
        .await
        .map_err(|e| format!("Gist update 响应解析失败: {e}"))?;
    Ok(gist.updated_at)
}

// ─── 备份发现（换机恢复免填 Gist ID） ───

/// GET /gists 列表项（只声明发现所需字段；files 的 value 在列表响应里是
/// 文件元数据对象，不含 content，结构无需展开）。
#[derive(Deserialize, Debug)]
struct GistListItem {
    id: String,
    updated_at: Option<String>,
    files: Option<std::collections::HashMap<String, serde_json::Value>>,
}

/// 发现的备份候选（sync_discover_gists 返回项）。
#[derive(Serialize, Debug, Clone)]
pub struct DiscoveredGist {
    pub gist_id: String,
    pub updated_at: Option<String>,
}

/// 列出授权用户自己的 Gist，过滤出含本应用同步文件的备份候选。
///
/// 判定标记是**文件名**（`GIST_FILENAME`，应用写入的稳定键）而非 description——
/// description 用户可以在 GitHub 上随手改掉。分页拉满（per_page=100，返回不足一页
/// 即为末页，硬上限 10 页），只返回元数据不读内容。顺序不做承诺，按时间排序交给
/// 前端解析 updated_at 后比较。
pub async fn gist_list(pat: &str) -> Result<Vec<DiscoveredGist>, String> {
    let client = crate::http::shared_client()?;
    let mut found = Vec::new();
    for page in 1..=10u32 {
        let resp = client
            .get(format!("{GITHUB_API_BASE}/gists?per_page=100&page={page}"))
            .header("Authorization", format!("Bearer {pat}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .map_err(|e| format!("Gist list 请求失败: {}", crate::http::error_chain(&e)))?;

        let status = resp.status();
        if status.as_u16() == 401 {
            // token 被 revoke / 用户改密：明确指向重新登录，而不是笼统的 HTTP 错误
            return Err("GitHub 授权已失效（401），请在同步面板「账号」中重新登录".to_string());
        }
        if !status.is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Gist list 失败 (HTTP {status}): {text}"));
        }
        let page_items: Vec<GistListItem> = resp
            .json()
            .await
            .map_err(|e| format!("Gist list 响应解析失败: {e}"))?;
        let is_last_page = page_items.len() < 100;
        found.extend(
            page_items
                .into_iter()
                .filter(|g| g.files.as_ref().is_some_and(|f| f.contains_key(GIST_FILENAME)))
                .map(|g| DiscoveredGist { gist_id: g.id, updated_at: g.updated_at }),
        );
        if is_last_page {
            break;
        }
    }
    Ok(found)
}
