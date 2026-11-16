//! 对齐 Android HibpClient.kt：HIBP range k-匿名查询，密码本体不出本机。

use std::sync::OnceLock;
use std::time::Duration;

use sha1::{Digest, Sha1};

#[derive(Debug, thiserror::Error)]
pub enum HibpError {
    #[error("泄露库查询失败（HTTP {0}）")]
    RangeHttp(u16),
    #[error("网络错误：{0}")]
    Network(String),
}

pub struct HibpClient {
    client: reqwest::Client,
}

/// 全局共享实例：reqwest::Client 内部连接池可跨请求复用，避免每次查询重新握手。
static SHARED: OnceLock<HibpClient> = OnceLock::new();

pub fn shared() -> &'static HibpClient {
    SHARED.get_or_init(HibpClient::new)
}

impl Default for HibpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HibpClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        HibpClient { client }
    }

    /// k-anonymity 查询：只发送 SHA-1 前 5 位，密码本身不离开设备。
    pub async fn password_count(&self, password: &str) -> Result<i32, HibpError> {
        if password.trim().is_empty() {
            return Ok(0);
        }
        let hash = sha1_hex_upper(password.as_bytes());
        let prefix = &hash[..5];
        let suffix = &hash[5..];
        let url = format!("https://api.pwnedpasswords.com/range/{prefix}");
        let response = self
            .client
            .get(&url)
            .header("Add-Padding", "true")
            .send()
            .await
            .map_err(|e| HibpError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        if status != 200 {
            return Err(HibpError::RangeHttp(status));
        }
        let text = response
            .text()
            .await
            .map_err(|e| HibpError::Network(e.to_string()))?;
        Ok(parse_range_body(&text, suffix).unwrap_or(0))
    }
}

fn sha1_hex_upper(bytes: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    let mut out = String::with_capacity(40);
    for b in digest {
        out.push_str(&format!("{b:02X}"));
    }
    out
}

/// range 响应行 "SUFFIX:COUNT"；大小写不敏感匹配 suffix，取第一个命中。
fn parse_range_body(text: &str, suffix: &str) -> Option<i32> {
    for line in text.lines() {
        let line = line.trim();
        let mut parts = line.split(':');
        let head = parts.next()?;
        let tail = parts.next()?;
        if parts.next().is_some() {
            continue; // Kotlin: parts.size == 2 才认
        }
        if head.eq_ignore_ascii_case(suffix) {
            if let Ok(n) = tail.trim().parse::<i32>() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_known_vector() {
        // SHA-1("password") 上半段来自 HIBP 实际数据
        assert_eq!(sha1_hex_upper(b"password"), "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8");
    }

    #[test]
    fn range_body_parses_first_match() {
        let body = "0018A45D4D1A1D7D8C4E1B6E3A2A5F3C2E1B:7\r\n5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8:12345\r\nAAAAA:9";
        assert_eq!(parse_range_body(body, "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8"), Some(12345));
        // 大小写不敏感
        assert_eq!(parse_range_body(body, "5baa61e4c9b93f3f0682250b6cf8331b7ee68fd8"), Some(12345));
        // 无命中 → None（调用方回落 0）
        assert_eq!(parse_range_body(body, "ZZZZZ"), None);
        // 三段式坏行忽略
        assert_eq!(parse_range_body("AAAAA:1:extra", "AAAAA"), None);
    }

    #[tokio::test]
    async fn blank_input_short_circuits() {
        let client = HibpClient::new();
        assert_eq!(client.password_count("  ").await.unwrap(), 0);
    }

    /// 实网测试：验证真实 range API 可达（Add-Padding 大写 SHA-1 前缀语义）。
    /// 默认不跑（会出网）：`cargo test -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "network: 需要外网访问 api.pwnedpasswords.com"]
    async fn password_count_against_live_hibp() {
        let client = HibpClient::new();
        let n = client
            .password_count("password")
            .await
            .expect("live HIBP 应可达");
        assert!(n > 1_000_000, "'password' 应是高频泄露口令，实际 {n}");
    }
}
