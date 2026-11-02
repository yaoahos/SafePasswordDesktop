//! 加密原语，逐条对应 Kotlin core/crypto/Crypto.kt。
//!
//! - PBKDF2-HMAC-SHA256，默认 210,000 迭代（打开文件时以文件头存储值为准）
//! - AES-256-GCM，12 字节随机 IV，128 位 tag，输出 IV||CT||TAG
//! - AAD：包裹 DEK 用 "SICV-WRAP"，payload 用 "SICV-PAYLOAD"，附件用附件 id

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use rand::rngs::OsRng;
use rand::RngCore;
use sha2::Sha256;

/// 与 Kotlin `Crypto.PBKDF2_ITERATIONS` 一致。
pub const PBKDF2_ITERATIONS: i32 = 210_000;
pub const IV_BYTES: usize = 12;
pub const TAG_BYTES: usize = 16;
pub const KEY_BYTES: usize = 32;

pub const WRAP_AAD: &[u8] = b"SICV-WRAP";

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// PBKDF2-HMAC-SHA256。口令按 UTF-8 字节派生（与 JCE PBKDF2WithHmacSHA256 一致，
/// 以真机 .sicv 实测为准，见 docs/core-spec.md 歧义 B）。
pub fn derive_key(password: &str, salt: &[u8], iterations: i32) -> [u8; KEY_BYTES] {
    pbkdf2::pbkdf2_hmac_array::<Sha256, KEY_BYTES>(password.as_bytes(), salt, iterations as u32)
}

/// AES-256-GCM 加密，返回 IV||CT||TAG。
pub fn encrypt(key: &[u8], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("密钥长度无效：{e}"))?;
    let mut iv = [0u8; IV_BYTES];
    OsRng.fill_bytes(&mut iv);
    let ct = cipher
        .encrypt(Nonce::from_slice(&iv), Payload { msg: plaintext, aad })
        .map_err(|_| "加密失败".to_string())?;
    let mut out = Vec::with_capacity(IV_BYTES + ct.len());
    out.extend_from_slice(&iv);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// AES-256-GCM 解密，blob 为 IV||CT||TAG。
pub fn decrypt(key: &[u8], blob: &[u8], aad: &[u8]) -> Result<Vec<u8>, String> {
    if blob.len() < IV_BYTES + TAG_BYTES {
        return Err("密文长度不足".to_string());
    }
    let (iv, ct) = blob.split_at(IV_BYTES);
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("密钥长度无效：{e}"))?;
    cipher
        .decrypt(Nonce::from_slice(iv), Payload { msg: ct, aad })
        .map_err(|_| "解密失败".to_string())
}

pub fn wrap_dek(kek: &[u8], dek: &[u8]) -> Result<Vec<u8>, String> {
    encrypt(kek, dek, WRAP_AAD)
}

pub fn unwrap_dek(kek: &[u8], wrapped: &[u8]) -> Result<[u8; KEY_BYTES], String> {
    let raw = decrypt(kek, wrapped, WRAP_AAD)?;
    let arr: [u8; KEY_BYTES] = raw
        .as_slice()
        .try_into()
        .map_err(|_| "被包裹的密钥长度异常".to_string())?;
    Ok(arr)
}

pub fn to_base64(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn from_base64(text: &str) -> Result<Vec<u8>, String> {
    B64.decode(text).map_err(|e| format!("Base64 解码失败：{e}"))
}

/// 大写 hex，对应 Kotlin `Crypto.toHex`（%02X）。
pub fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02X}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pbkdf2_known_vector() {
        // PBKDF2-HMAC-SHA256(password, salt, 1) = 120fb6c...be17b
        let key = derive_key("password", b"salt", 1);
        assert_eq!(
            to_hex(&key),
            "120FB6CFFCF8B32C43E7225256C4F837A86548C92CCC35480805987CB70BE17B"
        );
    }

    #[test]
    fn encrypt_decrypt_roundtrip_and_layout() {
        let key = [7u8; KEY_BYTES];
        let msg = "口令密码 🔐".as_bytes();
        let blob = encrypt(&key, msg, b"aad").unwrap();
        assert_eq!(blob.len(), IV_BYTES + msg.len() + TAG_BYTES);
        assert_eq!(decrypt(&key, &blob, b"aad").unwrap(), msg);
        // AAD 不匹配必须失败
        assert!(decrypt(&key, &blob, b"bad").is_err());
        // 错误密钥必须失败
        assert!(decrypt(&[8u8; KEY_BYTES], &blob, b"aad").is_err());
        // 篡改密文必须失败
        let mut tampered = blob.clone();
        tampered[IV_BYTES] ^= 0x01;
        assert!(decrypt(&key, &tampered, b"aad").is_err());
    }

    #[test]
    fn wrap_dek_layout_is_60_bytes() {
        let kek = [9u8; KEY_BYTES];
        let dek = [3u8; KEY_BYTES];
        let wrapped = wrap_dek(&kek, &dek).unwrap();
        assert_eq!(wrapped.len(), IV_BYTES + KEY_BYTES + TAG_BYTES);
        assert_eq!(unwrap_dek(&kek, &wrapped).unwrap(), dek);
        assert!(unwrap_dek(&[1u8; KEY_BYTES], &wrapped).is_err());
        // 短于 40 字节的包裹必须失败（.sicv 头部下限 40）
        assert!(unwrap_dek(&kek, &wrapped[..39]).is_err());
    }

    #[test]
    fn base64_and_hex() {
        assert_eq!(to_base64(b"hello"), "aGVsbG8=");
        assert_eq!(from_base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(to_hex(&[0x00, 0x0f, 0xff]), "000FFF");
    }
}
