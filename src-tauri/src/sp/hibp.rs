//! 对齐 Android HibpClient.kt：HIBP range k-匿名查询 + 邮箱泄露查询（v3 API Key）。
#![allow(dead_code)]

use std::time::Duration;

use sha1::{Digest, Sha1};

#[derive(Debug, thiserror::Error)]
pub enum HibpError {
    #[error("泄露库查询失败（HTTP {0}）")]
    RangeHttp(u16),
    #[error("邮箱泄露检查需要填写 HIBP API Key")]
    MissingApiKey,
    #[error("无法解析 HIBP 响应")]
    Parse,
    #[error("HIBP API Key 无效")]
    InvalidKey,
    #[error("HIBP 请求过于频繁，请稍后再试")]
    RateLimited,
    #[error("邮箱查询失败（HTTP {0}）")]
    EmailHttp(u16),
    #[error("网络错误：{0}")]
    Network(String),
}

pub struct HibpClient {
    client: reqwest::Client,
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

    /// 邮箱泄露检查需要 HIBP API Key（官方付费接口）。
    pub async fn email_breaches(&self, email: &str, api_key: &str) -> Result<Vec<String>, HibpError> {
        if email.trim().is_empty() {
            return Ok(Vec::new());
        }
        if api_key.trim().is_empty() {
            return Err(HibpError::MissingApiKey);
        }
        let encoded = form_encode(email);
        let url = format!(
            "https://haveibeenpwned.com/api/v3/breachedaccount/{encoded}?truncateResponse=true"
        );
        let response = self
            .client
            .get(&url)
            .header("hibp-api-key", api_key)
            .header("user-agent", "SafeInCloudClient-Android")
            .send()
            .await
            .map_err(|e| HibpError::Network(e.to_string()))?;
        match response.status().as_u16() {
            200 => {
                let text = response
                    .text()
                    .await
                    .map_err(|e| HibpError::Network(e.to_string()))?;
                parse_breach_names(&text).ok_or(HibpError::Parse)
            }
            404 => Ok(Vec::new()),
            401 => Err(HibpError::InvalidKey),
            429 => Err(HibpError::RateLimited),
            other => Err(HibpError::EmailHttp(other)),
        }
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

fn parse_breach_names(text: &str) -> Option<Vec<String>> {
    let parsed: serde_json::Value = serde_json::from_str(text).ok()?;
    let arr = parsed.as_array()?;
    let mut names = Vec::new();
    for element in arr {
        if let Some(name) = element.get("Name").and_then(|v| v.as_str()) {
            names.push(name.to_string());
        }
    }
    Some(names)
}

/// 对齐 java.net.URLEncoder.encode(email, "UTF-8") 的表单编码：
/// 保留 [A-Za-z0-9.*_-]，空格 → '+'，其余取 UTF-8 字节 %XX（大写 hex）。
fn form_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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

    #[test]
    fn breach_names_parse() {
        let text = r#"[{"Name":"Adobe"},{"Name":"LinkedIn"},{"Name":"Adobe"}]"#;
        assert_eq!(
            parse_breach_names(text),
            Some(vec!["Adobe".to_string(), "LinkedIn".to_string(), "Adobe".to_string()])
        );
        assert_eq!(parse_breach_names("not-json"), None);
        assert_eq!(parse_breach_names(r#"{"Name":"x"}"#), None); // 非数组
    }

    #[test]
    fn form_encode_java_compatible() {
        assert_eq!(form_encode("user@example.com"), "user%40example.com");
        assert_eq!(form_encode("a b"), "a+b");
        assert_eq!(form_encode("café"), "caf%C3%A9");
        // Java URLEncoder 保留 . * _ -，其余都编码
        assert_eq!(form_encode("a.b*c_d-e"), "a.b*c_d-e");
        assert_eq!(form_encode("~"), "%7E");
    }

    #[tokio::test]
    async fn blank_inputs_short_circuit() {
        let client = HibpClient::new();
        assert_eq!(client.password_count("  ").await.unwrap(), 0);
        assert_eq!(client.email_breaches("", "key").await.unwrap(), Vec::<String>::new());
        // 空 key（非空邮箱）→ MissingApiKey，不发网络请求
        let err = client.email_breaches("a@b.c", "").await.unwrap_err();
        assert!(matches!(err, HibpError::MissingApiKey));
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
