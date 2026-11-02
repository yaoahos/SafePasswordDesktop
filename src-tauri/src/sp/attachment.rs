//! 对齐 Android AttachmentStore.kt：.sica = "SICA" + version(1) + AES-GCM(id 为 AAD)。
#![allow(dead_code)]

use crate::sp::crypto;
use crate::sp::fsutil::write_atomic;
use crate::sp::model::{new_id, AttachmentRef};
use std::fs;
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 4] = b"SICA";
const VERSION: u8 = 1;
const DEFAULT_MAX_BYTES: u64 = 10 * 1024 * 1024;

pub struct AttachmentStore {
    root: PathBuf,
    content_key: Vec<u8>,
}

impl AttachmentStore {
    pub fn new(root: &Path, content_key: Vec<u8>) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| e.to_string())?;
        Ok(AttachmentStore {
            root: root.to_path_buf(),
            content_key,
        })
    }

    pub fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.sica"))
    }

    pub fn exists(&self, id: &str) -> bool {
        self.path_for(id).exists()
    }

    pub fn add(
        &self,
        bytes: &[u8],
        name: &str,
        mime: &str,
    ) -> Result<AttachmentRef, String> {
        self.add_limited(bytes, name, mime, DEFAULT_MAX_BYTES)
    }

    pub fn add_limited(
        &self,
        bytes: &[u8],
        name: &str,
        mime: &str,
        max_bytes: u64,
    ) -> Result<AttachmentRef, String> {
        if bytes.len() as u64 > max_bytes {
            return Err(format!(
                "附件不能超过 {} MB",
                max_bytes / 1024 / 1024
            ));
        }
        let id = new_id();
        let encrypted = crypto::encrypt(&self.content_key, bytes, id.as_bytes())?;
        let mut out = Vec::with_capacity(MAGIC.len() + 1 + encrypted.len());
        out.extend_from_slice(MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&encrypted);
        write_atomic(&self.path_for(&id), &out).map_err(|e| e.to_string())?;
        Ok(AttachmentRef {
            id,
            name: name.to_string(),
            mime: mime.to_string(),
            size: bytes.len() as i64,
            added_at: crate::sp::model::now_ms(),
        })
    }

    pub fn read(&self, r#ref: &AttachmentRef) -> Result<Vec<u8>, String> {
        let bytes = fs::read(self.path_for(&r#ref.id)).map_err(|e| e.to_string())?;
        if bytes.len() < MAGIC.len() + 1 || &bytes[..MAGIC.len()] != MAGIC {
            return Err("附件格式无效".to_string());
        }
        let version = bytes[MAGIC.len()];
        if version != VERSION {
            return Err("不支持的附件版本".to_string());
        }
        crypto::decrypt(&self.content_key, &bytes[MAGIC.len() + 1..], r#ref.id.as_bytes())
    }

    pub fn read_raw(&self, id: &str) -> Result<Option<Vec<u8>>, String> {
        if self.exists(id) {
            fs::read(self.path_for(id))
                .map(Some)
                .map_err(|e| e.to_string())
        } else {
            Ok(None)
        }
    }

    pub fn write_raw(&self, id: &str, bytes: &[u8]) -> Result<(), String> {
        write_atomic(&self.path_for(id), bytes).map_err(|e| e.to_string())
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        let path = self.path_for(id);
        if path.exists() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(tag: &str) -> (AttachmentStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("sp-attach-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let key: Vec<u8> = (0u8..32).collect();
        (AttachmentStore::new(&dir, key).unwrap(), dir)
    }

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let (s, dir) = store("rt");
        let payload = b"secret attachment \xE5\x86\x85\xE5\xAE\x99";
        let r = s.add(payload, "秘密.txt", "text/plain").unwrap();
        assert_eq!(r.size, payload.len() as i64);
        assert_eq!(r.mime, "text/plain");
        assert!(s.exists(&r.id));
        let out = s.read(&r).unwrap();
        assert_eq!(out, payload);
        // 文件确实加密（不包含明文）
        let raw = fs::read(s.path_for(&r.id)).unwrap();
        assert_eq!(&raw[..5], b"SICA\x01");
        assert!(!raw.windows(payload.len()).any(|w| w == payload));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_corrupt_magic_and_version() {
        let (s, dir) = store("corrupt");
        let r = s.add(b"data", "f.bin", "application/octet-stream").unwrap();
        // 篡改 magic
        let path = s.path_for(&r.id);
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] = b'X';
        fs::write(&path, &bytes).unwrap();
        assert_eq!(s.read(&r).unwrap_err(), "附件格式无效");
        // 篡改 version
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] = b'S';
        bytes[4] = 9;
        fs::write(&path, &bytes).unwrap();
        assert_eq!(s.read(&r).unwrap_err(), "不支持的附件版本");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn aad_is_id_so_cross_read_fails() {
        let (s, dir) = store("aad");
        let r = s.add(b"data", "f.bin", "application/octet-stream").unwrap();
        // 用另一个 id 造 ref 去读 → GCM 失败
        let mut other = r.clone();
        other.id = crate::sp::model::new_id();
        // 复制文件到新 id 路径再读
        fs::copy(s.path_for(&r.id), s.path_for(&other.id)).unwrap();
        assert!(s.read(&other).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn size_limit_message() {
        let (s, dir) = store("limit");
        let err = s
            .add_limited(&vec![0u8; 11 * 1024 * 1024], "big.bin", "application/octet-stream", 10 * 1024 * 1024)
            .unwrap_err();
        assert_eq!(err, "附件不能超过 10 MB");
        let _ = fs::remove_dir_all(&dir);
    }
}
