//! 加密保险库容器，逐条对应 Kotlin core/crypto/VaultContainer.kt。
//!
//! 文件结构（大端）：
//! magic "SICV" | version | iterations | salt_len | salt | wrapped_len | wrapped | payload_len | payload
//!
//! 主密码只用于派生 KEK；真正加密数据的是随机 DEK。修改主密码只需重新包裹 DEK。

use crate::sp::crypto::{
    ct_eq, decrypt, derive_key, encrypt, random_bytes, to_base64, unwrap_dek, wipe, wrap_dek,
    PBKDF2_ITERATIONS, KEY_BYTES,
};
use crate::sp::fsutil::write_atomic;
use crate::sp::model::Vault;
use std::path::Path;

const MAGIC: &[u8; 4] = b"SICV";
const VERSION: i32 = 1;
const SALT_BYTES: usize = 16;
const MIN_ITERATIONS: i32 = 10_000;
const MAX_ITERATIONS: i32 = 10_000_000;
/// 单个库文件的密文上限，防止伪造头部把内存撑爆。
const MAX_PAYLOAD_BYTES: i32 = 64 * 1024 * 1024;
const PAYLOAD_AAD: &[u8] = b"SICV-PAYLOAD";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContainerError {
    #[error("主密码错误，或数据库文件已损坏")]
    WrongPassword,
    #[error("{0}")]
    Format(String),
}

#[derive(Debug)]
pub struct VaultContainer {
    pub vault: Vault,
    pub dek: [u8; KEY_BYTES],
    salt: Vec<u8>,
    wrapped_dek: Vec<u8>,
    iterations: i32,
}

struct Header {
    iterations: i32,
    salt: Vec<u8>,
    wrapped: Vec<u8>,
    payload: Vec<u8>,
}

