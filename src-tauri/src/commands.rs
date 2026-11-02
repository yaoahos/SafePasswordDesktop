//! Tauri 命令层：React UI 通过 invoke 调用。
//!
//! 状态模型：一个全局 AppState（Mutex 保护）。
//! - current 为 Some 表示密码库已解锁；主密码只保存在内存里，进程退出即消失。
//! - 每次修改后立即落盘（对齐 Android 端"改完即存"的语义）。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::sp::attachment::AttachmentStore;
use crate::sp::backup::{BackupInfo, BackupManager};
use crate::sp::codec;
use crate::sp::container::VaultContainer;
use crate::sp::fsutil::write_atomic;
use crate::sp::generator::{self, GeneratorOptions};
use crate::sp::hibp::HibpClient;
use crate::sp::model::{new_id, now_ms, Label, Vault, VaultItem, VaultMeta};
use crate::sp::safeincloud_xml;
use crate::sp::security::{self, SecurityReport};
use crate::sp::store::VaultStore;
use crate::sp::strength::{self, StrengthResult};
use crate::sp::sync::{ConflictStrategy, SyncEngine, SyncResult};
use crate::sp::webdav::{WebDavClient, WebDavConfig};

type CmdResult<T> = Result<T, String>;

const LOCK_BROKEN: &str = "内部状态锁已损坏";
const NO_OPEN_VAULT: &str = "请先解锁一个密码库";

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------------------------------------------------------------- 状态

pub struct CurrentVault {
    pub container: VaultContainer,
    pub meta: VaultMeta,
    pub password: String,
}

pub struct AppStateInner {
    pub data_root: PathBuf,
    pub store: VaultStore,
    pub current: Option<CurrentVault>,
    pub settings: AppSettings,
    pub settings_path: PathBuf,
    /// 自动拉取：上次远端 Last-Modified 缓存（对齐 Android lastRemoteMtime）
    pub last_remote_mtime: i64,
    /// 自动拉取：上次检查时间（对齐 Android lastPullCheckAt，防坚果云限流）
    pub last_pull_check_at: i64,
}

pub struct AppState {
    pub inner: Mutex<AppStateInner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// 0 = 不自动锁定
    pub auto_lock_minutes: i32,
    pub hibp_api_key: String,
    pub webdav: WebDavConfig,
    /// 0 = 关闭自动备份，否则每 N 天（对齐手机端 autoBackupDays，默认 1）
    pub auto_backup_days: i32,
    /// 上次备份时间（epoch millis，对齐手机端 lastBackupAt）
    pub last_backup_at: i64,
    /// 保存后自动同步（对齐手机端 autoSync，默认开）
    pub auto_sync: bool,
    /// 自动拉取云端更新间隔秒数（0 = 关闭，对齐手机端 pullIntervalSeconds）
    pub pull_interval_seconds: i32,
    /// 搜索是否匹配密码/机密字段（对齐手机端 searchPasswords）
    pub search_passwords: bool,
    /// 默认展示的标签（对齐手机端 defaultLabelId，null = 全部条目）
    pub default_label_id: Option<String>,
    /// 最近打开的密码库（对齐手机端 lastVaultId）
    pub last_vault_id: Option<String>,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            theme: "system".to_string(),
            auto_lock_minutes: 5,
            hibp_api_key: String::new(),
            webdav: WebDavConfig::default(),
            auto_backup_days: 1,
            last_backup_at: 0,
            auto_sync: true,
            pull_interval_seconds: 60,
            search_passwords: false,
            default_label_id: None,
            last_vault_id: None,
        }
    }
}

impl AppState {
    pub fn new(data_root: &PathBuf) -> CmdResult<Self> {
        fs::create_dir_all(data_root).map_err(err)?;
        let store = VaultStore::new(data_root).map_err(err)?;
        let settings_path = data_root.join("settings.json");
        let settings = match fs::read_to_string(&settings_path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => AppSettings::default(),
        };
        Ok(AppState {
            inner: Mutex::new(AppStateInner {
                data_root: data_root.clone(),
                store,
                current: None,
                settings,
                settings_path,
                last_remote_mtime: 0,
                last_pull_check_at: 0,
            }),
        })
    }
}

