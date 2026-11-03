//! 对齐 Android SyncEngine.kt（224 行，多密码库云端布局）。
//!
//! 每个密码库一个云端文件：`<remoteDir>/SafePassword-<key>.spv`，key 由库名算出
//! （CloudManifest.keyFor）。读取时依次回退旧布局（SafePassword.spv / 1.4.1 按库名键），
//! 但只有内容密钥一致（同一密码库）才认领，避免读到别的库。
//! 上传顺带维护 `<remoteDir>/SafePassword-index.json`（失败不影响同步）。
//! 四个分支：无远端→上传；仅远端新→下载；仅本地新→上传；双新→冲突策略。
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sp::cloud::{key_for, CloudManifest};
use crate::sp::container::VaultContainer;
use crate::sp::crypto::to_hex;
use crate::sp::merger::merge;
use crate::sp::model::{now_ms, Vault};
use crate::sp::webdav::{RemoteStat, WebDavClient, WebDavConfig};

/// 云端版本的指纹：HEAD 拿到的 ETag / Last-Modified / Content-Length。
///
/// 用途：同步前先发一个 HEAD（约 200 字节），和上次同步记下的指纹比对，
/// 一致就说明云端没变，可以直接跳过整包 GET（大库省下数 MB 下载）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RemoteRev {
    pub etag: String,
    pub last_modified: i64,
    pub size: i64,
}

impl RemoteRev {
    pub fn from_stat(stat: &RemoteStat) -> Self {
        RemoteRev {
            etag: stat.etag.clone().unwrap_or_default(),
            last_modified: stat.last_modified as i64,
            size: stat.size,
        }
    }

    /// 服务器给了 ETag 就只认 ETag（坚果云、Nextcloud 都稳定返回）；
    /// 没给才退回 (Last-Modified, Content-Length) 组合。
    pub fn matches(&self, stat: &RemoteStat) -> bool {
        let etag = stat.etag.clone().unwrap_or_default();
        if !self.etag.is_empty() && !etag.is_empty() {
            return self.etag == etag;
        }
        self.last_modified == stat.last_modified as i64 && self.size == stat.size
    }
}

/// 每个密码库在本地记的同步状态（按云端键存放，改名不影响）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncCacheEntry {
    /// 上次与云端一致时的版本指纹；`None` = 还没有可用指纹（下一次走完整流程）
    pub rev: Option<RemoteRev>,
    /// 上次成功同步时的内容指纹，用来抑制"内容没变却整包重传"
    pub content_hash: String,
}

/// 同步意义上的内容指纹：抹掉每次同步都会变、但用户不可见的字段。
///
/// `last_synced_at` / `device_id` / `revision` 变化不该算"内容变了"，
/// 否则每次同步都会重新上传一遍整个库。
pub fn content_hash(vault: &Vault) -> String {
    let mut copy = vault.clone();
    copy.last_synced_at = 0;
    copy.device_id.clear();
    copy.revision = 0;
    match serde_json::to_vec(&copy) {
        Ok(bytes) => to_hex(&Sha256::digest(&bytes)),
        Err(_) => String::new(),
    }
}

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
    /// 云端文件名里的 ASCII 键。**只由内容密钥（contentKey）决定**：
    /// 内容密钥是建库时随机生成的，跨设备拷贝、跨改名都不变 → 同一密码库永远同一个键。
    /// 只有当库里还没有内容密钥（极老的库）时才退回按库名派生。
    remote_key: String,
    /// 上次同步缓存（版本指纹 + 内容指纹），同步成功后由调用方持久化。
    cache: SyncCacheEntry,
}

impl SyncEngine {
    pub fn new(
        webdav: WebDavClient,
        config: WebDavConfig,
        vault_name: &str,
        content_key: &str,
        cache: SyncCacheEntry,
    ) -> Self {
        let remote_key = crate::sp::cloud::key_for_content(content_key)
            .unwrap_or_else(|| key_for(vault_name));
        SyncEngine { webdav, config, remote_key, cache }
    }

    pub fn remote_key(&self) -> &str {
        &self.remote_key
    }

    /// 同步结束后的缓存快照，调用方负责落盘。
    pub fn cache_entry(&self) -> SyncCacheEntry {
        self.cache.clone()
    }