/// 对应 DataInputStream：大端读取 + available() 检查。
/// EOF 的错误消息为 "null"（Kotlin `e.message` 为 null 时字符串插值的结果）。
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Reader { bytes, pos: 0 }
    }

    fn available(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.available() < n {
            return Err("null".to_string());
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn i32(&mut self) -> Result<i32, String> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// 读取“长度 + 内容”字段：先校验长度范围与剩余可读字节数，再分配。
    fn chunk(&mut self, min: i32, max: i32, name: &str) -> Result<Vec<u8>, String> {
        let size = self.i32()?;
        if size < min || size > max {
            return Err(format!("{name} 长度异常"));
        }
        if size as usize > self.available() {
            return Err(format!("{name} 长度超出文件实际大小"));
        }
        Ok(self.take(size as usize)?.to_vec())
    }
}

fn read_header(bytes: &[u8]) -> Result<Header, String> {
    let mut r = Reader::new(bytes);
    let magic = r.take(4)?;
    if magic != MAGIC {
        return Err("不是有效的保险库文件".to_string());
    }
    let version = r.i32()?;
    if version != VERSION {
        return Err(format!("不支持的数据库版本：{version}"));
    }
    let iterations = r.i32()?;
    if !(MIN_ITERATIONS..=MAX_ITERATIONS).contains(&iterations) {
        return Err("PBKDF2 迭代次数异常".to_string());
    }
    let salt = r.chunk(8, 64, "salt")?;
    let wrapped = r.chunk(40, 256, "密钥包裹")?;
    let payload = r.chunk(1, MAX_PAYLOAD_BYTES, "密文")?;
    Ok(Header {
        iterations,
        salt,
        wrapped,
        payload,
    })
}

fn ensure_content_key(vault: &mut Vault) {
    if vault.content_key.trim().is_empty() {
        vault.content_key = to_base64(&random_bytes(32));
    }
}

impl VaultContainer {
    pub fn create(mut vault: Vault, password: &str) -> Self {
        let salt = random_bytes(SALT_BYTES);
        let mut kek = derive_key(password, &salt, PBKDF2_ITERATIONS);
        let dek = random_bytes(KEY_BYTES);
        let mut dek_arr = [0u8; KEY_BYTES];
        dek_arr.copy_from_slice(&dek);
        let wrapped = wrap_dek(&kek, &dek_arr).expect("AES-GCM wrap 不会失败");
        // KEK 只在包裹 DEK 时用一次，用完立刻擦掉，避免长期驻留内存
        wipe(&mut kek);
        ensure_content_key(&mut vault);
        VaultContainer {
            vault,
            dek: dek_arr,
            salt,
            wrapped_dek: wrapped,
            iterations: PBKDF2_ITERATIONS,
        }
    }

    pub fn open(bytes: &[u8], password: &str) -> Result<Self, ContainerError> {
        let header = read_header(bytes)
            .map_err(|e| ContainerError::Format(format!("无法读取数据库文件：{e}")))?;
        let mut kek = derive_key(password, &header.salt, header.iterations);
        let dek = unwrap_dek(&kek, &header.wrapped).map_err(|_| ContainerError::WrongPassword)?;
        wipe(&mut kek);
        let mut plain =
            decrypt(&dek, &header.payload, PAYLOAD_AAD).map_err(|_| ContainerError::WrongPassword)?;
        // Kotlin: plain.toString(Charsets.UTF_8) 再解析 —— 非法 UTF-8 会替换成 U+FFFD
        let parsed = {
            let text = String::from_utf8_lossy(&plain);
            serde_json::from_str::<Vault>(&text)
        };
        // 明文 JSON 里全是条目内容，解析完立即擦除
        wipe(&mut plain);
        let mut vault =
            parsed.map_err(|e| ContainerError::Format(format!("数据库内容无法解析：{e}")))?;
        ensure_content_key(&mut vault);
        Ok(VaultContainer {
            vault,
            dek,
            salt: header.salt,
            wrapped_dek: header.wrapped,
            iterations: header.iterations,
        })
    }

    pub fn to_bytes(&mut self) -> Vec<u8> {
        ensure_content_key(&mut self.vault);
        let json = serde_json::to_vec(&self.vault).expect("Vault 序列化不会失败");
        let payload = encrypt(&self.dek, &json, PAYLOAD_AAD).expect("AES-GCM 加密不会失败");
        let mut out = Vec::with_capacity(
            16 + self.salt.len() + 4 + self.wrapped_dek.len() + 4 + payload.len(),
        );
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_be_bytes());
        out.extend_from_slice(&self.iterations.to_be_bytes());
        out.extend_from_slice(&(self.salt.len() as i32).to_be_bytes());
        out.extend_from_slice(&self.salt);
        out.extend_from_slice(&(self.wrapped_dek.len() as i32).to_be_bytes());
        out.extend_from_slice(&self.wrapped_dek);
        out.extend_from_slice(&(payload.len() as i32).to_be_bytes());
        out.extend_from_slice(&payload);
        out
    }

    pub fn save_to(&mut self, path: &Path) -> Result<(), std::io::Error> {
        let bytes = self.to_bytes();
        write_atomic(path, &bytes)
    }

    /// 修改主密码：生成新 salt，用当前迭代次数重新包裹同一个 DEK（payload 不动）。
    pub fn change_password(&self, new_password: &str) -> Self {
        let salt = random_bytes(SALT_BYTES);
        let mut kek = derive_key(new_password, &salt, self.iterations);
        let wrapped = wrap_dek(&kek, &self.dek).expect("AES-GCM wrap 不会失败");
        wipe(&mut kek);
        VaultContainer {
            vault: self.vault.clone(),
            dek: self.dek,
            salt,
            wrapped_dek: wrapped,
            iterations: self.iterations,
        }
    }

    pub fn with_vault(&self, new_vault: Vault) -> Self {
        VaultContainer {
            vault: new_vault,
            dek: self.dek,
            salt: self.salt.clone(),
            wrapped_dek: self.wrapped_dek.clone(),
            iterations: self.iterations,
        }
    }

    pub fn ensure_content_key(&mut self) {
        ensure_content_key(&mut self.vault);
    }

    pub fn password_correct(&self, password: &str) -> bool {
        let mut kek = derive_key(password, &self.salt, self.iterations);
        let ok = match unwrap_dek(&kek, &self.wrapped_dek) {
            // 常数时间比较，避免比较过程泄露 DEK 前缀信息
            Ok(mut d) => {
                let same = ct_eq(&d, &self.dek);
                wipe(&mut d);
                same
            }
            Err(_) => false,
        };
        wipe(&mut kek);
        ok
    }

    /// 附件等内容密钥（base64 32 字节），contentKey 为空时先生成。
    pub fn content_key(&mut self) -> Result<Vec<u8>, String> {
        ensure_content_key(&mut self.vault);
        crate::sp::crypto::from_base64(&self.vault.content_key)
    }
}

