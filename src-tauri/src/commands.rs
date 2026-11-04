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

use crate::sp::backup::{BackupInfo, BackupManager};
use crate::sp::cloud::{key_for, CloudManifest, CloudVaultEntry};
use crate::sp::codec;
use crate::sp::container::VaultContainer;
use crate::sp::dpapi;
use crate::sp::fsutil::write_atomic;
use crate::sp::generator::{self, GeneratorOptions};
use crate::sp::hibp::HibpClient;
use crate::sp::model::{
    default_label_color, new_id, now_ms, FieldHistoryEntry, FieldType, Label, LabelSpec, Vault,
    VaultItem, VaultMeta,
};
use crate::sp::xml;
use crate::sp::security::{self, SecurityReport};
use crate::sp::store::VaultStore;
use crate::sp::strength::{self, StrengthResult};
use crate::sp::sync::{ConflictStrategy, SyncCacheEntry, SyncEngine, SyncResult};
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

/// 把当前库从内存里摘下来，并顺手把主密码缓冲擦零（DEK 由 VaultContainer::drop 擦）。
pub fn drop_current(inner: &mut AppStateInner) {
    if let Some(mut cur) = inner.current.take() {
        unsafe { crate::sp::crypto::wipe(cur.password.as_bytes_mut()) };
        cur.password.clear();
    }
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
    /// 同步缓存（按云端键）：版本指纹 + 内容指纹，用来跳过无谓的整包下载与重传。
    /// 只存指纹，不含任何密钥或明文内容。
    pub sync_cache: HashMap<String, SyncCacheEntry>,
    pub sync_cache_path: PathBuf,
}

pub struct AppState {
    pub inner: Mutex<AppStateInner>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// "system" | "light" | "dark"
    pub theme: String,
    /// 主题强调色（#RRGGBB，对齐手机端 ThemeScreen 的 themePresets）
    pub accent: String,
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
    /// 启动时自动检查更新（新版本只提示，不自动下载）
    #[serde(default = "default_true")]
    pub check_updates_on_launch: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings {
            theme: "system".to_string(),
            accent: "#1677FF".to_string(),
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
            check_updates_on_launch: true,
        }
    }
}

impl AppState {
    pub fn new(data_root: &PathBuf) -> CmdResult<Self> {
        fs::create_dir_all(data_root).map_err(err)?;
        let store = VaultStore::new(data_root).map_err(err)?;
        let settings_path = data_root.join("settings.json");
        let mut settings: AppSettings = match fs::read_to_string(&settings_path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => AppSettings::default(),
        };
        // 敏感字段落盘是 DPAPI 密文，读入时解码为内存明文（旧版明文无前缀原样通过）
        settings.hibp_api_key = dpapi::decode_secret(&settings.hibp_api_key);
        settings.webdav.password = dpapi::decode_secret(&settings.webdav.password);
        let sync_cache_path = data_root.join("sync-cache.json");
        let sync_cache: HashMap<String, SyncCacheEntry> = fs::read_to_string(&sync_cache_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        Ok(AppState {
            inner: Mutex::new(AppStateInner {
                data_root: data_root.clone(),
                store,
                current: None,
                settings,
                settings_path,
                last_remote_mtime: 0,
                last_pull_check_at: 0,
                sync_cache,
                sync_cache_path,
            }),
        })
    }
}

impl AppStateInner {
    fn persist_settings(&self) -> CmdResult<()> {
        // 敏感字段以 DPAPI 密文落盘，内存中保持明文
        let mut s = self.settings.clone();
        s.hibp_api_key = dpapi::encode_secret(&s.hibp_api_key);
        s.webdav.password = dpapi::encode_secret(&s.webdav.password);
        let text = serde_json::to_string_pretty(&s).map_err(err)?;
        write_atomic(&self.settings_path, text.as_bytes()).map_err(err)
    }

    /// 同步缓存落盘。写失败不致命：下一次同步只是退化成完整流程（下载整包）。
    fn persist_sync_cache(&self) {
        if let Ok(text) = serde_json::to_string(&self.sync_cache) {
            let _ = write_atomic(&self.sync_cache_path, text.as_bytes());
        }
    }