    /// 强制用云端覆盖本地，无视时间戳（WebDAV 界面的"拉取云端数据"）。
    pub async fn pull_remote(
        &mut self,
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
        if remote_bytes.is_none() {
            // 旧版本按库名写的文件（内容密钥方案之前上传的），也要能拉下来
            remote_bytes = self
                .webdav
                .get(&self.config.remote_path(&key_for(&local.vault.name)))
                .await
                .map_err(dav_err)?;
        }
        if remote_bytes.is_none() {
            // 上一版用完整内容密钥生成的键（k + 64 hex）
            if let Some(legacy) =
                crate::sp::cloud::legacy_key_for_content(&local.vault.content_key)
            {
                remote_bytes = self
                    .webdav
                    .get(&self.config.remote_path(&legacy))
                    .await
                    .map_err(dav_err)?;
            }
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
        // 内容是刚下载的，指纹未知（可能是旧键/旧布局文件）→ 下次同步走完整流程重新取指纹
        self.cache.rev = None;
        self.cache.content_hash = content_hash(&downloaded);
        Ok(SyncResult {
            container: local.with_vault(downloaded),
            direction: SyncDirection::Downloaded,
            conflicts: 0,
            message: "已用云端数据覆盖本地".to_string(),
        })
    }

    pub async fn sync(
        &mut self,
        mut local: VaultContainer,
        password: &str,
        conflict_strategy: ConflictStrategy,
    ) -> Result<SyncResult, SyncError> {
        let mut remote_key = self.remote_key.clone();
        let mut remote_path = self.config.remote_path(&remote_key);

        // ---- 快速判断：只有之前成功同步过（缓存里有版本指纹）才用 HEAD ----
        // 没有指纹（首次同步 / 服务器不支持 HEAD / 刚手动覆盖过）→ 走完整流程，
        // 与旧版行为完全一致；这样绝不会把"HEAD 不支持"误判成"云端已删除"。
        let mut head: Option<RemoteStat> = None;
        if let Some(rev) = self.cache.rev.clone() {
            if let Ok(Some(stat)) = self.webdav.stat(&remote_path).await {
                if rev.matches(&stat) {
                    let local_changed = local.vault.updated_at > local.vault.last_synced_at;
                    if !local_changed {
                        // 云端没变、本地也没改：一个 HEAD 结束，正文 0 字节
                        return Ok(SyncResult {
                            container: local,
                            direction: SyncDirection::None,
                            conflicts: 0,
                            message: "已是最新状态".to_string(),
                        });
                    }
                    // 云端没变、本地有改动：跳过下载，直接上传
                    if !self.cache.content_hash.is_empty()
                        && content_hash(&local.vault) == self.cache.content_hash
                    {
                        local.vault.last_synced_at = local.vault.updated_at.max(now_ms());
                        return Ok(SyncResult {
                            container: local,
                            direction: SyncDirection::None,
                            conflicts: 0,
                            message: "内容未变化，已跳过上传".to_string(),
                        });
                    }
                    local.ensure_content_key();
                    local.vault.last_synced_at = local.vault.updated_at.max(now_ms());
                    let stable_key = self.remote_key.clone();
                    self.upload(&remote_path, &mut local, &stable_key).await?;
                    return Ok(SyncResult {
                        container: local,
                        direction: SyncDirection::Uploaded,
                        conflicts: 0,
                        message: "已上传本地更新".to_string(),
                    });
                }
                head = Some(stat);
            }
        }

        let mut remote_bytes = self.webdav.get(&remote_path).await.map_err(dav_err)?;
        let mut adopted_legacy: Option<VaultContainer> = None;
        if remote_bytes.is_none() {
            // 升级迁移：旧版文件只有内容密钥一致（同一密码库）才认领，
            // 否则视为云端还没有本库文件，直接上传本地，避免读到别的密码库的数据。
            // 依次尝试：旧版单文件 SafePassword.spv → 1.4.1 的按库名键 → 现在的按库名键（v 前缀）
            // → 上一版用完整内容密钥生成的 65 位键（k + 64 hex，旧桌面构建上传过这种文件）。
            let mut candidates = vec![
                self.config.legacy_remote_path(),
                self.config.legacy_name_path(&local.vault.name),
                self.config.remote_path(&key_for(&local.vault.name)),
            ];
            if let Some(legacy) =
                crate::sp::cloud::legacy_key_for_content(&local.vault.content_key)
            {
                candidates.push(self.config.remote_path(&legacy));
            }
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

        // 认领到的旧文件名（按库名派生的键）→ 上传成功后做迁移清理，避免云端同名两份
        let mut migrate_from: Option<String> = None;
        if let Some((key, name, updated)) = adopted {
            let stable_key = self.remote_key.clone();
            if local.vault.updated_at > updated {
                remote_key = stable_key.clone();
                remote_path = self.config.remote_path(&remote_key);
                if key != remote_key {
                    migrate_from = Some(key);
                }
            } else {
                let trimmed = name.trim().to_string();
                if !trimmed.is_empty() && trimmed != local.vault.name {
                    local.vault.name = trimmed;
                    local.vault.touch();
                }
                // 云端内容更晚：内容已按旧键读入 remote_bytes，这里把"写"的目标统一切到稳定键
                if !stable_key.is_empty() && key != stable_key {
                    migrate_from = Some(key);
                    remote_key = stable_key;
                } else {
                    remote_key = key;
                }
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
        // 两端时钟不可比：按 updated_at 猜「谁更新」，时钟偏差时会把「对端有改动」
        // 误判成「没改动」，甚至拿本地旧数据覆盖云端。因此默认一律合并，
        // 用内容哈希决定要不要回传；只有显式策略才按条件走。
        const SKEW_MS: i64 = 3_000;
        let local_changed = local.vault.updated_at > last + SKEW_MS;
        let remote_changed = remote.vault.updated_at > last + SKEW_MS;

        let result = match conflict_strategy {
            ConflictStrategy::Detect if local_changed && remote_changed => Ok(SyncResult {
                container: local,
                direction: SyncDirection::Conflict,
                conflicts: 1,
                message: "检测到本地与云端都有新修改，请选择处理方式".to_string(),
            }),
            ConflictStrategy::KeepLocal if local_changed => {
                local.vault.last_synced_at = local.vault.updated_at.max(now_ms());
                self.upload(&remote_path, &mut local, &remote_key).await?;
                Ok(SyncResult {
                    container: local,
                    direction: SyncDirection::Uploaded,
                    conflicts: 0,
                    message: "已保留本地版本并覆盖云端".to_string(),
                })
            }
            ConflictStrategy::KeepRemote if remote_changed => {
                let mut downloaded = remote.vault.clone();
                downloaded.last_synced_at = now_ms();
                downloaded.device_id = local.vault.device_id.clone();
                self.cache.rev = head.as_ref().map(RemoteRev::from_stat);
                self.cache.content_hash = content_hash(&downloaded);
                Ok(SyncResult {
                    container: local.with_vault(downloaded),
                    direction: SyncDirection::Downloaded,
                    conflicts: 0,
                    message: "已使用云端版本覆盖本地".to_string(),
                })
            }
            _ => {
                let merged = merge(&local.vault, &remote.vault, last);
                let mut updated = local.with_vault(merged.vault.clone());
                updated.vault.last_synced_at = updated.vault.updated_at.max(now_ms());
                // 合并结果与云端完全一致（通常只有对端改过）：不必回传整包。
                if content_hash(&updated.vault) == content_hash(&remote.vault) {
                    self.cache.rev = head.as_ref().map(RemoteRev::from_stat);
                    self.cache.content_hash = content_hash(&updated.vault);
                    return Ok(SyncResult {
                        container: updated,
                        direction: if remote_changed {
                            SyncDirection::Downloaded
                        } else {
                            SyncDirection::None
                        },
                        conflicts: merged.conflicts as i32,
                        message: if merged.conflicts > 0 {
                            format!("已合并，产生 {} 个冲突副本", merged.conflicts)
                        } else if remote_changed {
                            "已下载远端更新".to_string()
                        } else {
                            "已是最新状态".to_string()
                        },
                    });
                }
                self.upload(&remote_path, &mut updated, &remote_key).await?;
                let message = if merged.conflicts > 0 {
                    format!("已合并，产生 {} 个冲突副本", merged.conflicts)
                } else if remote_changed {
                    "已合并本地与远端更新".to_string()
                } else {
                    "已上传本地更新".to_string()
                };
                Ok(SyncResult {
                    container: updated,
                    direction: SyncDirection::Merged,
                    conflicts: merged.conflicts as i32,
                    message,
                })
            }
        }?;
        // 迁移清理：从"按库名派的旧键"迁到"按内容密钥派生的稳定键"后，
        // 删掉旧的云端文件并从清单移除旧条目，避免云端出现同名两份。
        if let Some(old_key) = migrate_from {
            if matches!(
                result.direction,
                SyncDirection::Uploaded | SyncDirection::Merged
            ) {
                let old_path = self.config.remote_path(&old_key);
                let _ = self.webdav.delete(&old_path).await;
                if let Ok(Some(bytes)) = self.webdav.get(&self.config.manifest_path()).await {
                    let text = String::from_utf8_lossy(&bytes).to_string();
                    let mut manifest = CloudManifest::parse(Some(text.as_str()));
                    let before = manifest.vaults.len();
                    manifest.vaults.retain(|e| e.key != old_key);
                    if manifest.vaults.len() != before {
                        let _ = self
                            .webdav
                            .put(&self.config.manifest_path(), manifest.to_json().as_bytes())
                            .await;
                    }
                }
            }
        }
        Ok(result)
    }

    /// 把本地库强制上传到云端（"云端已删除"弹窗里的"恢复到云端"）：
    /// 不看时间戳、不判断云端是否存在，直接写云端文件并维护清单。
    pub async fn restore_to_cloud(
        &mut self,
        mut local: VaultContainer,
    ) -> Result<SyncResult, SyncError> {
        let remote_path = self.config.remote_path(&self.remote_key);
        let stable_key = self.remote_key.clone();
        local.ensure_content_key();
        local.vault.touch();
        local.vault.last_synced_at = local.vault.updated_at;
        self.upload(&remote_path, &mut local, &stable_key).await?;
        Ok(SyncResult {
            container: local,
            direction: SyncDirection::Uploaded,
            conflicts: 0,
            message: "已恢复到云端".to_string(),
        })
    }

    /// 上传密码库本体，并顺带维护云端清单（清单失败不影响整次同步）。
    async fn upload(
        &mut self,
        remote_path: &str,
        container: &mut VaultContainer,
        remote_key: &str,
    ) -> Result<(), SyncError> {
        let bytes = container.to_bytes();
        self.webdav.put(remote_path, &bytes).await.map_err(dav_err)?;
        // 记下新的版本指纹：下一次同步用 HEAD 比对即可，不必再下载整包。
        // 服务器不支持 HEAD 时留空 → 下次退回完整流程，行为安全。
        self.cache.rev = self
            .webdav
            .stat(remote_path)
            .await
            .ok()
            .flatten()
            .map(|s| RemoteRev::from_stat(&s));
        self.cache.content_hash = content_hash(&container.vault);
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
    use crate::sp::model::VaultItem;

    /// 合并策略不看时间戳：只要远端有内容，两边的条目都必须留下。
    /// （sync() 不再用 updated_at 判「谁更新」，两台设备时钟不一致不会再漏同步。）
    #[test]
    fn merge_keeps_both_sides_regardless_of_timestamps() {
        let mut local = Vault::default();
        local.updated_at = 5_000;
        local.items.push(VaultItem {
            id: "local-only".to_string(),
            title: "本机新增".to_string(),
            ..Default::default()
        });

        let mut remote = Vault::default();
        // 远端这条的时间戳比本地 lastSyncedAt 还早——旧逻辑会当成「远端没变」
        remote.updated_at = 1_000;
        remote.items.push(VaultItem {
            id: "remote-only".to_string(),
            title: "对端新增".to_string(),
            ..Default::default()
        });

        let merged = merge(&local, &remote, 3_000);
        let ids: Vec<&str> = merged.vault.items.iter().map(|i| i.id.as_str()).collect();
        assert!(ids.contains(&"local-only"), "本机条目不能丢：{ids:?}");
        assert!(ids.contains(&"remote-only"), "对端条目必须合并进来：{ids:?}");
    }
    /// 云端版本指纹：有 ETag 就只比 ETag，没有才退回 (Last-Modified, Content-Length)。
    #[test]
    fn remote_rev_prefers_etag() {
        let stat = |mtime: u64, size: i64, etag: Option<&str>| crate::sp::webdav::RemoteStat {
            last_modified: mtime,
            size,
            etag: etag.map(|s| s.to_string()),
        };
        let rev = RemoteRev::from_stat(&stat(100, 42, Some("\"abc\"")));
        // ETag 相同：mtime/size 变了也算没变（某些服务器 mtime 精度只到秒）
        assert!(rev.matches(&stat(999, 1, Some("\"abc\""))));
        assert!(!rev.matches(&stat(100, 42, Some("\"def\""))));
        // 服务器不给 ETag：退回 (mtime, size)
        let no_etag = RemoteRev::from_stat(&stat(5, 7, None));
        assert!(no_etag.matches(&stat(5, 7, None)));
        assert!(!no_etag.matches(&stat(5, 8, None)));
    }

    /// 内容指纹必须忽略"每次同步都会变"的三个字段，否则每次都判定为有改动而整包重传。
    #[test]
    fn content_hash_ignores_sync_bookkeeping() {
        let mut vault = Vault::default();
        let base = content_hash(&vault);
        vault.last_synced_at += 1_000;
        vault.device_id = "另一台设备".to_string();
        vault.revision += 3;
        assert_eq!(content_hash(&vault), base, "同步簿记字段不应算内容变化");
        vault.name = "改过的名字".to_string();
        assert_ne!(content_hash(&vault), base, "真实内容变化必须改变指纹");
    }

    /// 云端文件版本戳落到缓存条目后能原样往返（sync-cache.json 的线上格式）。
    #[test]
    fn sync_cache_entry_roundtrip() {
        let entry = SyncCacheEntry {
            rev: Some(RemoteRev {
                etag: "\"x\"".to_string(),
                last_modified: 1_700_000_000,
                size: 8_870_000,
            }),
            content_hash: "AB12".to_string(),
        };
        let text = serde_json::to_string(&entry).unwrap();
        let back: SyncCacheEntry = serde_json::from_str(&text).unwrap();
        assert_eq!(back.rev, entry.rev);
        assert_eq!(back.content_hash, entry.content_hash);
        // 旧版本 / 缺字段时用默认值，不影响启动
        let empty: SyncCacheEntry = serde_json::from_str("{}").unwrap();
        assert!(empty.rev.is_none());
        assert!(empty.content_hash.is_empty());
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
        // 1.4.1 旧键：小写、非 ASCII 字母数字转 '-'，落在 <dir>/<key>.spv
        assert_eq!(config.legacy_name_path("My Vault"), "SafePassword/my-vault.spv");
        assert_eq!(config.legacy_name_path("!!!"), "SafePassword/vault.spv");
    }

    /// 云端文件名只由内容密钥派生：同一密码库改名后文件名不变（改名不会再触发"云端被删除"）。
    #[test]
    fn remote_key_follows_content_key_not_name() {
        use base64::Engine as _;
        let config = WebDavConfig::default();
        let content_key = base64::engine::general_purpose::STANDARD.encode([0x42u8; 32]);
        let key = crate::sp::cloud::key_for_content(&content_key).expect("content key");
        assert!(key.starts_with('k'));
        // k + 前 8 字节 hex（与手机端一致）
        assert_eq!(key.len(), 1 + 16);
        assert_eq!(
            crate::sp::cloud::legacy_key_for_content(&content_key).map(|k| k.len()),
            Some(1 + 64)
        );
        assert_eq!(
            config.remote_path(&key),
            format!("SafePassword/SafePassword-{key}.spv")
        );
        // 库名怎么改都不影响：键取自内容密钥，与名字无关
        for name in ["我的密码库", "改名后", "Renamed"] {
            let engine_key =
                crate::sp::cloud::key_for_content(&content_key).unwrap_or_else(|| key_for(name));
            assert_eq!(engine_key, key);
        }
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
