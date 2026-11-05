//! 对齐 Android CloudManifest.kt：云端密码库清单 + 库名 → ASCII 键。
//!
//! 云端文件名必须是纯 ASCII（有些网盘对中文文件名支持很差），所以文件名里不放库名；
//! 清单把「传输用的 key」映射回「人看的名字」，同时带更新时间和大小，供恢复时选择。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudVaultEntry {
    pub key: String,
    pub name: String,
    pub updated_at: i64,
    pub size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudManifest {
    pub vaults: Vec<CloudVaultEntry>,
}

impl CloudManifest {
    /// 同名键覆盖，新键追加（对齐 Kotlin upsert）。
    pub fn upsert(&mut self, key: &str, name: &str, updated_at: i64, size: i64) {
        let entry = CloudVaultEntry {
            key: key.to_string(),
            name: name.to_string(),
            updated_at,
            size,
        };
        match self.vaults.iter().position(|e| e.key == key) {
            Some(index) => self.vaults[index] = entry,
            None => self.vaults.push(entry),
        }
    }

    /// 移除指定条目（用于云端密码库删除 / 改名后清理旧键）。
    pub fn remove(&mut self, key: &str) {
        self.vaults.retain(|e| e.key != key);
    }

    /// 空/坏 JSON 一律回落到空清单（清单只服务「从云端恢复」的选择列表）。
    pub fn parse(text: Option<&str>) -> Self {
        match text {
            Some(t) if !t.trim().is_empty() => serde_json::from_str(t).unwrap_or_default(),
            _ => CloudManifest::default(),
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"vaults\":[]}".to_string())
    }
}

/// 密码库名 → 稳定 ASCII 键（对齐 CloudManifest.keyFor）：
/// 只由库名决定（trim 后 SHA-256，取前 8 字节 16 位小写 hex，前缀 "v"），
/// 跨设备稳定，且不含中文。
pub fn key_for(name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(name.trim().as_bytes());
    let digest = hasher.finalize();
    let mut key = String::with_capacity(17);
    key.push('v');
    for byte in digest.iter().take(8) {
        key.push_str(&format!("{byte:02x}"));
    }
    key
}

/// 密码库内容密钥 → 稳定 ASCII 键：**只由内容密钥决定，改名不变**。
/// contentKey 是建库时随机生成的 32 字节（Base64），跨设备拷贝、跨改名都保持不变，
/// 因此它就是"这个密码库"的身份。键形如 `k` + 64 位小写 hex；Base64 解析失败时退回哈希。
pub fn key_for_content(content_key: &str) -> Option<String> {
    use base64::Engine as _;
    let trimmed = content_key.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .ok()
        .or_else(|| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed).ok());
    match bytes {
        Some(bytes) if !bytes.is_empty() => {
            // 只取前 8 字节：键短、文件名短，64 位随机实际不会撞。与安卓端保持一致。
            let mut key = String::with_capacity(17);
            key.push('k');
            for byte in bytes.iter().take(8) {
                key.push_str(&format!("{byte:02x}"));
            }
            Some(key)
        }
        _ => {
            // 不是标准 Base64（历史数据）：退化为内容密钥的哈希，但仍与库名无关。
            let mut hasher = Sha256::new();
            hasher.update(trimmed.as_bytes());
            let digest = hasher.finalize();
            let mut key = String::from("k");
            for byte in digest.iter().take(8) {
                key.push_str(&format!("{byte:02x}"));
            }
            Some(key)
        }
    }
}

/// 上一版用**完整** contentKey 生成的键（`k` + 64 位 hex）。
/// 只用于升级时认领旧文件，认领后按新键重新上传（与手机端 `legacyKeyForContent` 对齐）。
pub fn legacy_key_for_content(content_key: &str) -> Option<String> {
    use base64::Engine as _;
    let trimmed = content_key.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .ok()
        .or_else(|| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed).ok())?;
    if bytes.is_empty() {
        return None;
    }
    let mut key = String::with_capacity(1 + 2 * bytes.len());
    key.push('k');
    for byte in bytes.iter() {
        key.push_str(&format!("{byte:02x}"));
    }
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_shape_and_stability() {
        let key = key_for("我的密码库");
        assert_eq!(key.len(), 17);
        assert!(key.starts_with('v'));
        assert!(key[1..].chars().all(|c| c.is_ascii_hexdigit()));
        // trim 后同名 → 同键
        assert_eq!(key_for(" 我的密码库 "), key_for("我的密码库"));
        assert_ne!(key_for("我的密码库"), key_for("工作库"));
    }

    #[test]
    fn content_key_is_name_independent() {
        use base64::Engine as _;
        let ck = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        let key = key_for_content(&ck).expect("有内容密钥就有稳定键");
        assert!(key.starts_with('k'));
        // `k` + 前 8 字节 hex（与手机端 keyForContent 一致，17 字符，文件名短）
        assert_eq!(key.len(), 1 + 16);
        assert_eq!(key, "k0707070707070707");
        // 同一个内容密钥 → 永远同一个键（与库名无关，改名不影响）
        assert_eq!(key_for_content(&ck), Some(key.clone()));
        // 旧版完整键：仅用于迁移认领
        assert_eq!(
            legacy_key_for_content(&ck).map(|k| k.len()),
            Some(1 + 64)
        );
        // 空/坏值
        assert!(key_for_content("").is_none());
        assert!(key_for_content("   ").is_none());
        assert!(key_for_content("不是一个 base64!!").is_some());
    }

    #[test]
    fn manifest_roundtrip_and_upsert() {
        let mut manifest = CloudManifest::default();
        manifest.upsert("vaaa", "我的密码库", 1_700_000_000_000, 4_261);
        manifest.upsert("vbbb", "工作库", 1_700_000_100_000, 440);
        manifest.upsert("vaaa", "我的密码库", 1_700_000_200_000, 4_300);
        assert_eq!(manifest.vaults.len(), 2);
        let text = manifest.to_json();
        let parsed = CloudManifest::parse(Some(&text));
        assert_eq!(parsed, manifest);
        assert_eq!(parsed.vaults[0].size, 4_300);
        // 坏 JSON → 空清单
        assert!(CloudManifest::parse(Some("oops")).vaults.is_empty());
        assert!(CloudManifest::parse(None).vaults.is_empty());
    }
}
