//! 对齐 Android VaultStore.kt 的保险库存储层：
//! root/vaults/<vaultId>.spv、root/vaults/index.json、root/attachments/<vaultId>/。
#![allow(dead_code)]

use crate::sp::container::{ContainerError, VaultContainer};
use crate::sp::fsutil::write_atomic;
use crate::sp::model::{new_id, now_ms, Vault, VaultIndex, VaultMeta};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error(transparent)]
    Container(#[from] ContainerError),
    #[error("io: {0}")]
    Io(String),
}

impl From<std::io::Error> for StoreError {
    fn from(e: std::io::Error) -> Self {
        StoreError::Io(e.to_string())
    }
}

fn io<E: std::fmt::Display>(e: E) -> StoreError {
    StoreError::Io(e.to_string())
}

pub struct VaultStore {
    vaults_dir: PathBuf,
    attachments_dir: PathBuf,
    index_file: PathBuf,
}

impl VaultStore {
    pub fn new(root: &Path) -> Result<Self, StoreError> {
        let vaults_dir = root.join("vaults");
        let attachments_dir = root.join("attachments");
        fs::create_dir_all(&vaults_dir).map_err(io)?;
        fs::create_dir_all(&attachments_dir).map_err(io)?;
        Ok(VaultStore {
            index_file: vaults_dir.join("index.json"),
            vaults_dir,
            attachments_dir,
        })
    }

    pub fn vault_file(&self, meta: &VaultMeta) -> PathBuf {
        self.vaults_dir.join(&meta.file_name)
    }

    pub fn attachments_dir_for(&self, vault_id: &str) -> PathBuf {
        self.attachments_dir.join(vault_id)
    }

    pub fn list(&self) -> Result<Vec<VaultMeta>, StoreError> {
        let mut index = self.read_index();
        let mut changed = false;
        // 目录只扫一次：既剔除索引里已不存在的条目，也认领没有索引条目的孤儿库
        let mut files: Vec<String> = Vec::new();
        for entry in fs::read_dir(&self.vaults_dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            let file_name = entry.file_name().to_string_lossy().into_owned();
            // 只收 .spv 保险库文件
            if file_name.ends_with(".spv") {
                files.push(file_name);
            }
        }
        let on_disk: HashSet<&str> = files.iter().map(|s| s.as_str()).collect();
        let before = index.vaults.len();
        index.vaults.retain(|m| on_disk.contains(m.file_name.as_str()));
        if index.vaults.len() != before {
            changed = true;
        }
        let known: HashSet<String> = index.vaults.iter().map(|m| m.file_name.clone()).collect();
        for file_name in files {
            let stem = file_name.strip_suffix(".spv");
            let Some(stem) = stem else { continue };
            if known.contains(&file_name) {
                continue;
            }
            let created_at = fs::metadata(self.vaults_dir.join(&file_name))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            index.vaults.push(VaultMeta {
                id: stem.to_string(),
                name: "密码库".to_string(),
                file_name,
                created_at,
                last_opened_at: 0,
                last_synced_at: 0,
            });
            changed = true;
        }
        if changed {
            self.write_index(&index)?;
        }
        let mut metas = index.vaults;
        metas.sort_by_key(|m| std::cmp::Reverse(m.last_opened_at));
        Ok(metas)
    }

