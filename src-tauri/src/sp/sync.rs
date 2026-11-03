//! 对齐 Android SyncEngine.kt（224 行，多密码库云端布局）。
//!
//! 每个密码库一个云端文件：`<remoteDir>/SafePassword-<key>.spv`，key 由库名算出
//! （CloudManifest.keyFor）。读取时依次回退旧布局（SafePassword.spv / 1.4.1 按库名键），
//! 但只有内容密钥一致（同一密码库）才认领，避免读到别的库。
//! 上传顺带维护 `<remoteDir>/SafePassword-index.json`（失败不影响同步）。
//! 四个分支：无远端→上传；仅远端新→下载；仅本地新→上传；双新→冲突策略。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::sp::cloud::{key_for, CloudManifest};
use crate::sp::container::VaultContainer;
use crate::sp::merger::merge;
use crate::sp::model::now_ms;
use crate::sp::webdav::{WebDavClient, WebDavConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SyncDirection {
    None,
    Uploaded,
    Downloaded,
    Merged,
    Conflict,
    /// 云端已删除该密码库（别处删除），调用方应删除本地库并回到解锁 / 初始界面
    RemoteDeleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConflictStrategy {
    /// 只检测双端变更，交给 UI 询问用户。
    Detect,
    /// 保留本地并覆盖云端。
    KeepLocal,
    /// 用云端覆盖本地。
    KeepRemote,
    /// 双端合并，产生冲突副本。
    Merge,
}

#[derive(Debug)]
pub struct SyncResult {
    /// 调用方必须持久化这个容器（下载/合并后可能是新实例）。
    pub container: VaultContainer,
    pub direction: SyncDirection,
    pub conflicts: i32,
    pub message: String,
}

/// 对齐 SyncException(message, cause)：cause 不进 message。
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SyncError(pub String);

fn dav_err(err: crate::sp::webdav::WebDavError) -> SyncError {
    SyncError(err.message)
}

pub struct SyncEngine {
    webdav: WebDavClient,
    config: WebDavConfig,
    /// 云端文件名里的 ASCII 键；只由库名决定，跨设备稳定，且不含中文。
    remote_key: String,
}

impl SyncEngine {
    pub fn new(webdav: WebDavClient, config: WebDavConfig, vault_name: &str) -> Self {
        let remote_key = key_for(vault_name);
        SyncEngine { webdav, config, remote_key }
    }

    pub fn remote_key(&self) -> &str {
        &self.remote_key
    }

    /// 强制用云端覆盖本地，无视时间戳（WebDAV 界面的"拉取云端数据"）。
    pub async fn pull_remote(
        &self,
        local: VaultContainer,
        password: &str,
    ) -> Result<SyncResult, SyncError> {
        let remote_path = self.config.remote_path(&self.remote_key);
        let mut remote_bytes = self.webdav.get(&remote_path).await.map_err(dav_err)?;
        if remote_bytes.is_none() {
            remote_bytes = self
                .webdav
                .get(&self.config.legacy_remote_path())
                .await
                .map_err(dav_err)?;
        }
        let remote_bytes = remote_bytes.ok_or_else(|| {
            SyncError("云端没有找到这个密码库的文件，请确认已同步过或检查远程目录".to_string())
        })?;
        let remote = VaultContainer::open(&remote_bytes, password).map_err(|_| {
            SyncError("无法用当前主密码解密远端数据库，请确认两端使用同一主密码".to_string())
        })?;
        if !local.vault.content_key.is_empty()
            && !remote.vault.content_key.is_empty()
            && local.vault.content_key != remote.vault.content_key
        {
            return Err(SyncError(
                "远端数据库与本地数据库不是同一来源（内容密钥不一致），已停止拉取".to_string(),
            ));
        }
        let mut downloaded = remote.vault.clone();
        downloaded.last_synced_at = now_ms();
        downloaded.device_id = local.vault.device_id.clone();
        Ok(SyncResult {
            container: local.with_vault(downloaded),
            direction: SyncDirection::Downloaded,
            conflicts: 0,
            message: "已用云端数据覆盖本地".to_string(),
        })
    }

    pub async fn sync(
        &self,
        mut local: VaultContainer,
        password: &str,
        conflict_strategy: ConflictStrategy,
    ) -> Result<SyncResult, SyncError> {
        let mut remote_key = self.remote_key.clone();
        let mut remote_path = self.config.remote_path(&remote_key);
        let mut remote_bytes = self.webdav.get(&remote_path).await.map_err(dav_err)?;
        let mut adopted_legacy: Option<VaultContainer> = None;
        if remote_bytes.is_none() {
            // 升级迁移：旧版文件只有内容密钥一致（同一密码库）才认领，
            // 否则视为云端还没有本库文件，直接上传本地，避免读到别的密码库的数据。
            // 依次尝试：旧版单文件 SafePassword.spv → 1.4.1 的按库名键。
            let candidates = [
                self.config.legacy_remote_path(),
                self.config.legacy_name_path(&local.vault.name),
            ];
            for candidate in candidates.iter().filter(|p| **p != remote_path) {
                let Some(bytes) = self.webdav.get(candidate).await.map_err(dav_err)? else {
                    continue;
                };
                if let Ok(container) = VaultContainer::open(&bytes, password) {
                    if !container.vault.content_key.is_empty()
                        && container.vault.content_key == local.vault.content_key
                    {
                        remote_bytes = Some(bytes);
                        adopted_legacy = Some(container);
                        break;
                    }
                }
            }
        }

        // 云端既没有本库文件，也没有可认领的旧文件：
        // - 若曾同步过（lastSyncedAt > 0），先尝试从云端清单认领改名后的新键；
        //   ① 同名条目；② 内容密钥相同（手机端改名后旧键被删，靠 contentKey 找回）
        // - 仍找不到 → 视为"云端已删除该库"，交给调用方弹窗询问（删除本地 / 恢复到云端）。
        // 认领到的云端条目：(键, 库名, 该文件更新时间)
        let mut adopted: Option<(String, String, i64)> = None;
        if remote_bytes.is_none() && local.vault.last_synced_at > 0 {
            let manifest_text = self
                .webdav
                .get(&self.config.manifest_path())
                .await
                .map_err(dav_err)?;
            let manifest = match manifest_text.as_ref() {
                Some(bytes) => CloudManifest::parse(Some(String::from_utf8_lossy(bytes).as_ref())),
                None => CloudManifest::parse(None),
            };
            // ① 云端清单里已有同名条目（正常同步/改名后两端都更新过的情况）
            let want = local.vault.name.trim().to_lowercase();
            if let Some(entry) = manifest
                .vaults
                .iter()
                .find(|e| e.name.trim().to_lowercase() == want)
            {
                if entry.key != remote_key {
                    let path = self.config.remote_path(&entry.key);
                    if let Some(bytes) = self.webdav.get(&path).await.map_err(dav_err)? {
                        remote_key = entry.key.clone();
                        remote_path = path;
                        remote_bytes = Some(bytes);
                        adopted = Some((entry.key.clone(), entry.name.clone(), entry.updated_at));
                    }
                }
            }
            // ② 改名认领：手机端改名后按新名字散列出新键、删掉旧键文件。
            //    内容密钥（contentKey）改名不变，用它逐条比对云端库文件即可认出"同一个库"。
            if remote_bytes.is_none() && !local.vault.content_key.is_empty() {
                for entry in manifest.vaults.iter() {
                    if entry.key == remote_key {
                        continue;
                    }
                    let path = self.config.remote_path(&entry.key);
                    let Ok(Some(bytes)) = self.webdav.get(&path).await else {
                        continue;
                    };
                    let Ok(container) = VaultContainer::open(&bytes, password) else {
                        continue;
                    };
                    if container.vault.content_key == local.vault.content_key {
                        remote_key = entry.key.clone();
                        remote_path = path;
                        remote_bytes = Some(bytes);
                        adopted = Some((entry.key.clone(), entry.name.clone(), entry.updated_at));
                        break;
                    }
                }
            }
            if remote_bytes.is_none() {
                // 本地若还有未同步修改，提示一并带出（删本地会丢数据）。
                let pending = local.vault.updated_at > local.vault.last_synced_at + 3_000;
                return Ok(SyncResult {
                    container: local,
                    direction: SyncDirection::RemoteDeleted,
                    conflicts: 0,
                    message: if pending {
                        "云端已删除该密码库（本地有未同步的修改）".to_string()
                    } else {
                        "云端已删除该密码库".to_string()
                    },
                });
            }
        }

        let Some(remote_bytes) = remote_bytes else {
            local.ensure_content_key();
            local.vault.touch();
            // 不变式：lastSyncedAt >= updatedAt（"本地是否有未同步修改"的判定依赖它）
            local.vault.last_synced_at = local.vault.updated_at;
            self.upload(&remote_path, &mut local, &remote_key).await?;
            return Ok(SyncResult {
                container: local,
                direction: SyncDirection::Uploaded,
                conflicts: 0,
                message: "已上传到 WebDAV".to_string(),
            });
        };

        let remote = match adopted_legacy {
            Some(container) => container,
            None => VaultContainer::open(&remote_bytes, password).map_err(|_| {
                SyncError("无法用当前主密码解密远端数据库，请确认两端使用同一主密码".to_string())
            })?,
        };

        // 认出云端里属于本库的文件（改名/迁移）后，决定用哪个名字：
        // - 本地更晚（本端刚改名）→ 保留本地名，按新名上传；旧键由调用方清理
        // - 云端更晚（其它设备改名）→ 本地库名跟随云端，并继续沿用云端那个键
        if let Some((key, name, updated)) = adopted {
            if local.vault.updated_at > updated {
                remote_key = self.remote_key.clone();
                remote_path = self.config.remote_path(&remote_key);
            } else {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && trimmed != local.vault.name {
                    local.vault.name = trimmed;
                    local.vault.touch();
                }
                remote_key = key.clone();
                remote_path = self.config.remote_path(&remote_key);
            }
        }

        if !local.vault.content_key.is_empty()
            && !remote.vault.content_key.is_empty()
            && local.vault.content_key != remote.vault.content_key
        {
            return Err(SyncError(
                "远端数据库与本地数据库不是同一来源（内容密钥不一致），已停止同步".to_string(),
            ));
        }

        let last = local.vault.last_synced_at;
        let local_changed = local.vault.updated_at > last;
        let remote_changed = remote.vault.updated_at > last;

        match (local_changed, remote_changed) {
            (false, false) => Ok(SyncResult {
                container: local,
                direction: SyncDirection::None,
                conflicts: 0,
                message: "已是最新状态".to_string(),
            }),
            (false, true) => {
                let mut downloaded = remote.vault.clone();
                downloaded.last_synced_at = now_ms();
                downloaded.device_id = local.vault.device_id.clone();
                Ok(SyncResult {
                    container: local.with_vault(downloaded),
                    direction: SyncDirection::Downloaded,
                    conflicts: 0,
                    message: "已下载远端更新".to_string(),
                })
            }
            (true, false) => {
                local.vault.last_synced_at = local.vault.updated_at.max(now_ms());
                self.upload(&remote_path, &mut local, &remote_key).await?;
                Ok(SyncResult {
                    container: local,
                    direction: SyncDirection::Uploaded,
                    conflicts: 0,
                    message: "已上传本地更新".to_string(),
                })
            }
            (true, true) => match conflict_strategy {
                ConflictStrategy::Detect => Ok(SyncResult {
                    container: local,
                    direction: SyncDirection::Conflict,
                    conflicts: 1,
                    message: "检测到本地与云端都有新修改，请选择处理方式".to_string(),
                }),
                ConflictStrategy::KeepLocal => {
                    local.vault.last_synced_at = local.vault.updated_at.max(now_ms());
                    self.upload(&remote_path, &mut local, &remote_key).await?;
                    Ok(SyncResult {
                        container: local,
                        direction: SyncDirection::Uploaded,
                        conflicts: 0,
                        message: "已保留本地版本并覆盖云端".to_string(),
                    })
                }
                ConflictStrategy::KeepRemote => {
                    let mut downloaded = remote.vault.clone();
                    downloaded.last_synced_at = now_ms();
                    downloaded.device_id = local.vault.device_id.clone();
                    Ok(SyncResult {
                        container: local.with_vault(downloaded),
                        direction: SyncDirection::Downloaded,
                        conflicts: 0,
                        message: "已使用云端版本覆盖本地".to_string(),
                    })
                }
                ConflictStrategy::Merge => {
                    let merged = merge(&local.vault, &remote.vault, last);
                    let mut updated = local.with_vault(merged.vault.clone());
                    updated.vault.last_synced_at = updated.vault.updated_at.max(now_ms());
                    self.upload(&remote_path, &mut updated, &remote_key).await?;
                    let message = if merged.conflicts > 0 {
                        format!("已合并，产生 {} 个冲突副本", merged.conflicts)
                    } else {
                        "已合并本地与远端更新".to_string()
                    };
                    Ok(SyncResult {
                        container: updated,
                        direction: SyncDirection::Merged,
                        conflicts: merged.conflicts as i32,
                        message,
                    })
                }
            },
        }
    }

    /// 把本地库强制上传到云端（"云端已删除"弹窗里的"恢复到云端"）：
    /// 不看时间戳、不判断云端是否存在，直接写云端文件并维护清单。
    pub async fn restore_to_cloud(
        &self,
        mut local: VaultContainer,
    ) -> Result<SyncResult, SyncError> {
        let remote_path = self.config.remote_path(&self.remote_key);
        local.ensure_content_key();
        local.vault.touch();
        local.vault.last_synced_at = local.vault.updated_at;
        self.upload(&remote_path, &mut local, &self.remote_key)
            .await?;
        Ok(SyncResult {
            container: local,
            direction: SyncDirection::Uploaded,
            conflicts: 0,
            message: "已恢复到云端".to_string(),
        })
    }

    /// 上传密码库本体，并顺带维护云端清单（清单失败不影响整次同步）。
    async fn upload(
        &self,
        remote_path: &str,
        container: &mut VaultContainer,
        remote_key: &str,
    ) -> Result<(), SyncError> {
        let bytes = container.to_bytes();
        self.webdav.put(remote_path, &bytes).await.map_err(dav_err)?;
        let manifest_text = self
            .webdav
            .get(&self.config.manifest_path())
            .await
            .ok()
            .flatten()
            .and_then(|b| String::from_utf8(b).ok());
        let mut manifest = CloudManifest::parse(manifest_text.as_deref());
        manifest.upsert(
            remote_key,            &container.vault.name,
            container.vault.updated_at,
            bytes.len() as i64,
        );
        let _ = self
            .webdav
            .put(&self.config.manifest_path(), manifest.to_json().as_bytes())
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 纯逻辑分支分类（与 sync() 的时间戳判断一致）：
    /// localChanged = local.updatedAt > lastSyncedAt；remoteChanged 同理。
    fn classify(local_updated: i64, remote_updated: i64, last: i64) -> (&'static str, &'static str) {
        let local_changed = local_updated > last;
        let remote_changed = remote_updated > last;
        match (local_changed, remote_changed) {
            (false, false) => ("NONE", "已是最新状态"),
            (false, true) => ("DOWNLOADED", "已下载远端更新"),
            (true, false) => ("UPLOADED", "已上传本地更新"),
            (true, true) => ("BOTH", "需要冲突策略"),
        }
    }

    #[test]
    fn four_branches() {
        let last = 1_000;
        assert_eq!(classify(1_000, 1_000, last), ("NONE", "已是最新状态"));
        assert_eq!(classify(999, 999, last), ("NONE", "已是最新状态"));
        assert_eq!(classify(1_000, 1_001, last), ("DOWNLOADED", "已下载远端更新"));
        assert_eq!(classify(999, 1_001, last), ("DOWNLOADED", "已下载远端更新"));
        assert_eq!(classify(1_001, 1_000, last), ("UPLOADED", "已上传本地更新"));
        assert_eq!(classify(1_001, 999, last), ("UPLOADED", "已上传本地更新"));
        assert_eq!(classify(1_001, 1_002, last), ("BOTH", "需要冲突策略"));
        // 边界：等于 lastSyncedAt 算未变（严格大于）
        assert_eq!(classify(1_000, 1_001, last), ("DOWNLOADED", "已下载远端更新"));
    }

    #[test]
    fn conflict_strategy_serializes_uppercase() {
        let json = serde_json::to_string(&ConflictStrategy::KeepRemote).unwrap();
        assert_eq!(json, "\"KEEP_REMOTE\"");
        let dir = serde_json::to_string(&SyncDirection::Merged).unwrap();
        assert_eq!(dir, "\"MERGED\"");
    }

    #[test]
    fn remote_keys_match_mobile_layout() {
        let config = WebDavConfig::default();
        assert_eq!(key_for("我的密码库").len(), 17);
        assert_eq!(
            config.remote_path(&key_for("我的密码库")),
            format!("SafePassword/SafePassword-{}.spv", key_for("我的密码库"))
        );
        assert_eq!(config.legacy_remote_path(), "SafePassword/SafePassword.spv");
        assert_eq!(
            config.manifest_path(),
            "SafePassword/SafePassword-index.json"
        );
        // 1.4.1 旧键：小写、非 ASCII 字母数字转 '-'，落在 <dir>/<key>.sicv
        assert_eq!(config.legacy_name_path("My Vault"), "SafePassword/my-vault.sicv");
        assert_eq!(config.legacy_name_path("!!!"), "SafePassword/vault.sicv");
    }

    /// 前端按字符串判断同步方向：必须是 SCREAMING_SNAKE_CASE（曾经 "UPPERCASE"
    /// 把 RemoteDeleted 序列化成 "REMOTEDELETED"，导致云端删除弹窗永远不触发）。
    #[test]
    fn sync_direction_wire_names() {
        use serde_json::json;
        assert_eq!(serde_json::to_value(SyncDirection::None).unwrap(), json!("NONE"));
        assert_eq!(
            serde_json::to_value(SyncDirection::Uploaded).unwrap(),
            json!("UPLOADED")
        );
        assert_eq!(
            serde_json::to_value(SyncDirection::Downloaded).unwrap(),
            json!("DOWNLOADED")
        );
        assert_eq!(
            serde_json::to_value(SyncDirection::Merged).unwrap(),
            json!("MERGED")
        );
        assert_eq!(
            serde_json::to_value(SyncDirection::Conflict).unwrap(),
            json!("CONFLICT")
        );
        assert_eq!(
            serde_json::to_value(SyncDirection::RemoteDeleted).unwrap(),
            json!("REMOTE_DELETED")
        );
    }
}