    fn backups_dir(&self) -> PathBuf {
        // 安装目录下 backup（对齐手机端"默认备份目录"约定）；调试构建固定用数据目录，
        // 避免测试互相污染 target 目录。
        if !cfg!(debug_assertions) {
            if let Some(dir) = default_backup_dir() {
                return dir;
            }
        }
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
    if password.chars().count() < 8 {
        return Err("主密码至少 8 位".to_string());
    }
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        let exists = inner
            .store
            .list()
            .map_err(err)?
            .into_iter()
            .any(|m| m.name.eq_ignore_ascii_case(trimmed));
        if exists {
            return Err(format!("已存在同名密码库“{trimmed}”"));
        }
    }
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
        drop_current(&mut inner);
    }
    Ok(())
}

#[tauri::command]
pub fn current_vault(state: State<AppState>) -> CmdResult<Option<Vault>> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    Ok(inner.current.as_ref().map(|c| c.container.vault.clone()))
}

#[tauri::command]
pub async fn delete_vault(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let (meta, cfg, content_key) = {
        let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        // 删之前先记下内容密钥：云端文件名由它派生（改名不变），用来精确删云端文件
        let content_key = inner
            .current
            .as_ref()
            .filter(|c| c.meta.id == id)
            .map(|c| c.container.vault.content_key.clone())
            .filter(|k| !k.trim().is_empty());
        if inner.current.as_ref().is_some_and(|c| c.meta.id == id) {
            drop_current(&mut inner);
        }
        let cfg = inner.settings.webdav.clone();
        let found = inner
            .store
            .list()
            .map_err(err)?
            .into_iter()
            .find(|m| m.id == id);
        match found {
            Some(meta) => {
                inner.store.delete(&meta).map_err(err)?;
                (Some(meta), cfg, content_key)
            }
            // 本地已经不存在这个库（例如文件已被删掉、只是界面上还残留一条）：
            // 删除是幂等操作，这里按成功处理，让界面刷新后自然消失。
            None => (None, cfg, content_key),
        }
    };
    let Some(meta) = meta else {
        return Ok(());
    };
    // 云端同步删除（尽力而为：失败不影响本地删除结果，保证一端操作另一端也能感知）
    if cfg.is_configured() && cfg.enabled {
        if let Ok(client) = WebDavClient::new(cfg.clone()) {
            // 云端文件名可能有两种：内容密钥键（新方案，改名不变）与库名键（旧版本写的）
            let mut keys: Vec<String> = Vec::new();
            if let Some(ck) = content_key.as_deref() {
                if let Some(k) = crate::sp::cloud::key_for_content(ck) {
                    keys.push(k);
                }
            }
            let name_key = key_for(&meta.name);
            if !keys.contains(&name_key) {
                keys.push(name_key);
            }
            for key in keys.iter() {
                let _ = client.delete(&cfg.remote_path(key)).await;
            }
            if let Ok(Some(bytes)) = client.get(&cfg.manifest_path()).await {
                let text = String::from_utf8_lossy(&bytes).to_string();
                let mut manifest = CloudManifest::parse(Some(text.as_str()));
                let before = manifest.vaults.len();
                manifest
                    .vaults
                    .retain(|e| !keys.contains(&e.key) && e.name.trim() != meta.name.trim());
                if manifest.vaults.len() != before {
                    let _ = client
                        .put(&cfg.manifest_path(), manifest.to_json().as_bytes())
                        .await;
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn rename_vault(state: State<AppState>, id: String, name: String) -> CmdResult<()> {
    let trimmed = name.trim().to_string();
    if trimmed.is_empty() {
        return Err("名称不能为空".to_string());
    }
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let matches_current = inner.current.as_ref().is_some_and(|c| c.meta.id == id);
    if matches_current {
        let cur = inner.current.as_mut().unwrap();
        cur.container.vault.name = trimmed;
        cur.meta.name = cur.container.vault.name.clone();
        cur.container.vault.touch();
        return inner.save_current();
    }
    let found = inner
        .store
        .list()
        .map_err(err)?
        .into_iter()
        .find(|m| m.id == id);
    if let Some(mut meta) = found {
        meta.name = trimmed;
        return inner.store.update_meta(&meta).map_err(err);
    }
    // 兜底：界面上的库对象可能是导入/恢复前的旧 id（恢复会重新分配 id），
    // 这种情况直接改"当前打开的库"——用户看到的同一个库，不会误改别的库。
    match inner.current.as_mut() {
        Some(cur) => {
            cur.container.vault.name = trimmed;
            cur.meta.name = cur.container.vault.name.clone();
            cur.container.vault.touch();
            inner.save_current()
        }
        None => Err("找不到该密码库".to_string()),
    }
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
pub fn put_item(state: State<AppState>, mut item: VaultItem) -> CmdResult<Vault> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    let previous = cur
        .container
        .vault
        .items
        .iter()
        .find(|i| i.id == item.id)
        .cloned();
    if let Some(old_item) = previous {
        record_field_history(&old_item, &mut item);
        if let Some(existing) = cur.container.vault.items.iter_mut().find(|i| i.id == item.id) {
            *existing = item;
        }
    } else {
        cur.container.vault.items.push(item);
    }
    cur.container.vault.touch();
    inner.save_current()?;
    Ok(inner.current.as_ref().unwrap().container.vault.clone())
}

/// 密码/秘密字段的值变了，就把旧值记进历史（新的在前，最多 20 条）。
fn record_field_history(old: &VaultItem, new: &mut VaultItem) {
    for field in new.fields.iter_mut() {
        if !matches!(field.r#type, FieldType::Password | FieldType::Secret) {
            continue;
        }
        let Some(prev) = old.fields.iter().find(|f| f.id == field.id) else {
            continue;
        };
        if prev.value.is_empty() || prev.value == field.value {
            continue;
        }
        // 只跳过"相邻重复"，A→B→A 这种回退仍会留下记录
        if field.history.first().is_some_and(|h| h.value == prev.value) {
            continue;
        }
        field.history.insert(
            0,
            FieldHistoryEntry {
                value: prev.value.clone(),
                changed_at: now_ms(),
            },
        );
        field.history.truncate(HISTORY_LIMIT);
    }
}

/// 每个字段最多保留的历史条数。
const HISTORY_LIMIT: usize = 20;

/// 删除某条密码历史（立即落盘）。
#[tauri::command]
pub fn delete_field_history(
    state: State<AppState>,
    item_id: String,
    field_id: String,
    index: usize,
) -> CmdResult<Vault> {
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    let item = cur
        .container
        .vault
        .items
        .iter_mut()
        .find(|i| i.id == item_id)
        .ok_or_else(|| "找不到该条目".to_string())?;
    let field = item
        .fields
        .iter_mut()
        .find(|f| f.id == field_id)
        .ok_or_else(|| "找不到该字段".to_string())?;
    if index >= field.history.len() {
        return Err("找不到该历史记录".to_string());
    }
    field.history.remove(index);
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

/// 导入明文 JSON 导出文件里的**条目**到当前密码库（对齐手机端「导入条目」，不新建库）。
#[tauri::command]
pub fn import_json_items(state: State<AppState>, json: String) -> CmdResult<u32> {
    let vault = codec::from_json(&json).map_err(err)?;
    if vault.items.is_empty() {
        return Err("JSON 里没有可导入的条目".to_string());
    }
    let mut by_id: HashMap<String, LabelSpec> = HashMap::new();
    for label in &vault.labels {
        by_id.insert(
            label.id.clone(),
            LabelSpec {
                name: label.name.clone(),
                color: Some(label.color),
                sort_order: Some(label.sort_order),
            },
        );
    }
    let pairs: Vec<(VaultItem, Vec<LabelSpec>)> = vault
        .items
        .into_iter()
        .map(|it| {
            let specs = it
                .label_ids
                .iter()
                .filter_map(|id| by_id.get(id).cloned())
                .collect::<Vec<_>>();
            (it, specs)
        })
        .collect();
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    append_items(&mut inner, pairs)
}

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

// ---------------------------------------------------------------- 导入 / 导出

/// 给导入的条目解析标签：按名字找已有标签，找不到就建一个。
///
/// 导入文件里带的颜色/排序只在"新建标签"时生效；同名标签已存在时沿用库里
/// 的样式，只有当库里还是默认颜色时才补上（避免导入把用户调过的样式覆盖掉）。
fn resolve_labels(vault: &mut Vault, specs: Vec<LabelSpec>) -> Vec<String> {
    let mut ids = Vec::new();
    for spec in specs {
        let name = spec.name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let existing = vault.labels.iter().position(|l| l.name == name);
        let id = match existing {
            Some(index) => {
                let label = &mut vault.labels[index];
                if label.color == default_label_color() {
                    if let Some(color) = spec.color {
                        label.color = color;
                    }
                }
                if label.sort_order == 0 {
                    if let Some(order) = spec.sort_order {
                        label.sort_order = order;
                    }
                }
                label.id.clone()
            }
            None => {
                let sort_order = spec.sort_order.unwrap_or_else(|| {
                    vault
                        .labels
                        .iter()
                        .map(|l| l.sort_order)
                        .max()
                        .unwrap_or(-1)
                        + 1
                });
                let label = Label {
                    id: new_id(),
                    name: name.clone(),
                    color: spec.color.unwrap_or_else(default_label_color),
                    sort_order,
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

fn append_items(
    inner: &mut AppStateInner,
    pairs: Vec<(VaultItem, Vec<LabelSpec>)>,
) -> CmdResult<u32> {
    let cur = inner.current.as_mut().ok_or(NO_OPEN_VAULT)?;
    let mut count = 0u32;
    for (mut item, specs) in pairs {
        item.label_ids = resolve_labels(&mut cur.container.vault, specs);
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
pub fn export_xml(state: State<AppState>) -> CmdResult<String> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    Ok(xml::export(&cur.container.vault))
}

#[tauri::command]
pub fn export_vault_json(state: State<AppState>) -> CmdResult<String> {
    let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    codec::to_json(&cur.container.vault).map_err(err)
}

#[tauri::command]
pub fn import_csv(state: State<AppState>, text: String) -> CmdResult<u32> {
    let pairs = codec::items_from_csv(&text);
    if pairs.is_empty() {
        return Err("没有解析到可导入的条目（需要标题/用户名/密码列）".to_string());
    }
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    append_items(&mut inner, pairs)
}

#[tauri::command]
pub fn import_xml(state: State<AppState>, text: String) -> CmdResult<u32> {
    let cards = xml::parse(&text)?;
    if cards.is_empty() {
        return Err("XML 里没有可导入的条目".to_string());
    }
    let pairs: Vec<(VaultItem, Vec<LabelSpec>)> =
        cards.into_iter().map(|c| (c.item, c.labels)).collect();
    let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
    append_items(&mut inner, pairs)
}

/// 把当前库导出为加密 .spv 文件的 Base64（前端 Blob 下载，可拷回手机）。
#[tauri::command]
pub fn export_vault_spv(state: State<AppState>) -> CmdResult<String> {
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

// ---------------------------------------------------------------- 保存对话框

/// 安装目录下 backup（默认导出/备份目录）；目录不可写则返回 None。
fn default_backup_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.join("backup");
    std::fs::create_dir_all(&dir).ok()?;
    let probe = dir.join(".write-probe");
    std::fs::write(&probe, b"ok").ok()?;
    let _ = std::fs::remove_file(&probe);
    Some(dir)
}

/// 弹出系统保存对话框并写入内容（base64 传入，文本与二进制导出通用）。
#[tauri::command]
pub async fn save_export(
    app: tauri::AppHandle,
    default_name: String,
    base64_data: String,
) -> CmdResult<Option<String>> {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_data.as_bytes())
        .map_err(|e| format!("内容解码失败：{e}"))?;
    let dir = default_backup_dir();
    let name = default_name.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let mut dlg = app.dialog().file();
        if let Some(dir) = dir {
            dlg = dlg.set_directory(dir);
        }
        dlg.set_file_name(name).blocking_save_file()
    })
    .await
    .map_err(|e| format!("保存对话框失败：{e}"))?;

    let Some(fp) = picked else {
        return Ok(None); // 用户取消
    };
    let Some(path) = fp.as_path().map(|p| p.to_path_buf()) else {
        return Err("保存位置无效".to_string());
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, bytes).map_err(|e| format!("写入失败：{e}"))?;
    Ok(Some(path.to_string_lossy().to_string()))
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
    // 合并"安装目录 backup"与旧数据目录备份：升级后旧备份仍可见
    let mut all: Vec<BackupInfo> = Vec::new();
    for dir in [inner.backups_dir(), inner.data_root.join("backups")] {
        if !dir.exists() {
            continue;
        }
        if let Ok(mgr) = BackupManager::new(&dir) {
            if let Ok(list) = mgr.list() {
                for b in list {
                    if !all.iter().any(|x| x.path == b.path) {
                        all.push(b);
                    }
                }
            }
        }
    }
    all.sort_by_key(|b| std::cmp::Reverse(b.created_at));
    Ok(all)
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
    pub message: String,
}

fn to_summary(r: &SyncResult) -> SyncSummary {
    let direction = serde_json::to_value(r.direction)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    SyncSummary {
        direction,
        conflicts: r.conflicts,
        message: r.message.clone(),
    }
}

#[tauri::command]
pub async fn webdav_test_connection(config: WebDavConfig) -> CmdResult<String> {
    let client = WebDavClient::new(config).map_err(err)?;
    client.test().await.map_err(err)?;
    Ok("连接成功".to_string())
}

/// 列出云端密码库（读取 WebDAV 上的清单 SafePassword-index.json）。
#[tauri::command]
pub async fn webdav_list_cloud(config: WebDavConfig) -> CmdResult<Vec<CloudVaultEntry>> {
    let client = WebDavClient::new(config.clone()).map_err(err)?;
    match client.get(&config.manifest_path()).await.map_err(err)? {
        Some(bytes) => Ok(CloudManifest::parse(Some(&String::from_utf8_lossy(&bytes))).vaults),
        None => Ok(Vec::new()),
    }
}

/// 下载指定云端密码库的密文（Base64），供前端用主密码导入并切换。
#[tauri::command]
pub async fn webdav_get_vault(config: WebDavConfig, key: String) -> CmdResult<Option<String>> {
    let client = WebDavClient::new(config.clone()).map_err(err)?;
    let bytes = client.get(&config.remote_path(&key)).await.map_err(err)?;
    Ok(bytes.map(|b| {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(b)
    }))
}

/// 删除云端密码库：删除密文文件并从清单中移除该条目（用于改名后清理旧键）。
#[tauri::command]
pub async fn webdav_delete_cloud(config: WebDavConfig, key: String) -> CmdResult<()> {
    let client = WebDavClient::new(config.clone()).map_err(err)?;
    let _ = client.delete(&config.remote_path(&key)).await;
    let mut manifest = match client.get(&config.manifest_path()).await.map_err(err)? {
        Some(bytes) => CloudManifest::parse(Some(&String::from_utf8_lossy(&bytes))),
        None => CloudManifest::default(),
    };
    manifest.remove(&key);
    client
        .put(&config.manifest_path(), manifest.to_json().as_bytes())
        .await
        .map_err(err)
}

/// 云端稳定键：由库的**内容密钥**决定（改名不变）。库里没有内容密钥时才退回按库名派生。
#[tauri::command]
pub fn cloud_key(state: State<AppState>, name: String) -> CmdResult<String> {
    let content_key = {
        let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        inner
            .current
            .as_ref()
            .map(|c| c.container.vault.content_key.clone())
            .unwrap_or_default()
    };
    Ok(crate::sp::cloud::key_for_content(&content_key)
        .unwrap_or_else(|| crate::sp::cloud::key_for(&name)))
}

/// 同步缓存用的键：与云端文件名同源（内容密钥优先），改名不影响。
fn cloud_cache_key(vault: &Vault) -> String {
    crate::sp::cloud::key_for_content(&vault.content_key).unwrap_or_else(|| key_for(&vault.name))
}

fn build_engine(inner: &AppStateInner, config: &WebDavConfig) -> CmdResult<SyncEngine> {
    let cur = inner.current.as_ref().ok_or(NO_OPEN_VAULT)?;
    let client = WebDavClient::new(config.clone()).map_err(err)?;
    let cache = inner
        .sync_cache
        .get(&cloud_cache_key(&cur.container.vault))
        .cloned()
        .unwrap_or_default();
    Ok(SyncEngine::new(
        client,
        config.clone(),
        &cur.container.vault.name,
        &cur.container.vault.content_key,
        cache,
    ))
}

async fn run_sync(
    state: State<'_, AppState>,
    config: WebDavConfig,
    strategy: Option<ConflictStrategy>,
    pull: bool,
) -> CmdResult<SyncSummary> {
    // 引擎构建（需要引用 current），随后把 current 整体取出，await 期间不碰锁。
    let (mut engine, cur) = {
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
            .sync(container, &password, strategy.unwrap_or(ConflictStrategy::Merge))
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
            // 记下这次的版本指纹与内容指纹，下次同步先用 HEAD 比对再决定要不要下载
            inner.sync_cache.insert(
                cloud_cache_key(&restored.container.vault),
                engine.cache_entry(),
            );
            inner.persist_sync_cache();
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
                Err(_) => drop_current(&mut inner),
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

/// 「云端已删除」弹窗里的"恢复到云端"：无条件把本地库推上去并维护清单。
#[tauri::command]
pub async fn webdav_restore_to_cloud(
    state: State<'_, AppState>,
    config: WebDavConfig,
) -> CmdResult<SyncSummary> {
    let (mut engine, cur) = {
        let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        let engine = build_engine(&inner, &config)?;
        let cur = inner.current.take().ok_or(NO_OPEN_VAULT)?;
        (engine, cur)
    };
    let CurrentVault {
        container,
        meta,
        password,
    } = cur;
    let outcome = engine.restore_to_cloud(container).await;
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
            inner.sync_cache.insert(
                cloud_cache_key(&restored.container.vault),
                engine.cache_entry(),
            );
            inner.persist_sync_cache();
            inner.current = Some(restored);
            Ok(summary)
        }
        Err(e) => {
            match inner.store.open(&meta, &password) {
                Ok(c) => {
                    inner.current = Some(CurrentVault {
                        container: c,
                        meta,
                        password,
                    });
                }
                Err(_) => drop_current(&mut inner),
            }
            Err(e.to_string())
        }
    }
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
    let (name, last_synced, interval, content_key, cached_rev, cached_size) = {
        let inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        let cur = match inner.current.as_ref() {
            Some(c) => c,
            None => return Ok(None),
        };
        let key = cloud_cache_key(&cur.container.vault);
        let cached = inner.sync_cache.get(&key);
        (
            cur.container.vault.name.clone(),
            cur.container.vault.last_synced_at,
            inner.settings.pull_interval_seconds,
            cur.container.vault.content_key.clone(),
            cached.and_then(|c| c.rev.clone()),
            cached.map(|c| c.rev.as_ref().map(|r| r.size).unwrap_or(0)).unwrap_or(0),
        )
    };
    if interval <= 0 {
        return Ok(None);
    }
    // 自适应间隔：库越大，检查越稀（坚果云免费版 600 次 / 30 分钟、下载 3 GB / 月）。
    // 8.9 MB 的库 ≈ 4.5 分钟一次；小库保持用户设置的 60 秒。
    let size_mb = cached_size.max(0) as f64 / 1_048_576.0;
    let adaptive = (30.0 * size_mb).clamp(60.0, 900.0) as i64;
    let effective = i64::from(interval).max(adaptive);
    let now = now_ms();
    {
        // 限流（坚果云免费版 600 次 / 30 分钟）：先记时间再发请求
        let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
        if now - inner.last_pull_check_at < effective * 1_000 {
            return Ok(None);
        }
        inner.last_pull_check_at = now;
    }
    let client = WebDavClient::new(config.clone()).map_err(err)?;
    // 云端文件名与库名无关：由内容密钥派生（改名/换服务器都不会变）
    let key = crate::sp::cloud::key_for_content(&content_key)
        .unwrap_or_else(|| key_for(&name));
    let path = config.remote_path(&key);
    let stat = client.stat(&path).await.map_err(err)?;
    let changed = match stat {
        // 云端文件不存在：可能被别处删除，交给同步引擎判断（会给出 RemoteDeleted）
        None => true,
        Some(s) => {
            let mtime = s.last_modified as i64;
            let changed = match cached_rev.as_ref() {
                // 有版本指纹时直接比 ETag（最准，不受服务器时间精度影响）
                Some(rev) => !rev.matches(&s),
                None => {
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
                }
            };
            {
                let mut inner = state.inner.lock().map_err(|_| LOCK_BROKEN)?;
                inner.last_remote_mtime = mtime;
            }
            changed
        }
    };
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

// ---------------------------------------------------------------- 更新检查（对齐手机端 AboutScreen）

/// 更新信息
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub url: String,
    pub asset_url: String,
    pub notes: String,
}

fn version_tuple(v: &str) -> (u32, u32, u32) {
    let clean = v.trim().trim_start_matches(['v', 'V']);
    let mut parts = clean.split('.').map(|p| p.trim().parse::<u32>().unwrap_or(0));
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

/// 桌面版 Release 仓库（推送后即可用；未发布时返回可读错误）
const UPDATE_REPO: &str = "yaoahos/SafePasswordDesktop";

#[tauri::command]
pub async fn check_update(app_version: String) -> CmdResult<UpdateInfo> {
    let client = reqwest::Client::builder()
        .user_agent("SafePassword-Desktop")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("无法初始化网络：{e}"))?;
    let url = format!("https://api.github.com/repos/{UPDATE_REPO}/releases/latest");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub：{e}"))?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("GitHub 上还没有桌面版 Release，暂时无法检查更新".to_string());
    }
    if !resp.status().is_success() {
        return Err(format!("检查更新失败（HTTP {}）", resp.status().as_u16()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析更新信息失败：{e}"))?;
    let tag = body
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let html_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let notes = body
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let asset_url = body
        .get("assets")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter()
                .map(|a| {
                    a.get("browser_download_url")
                        .and_then(|u| u.as_str())
                        .unwrap_or("")
                })
                .find(|u| u.ends_with(".exe") || u.ends_with(".msi"))
        })
        .unwrap_or("")
        .to_string();
    let latest = tag.trim_start_matches(['v', 'V']).to_string();
    let has_update = version_tuple(&latest) > version_tuple(&app_version);
    Ok(UpdateInfo {
        current: app_version,
        latest,
        has_update,
        url: html_url,
        asset_url,
        notes,
    })
}

/// 下载安装包并启动安装（软件内自更新）
#[tauri::command]
pub async fn update_download_and_install(asset_url: String) -> CmdResult<String> {
    if !asset_url.starts_with("https://") {
        return Err("安装包地址无效".to_string());
    }
    // 只允许从 GitHub 的域名取安装包（重定向到 objects.githubusercontent.com 由 reqwest 跟随）
    let host = asset_url["https://".len()..]
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    let host_ok = host.eq_ignore_ascii_case("github.com")
        || host.eq_ignore_ascii_case("objects.githubusercontent.com")
        || host.eq_ignore_ascii_case("github-releases.githubusercontent.com");
    if !host_ok {
        return Err("安装包地址无效".to_string());
    }
    let client = reqwest::Client::builder()
        .user_agent("SafePassword-Desktop")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("无法初始化网络：{e}"))?;
    let resp = client
        .get(&asset_url)
        .send()
        .await
        .map_err(|e| format!("下载失败：{e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败（HTTP {}）", resp.status().as_u16()));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读取安装包失败：{e}"))?;
    let dir = std::env::temp_dir().join("SafePassword-update");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建临时目录失败：{e}"))?;
    let raw_name = asset_url
        .rsplit('/')
        .next()
        .unwrap_or("SafePassword-setup.exe");
    // 文件名白名单：只留安全字符并强制 exe/msi 后缀，避免 ".."、分隔符之类造成路径穿越
    let name: String = raw_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    let lower = name.to_ascii_lowercase();
    if !(lower.ends_with(".exe") || lower.ends_with(".msi")) {
        return Err("安装包格式不支持".to_string());
    }
    // 校验文件头，避免把一个错误页 / 被替换的内容当安装包执行
    let magic_ok = if lower.ends_with(".exe") {
        bytes.starts_with(b"MZ")
    } else {
        bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
    };
    if !magic_ok || bytes.len() < 512 * 1024 {
        return Err("安装包内容异常，已取消安装".to_string());
    }
    let path = dir.join(name);
    std::fs::write(&path, &bytes).map_err(|e| format!("写入安装包失败：{e}"))?;
    std::process::Command::new(&path)
        .spawn()
        .map_err(|e| format!("启动安装程序失败：{e}"))?;
    Ok(path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sp::model::Field;

    fn item_with_password(value: &str) -> VaultItem {
        VaultItem {
            id: "item-1".to_string(),
            title: "测试".to_string(),
            fields: vec![Field {
                id: "field-1".to_string(),
                label: "密码".to_string(),
                value: value.to_string(),
                r#type: FieldType::Password,
                sensitive: true,
                history: Vec::new(),
            }],
            ..VaultItem::default()
        }
    }

    #[test]
    fn password_change_appends_previous_value() {
        let old = item_with_password("old-pass");
        let mut new = item_with_password("new-pass");
        record_field_history(&old, &mut new);
        assert_eq!(new.fields[0].history.len(), 1);
        assert_eq!(new.fields[0].history[0].value, "old-pass");
        assert!(new.fields[0].history[0].changed_at > 0);
    }

    #[test]
    fn unchanged_or_empty_value_records_nothing() {
        let old = item_with_password("same");
        let mut new = item_with_password("same");
        record_field_history(&old, &mut new);
        assert!(new.fields[0].history.is_empty());

        let empty_old = item_with_password("");
        let mut filled = item_with_password("filled");
        record_field_history(&empty_old, &mut filled);
        assert!(filled.fields[0].history.is_empty());
    }

    #[test]
    fn history_is_capped_and_skips_adjacent_duplicates() {
        let mut new = item_with_password("v0");
        new.fields[0].history = (0..HISTORY_LIMIT)
            .map(|i| FieldHistoryEntry {
                value: format!("h{i}"),
                changed_at: i as i64,
            })
            .collect();
        let old = item_with_password("previous");
        record_field_history(&old, &mut new);
        assert_eq!(new.fields[0].history.len(), HISTORY_LIMIT);
        assert_eq!(new.fields[0].history[0].value, "previous");

        // 相邻重复不再记录
        let old2 = item_with_password("previous");
        let mut new2 = new.clone();
        record_field_history(&old2, &mut new2);
        assert_eq!(new2.fields[0].history.len(), HISTORY_LIMIT);
    }

    #[test]
    fn imported_labels_keep_color_and_order() {
        let mut vault = Vault::default();
        let color: i32 = 0xFF33_66CCu32 as i32;
        // 新标签：颜色/排序按文件里的来
        let ids = resolve_labels(
            &mut vault,
            vec![LabelSpec {
                name: "工作".to_string(),
                color: Some(color),
                sort_order: Some(3),
            }],
        );
        assert_eq!(ids.len(), 1);
        let created = vault.labels.iter().find(|l| l.name == "工作").unwrap();
        assert_eq!(created.color, color);
        assert_eq!(created.sort_order, 3);

        // 同名标签已存在：沿用库里的 id，不重复创建
        let ids2 = resolve_labels(&mut vault, vec![LabelSpec::named("工作")]);
        assert_eq!(ids2, ids);
        assert_eq!(vault.labels.len(), 1);
    }
}