impl AppStateInner {
    fn persist_settings(&self) -> CmdResult<()> {
        let text = serde_json::to_string_pretty(&self.settings).map_err(err)?;
        write_atomic(&self.settings_path, text.as_bytes()).map_err(err)
    }

    fn backups_dir(&self) -> PathBuf {
        self.data_root.join("backups")
    }

    /// 改动后统一落盘（自动保存语义）。
    fn save_current(&mut self) -> CmdResult<()> {
        let cur = self.current.as_mut().ok_or(NO_OPEN_VAULT)?;
        self.store.save(&mut cur.container, &cur.meta).map_err(err)
    }

    /// 记录最近打开的密码库（对齐手机端 lastVaultId；写盘失败不致命）。
    fn mark_last_vault(&mut self, id: &str) {
        if self.settings.last_vault_id.as_deref() == Some(id) {
            return;
        }
        self.settings.last_vault_id = Some(id.to_string());
        let _ = self.persist_settings();
    }

    /// 对齐手机端 AppViewModel.maybeAutoBackup：创建/解锁后检查间隔，超期则
    /// 备份 + 保留 10 份 + 记录 lastBackupAt；失败静默（Kotlin runCatching）。
    fn maybe_auto_backup(&mut self) {
        let days = self.settings.auto_backup_days;
        if days <= 0 {
            return;
        }
        let now = now_ms();
        if now - self.settings.last_backup_at < i64::from(days) * 86_400_000 {
            return;
        }
        let (file, name) = match self.current.as_ref() {
            Some(cur) => (self.store.vault_file(&cur.meta), cur.container.vault.name.clone()),
            None => return,
        };
        let mgr = match BackupManager::new(&self.backups_dir()) {
            Ok(m) => m,
            Err(_) => return,
        };
        if mgr.backup(&file, &name).is_err() {
            return;
        }
        let _ = mgr.prune(10);
        self.settings.last_backup_at = now;
        let _ = self.persist_settings();
    }
}

// ---------------------------------------------------------------- 保险库管理

#[tauri::command]
pub fn list_vaults(state: State<AppState>) -> CmdResult<Vec<VaultMeta>> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    inner.store.list().map_err(err)
}

#[tauri::command]
pub fn create_vault(state: State<AppState>, name: String, password: String) -> CmdResult<VaultMeta> {
    if password.is_empty() {
        return Err("主密码不能为空".to_string());
    }
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let (container, meta) = inner.store.create(&name, &password).map_err(err)?;
    inner.current = Some(CurrentVault {
        container,
        meta: meta.clone(),
        password,
    });
    inner.mark_last_vault(&meta.id);
    inner.maybe_auto_backup();
    Ok(meta)
}

#[tauri::command]
pub fn open_vault(state: State<AppState>, id: String, password: String) -> CmdResult<Vault> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let meta = inner
        .store
        .list()
        .map_err(err)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or("找不到该密码库")?;
    let container = inner.store.open(&meta, &password).map_err(err)?;
    let vault = container.vault.clone();
    let meta_id = meta.id.clone();
    inner.current = Some(CurrentVault {
        container,
        meta,
        password,
    });
    inner.mark_last_vault(&meta_id);
    inner.maybe_auto_backup();
    Ok(vault)
}

#[tauri::command]
pub fn close_vault(state: State<AppState>) -> CmdResult<()> {
    if let Ok(mut inner) = state.inner.lock() {
        inner.current = None;
    }
    Ok(())
}

#[tauri::command]
pub fn current_vault(state: State<AppState>) -> CmdResult<Option<Vault>> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    Ok(inner.current.as_ref().map(|c| c.container.vault.clone()))
}

