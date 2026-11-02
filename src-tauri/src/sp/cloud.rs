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
