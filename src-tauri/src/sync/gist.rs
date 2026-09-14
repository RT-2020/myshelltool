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
        .map_err(|e| format!("Gist update 响应解析失败 (HTTP {status}): {e}"))?;
    Ok(gist.updated_at)
}