#[tauri::command]
pub fn delete_vault(state: State<AppState>, id: String) -> CmdResult<()> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    if inner.current.as_ref().is_some_and(|c| c.meta.id == id) {
        inner.current = None;
    }
    let meta = inner
        .store
        .list()
        .map_err(err)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or("找不到该密码库")?;
    inner.store.delete(&meta).map_err(err)
}

#[tauri::command]
pub fn rename_vault(state: State<AppState>, id: String, name: String) -> CmdResult<()> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    if inner.current.as_ref().is_some_and(|c| c.meta.id == id) {
        let cur = inner.current.as_mut().unwrap();
        cur.container.vault.name = trimmed;
        cur.meta.name = cur.container.vault.name.clone();
        cur.container.vault.touch();
        return inner.save_current();
    }
    let mut meta = inner
        .store
        .list()
        .map_err(err)?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or("找不到该密码库")?;
    meta.name = trimmed;
    inner.store.update_meta(&meta).map_err(err)
}

/// UI 整体编辑后回写（替换当前 vault 并 touch + 落盘）。
#[tauri::command]
pub fn update_vault(state: State<AppState>, vault: Vault) -> CmdResult<()> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    cur.container.vault = vault;
    cur.container.vault.touch();
    inner.save_current()
}

#[tauri::command]
pub fn put_item(state: State<AppState>, item: VaultItem) -> CmdResult<Vault> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    if let Some(existing) = cur.container.vault.items.iter_mut().find(|i| i.id == item.id) {
        *existing = item;
    } else {
        cur.container.vault.items.push(item);
    }
    cur.container.vault.touch();
    inner.save_current()?;
    Ok(inner.current.as_ref().unwrap().container.vault.clone())
}

#[tauri::command]
pub fn delete_item(state: State<AppState>, id: String) -> CmdResult<Vault> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    cur.container.vault.purge_item(&id, None);
    cur.container.vault.touch();
    inner.save_current()?;
    Ok(inner.current.as_ref().unwrap().container.vault.clone())
}

#[tauri::command]
pub fn change_master_password(
    state: State<AppState>,
    old_password: String,
    new_password: String,
) -> CmdResult<()> {
    if new_password.is_empty() {
        return Err("新主密码不能为空".to_string());
    }
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    if !cur.container.password_correct(&old_password) {
        return Err("主密码错误".to_string());
    }
    let changed = cur.container.change_password(&new_password);
    cur.container = changed;
    cur.password = new_password;
    inner.save_current()
}

#[tauri::command]
pub fn import_vault_bytes(
    state: State<AppState>,
    base64_data: String,
    password: String,
    name: String,
) -> CmdResult<VaultMeta> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.trim())
        .map_err(|_| "文件内容解码失败（应为 Base64）".to_string())?;
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let (container, meta) = inner
        .store
        .import_encrypted(&bytes, &password, &name)
        .map_err(err)?;
    inner.current = Some(CurrentVault {
        container,
        meta: meta.clone(),
        password,
    });
    inner.mark_last_vault(&meta.id);
    Ok(meta)
}

#[tauri::command]
pub fn import_vault_json(
    state: State<AppState>,
    json: String,
    password: String,
    name: String,
) -> CmdResult<VaultMeta> {
    let vault = codec::from_json(&json).map_err(err)?;
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let (container, meta) = inner
        .store
        .import_plain(vault, &password, &name)
        .map_err(err)?;
    inner.current = Some(CurrentVault {
        container,
        meta: meta.clone(),
        password,
    });
    inner.mark_last_vault(&meta.id);
    Ok(meta)
}

// ---------------------------------------------------------------- 生成器 / 强度 / 安全检查

#[tauri::command]
pub fn generate_password(options: Option<GeneratorOptions>) -> String {
    generator::generate(&options.unwrap_or_default())
}

#[tauri::command]
pub fn generate_passphrase(
    word_count: Option<i32>,
    separator: Option<String>,
    capitalize: Option<bool>,
    include_number: Option<bool>,
) -> String {
    generator::passphrase(
        word_count.unwrap_or(4),
        separator.as_deref().unwrap_or("-"),
        capitalize.unwrap_or(false),
        include_number.unwrap_or(false),
    )
}

