//! 对齐 Android WebDavClient.kt（244 行）。
//!
//! 与 Kotlin 一致的关键行为：Basic 凭据只发给 baseUrl 主机（reqwest 跨源重定向
//! 自动剥离 Authorization，等价 ktor 的 authHost 守卫）；GET 的 409 用 HEAD
//! 探测区分坚果云怪癖；超时 60-20-60s；仅 GET 设置 UA "SafePassword-Android/1.4"。
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct WebDavConfig {
    pub base_url: String,
    pub username: String,
    pub password: String,
    pub remote_dir: String,
    pub enabled: bool,
}

impl Default for WebDavConfig {
    fn default() -> Self {
        WebDavConfig {
            base_url: String::new(),
            username: String::new(),
            password: String::new(),
            remote_dir: "SafePassword".to_string(),
            enabled: false,
        }
    }
}

impl WebDavConfig {
    pub fn normalized_base_url(&self) -> String {
        format!("{}/", self.base_url.trim().trim_end_matches('/'))
    }

    pub fn normalized_dir(&self) -> String {
        let trimmed = self.remote_dir.trim().trim_matches('/');
        if trimmed.is_empty() {
            "SafePassword".to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// 每个密码库一个云端文件：`<remoteDir>/SafePassword-<ASCII 键>.spv`
    /// （对齐 CloudManifest.keyFor，避免中文文件名与多库互相覆盖）。
    pub fn remote_path(&self, vault_key: &str) -> String {
        format!("{}/SafePassword-{vault_key}.spv", self.normalized_dir())
    }

    /// 旧版单文件布局：升级后只用于读取回退，写入一律走 remote_path。
    pub fn legacy_remote_path(&self) -> String {
        format!("{}/SafePassword.spv", self.normalized_dir())
    }

    /// 1.4.1 那批按库名生成的键（可能含中文），只为升级时认领旧文件。
    pub fn legacy_name_path(&self, name: &str) -> String {
        let lowered = name.trim().to_lowercase();
        let mut key = String::with_capacity(lowered.len());
        let mut last_dash = false;
        for c in lowered.chars() {
            let keep = c.is_ascii_lowercase()
                || c.is_ascii_digit()
                || ('\u{4e00}'..='\u{9fa5}').contains(&c);
            if keep {
                key.push(c);
                last_dash = false;
            } else if !last_dash {
                key.push('-');
                last_dash = true;
            }
        }
        let trimmed = key.trim_matches('-');
        let taken: String = trimmed.chars().take(64).collect();
        let key = if taken.trim_matches('-').is_empty() {
            "vault".to_string()
        } else {
            taken
        };
        format!("{}/{key}.spv", self.normalized_dir())
    }

    /// 云端密码库清单（key → 显示名），用于「从云端恢复」时列出可选的库。
    pub fn manifest_path(&self) -> String {
        format!("{}/SafePassword-index.json", self.normalized_dir())
    }

    pub fn is_configured(&self) -> bool {
        self.base_url.starts_with("http://") || self.base_url.starts_with("https://")
    }
}

/// 对齐 WebDavException(message, status)。
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct WebDavError {
    pub message: String,
    pub status: Option<i32>,
}

impl WebDavError {
    fn new(message: impl Into<String>, status: Option<i32>) -> Self {
        WebDavError { message: message.into(), status }
    }
}

impl From<reqwest::Error> for WebDavError {
    fn from(err: reqwest::Error) -> Self {
        WebDavError::new(err.to_string(), None)
    }
}

/// 远端文件轻量元数据（Kotlin RemoteStat；etag 在同步里未使用，仅记录）。
#[derive(Debug, Clone)]
pub struct RemoteStat {
    pub last_modified: u64,
    pub size: i64,
    pub etag: Option<String>,
}

pub struct WebDavClient {
    config: WebDavConfig,
    client: reqwest::Client,
}

/// 复用连接池：自动拉取/同步每次都会新建 WebDavClient，若每次都新建 reqwest::Client
/// 就要重做 TCP+TLS 握手。按"服务器 + 账号 + 口令"缓存，凭据变化自然换新客户端。
fn client_cache() -> &'static Mutex<HashMap<String, reqwest::Client>> {
    static CACHE: OnceLock<Mutex<HashMap<String, reqwest::Client>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn client_cache_key(config: &WebDavConfig) -> String {
    let mut h = Sha256::new();
    h.update(config.normalized_base_url().as_bytes());
    h.update([0u8]);
    h.update(config.username.as_bytes());
    h.update([0u8]);
    h.update(config.password.as_bytes());
    crate::sp::crypto::to_hex(&h.finalize())
}

const PROPFIND_BODY: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><D:propfind xmlns:D=\"DAV:\"><D:prop><D:resourcetype/></D:prop></D:propfind>";

impl WebDavClient {
    pub fn new(config: WebDavConfig) -> Result<Self, WebDavError> {
        let cache_key = client_cache_key(&config);
        if let Some(client) = client_cache()
            .lock()
            .ok()
            .and_then(|m| m.get(&cache_key).cloned())
        {
            return Ok(WebDavClient { config, client });
        }
        // Basic 认证：全部请求默认带 Authorization（桌面端所有 URL 都由 base_url 派生，等价 Kotlin 的 authHost 守卫）
        let mut headers = reqwest::header::HeaderMap::new();
        if !config.username.is_empty() {
            let raw = format!("{}:{}", config.username, config.password);
            let encoded = BASE64_STANDARD.encode(raw.as_bytes());
            if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Basic {encoded}")) {
                headers.insert(reqwest::header::AUTHORIZATION, value);
            }
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60))
            .default_headers(headers)
            .build()?;
        if let Ok(mut cache) = client_cache().lock() {
            // 只保留最近几种凭据，防止长时间运行后缓存无限增长
            if cache.len() >= 8 {
                cache.clear();
            }
            cache.insert(cache_key, client.clone());
        }
        Ok(WebDavClient { config, client })
    }

