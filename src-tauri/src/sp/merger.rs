//! 对齐 Android VaultMerger.kt：本地库 + 远端库的纯函数合并。
//! “删除不复活”规则集中在此，任何一端有比删除更新的修改才撤销墓碑。
#![allow(dead_code)]

use crate::sp::model::{now_ms, Label, Tombstone, Vault, VaultItem};

#[derive(Debug, Clone)]
pub struct MergeResult {
    pub vault: Vault,
    pub conflicts: usize,
}

pub fn merge(local: &Vault, remote: &Vault, last_synced_at: i64) -> MergeResult {
    // 墓碑取并集（LinkedHashMap 语义：先插入者占位，值保留更晚的删除时间）。
    let mut tombstones: Vec<Tombstone> = Vec::new();
    for t in local.tombstones.iter().chain(remote.tombstones.iter()) {
        match tombstones.iter_mut().find(|x| x.id == t.id) {
            Some(existing) => {
                if t.deleted_at > existing.deleted_at {
                    *existing = t.clone();
                }
            }
            None => tombstones.push(t.clone()),
        }
    }

    let mut merged: Vec<VaultItem> = Vec::new();
    let mut conflicts = 0usize;

    // 遍历顺序：本地顺序优先，再补远端新增（对应 localMap.keys + remoteMap.keys）。
    let mut order: Vec<&str> = Vec::new();
    for item in &local.items {
        order.push(item.id.as_str());
    }
    for item in &remote.items {
        if !local.items.iter().any(|l| l.id == item.id) {
            order.push(item.id.as_str());
        }
    }

    for id in order {
        let l = local.items.iter().find(|i| i.id == id);
        let r = remote.items.iter().find(|i| i.id == id);
        if let Some(tp) = tombstones.iter().position(|t| t.id == id) {
            let deleted_at = tombstones[tp].deleted_at;
            let local_gone = l.map_or(true, |li| deleted_at >= li.updated_at);
            let remote_gone = r.map_or(true, |ri| deleted_at >= ri.updated_at);
            if local_gone && remote_gone {
                continue;
            }
            tombstones.remove(tp);
        }
        match (l, r) {
            (None, Some(ri)) => merged.push(ri.clone()),
            (Some(li), None) => merged.push(li.clone()),
            (Some(li), Some(ri)) => {
                let local_new = li.updated_at > last_synced_at;
                let remote_new = ri.updated_at > last_synced_at;
                if local_new
                    && remote_new
                    && li.updated_at != ri.updated_at
                    && !same_content(li, ri)
                {
                    let (winner, loser) = if li.updated_at >= ri.updated_at {
                        (li, ri)
                    } else {
                        (ri, li)
                    };
                    merged.push(winner.clone());
                    merged.push(loser.copy_as_conflict_copy());
                    conflicts += 1;
                } else if ri.updated_at > li.updated_at {
                    merged.push(ri.clone());
                } else {
                    merged.push(li.clone());
                }
            }
            (None, None) => {}
        }
    }

    // 标签：本地顺序优先，同 id 本地值获胜。
    let mut labels: Vec<Label> = Vec::new();
    for l in &local.labels {
        match labels.iter_mut().find(|x| x.id == l.id) {
            Some(existing) => *existing = l.clone(),
            None => labels.push(l.clone()),
        }
    }
    for l in &remote.labels {
        if !labels.iter().any(|x| x.id == l.id) {
            labels.push(l.clone());
        }
    }

    let now = now_ms();
    let mut result = local.clone();
    result.items = merged;
    result.labels = labels;
    result.tombstones = tombstones;
    result.updated_at = now;
    result.last_synced_at = now;
    result.revision = local.revision.max(remote.revision) + 1;
    result.purge_old_tombstones(None);
    MergeResult {
        vault: result,
        conflicts,
    }
}