#[tauri::command]
pub fn evaluate_strength(password: String) -> StrengthResult {
    strength::evaluate(&password)
}

#[tauri::command]
pub fn security_report(state: State<AppState>) -> CmdResult<SecurityReport> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    Ok(security::analyze(&cur.container.vault, &HashMap::new()))
}

/// 带 HIBP 泄露检查的完整安全报告（k-匿名：密码本身不出本机）。
#[tauri::command]
pub async fn security_report_full(state: State<'_, AppState>) -> CmdResult<SecurityReport> {
    let vault = {
        let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        inner
            .current
            .as_ref()
            .ok_or(NO_OPEN_VAULT)?
            .container
            .vault
            .clone()
    };
    let index = security::password_index(&vault);
    let hibp = HibpClient::new();
    let mut counts: HashMap<String, i64> = HashMap::new();
    for (password, ids) in index {
        if let Ok(n) = hibp.password_count(&password).await {
            let n = n as i64;
            for id in ids {
                counts.insert(id, n);
            }
        }
    }
    Ok(security::analyze(&vault, &counts))
}

#[tauri::command]
pub async fn check_password_breach(password: String) -> CmdResult<i32> {
    let hibp = HibpClient::new();
    hibp.password_count(&password).await.map_err(err)
}

#[tauri::command]
pub async fn check_email_breaches(state: State<'_, AppState>, email: String) -> CmdResult<Vec<String>> {
    let key = {
        let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        inner.settings.hibp_api_key.clone()
    };
    let hibp = HibpClient::new();
    hibp.email_breaches(&email, &key).await.map_err(err)
}

// ---------------------------------------------------------------- 附件

/// 当前库的附件仓库（内容密钥从已解锁的容器里来）。
fn attachment_store_for(inner: &AppStateInner) -> CmdResult<AttachmentStore> {
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    let key = base64::engine::general_purpose::STANDARD
        .decode(cur.container.vault.content_key.trim())
        .unwrap_or_default();
    let root = inner.store.attachments_dir_for(&cur.meta.id);
    AttachmentStore::new(&root, key).map_err(err)
}

/// 往条目里添加附件（bytes 已解密明文，落盘时用内容密钥加密）。返回更新后的库。
#[tauri::command]
pub fn attachment_add(
    state: State<AppState>,
    item_id: String,
    name: String,
    mime: String,
    data: String,
) -> CmdResult<Vault> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.trim())
        .map_err(|_| "附件内容解码失败（应为 Base64）".to_string())?;
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let (root, key) = {
        let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
        let key = base64::engine::general_purpose::STANDARD
            .decode(cur.container.vault.content_key.trim())
            .unwrap_or_default();
        (inner.store.attachments_dir_for(&cur.meta.id), key)
    };
    let store = AttachmentStore::new(&root, key).map_err(err)?;
    let r#ref = store.add(&bytes, &name, &mime).map_err(err)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    let item = cur
        .container
        .vault
        .items
        .iter_mut()
        .find(|i| i.id == item_id)
        .ok_or("找不到该条目")?;
    item.attachments.push(r#ref);
    cur.container.vault.touch();
    inner.save_current()?;
    Ok(inner.current.as_ref().unwrap().container.vault.clone())
}