    pub fn create(
        &self,
        name: &str,
        password: &str,
    ) -> Result<(VaultContainer, VaultMeta), StoreError> {
        let mut vault = Vault::default();
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            vault.name = trimmed.to_string();
        }
        let container = VaultContainer::create(vault, password);
        self.create_from_container(container)
    }

    fn create_from_container(
        &self,
        mut container: VaultContainer,
    ) -> Result<(VaultContainer, VaultMeta), StoreError> {
        let vault_id = container.vault.id.clone();
        let file_name = format!("{vault_id}.spv");
        let meta = VaultMeta {
            id: vault_id,
            name: container.vault.name.clone(),
            file_name,
            created_at: now_ms(),
            last_opened_at: now_ms(),
            last_synced_at: 0,
        };
        container.save_to(&self.vault_file(&meta))?;
        let mut index = self.read_index();
        index.vaults.push(meta.clone());
        self.write_index(&index)?;
        Ok((container, meta))
    }

    pub fn open(
        &self,
        meta: &VaultMeta,
        password: &str,
    ) -> Result<VaultContainer, StoreError> {
        let data = fs::read(self.vault_file(meta)).map_err(io)?;
        let container = VaultContainer::open(&data, password)?;
        let mut index = self.read_index();
        if let Some(stored) = index.vaults.iter_mut().find(|m| m.id == meta.id) {
            stored.last_opened_at = now_ms();
            stored.name = container.vault.name.clone();
            self.write_index(&index)?;
        }
        Ok(container)
    }

    pub fn save(
        &self,
        container: &mut VaultContainer,
        meta: &VaultMeta,
    ) -> Result<(), StoreError> {
        container.save_to(&self.vault_file(meta))?;
        let mut index = self.read_index();
        match index.vaults.iter_mut().find(|m| m.id == meta.id) {
            Some(stored) => {
                stored.name = container.vault.name.clone();
                stored.last_opened_at = now_ms();
            }
            // 索引里丢了这条（别的实例覆盖索引 / 恢复后索引缺失）：补回来，
            // 否则后续按 id 找库（解锁、改名、删除）都会报"找不到该密码库"
            None => {
                let mut back = meta.clone();
                back.name = container.vault.name.clone();
                back.last_opened_at = now_ms();
                index.vaults.push(back);
            }
        }
        self.write_index(&index)
    }

    pub fn update_meta(&self, meta: &VaultMeta) -> Result<(), StoreError> {
        let mut index = self.read_index();
        match index.vaults.iter_mut().find(|m| m.id == meta.id) {
            Some(stored) => *stored = meta.clone(),
            // 索引里没有这条（例如索引被别的实例覆盖、或从云端恢复后索引未落盘）：
            // 直接补一条，避免"改了但没生效"的静默失败
            None => index.vaults.push(meta.clone()),
        }
        self.write_index(&index)
    }

    pub fn delete(&self, meta: &VaultMeta) -> Result<(), StoreError> {
        let file = self.vault_file(meta);
        if file.exists() {
            fs::remove_file(&file).map_err(io)?;
        }
        let attachments = self.attachments_dir_for(&meta.id);
        delete_recursively(&attachments)?;
        let mut index = self.read_index();
        index.vaults.retain(|m| m.id != meta.id);
        self.write_index(&index)
    }

    pub fn import_encrypted(
        &self,
        bytes: &[u8],
        password: &str,
        new_name: &str,
    ) -> Result<(VaultContainer, VaultMeta), StoreError> {
        let opened = VaultContainer::open(bytes, password)?;
        let mut imported = opened.vault.clone();
        imported.id = new_id();
        let trimmed = new_name.trim();
        if !trimmed.is_empty() {
            imported.name = trimmed.to_string();
        }
        imported.updated_at = now_ms();
        self.create_from_vault(imported, password)
    }

    pub fn import_plain(
        &self,
        vault: Vault,
        password: &str,
        new_name: &str,
    ) -> Result<(VaultContainer, VaultMeta), StoreError> {
        let mut imported = vault;
        imported.id = new_id();
        let trimmed = new_name.trim();
        if !trimmed.is_empty() {
            imported.name = trimmed.to_string();
        }
        imported.updated_at = now_ms();
        self.create_from_vault(imported, password)
    }

    pub fn create_from_vault(
        &self,
        vault: Vault,
        password: &str,
    ) -> Result<(VaultContainer, VaultMeta), StoreError> {
        let container = VaultContainer::create(vault, password);
        self.create_from_container(container)
    }

    fn read_index(&self) -> VaultIndex {
        let Ok(bytes) = fs::read(&self.index_file) else {
            return VaultIndex::default();
        };
        serde_json::from_slice::<VaultIndex>(&bytes).unwrap_or_default()
    }

    fn write_index(&self, index: &VaultIndex) -> Result<(), StoreError> {
        fs::create_dir_all(&self.vaults_dir).map_err(io)?;
        let bytes = serde_json::to_vec(index).map_err(io)?;
        write_atomic(&self.index_file, &bytes).map_err(io)
    }
}

