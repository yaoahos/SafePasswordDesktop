//! 对齐 Android SyncEngine.kt（241 行）。
//!
//! 四分支同步：无远端→上传；仅远端新→下载；仅本地新→上传；双新→按冲突策略
//! （DETECT/KEEP_LOCAL/KEEP_REMOTE/MERGE）。contentKey 不同源直接停止。
//! 附件双向按 id 对账：本地缺→下载，远端缺→上传。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::sp::attachment::AttachmentStore;
use crate::sp::container::VaultContainer;
use crate::sp::merger::merge;
use crate::sp::model::{now_ms, Vault};
use crate::sp::webdav::{WebDavClient, WebDavConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SyncDirection {
    None,
    Uploaded,
    Downloaded,
    Merged,
    Conflict,
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
    pub attachments_uploaded: i32,
    pub attachments_downloaded: i32,
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
    attachment_store: AttachmentStore,
    remote_key: String,
}

impl SyncEngine {
    pub fn new(
        webdav: WebDavClient,
        config: WebDavConfig,
        attachment_store: AttachmentStore,
        vault_id: &str,
        vault_name: &str,
    ) -> Self {
        let name = if vault_name.trim().is_empty() { vault_id } else { vault_name };
        let remote_key = config.vault_key(name);
        SyncEngine { webdav, config, attachment_store, remote_key }
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
        let remote_bytes = self
            .webdav
            .get(&remote_path)
            .await
            .map_err(dav_err)?
            .ok_or_else(|| SyncError("云端没有找到密码库文件，无法拉取".to_string()))?;
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
        let (uploaded, downloaded_count) = self.sync_attachments(&downloaded).await?;
        Ok(SyncResult {
            container: local.with_vault(downloaded),
            direction: SyncDirection::Downloaded,
            conflicts: 0,
            attachments_uploaded: uploaded,
            attachments_downloaded: downloaded_count,
            message: "已用云端数据覆盖本地".to_string(),
        })
    }

    pub async fn sync(
        &self,
        mut local: VaultContainer,
        password: &str,
        conflict_strategy: ConflictStrategy,
    ) -> Result<SyncResult, SyncError> {
        let remote_path = self.config.remote_path(&self.remote_key);
        let remote_bytes = self.webdav.get(&remote_path).await.map_err(dav_err)?;

        let Some(remote_bytes) = remote_bytes else {
            local.ensure_content_key();
            local.vault.last_synced_at = now_ms();
            local.vault.touch();
            let bytes = local.to_bytes();
            self.webdav.put(&remote_path, &bytes).await.map_err(dav_err)?;
            let (uploaded, downloaded) = self.sync_attachments(&local.vault).await?;
            return Ok(SyncResult {
                container: local,
                direction: SyncDirection::Uploaded,
                conflicts: 0,
                attachments_uploaded: uploaded,
                attachments_downloaded: downloaded,
                message: "已上传到 WebDAV".to_string(),
            });
        };

        let remote = VaultContainer::open(&remote_bytes, password).map_err(|_| {
            SyncError("无法用当前主密码解密远端数据库，请确认两端使用同一主密码".to_string())
        })?;

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
            (false, false) => {
                let (uploaded, downloaded) = self.sync_attachments(&local.vault).await?;
                Ok(SyncResult {
                    container: local,
                    direction: SyncDirection::None,
                    conflicts: 0,
                    attachments_uploaded: uploaded,
                    attachments_downloaded: downloaded,
                    message: "已是最新状态".to_string(),
                })
            }
            (false, true) => {
                let mut downloaded_vault = remote.vault.clone();
                downloaded_vault.last_synced_at = now_ms();
                downloaded_vault.device_id = local.vault.device_id.clone();
                let (uploaded, downloaded) = self.sync_attachments(&downloaded_vault).await?;
                Ok(SyncResult {
                    container: local.with_vault(downloaded_vault),
                    direction: SyncDirection::Downloaded,
                    conflicts: 0,
                    attachments_uploaded: uploaded,
                    attachments_downloaded: downloaded,
                    message: "已下载远端更新".to_string(),
                })
            }
            (true, false) => {
                local.vault.last_synced_at = now_ms();
                let bytes = local.to_bytes();
                self.webdav.put(&remote_path, &bytes).await.map_err(dav_err)?;
                let (uploaded, downloaded) = self.sync_attachments(&local.vault).await?;
                Ok(SyncResult {
                    container: local,
                    direction: SyncDirection::Uploaded,
                    conflicts: 0,
                    attachments_uploaded: uploaded,
                    attachments_downloaded: downloaded,
                    message: "已上传本地更新".to_string(),
                })
            }
            (true, true) => match conflict_strategy {
                ConflictStrategy::Detect => Ok(SyncResult {
                    container: local,
                    direction: SyncDirection::Conflict,
                    conflicts: 1,
                    attachments_uploaded: 0,
                    attachments_downloaded: 0,
                    message: "检测到本地与云端都有新修改，请选择处理方式".to_string(),
                }),
                ConflictStrategy::KeepLocal => {
                    local.vault.last_synced_at = now_ms();
                    let bytes = local.to_bytes();
                    self.webdav.put(&remote_path, &bytes).await.map_err(dav_err)?;
                    let (uploaded, downloaded) = self.sync_attachments(&local.vault).await?;
                    Ok(SyncResult {
                        container: local,
                        direction: SyncDirection::Uploaded,
                        conflicts: 0,
                        attachments_uploaded: uploaded,
                        attachments_downloaded: downloaded,
                        message: "已保留本地版本并覆盖云端".to_string(),
                    })
                }
                ConflictStrategy::KeepRemote => {
                    let mut downloaded_vault = remote.vault.clone();
                    downloaded_vault.last_synced_at = now_ms();
                    downloaded_vault.device_id = local.vault.device_id.clone();
                    let (uploaded, downloaded) = self.sync_attachments(&downloaded_vault).await?;
                    Ok(SyncResult {
                        container: local.with_vault(downloaded_vault),
                        direction: SyncDirection::Downloaded,
                        conflicts: 0,
                        attachments_uploaded: uploaded,
                        attachments_downloaded: downloaded,
                        message: "已使用云端版本覆盖本地".to_string(),
                    })
                }
                ConflictStrategy::Merge => {
                    let merged = merge(&local.vault, &remote.vault, last);
                    let mut updated = local.with_vault(merged.vault.clone());
                    let bytes = updated.to_bytes();
                    self.webdav.put(&remote_path, &bytes).await.map_err(dav_err)?;
                    let (uploaded, downloaded) = self.sync_attachments(&merged.vault).await?;
                    let message = if merged.conflicts > 0 {
                        format!("已合并，产生 {} 个冲突副本", merged.conflicts)
                    } else {
                        "已合并本地与远端更新".to_string()
                    };
                    Ok(SyncResult {
                        container: updated,
                        direction: SyncDirection::Merged,
                        conflicts: merged.conflicts as i32,
                        attachments_uploaded: uploaded,
                        attachments_downloaded: downloaded,
                        message,
                    })
                }
            },
        }
    }

    /// 附件按 id 对账：本地缺→下载；远端缺→上传。返回 (uploaded, downloaded)。
    async fn sync_attachments(&self, vault: &Vault) -> Result<(i32, i32), SyncError> {
        let mut seen = std::collections::HashSet::new();
        let refs: Vec<&crate::sp::model::AttachmentRef> = vault
            .items
            .iter()
            .flat_map(|item| item.attachments.iter())
            .filter(|r| seen.insert(r.id.clone()))
            .collect();
        if refs.is_empty() {
            return Ok((0, 0));
        }
        let dir = self.config.attachments_dir(&self.remote_key);
        let mut uploaded = 0;
        let mut downloaded = 0;
        for r in refs {
            let remote_path = format!("{dir}/{}.sica", r.id);
            if !self.attachment_store.exists(&r.id) {
                let Some(remote_bytes) = self.webdav.get(&remote_path).await.map_err(dav_err)?
                else {
                    continue;
                };
                self.attachment_store
                    .write_raw(&r.id, &remote_bytes)
                    .map_err(SyncError)?;
                downloaded += 1;
            } else if !self.webdav.exists(&remote_path).await.map_err(dav_err)? {
                let Some(bytes) = self.attachment_store.read_raw(&r.id).map_err(SyncError)?
                else {
                    continue;
                };
                self.webdav.put(&remote_path, &bytes).await.map_err(dav_err)?;
                uploaded += 1;
            }
        }
        Ok((uploaded, downloaded))
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
}