    pub fn config(&self) -> &WebDavConfig {
        &self.config
    }

    fn url(&self, path: &str) -> String {
        let mut url = self.config.normalized_base_url();
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let encoded: Vec<String> = segments.iter().map(|s| percent_encode_path_segment(s)).collect();
        url.push_str(&encoded.join("/"));
        url
    }

    /// 连通性测试：PROPFIND Depth:0（2xx 即认为可用）。
    pub async fn test(&self) -> Result<(), WebDavError> {
        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"PROPFIND").unwrap(), self.url(""))
            .header("Depth", "0")
            .header("Content-Type", "application/xml")
            .body(PROPFIND_BODY)
            .send()
            .await?;
        let status = response.status().as_u16();
        match status {
            200..=299 => Ok(()),
            401 | 403 => Err(WebDavError::new("认证失败，请检查用户名和密码", Some(status as i32))),
            404 => Err(WebDavError::new("服务器地址或远程目录不存在", Some(404))),
            _ => {
                let body = body_take(response).await;
                Err(WebDavError::new(
                    format!("连接失败（HTTP {status}）：{body}"),
                    Some(status as i32),
                ))
            }
        }
    }

    /// 下载；不存在返回 None。409 先 HEAD 探测（坚果云对未创建文件回 409 而非 404）。
    pub async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, WebDavError> {
        let mut last_status: u16 = 0;
        for attempt in 0..2 {
            let response = self
                .client
                .get(self.url(path))
                .header("Accept", "*/*")
                .header("User-Agent", "SafePassword-Android/1.4")
                .send()
                .await?;
            last_status = response.status().as_u16();
            match last_status {
                404 => return Ok(None),
                409 => {
                    let exists = self.exists(path).await.unwrap_or(false);
                    if !exists {
                        return Ok(None);
                    }
                    return Err(WebDavError::new(
                        format!(
                            "下载失败（HTTP 409，路径 {path}）：远端路径存在冲突，通常是同名目录。请在 WebDAV 中删除该目录，或更换远程目录后重试"
                        ),
                        Some(409),
                    ));
                }
                200..=299 => {
                    let bytes = response.bytes().await?;
                    if bytes.is_empty() {
                        return Err(WebDavError::new(
                            format!("下载失败：服务器返回了空文件（{path}）"),
                            Some(last_status as i32),
                        ));
                    }
                    return Ok(Some(bytes.to_vec()));
                }
                500..=599 if attempt == 0 => {
                    tokio::time::sleep(Duration::from_millis(600)).await;
                    continue;
                }
                401 | 403 => {
                    return Err(WebDavError::new(
                        "WebDAV 认证失败，请检查用户名、密码或应用专用密码",
                        Some(last_status as i32),
                    ));
                }
                _ => {
                    let location = header_str(&response, "location");
                    let suffix = match location {
                        Some(loc) if !loc.trim().is_empty() => format!("，重定向到 {loc}"),
                        _ => String::new(),
                    };
                    let body = body_take(response).await;
                    return Err(WebDavError::new(
                        format!("下载失败（HTTP {last_status}{suffix}，路径 {path}）：{body}"),
                        Some(last_status as i32),
                    ));
                }
            }
        }
        Err(WebDavError::new(
            format!("下载失败（HTTP {last_status}，路径 {path}），服务器暂时不可用，请稍后重试"),
            Some(last_status as i32),
        ))
    }

    pub async fn exists(&self, path: &str) -> Result<bool, WebDavError> {
        let response = self.client.head(self.url(path)).send().await?;
        Ok(response.status().is_success())
    }

    /// 读取 Last-Modified / Content-Length / ETag（无则 0 / -1 / None）。
    pub async fn stat(&self, path: &str) -> Result<Option<RemoteStat>, WebDavError> {
        let response = self.client.head(self.url(path)).send().await?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let last_modified = header_str(&response, "last-modified")
            .and_then(|v| parse_http_date(&v))
            .unwrap_or(0);
        let size = header_str(&response, "content-length")
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(-1);
        let etag = header_str(&response, "etag");
        Ok(Some(RemoteStat { last_modified, size, etag }))
    }

    pub async fn put(&self, path: &str, bytes: &[u8]) -> Result<(), WebDavError> {
        self.ensure_parent(path).await?;
        let response = self
            .client
            .put(self.url(path))
            .header("Content-Type", "application/octet-stream")
            .body(bytes.to_vec())
            .send()
            .await?;
        let status = response.status().as_u16();
        if !(200..=299).contains(&status) {
            let body = body_take(response).await;
            return Err(WebDavError::new(
                format!("上传失败（HTTP {status}）：{body}"),
                Some(status as i32),
            ));
        }
        Ok(())
    }

    pub async fn delete(&self, path: &str) -> Result<(), WebDavError> {
        let response = self.client.delete(self.url(path)).send().await?;
        let status = response.status().as_u16();
        if !(200..=299).contains(&status) && status != 404 {
            return Err(WebDavError::new(
                format!("删除失败（HTTP {status}）"),
                Some(status as i32),
            ));
        }
        Ok(())
    }

    pub async fn ensure_parent(&self, path: &str) -> Result<(), WebDavError> {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() <= 1 {
            return Ok(());
        }
        let mut current = String::new();
        for segment in &segments[..segments.len() - 1] {
            if current.is_empty() {
                current = (*segment).to_string();
            } else {
                current.push('/');
                current.push_str(segment);
            }
            self.mkcol(&current).await?;
        }
        Ok(())
    }

    async fn mkcol(&self, path: &str) -> Result<(), WebDavError> {
        let response = self
            .client
            .request(reqwest::Method::from_bytes(b"MKCOL").unwrap(), self.url(path))
            .send()
            .await?;
        let status = response.status().as_u16();
        match status {
            200..=299 => Ok(()),
            405 => Ok(()),
            401 | 403 => Err(WebDavError::new("没有创建目录的权限", Some(status as i32))),
            _ => {
                let body = body_take(response).await;
                Err(WebDavError::new(
                    format!("创建目录失败（HTTP {status}）：{body}"),
                    Some(status as i32),
                ))
            }
        }
    }
}

