//! 对齐 Android SecurityAnalyzer.kt：弱/重用/泄露/过期四类风险统计。
#![allow(dead_code)]

use std::collections::HashMap;

use crate::sp::model::{ItemStatus, Vault};
use crate::sp::strength;

const OLD_THRESHOLD_MILLIS: i64 = 180 * 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecurityReport {
    pub total: i32,
    #[serde(default)]
    pub weak: Vec<crate::sp::model::VaultItem>,
    #[serde(default)]
    pub reused: Vec<crate::sp::model::VaultItem>,
    #[serde(default)]
    pub breached: Vec<crate::sp::model::VaultItem>,
    #[serde(default)]
    pub old: Vec<crate::sp::model::VaultItem>,
}

/// breachedCounts: itemId → 泄露命中次数（HIBP 语义，>0 即泄露）。
pub fn analyze(vault: &Vault, breached_counts: &HashMap<String, i64>) -> SecurityReport {
    let active: Vec<&crate::sp::model::VaultItem> = vault
        .items
        .iter()
        .filter(|it| it.status == ItemStatus::Active)
        .collect();

    // active 中有主密码的 (item, password)
    let with_password: Vec<(String, String)> = active
        .iter()
        .filter_map(|it| it.primary_password().map(|p| (it.id.clone(), p)))
        .collect();

    // weak：强度 score <= 1
    let weak: Vec<crate::sp::model::VaultItem> = with_password
        .iter()
        .filter(|(_, pw)| strength::evaluate(pw).score <= 1)
        .filter_map(|(id, _)| vault.item(id))
        .cloned()
        .collect();

    // reused：同一密码出现次数 > 1
    let mut counts: HashMap<&str, i64> = HashMap::new();
    for (_, pw) in &with_password {
        *counts.entry(pw.as_str()).or_insert(0) += 1;
    }
    let reused: Vec<crate::sp::model::VaultItem> = with_password
        .iter()
        .filter(|(_, pw)| counts.get(pw.as_str()).copied().unwrap_or(0) > 1)
        .filter_map(|(id, _)| vault.item(id))
        .cloned()
        .collect();

    // breached：HIBP 命中 > 0
    let breached: Vec<crate::sp::model::VaultItem> = active
        .iter()
        .filter(|it| breached_counts.get(&it.id).copied().unwrap_or(0) > 0)
        .cloned()
        .cloned()
        .collect();

    // old：updatedAt 距今超过 180 天
    let now = crate::sp::model::now_ms();
    let old: Vec<crate::sp::model::VaultItem> = active
        .iter()
        .filter(|it| now - it.updated_at > OLD_THRESHOLD_MILLIS)
        .cloned()
        .cloned()
        .collect();

    SecurityReport {
        total: active.len() as i32,
        weak,
        reused,
        breached,
        old,
    }
}

/// password → [itemId...]，仅 ACTIVE 且有主密码，保持条目顺序。
pub fn password_index(vault: &Vault) -> Vec<(String, Vec<String>)> {
    let mut result: Vec<(String, Vec<String>)> = Vec::new();
    for item in vault.items.iter().filter(|it| it.status == ItemStatus::Active) {
        let Some(password) = item.primary_password() else {
            continue;
        };
        match result.iter_mut().find(|(k, _)| *k == password) {
            Some((_, ids)) => ids.push(item.id.clone()),
            None => result.push((password, vec![item.id.clone()])),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sp::model::{now_ms, Field, FieldType, ItemStatus, VaultItem};

    fn pw_item(id: &str, password: &str, updated_at: i64) -> VaultItem {
        let mut item = VaultItem {
            id: id.to_string(),
            title: format!("item-{id}"),
            updated_at,
            ..Default::default()
        };
        item.fields.push(Field {
            label: "密码".to_string(),
            value: password.to_string(),
            r#type: FieldType::Password,
            sensitive: true,
            ..Default::default()
        });
        item
    }

    #[test]
    fn weak_reused_breached_old() {
        let now = now_ms();
        let mut vault = Vault::default();
        vault.items.push(pw_item("weak", "123456", now)); // 常见 → score 0
        vault.items.push(pw_item("dup1", "SamePass!9x", now));
        vault.items.push(pw_item("dup2", "SamePass!9x", now));
        vault.items.push(pw_item("breached", "Str0ng#Pass99", now));
        vault.items.push(pw_item(
            "old",
            "Another#Pass77",
            now - OLD_THRESHOLD_MILLIS - 1_000,
        ));
        // 非 ACTIVE 不参与
        let mut trashed = pw_item("trashed", "123456", now);
        trashed.status = ItemStatus::Trashed;
        vault.items.push(trashed);

        let mut breached = HashMap::new();
        breached.insert("breached".to_string(), 3_i64);

        let report = analyze(&vault, &breached);
        assert_eq!(report.total, 5);
        let ids = |list: &[VaultItem]| -> Vec<String> {
            list.iter().map(|i| i.id.clone()).collect()
        };
        assert_eq!(ids(&report.weak), vec!["weak".to_string()]);
        assert_eq!(ids(&report.reused), vec!["dup1".to_string(), "dup2".to_string()]);
        assert_eq!(ids(&report.breached), vec!["breached".to_string()]);
        assert_eq!(ids(&report.old), vec!["old".to_string()]);
    }

    #[test]
    fn password_index_groups_active_only() {
        let now = now_ms();
        let mut vault = Vault::default();
        vault.items.push(pw_item("a", "x1", now));
        vault.items.push(pw_item("b", "x1", now));
        vault.items.push(pw_item("c", "y2", now));
        let mut archived = pw_item("d", "x1", now);
        archived.status = ItemStatus::Archived;
        vault.items.push(archived);
        // 无密码的条目不出现
        vault.items.push(VaultItem {
            id: "nopw".to_string(),
            ..Default::default()
        });

        let idx = password_index(&vault);
        assert_eq!(idx.len(), 2);
        let (p1, ids1) = idx.iter().find(|(p, _)| p == "x1").unwrap();
        assert_eq!(p1, "x1");
        assert_eq!(*ids1, vec!["a".to_string(), "b".to_string()]);
        let (_, ids2) = idx.iter().find(|(p, _)| p == "y2").unwrap();
        assert_eq!(*ids2, vec!["c".to_string()]);
    }
}