impl Drop for VaultContainer {
    fn drop(&mut self) {
        // 关闭/切换库时擦掉 DEK：这把钥匙能解开该库的全部密文，不该长期留在进程堆里
        wipe(&mut self.dek);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sp::crypto::IV_BYTES;
    use crate::sp::model::{FieldType, Field, VaultItem};

    fn sample_vault() -> Vault {
        let mut vault = Vault {
            name: "测试密码库".to_string(),
            ..Vault::default()
        };
        let mut item = VaultItem {
            title: "招商银行".to_string(),
            ..VaultItem::default()
        };
        item.fields = vec![
            Field {
                label: "账号".to_string(),
                value: "user@example.com".to_string(),
                ..Field::default()
            },
            Field {
                label: "密码".to_string(),
                value: "p@ssw0rd".to_string(),
                r#type: FieldType::Password,
                sensitive: true,
                ..Field::default()
            },
        ];
        vault.items.push(item);
        vault
    }

    #[test]
    fn create_open_roundtrip() {
        let mut c = VaultContainer::create(sample_vault(), "口令123");
        let bytes = c.to_bytes();

        // 文件头布局
        assert_eq!(&bytes[0..4], b"SICV");
        assert_eq!(i32::from_be_bytes(bytes[4..8].try_into().unwrap()), 1);
        assert_eq!(
            i32::from_be_bytes(bytes[8..12].try_into().unwrap()),
            PBKDF2_ITERATIONS
        );
        let salt_len = i32::from_be_bytes(bytes[12..16].try_into().unwrap());
        assert_eq!(salt_len, 16);

        let mut c2 = VaultContainer::open(&bytes, "口令123").unwrap();
        assert_eq!(c2.vault.name, "测试密码库");
        assert_eq!(c2.vault.items.len(), 1);
        assert_eq!(c2.vault.items[0].title, "招商银行");
        assert_eq!(c2.vault.items[0].fields.len(), 2);
        assert!(!c2.vault.content_key.trim().is_empty());
        assert_eq!(c2.iterations, PBKDF2_ITERATIONS);
        // 重新序列化能再次打开（往返稳定）
        let bytes2 = c2.to_bytes();
        let c3 = VaultContainer::open(&bytes2, "口令123").unwrap();
        assert_eq!(c3.vault, c2.vault);
    }

    #[test]
    fn wrong_password_maps_to_wrong_password() {
        let mut c = VaultContainer::create(sample_vault(), "正确口令");
        let bytes = c.to_bytes();
        let err = VaultContainer::open(&bytes, "错误口令").unwrap_err();
        assert_eq!(err, ContainerError::WrongPassword);
        assert_eq!(err.to_string(), "主密码错误，或数据库文件已损坏");
    }

    #[test]
    fn header_corruption_messages_match_kotlin() {
        let mut c = VaultContainer::create(sample_vault(), "pw");
        let mut bytes = c.to_bytes();

        // magic 不符
        bytes[0] = b'X';
        let err = VaultContainer::open(&bytes, "pw").unwrap_err();
        assert_eq!(
            err.to_string(),
            "无法读取数据库文件：不是有效的保险库文件"
        );
        bytes[0] = b'S';

        // 版本不支持
        bytes[4..8].copy_from_slice(&2i32.to_be_bytes());
        let err = VaultContainer::open(&bytes, "pw").unwrap_err();
        assert_eq!(err.to_string(), "无法读取数据库文件：不支持的数据库版本：2");
        bytes[4..8].copy_from_slice(&1i32.to_be_bytes());

        // 迭代次数异常
        bytes[8..12].copy_from_slice(&5_000i32.to_be_bytes());
        let err = VaultContainer::open(&bytes, "pw").unwrap_err();
        assert_eq!(err.to_string(), "无法读取数据库文件：PBKDF2 迭代次数异常");
        bytes[8..12].copy_from_slice(&PBKDF2_ITERATIONS.to_be_bytes());

        // 截断（EOF → "null"）
        let err = VaultContainer::open(&bytes[..10], "pw").unwrap_err();
        assert_eq!(err.to_string(), "无法读取数据库文件：null");

        // salt 长度越界
        bytes[12..16].copy_from_slice(&7i32.to_be_bytes());
        let err = VaultContainer::open(&bytes, "pw").unwrap_err();
        assert_eq!(err.to_string(), "无法读取数据库文件：salt 长度异常");
        bytes[12..16].copy_from_slice(&16i32.to_be_bytes());

        // 密文长度声明超出文件实际大小
        let wrapped_len_offset = 16 + 16; // magic+version+iterations+salt_len(16) + salt(16)
        let w_len = i32::from_be_bytes(
            bytes[wrapped_len_offset..wrapped_len_offset + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        let payload_len_offset = wrapped_len_offset + 4 + w_len;
        let total = bytes.len();
        let declared = (total - payload_len_offset - 4 + 1000) as i32;
        bytes[payload_len_offset..payload_len_offset + 4].copy_from_slice(&declared.to_be_bytes());
        let err = VaultContainer::open(&bytes, "pw").unwrap_err();
        assert_eq!(
            err.to_string(),
            "无法读取数据库文件：密文 长度超出文件实际大小"
        );
    }

    #[test]
    fn payload_parse_failure_message() {
        let mut c = VaultContainer::create(sample_vault(), "pw");
        let mut bytes = c.to_bytes();
        // 找到 payload 起点并篡改一个字节 → GCM 解密失败 → WrongPassword（与 Kotlin 一致）
        let w_offset = 16 + 16;
        let w_len = i32::from_be_bytes(bytes[w_offset..w_offset + 4].try_into().unwrap()) as usize;
        let p_offset = w_offset + 4 + w_len;
        let p_len = i32::from_be_bytes(bytes[p_offset..p_offset + 4].try_into().unwrap()) as usize;
        assert!(p_len > IV_BYTES);
        bytes[p_offset + 4 + IV_BYTES] ^= 0x01;
        let err = VaultContainer::open(&bytes, "pw").unwrap_err();
        assert_eq!(err, ContainerError::WrongPassword);
    }

    #[test]
    fn change_password_rewraps_dek() {
        let mut c = VaultContainer::create(sample_vault(), "旧口令");
        let old_bytes = c.to_bytes();
        let mut c2 = c.change_password("新口令");
        let new_bytes = c2.to_bytes();

        // 新口令能开新文件，旧口令不行
        let opened = VaultContainer::open(&new_bytes, "新口令").unwrap();
        assert_eq!(opened.vault.items.len(), 1);
        assert!(matches!(
            VaultContainer::open(&new_bytes, "旧口令"),
            Err(ContainerError::WrongPassword)
        ));
        // 旧文件不受影响（改密码前的文件仍用旧口令）
        assert!(VaultContainer::open(&old_bytes, "旧口令").is_ok());
        // DEK 未变：同一 payload 可以用新容器解开
        assert_eq!(opened.dek, c.dek);
    }

    #[test]
    fn password_correct_check() {
        let mut c = VaultContainer::create(sample_vault(), "pw123");
        let _ = c.to_bytes();
        assert!(c.password_correct("pw123"));
        assert!(!c.password_correct("nope"));
    }

    #[test]
    fn save_to_writes_and_reopens() {
        let dir = std::env::temp_dir().join(format!("sp-container-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v.sicv");
        let mut c = VaultContainer::create(sample_vault(), "pw");
        c.save_to(&path).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let c2 = VaultContainer::open(&bytes, "pw").unwrap();
        assert_eq!(c2.vault.name, "测试密码库");
        // 再次保存覆盖旧文件
        c.save_to(&path).unwrap();
        assert!(std::fs::read(&path).unwrap().len() > 100);
        std::fs::remove_dir_all(&dir).ok();
    }
}