async fn body_take(response: reqwest::Response) -> String {
    let text = response.text().await.unwrap_or_default();
    text.chars().take(120).collect()
}

fn header_str(response: &reqwest::Response, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn parse_http_date(value: &str) -> Option<u64> {
    // Kotlin: RFC_1123_DATE_TIME → 失败回退 Instant.parse（RFC 3339）
    chrono::DateTime::parse_from_rfc2822(value.trim())
        .or_else(|_| chrono::DateTime::parse_from_rfc3339(value.trim()))
        .ok()
        .map(|dt| dt.timestamp_millis() as u64)
}

/// 路径段百分号编码：保留子集字符与非 ASCII 全部 UTF-8 编码。
fn percent_encode_path_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.' | b'_' | b'~'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'*'
                    | b'+'
                    | b','
                    | b';'
                    | b'='
                    | b':'
                    | b'@'
                    | b'!'
                    | b'('
                    | b')'
            );
        if keep {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_name_rules() {
        let config = WebDavConfig::default();
        assert_eq!(config.legacy_name_path("My Vault"), "SafePassword/my-vault.spv");
        assert_eq!(config.legacy_name_path("  我的密码库  "), "SafePassword/我的密码库.spv");
        assert_eq!(config.legacy_name_path("Bank___Card"), "SafePassword/bank-card.spv");
        assert_eq!(config.legacy_name_path("---"), "SafePassword/vault.spv");
        assert_eq!(config.legacy_name_path(""), "SafePassword/vault.spv");
        // 64 字符截断（去掉 .spv 后缀后）
        let long = "a".repeat(100);
        assert_eq!(config.legacy_name_path(&long), format!("SafePassword/{}.spv", "a".repeat(64)));
        assert_eq!(config.legacy_name_path("a!!!b"), "SafePassword/a-b.spv");
    }

    #[test]
    fn remote_paths_follow_mobile_multi_vault_layout() {
        let config = WebDavConfig { remote_dir: "/SafePassword/".to_string(), ..Default::default() };
        assert_eq!(config.remote_path("vabc123"), "SafePassword/SafePassword-vabc123.spv");
        assert_eq!(config.legacy_remote_path(), "SafePassword/SafePassword.spv");
        assert_eq!(config.manifest_path(), "SafePassword/SafePassword-index.json");
    }

    #[test]
    fn normalized_urls() {
        let config = WebDavConfig {
            base_url: " https://dav.example.com/dav/ ".to_string(),
            remote_dir: "/SafePassword/".to_string(),
            ..Default::default()
        };
        assert_eq!(config.normalized_base_url(), "https://dav.example.com/dav/");
        assert_eq!(config.normalized_dir(), "SafePassword");
        // 多库布局：按库名哈希 key 分文件；旧路径仅作回退
        assert_eq!(config.remote_path("vwhatever"), "SafePassword/SafePassword-vwhatever.spv");
        assert_eq!(config.legacy_remote_path(), "SafePassword/SafePassword.spv");
        // isConfigured 与 Kotlin 一致：对原始 baseUrl 判断、不 trim——
        // 带前导空格的 " https://…" 在 Kotlin 里同样是 false，这里如实复现该怪癖
        assert!(!config.is_configured());
        let clean = WebDavConfig { base_url: "https://dav.example.com/dav".to_string(), ..Default::default() };
        assert!(clean.is_configured());
        assert!(!WebDavConfig::default().is_configured());

        let blank = WebDavConfig { remote_dir: "  /  ".to_string(), ..Default::default() };
        assert_eq!(blank.normalized_dir(), "SafePassword");
    }

    #[test]
    fn path_segment_encoding() {
        assert_eq!(percent_encode_path_segment("SafePassword.spv"), "SafePassword.spv");
        assert_eq!(percent_encode_path_segment("我的 密码"), "%E6%88%91%E7%9A%84%20%E5%AF%86%E7%A0%81");
        assert_eq!(percent_encode_path_segment("a/b?c#d"), "a%2Fb%3Fc%23d");
        assert_eq!(percent_encode_path_segment("sub dir"), "sub%20dir");
    }

    #[test]
    fn url_building() {
        let config = WebDavConfig {
            base_url: "https://dav.example.com/dav".to_string(),
            ..Default::default()
        };
        let client = WebDavClient::new(config).unwrap();
        assert_eq!(client.url(""), "https://dav.example.com/dav/");
        assert_eq!(
            client.url("SafePassword/SafePassword.spv"),
            "https://dav.example.com/dav/SafePassword/SafePassword.spv"
        );
        assert_eq!(
            client.url("/dir//with spaces/file"),
            "https://dav.example.com/dav/dir/with%20spaces/file"
        );
    }

    #[test]
    fn http_date_parsing() {
        assert_eq!(
            parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(1445412480000)
        );
        assert_eq!(
            parse_http_date("2015-10-21T07:28:00Z"),
            Some(1445412480000)
        );
        assert_eq!(parse_http_date("not a date"), None);
    }
}