/// Kotlin sameContent：只比较这7项（favorite/notesLabel/notesType/notesPosition/iconKey/iconData 不参与）。
fn same_content(a: &VaultItem, b: &VaultItem) -> bool {
    a.title == b.title
        && a.fields == b.fields
        && a.notes == b.notes
        && a.url == b.url
        && a.label_ids == b.label_ids
        && a.status == b.status
        && a.attachments == b.attachments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sp::model::{FieldType, Field, ItemStatus};

    const SYNCED: i64 = 1_000_000;

    fn item(id: &str, title: &str, updated_at: i64) -> VaultItem {
        VaultItem {
            id: id.to_string(),
            title: title.to_string(),
            updated_at,
            ..Default::default()
        }
    }

    #[test]
    fn remote_wins_when_only_remote_changed() {
        let local = Vault::default();
        let mut remote = Vault::default();
        remote.items.push(item("a", "远端新版", SYNCED + 10));
        let result = merge(&local, &remote, SYNCED);
        assert_eq!(result.vault.items.len(), 1);
        assert_eq!(result.vault.items[0].title, "远端新版");
        assert_eq!(result.conflicts, 0);
    }

    #[test]
    fn local_wins_when_only_local_changed_or_equal() {
        let mut local = Vault::default();
        local.items.push(item("a", "本地", SYNCED + 10));
        let mut remote = Vault::default();
        remote.items.push(item("a", "远端", SYNCED + 5));
        let result = merge(&local, &remote, SYNCED);
        assert_eq!(result.vault.items[0].title, "本地");
        // 时间戳相等时取本地
        let mut local2 = Vault::default();
        local2.items.push(item("a", "本地", SYNCED + 10));
        let mut remote2 = Vault::default();
        remote2.items.push(item("a", "远端", SYNCED + 10));
        assert_eq!(merge(&local2, &remote2, SYNCED).vault.items[0].title, "本地");
    }

    #[test]
    fn both_edited_conflict_creates_copy() {
        let mut local = Vault::default();
        local.items.push(item("a", "本地编辑", SYNCED + 20));
        let mut remote = Vault::default();
        remote.items.push(item("a", "远端编辑", SYNCED + 10));
        let result = merge(&local, &remote, SYNCED);
        assert_eq!(result.conflicts, 1);
        assert_eq!(result.vault.items.len(), 2);
        // 胜者在前（本地更新）
        assert_eq!(result.vault.items[0].title, "本地编辑");
        // 败者成为冲突副本
        assert_eq!(result.vault.items[1].title, "远端编辑（冲突副本）");
        assert_ne!(result.vault.items[0].id, result.vault.items[1].id);
    }

    #[test]
    fn both_edited_same_content_no_conflict() {
        // 内容相同（只有 updatedAt 不同）→ 不产生冲突，取较新
        let mut local = Vault::default();
        local.items.push(item("a", "同内容", SYNCED + 20));
        let mut remote = Vault::default();
        remote.items.push(item("a", "同内容", SYNCED + 10));
        let result = merge(&local, &remote, SYNCED);
        assert_eq!(result.conflicts, 0);
        assert_eq!(result.vault.items.len(), 1);
        assert_eq!(result.vault.items[0].updated_at, SYNCED + 20);
    }

    #[test]
    fn tombstone_prevents_resurrection() {
        // 用新鲜时间戳：merge 会 purge 超过 TOMBSTONE_TTL_MS 的旧墓碑
        let synced = now_ms();
        let mut local = Vault::default();
        local.items.push(item("a", "旧标题", synced - 50));
        local.tombstones.push(Tombstone {
            id: "a".to_string(),
            deleted_at: synced - 10,
        });
        let mut remote = Vault::default();
        remote.items.push(item("a", "远端旧", synced - 40));
        remote.tombstones.push(Tombstone {
            id: "a".to_string(),
            deleted_at: synced - 10,
        });
        let result = merge(&local, &remote, synced);
        assert!(result.vault.items.is_empty());
        assert_eq!(result.vault.tombstones.len(), 1);
    }

    #[test]
    fn edit_after_delete_resurrects() {
        // 删除之后本地又编辑过 → 撤销墓碑
        let mut local = Vault::default();
        local.items.push(item("a", "复活了", SYNCED + 5));
        local.tombstones.push(Tombstone {
            id: "a".to_string(),
            deleted_at: SYNCED - 10,
        });
        let remote = Vault::default();
        let result = merge(&local, &remote, SYNCED);
        assert_eq!(result.vault.items.len(), 1);
        assert_eq!(result.vault.items[0].title, "复活了");
        assert!(result.vault.tombstones.is_empty());
    }

    #[test]
    fn labels_local_wins_revision_max_plus_one() {
        let mut local = Vault::default();
        local.labels.push(Label {
            id: "l1".to_string(),
            name: "本地标签".to_string(),
            ..Default::default()
        });
        local.revision = 7;
        let mut remote = Vault::default();
        remote.labels.push(Label {
            id: "l1".to_string(),
            name: "远端标签".to_string(),
            ..Default::default()
        });
        remote.labels.push(Label {
            id: "l2".to_string(),
            name: "远端独有".to_string(),
            ..Default::default()
        });
        remote.revision = 5;
        let result = merge(&local, &remote, SYNCED);
        assert_eq!(result.vault.revision, 8);
        assert_eq!(result.vault.labels.len(), 2);
        assert_eq!(result.vault.labels[0].name, "本地标签");
        assert_eq!(result.vault.labels[1].name, "远端独有");
    }

    #[test]
    fn remote_only_items_are_added_local_only_survive() {
        let mut local = Vault::default();
        local.items.push(item("l", "本地独有", SYNCED + 1));
        let mut remote = Vault::default();
        remote.items.push(item("r", "远端独有", SYNCED + 1));
        let result = merge(&local, &remote, SYNCED);
        assert_eq!(result.vault.items.len(), 2);
        // 顺序：本地在前
        assert_eq!(result.vault.items[0].id, "l");
        assert_eq!(result.vault.items[1].id, "r");
    }

    #[test]
    fn conflict_copy_fields_get_fresh_ids() {
        let mut loser = item("a", "标题", 1);
        loser.fields.push(Field {
            label: "密码".to_string(),
            value: "pw".to_string(),
            r#type: FieldType::Password,
            sensitive: true,
            ..Default::default()
        });
        let copy = loser.copy_as_conflict_copy();
        assert_ne!(copy.id, loser.id);
        assert_ne!(copy.fields[0].id, loser.fields[0].id);
        assert_eq!(copy.fields[0].value, "pw");
        assert!(copy.title.ends_with("（冲突副本）"));
        // Kotlin copyAsConflictCopy 强制 status = ACTIVE，即使原条目是 Archived
        let mut archived = item("b", "x", 1);
        archived.status = ItemStatus::Archived;
        assert_eq!(archived.copy_as_conflict_copy().status, ItemStatus::Active);
    }
}