/// 读取附件明文，返回 Base64（前端转 Blob 下载）。
#[tauri::command]
pub fn attachment_read(
    state: State<AppState>,
    attachment_id: String,
) -> CmdResult<String> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    let store = attachment_store_for(&inner)?;
    let r#ref = cur
        .container
        .vault
        .items
        .iter()
        .flat_map(|i| i.attachments.iter())
        .find(|a| a.id == attachment_id)
        .cloned()
        .ok_or("找不到该附件")?;
    let bytes = store.read(&r#ref).map_err(err)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// 从条目里移除附件（删 .sica 文件 + 去掉引用）。
#[tauri::command]
pub fn attachment_remove(
    state: State<AppState>,
    item_id: String,
    attachment_id: String,
) -> CmdResult<Vault> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let store = attachment_store_for(&inner)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    let item = cur
        .container
        .vault
        .items
        .iter_mut()
        .find(|i| i.id == item_id)
        .ok_or("找不到该条目")?;
    item.attachments.retain(|a| a.id != attachment_id);
    cur.container.vault.touch();
    inner.save_current()?;
    let _ = store.delete(&attachment_id); // 引用已摘除，文件尽力删
    Ok(inner.current.as_ref().unwrap().container.vault.clone())
}

// ---------------------------------------------------------------- 导入 / 导出

/// 给导入的条目解析标签：按名字找已有标签，找不到就建一个。
fn resolve_labels(vault: &mut Vault, names: Vec<String>) -> Vec<String> {
    let mut ids = Vec::new();
    for name in names {
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let existing = vault.labels.iter().find(|l| l.name == name);
        let id = match existing {
            Some(l) => l.id.clone(),
            None => {
                let label = Label {
                    id: new_id(),
                    name: name.clone(),
                    ..Default::default()
                };
                let id = label.id.clone();
                vault.labels.push(label);
                id
            }
        };
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

fn append_items(inner: &mut AppStateInner, pairs: Vec<(VaultItem, Vec<String>)>) -> CmdResult<u32> {
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    let mut count = 0u32;
    for (mut item, names) in pairs {
        item.label_ids = resolve_labels(&mut cur.container.vault, names);
        cur.container.vault.items.push(item);
        count += 1;
    }
    cur.container.vault.touch();
    inner.save_current()?;
    Ok(count)
}

#[tauri::command]
pub fn export_csv(state: State<AppState>) -> CmdResult<String> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    Ok(codec::to_csv(&cur.container.vault))
}

#[tauri::command]
pub fn export_safeincloud_xml(state: State<AppState>) -> CmdResult<String> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    Ok(safeincloud_xml::export(&cur.container.vault))
}

#[tauri::command]
pub fn export_vault_json(state: State<AppState>) -> CmdResult<String> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    codec::to_json(&cur.container.vault).map_err(err)
}

#[tauri::command]
pub fn import_csv(state: State<AppState>, text: String) -> CmdResult<u32> {
    let items = codec::items_from_csv(&text);
    if items.is_empty() {
        return Err("没有解析到可导入的条目（需要标题/用户名/密码列）".to_string());
    }
    let pairs: Vec<(VaultItem, Vec<String>)> = items.into_iter().map(|i| (i, Vec::new())).collect();
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    append_items(&mut inner, pairs)
}

#[tauri::command]
pub fn import_safeincloud_xml(state: State<AppState>, text: String) -> CmdResult<u32> {
    let cards = safeincloud_xml::parse(&text)?;
    if cards.is_empty() {
        return Err("XML 里没有可导入的条目".to_string());
    }
    let pairs: Vec<(VaultItem, Vec<String>)> = cards
        .into_iter()
        .map(|c| (c.item, c.label_names))
        .collect();
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    append_items(&mut inner, pairs)
}

/// 把当前库导出为加密 .sicv 文件的 Base64（前端 Blob 下载，可拷回手机）。
#[tauri::command]
pub fn export_vault_sicv(state: State<AppState>) -> CmdResult<String> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let file = {
        let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
        inner.store.vault_file(&cur.meta)
    };
    // 先落盘，再读盘，保证导出的是最新状态
    inner.save_current()?;
    let bytes = fs::read(&file).map_err(err)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(bytes))
}

// ---------------------------------------------------------------- 备份

#[tauri::command]
pub fn backup_create(state: State<AppState>) -> CmdResult<BackupInfo> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let (file, name) = {
        let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
        (inner.store.vault_file(&cur.meta), cur.container.vault.name.clone())
    };
    let mgr = BackupManager::new(&inner.backups_dir()).map_err(err)?;
    let info = mgr.backup(&file, &name).map_err(err)?;
    let _ = mgr.prune(10);
    // 对齐手机端 backupNow()：记录 lastBackupAt 并落盘
    inner.settings.last_backup_at = now_ms();
    inner.persist_settings()?;
    Ok(info)
}