fn delete_recursively(path: &Path) -> Result<(), StoreError> {
    if !path.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(io)? {
        let entry = entry.map_err(io)?;
        let p = entry.path();
        if p.is_dir() {
            delete_recursively(&p)?;
        } else {
            fs::remove_file(&p).map_err(io)?;
        }
    }
    fs::remove_dir(path).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sp-store-test-{tag}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_save_open_list_roundtrip() {
        let dir = temp_dir("roundtrip");
        let store = VaultStore::new(&dir).unwrap();
        let (mut c, meta) = store.create("测试库", "pw-测试!🚀").unwrap();
        assert_eq!(c.vault.name, "测试库");
        assert!(!c.vault.id.is_empty());

        // 修改后保存
        c.vault.items.push(Default::default());
        c.vault.touch();
        store.save(&mut c, &meta).unwrap();

        // 重新打开
        let (c2, meta2) = {
            let c2 = store.open(&meta, "pw-测试!🚀").unwrap();
            (c2, meta.clone())
        };
        assert_eq!(c2.vault.items.len(), 1);
        assert_eq!(c2.vault.name, "测试库");

        // list 只有一项且 name 来自打开时同步
        let metas = store.list().unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].id, meta2.id);
        assert_eq!(metas[0].name, "测试库");
        assert!(metas[0].last_opened_at > 0);

        // 错误口令
        assert!(matches!(
            store.open(&meta, "wrong"),
            Err(StoreError::Container(ContainerError::WrongPassword))
        ));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_adopts_orphan_spv_files() {
        let dir = temp_dir("orphan");
        let store = VaultStore::new(&dir).unwrap();
        // 手工放一个不在 index 里的 .spv
        let orphan_id = "vault-0000-0000-0000-00000000or42";
        let vault_file = dir.join("vaults").join(format!("{orphan_id}.spv"));
        let (_c, meta) = store.create("正规库", "pw").unwrap();
        fs::copy(store.vault_file(&meta), &vault_file).unwrap();
        let metas = store.list().unwrap();
        assert_eq!(metas.len(), 2);
        let adopted = metas.iter().find(|m| m.id == orphan_id).expect("收编孤儿");
        assert_eq!(adopted.name, "密码库");
        assert_eq!(adopted.last_opened_at, 0);
        // 再 list 一次不重复收编
        assert_eq!(store.list().unwrap().len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn delete_removes_file_and_index_entry() {
        let dir = temp_dir("delete");
        let store = VaultStore::new(&dir).unwrap();
        let (_c, meta) = store.create("待删", "pw").unwrap();
        let attachments = store.attachments_dir_for(&meta.id);
        fs::create_dir_all(&attachments).unwrap();
        fs::write(attachments.join("a.bin"), b"x").unwrap();
        store.delete(&meta).unwrap();
        assert!(store.list().unwrap().is_empty());
        assert!(!store.vault_file(&meta).exists());
        assert!(!attachments.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_encrypted_assigns_new_id_and_name() {
        let dir = temp_dir("import");
        let store = VaultStore::new(&dir).unwrap();
        let (_c, meta) = store.create("源库", "pw-src").unwrap();
        let bytes = fs::read(store.vault_file(&meta)).unwrap();
        let (c2, meta2) = store
            .import_encrypted(&bytes, "pw-src", "导入副本")
            .unwrap();
        assert_ne!(meta2.id, meta.id);
        assert_eq!(meta2.name, "导入副本");
        assert_eq!(c2.vault.name, "导入副本");
        // 空名字回落到原名
        let (_c3, meta3) = store.import_encrypted(&bytes, "pw-src", "  ").unwrap();
        assert_eq!(meta3.name, "源库");
        assert_eq!(store.list().unwrap().len(), 3);
        let _ = fs::remove_dir_all(&dir);
    }
}
