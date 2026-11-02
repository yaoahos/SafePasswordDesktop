//! 对齐 Android BackupManager.kt：原始复制 .sicv，命名 <safeName>-<yyyyMMdd-HHmmss>.sicv。
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: PathBuf,
    pub name: String,
    pub created_at: i64,
    pub size: i64,
}

pub struct BackupManager {
    root: PathBuf,
}

fn io<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

impl BackupManager {
    pub fn new(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(io)?;
        Ok(BackupManager {
            root: root.to_path_buf(),
        })
    }

    pub fn backup(&self, vault_file: &Path, vault_name: &str) -> Result<BackupInfo, String> {
        fs::create_dir_all(&self.root).map_err(io)?;
        let now = chrono::Local::now();
        let stamp = now.format("%Y%m%d-%H%M%S").to_string();
        let safe_name = sanitize_name(vault_name);
        let target = self.root.join(format!("{safe_name}-{stamp}.sicv"));
        fs::copy(vault_file, &target).map_err(io)?;
        info_of(&target)
    }

    pub fn list(&self) -> Result<Vec<BackupInfo>, String> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(io)? {
            let entry = entry.map_err(io)?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("sicv") {
                continue;
            }
            if let Ok(info) = info_of(&path) {
                out.push(info);
            }
        }
        out.sort_by_key(|i| std::cmp::Reverse(i.created_at));
        Ok(out)
    }

    pub fn restore(&self, backup: &Path, target: &Path) -> Result<(), String> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        fs::copy(backup, target).map_err(io)?;
        Ok(())
    }

    pub fn prune(&self, keep: usize) -> Result<(), String> {
        for info in self.list()?.into_iter().skip(keep) {
            let _ = fs::remove_file(&info.path);
        }
        Ok(())
    }

    pub fn delete(&self, info: &BackupInfo) -> Result<(), String> {
        if info.path.exists() {
            fs::remove_file(&info.path).map_err(io)?;
        }
        Ok(())
    }
}

/// Kotlin: replace(Regex("[^\\p{L}\\p{N}_-]"), "_").take(40).ifBlank { "vault" }
fn sanitize_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    let taken: String = replaced.chars().take(40).collect();
    if taken.trim().is_empty() {
        "vault".to_string()
    } else {
        taken
    }
}

fn info_of(path: &Path) -> Result<BackupInfo, String> {
    let meta = fs::metadata(path).map_err(io)?;
    let created_at = meta
        .modified()
        .map_err(io)?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(io)?
        .as_millis() as i64;
    Ok(BackupInfo {
        name: path.file_name().unwrap().to_string_lossy().into_owned(),
        path: path.to_path_buf(),
        created_at,
        size: meta.len() as i64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rules() {
        assert_eq!(sanitize_name("我的密码库"), "我的密码库");
        assert_eq!(sanitize_name("a/b\\c:d*e?f"), "a_b_c_d_e_f");
        // Kotlin: 空白被替换成 '_' 后不是 blank → "___"；只有真空串才回落 "vault"
        assert_eq!(sanitize_name("   "), "___");
        assert_eq!(sanitize_name(""), "vault");
        assert_eq!(sanitize_name("正常名字"), "正常名字");
        // Kotlin take(40) 按字符截断
        let long = "超".repeat(60);
        assert_eq!(sanitize_name(&long).chars().count(), 40);
    }

    #[test]
    fn backup_list_prune_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sp-backup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let bm = BackupManager::new(&dir).unwrap();
        // 源保险库文件必须放在备份目录之外（Kotlin list() 只列 backupsDir/*.sicv）
        let src_dir = std::env::temp_dir().join(format!("sp-backup-src-{}", std::process::id()));
        let _ = fs::remove_dir_all(&src_dir);
        fs::create_dir_all(&src_dir).unwrap();
        let src = src_dir.join("src.sicv");
        fs::write(&src, b"dummy-vault-bytes").unwrap();
        let info = bm.backup(&src, "测试库").unwrap();
        assert!(info.name.starts_with("测试库-"));
        assert!(info.name.ends_with(".sicv"));
        assert_eq!(info.size, 17);
        let listed = bm.list().unwrap();
        assert_eq!(listed.len(), 1);
        // restore
        let target = std::env::temp_dir().join(format!("sp-backup-restore-{}", std::process::id()));
        bm.restore(&info.path, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"dummy-vault-bytes");
        // delete
        bm.delete(&info).unwrap();
        assert!(bm.list().unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&src_dir);
        let _ = fs::remove_file(&target);
    }
}