#[tauri::command]
pub fn backup_list(state: State<AppState>) -> CmdResult<Vec<BackupInfo>> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let mgr = BackupManager::new(&inner.backups_dir()).map_err(err)?;
    mgr.list().map_err(err)
}

/// 用备份覆盖（同 id 库）或按新库导入（id 已不存在）。
#[tauri::command]
pub fn backup_restore(state: State<AppState>, path: String, password: String) -> CmdResult<Vault> {
    let bytes = fs::read(&path).map_err(err)?;
    let container = VaultContainer::open(&bytes, &password).map_err(err)?;
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let existing = inner
        .store
        .list()
        .map_err(err)?
        .into_iter()
        .find(|m| m.id == container.vault.id);
    match existing {
        Some(meta) => {
            let mut c = container;
            inner.store.save(&mut c, &meta).map_err(err)?;
            let vault = c.vault.clone();
            inner.mark_last_vault(&meta.id);
            inner.current = Some(CurrentVault {
                container: c,
                meta,
                password,
            });
            Ok(vault)
        }
        None => {
            let name = container.vault.name.clone();
            let (c, meta) = inner
                .store
                .create_from_vault(container.vault.clone(), &password)
                .map_err(err)?;
            let vault = c.vault.clone();
            let _ = name;
            inner.mark_last_vault(&meta.id);
            inner.current = Some(CurrentVault {
                container: c,
                meta,
                password,
            });
            Ok(vault)
        }
    }
}

#[tauri::command]
pub fn backup_delete(state: State<AppState>, path: String) -> CmdResult<()> {
    let _ = &state;
    fs::remove_file(&path).map_err(err)
}

// ---------------------------------------------------------------- 设置

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> CmdResult<AppSettings> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    Ok(inner.settings.clone())
}

#[tauri::command]
pub fn set_settings(state: State<AppState>, settings: AppSettings) -> CmdResult<()> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    inner.settings = settings;
    inner.persist_settings()
}

// ---------------------------------------------------------------- WebDAV 同步

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub direction: String,
    pub conflicts: i32,
    pub attachments_uploaded: i32,
    pub attachments_downloaded: i32,
    pub message: String,
}

fn to_summary(r: &SyncResult) -> SyncSummary {
    let direction = serde_json::to_value(&r.direction)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    SyncSummary {
        direction,
        conflicts: r.conflicts,
        attachments_uploaded: r.attachments_uploaded,
        attachments_downloaded: r.attachments_downloaded,
        message: r.message.clone(),
    }
}

#[tauri::command]
pub async fn webdav_test_connection(config: WebDavConfig) -> CmdResult<String> {
    let client = WebDavClient::new(config).map_err(err)?;
    client.test().await.map_err(err)?;
    Ok("连接成功".to_string())
}

fn build_engine(inner: &AppStateInner, config: &WebDavConfig) -> CmdResult<SyncEngine> {
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    let client = WebDavClient::new(config.clone()).map_err(err)?;
    let key = base64::engine::general_purpose::STANDARD
        .decode(cur.container.vault.content_key.trim())
        .unwrap_or_default();
    let attachments_root = inner.store.attachments_dir_for(&cur.meta.id);
    let attachment_store = AttachmentStore::new(&attachments_root, key).map_err(err)?;
    Ok(SyncEngine::new(
        client,
        config.clone(),
        attachment_store,
        &cur.meta.id,
        &cur.container.vault.name,
    ))
}

async fn run_sync(
    state: State<'_, AppState>,
    config: WebDavConfig,
    strategy: Option<ConflictStrategy>,
    pull: bool,
) -> CmdResult<SyncSummary> {
    // 引擎构建（需要引用 current），随后把 current 整体取出，await 期间不碰锁。
    let (engine, cur) = {
        let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        let engine = build_engine(&inner, &config)?;
        let cur = inner.current.take().ok_or(NO_OPEN_VAULT)?;
        (engine, cur)
    };
    let password = cur.password.clone();
    let CurrentVault {
        container,
        meta,
        password: _,
    } = cur;
    let outcome = if pull {
        engine.pull_remote(container, &password).await
    } else {
        engine
            .sync(container, &password, strategy.unwrap_or(ConflictStrategy::Detect))
            .await
    };
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    match outcome {
        Ok(result) => {
            let summary = to_summary(&result);
            let mut restored = CurrentVault {
                container: result.container,
                meta,
                password,
            };
            inner
                .store
                .save(&mut restored.container, &restored.meta)
                .map_err(err)?;
            inner.current = Some(restored);
            Ok(summary)
        }
        Err(e) => {
            // 容器已被引擎消费，但同步失败时本地文件没有改动——重新打开恢复解锁状态
            match inner.store.open(&meta, &password) {
                Ok(c) => {
                    inner.current = Some(CurrentVault {
                        container: c,
                        meta,
                        password,
                    });
                }
                Err(_) => inner.current = None,
            }
            Err(e.to_string())
        }
    }
}

#[tauri::command]
pub async fn webdav_sync(
    state: State<'_, AppState>,
    config: WebDavConfig,
    strategy: Option<ConflictStrategy>,
) -> CmdResult<SyncSummary> {
    run_sync(state, config, strategy, false).await
}

#[tauri::command]
pub async fn webdav_pull(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> CmdResult<SyncSummary> {
    run_sync(state, config, None, true).await
}

/// 自动拉取：只做 HEAD 比对，云端没变化时不下载整库（对齐 Android pullIfRemoteChanged）。
/// 返回 Some(summary) 表示检测到云端变化并已按“本地修改优先”静默同步。
#[tauri::command]
pub async fn webdav_pull_if_changed(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> CmdResult<Option<SyncSummary>> {
    if !config.is_configured() {
        return Ok(None);
    }
    let (name, last_synced, interval) = {
        let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        let cur = match inner.current.as_ref() {
            Some(c) => c,
            None => return Ok(None),
        };
        (
            cur.container.vault.name.clone(),
            cur.container.vault.last_synced_at,
            inner.settings.pull_interval_seconds,
        )
    };
    if interval <= 0 {
        return Ok(None);
    }
    let now = now_ms();
    {
        // 限流（坚果云免费版 600 次 / 30 分钟）：先记时间再发请求
        let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        if now - inner.last_pull_check_at < i64::from(interval) * 1_000 {
            return Ok(None);
        }
        inner.last_pull_check_at = now;
    }
    let client = WebDavClient::new(config.clone()).map_err(err)?;
    let path = config.remote_path(&name);
    let stat = match client.stat(&path).await.map_err(err)? {
        Some(s) => s,
        None => return Ok(None),
    };
    let mtime = stat.last_modified as i64;
    let changed = {
        let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        if mtime <= 0 {
            // 服务器没给 Last-Modified，只能保守地认为有变化
            true
        } else if inner.last_remote_mtime <= 0 {
            // 首次检查：比本机上次同步时间新才算有更新
            mtime > last_synced
        } else {
            mtime != inner.last_remote_mtime
        }
    };
    {
        let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        inner.last_remote_mtime = mtime;
    }
    if !changed {
        return Ok(None);
    }
    // 确有变化 → 静默同步（本地修改优先，对齐 doSync 默认 KEEP_LOCAL）
    let summary = run_sync(state.clone(), config, Some(ConflictStrategy::KeepLocal), false).await?;
    // 同步后服务器时间会变，重新记录，避免下一次把自己刚上传的当成远端更新
    if let Ok(Some(s2)) = client.stat(&path).await {
        let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        inner.last_remote_mtime = s2.last_modified as i64;
    }
    Ok(Some(summary))
}
