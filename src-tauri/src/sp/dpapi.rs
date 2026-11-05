//! Windows DPAPI 敏感字段加密（对齐 Android 端 EncryptedSharedPreferences 的静态保护）。
//!
//! settings.json 里的 WebDAV 密码与 HIBP API Key 落盘前用 DPAPI 加密成 `dpapi:<base64>`；
//! 内存中始终是明文（AppState 读时解码、写时编码），调用方无需感知。
//! 用户把 settings.json 拷到别的机器时密文解不开 → 解码回落为空串，重新输入即可。

use base64::Engine as _;
use std::ffi::c_void;

/// CRYPTPROTECT_UI_FORBIDDEN：不允许弹 UI（服务/后台场景必带）。
const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x1;
/// 固定附加熵：即使密文被拿走，没有本应用的熵串也无法在同机解出（防直接复制字段）。
const ENTROPY: &[u8] = b"com.yaoahos.safepassword.desktop.settings.v1";
/// 描述字符串（会进 DPAPI 元数据，仅诊断用）。
const DESC: &str = "SafePasswordDesktop settings";

#[repr(C)]
struct DataBlob {
    cb_data: u32,
    pb_data: *mut u8,
}

impl DataBlob {
    fn from_slice(v: &[u8]) -> DataBlob {
        DataBlob {
            cb_data: v.len() as u32,
            pb_data: v.as_ptr() as *mut u8,
        }
    }

    /// 拷出数据并 LocalFree 释放系统分配的缓冲。
    unsafe fn take(out: DataBlob) -> Vec<u8> {
        if out.pb_data.is_null() {
            return Vec::new();
        }
        let v = if out.cb_data == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(out.pb_data, out.cb_data as usize).to_vec()
        };
        LocalFree(out.pb_data as *mut c_void);
        v
    }
}

#[link(name = "crypt32")]
extern "system" {
    fn CryptProtectData(
        pDataIn: *const DataBlob,
        szDataDescr: *const u16,
        pOptionalEntropy: *const DataBlob,
        pvReserved: *mut c_void,
        pPromptStruct: *mut c_void,
        dwFlags: u32,
        pDataOut: *mut DataBlob,
    ) -> i32;

    fn CryptUnprotectData(
        pDataIn: *const DataBlob,
        ppszDataDescr: *mut *mut u16,
        pOptionalEntropy: *const DataBlob,
        pvReserved: *mut c_void,
        pPromptStruct: *mut c_void,
        dwFlags: u32,
        pDataOut: *mut DataBlob,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(hMem: *mut c_void) -> *mut c_void;
}

fn last_error() -> String {
    format!("GetLastError={}", std::io::Error::last_os_error())
}

/// 用 DPAPI 加密任意字节。
pub fn protect(plaintext: &[u8]) -> Result<Vec<u8>, String> {
    let descr: Vec<u16> = DESC.encode_utf16().chain(std::iter::once(0)).collect();
    let entropy = DataBlob::from_slice(ENTROPY);
    let input = DataBlob::from_slice(plaintext);
    let mut out = DataBlob {
        cb_data: 0,
        pb_data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            descr.as_ptr(),
            &entropy,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if ok == 0 {
        return Err(format!("DPAPI 加密失败，{}", last_error()));
    }
    Ok(unsafe { DataBlob::take(out) })
}

/// 用 DPAPI 解密（同一 Windows 用户上下文内）。
pub fn unprotect(blob: &[u8]) -> Result<Vec<u8>, String> {
    let entropy = DataBlob::from_slice(ENTROPY);
    let input = DataBlob::from_slice(blob);
    let mut out = DataBlob {
        cb_data: 0,
        pb_data: std::ptr::null_mut(),
    };
    let mut descr: *mut u16 = std::ptr::null_mut();
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            &mut descr,
            &entropy,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
    };
    if !descr.is_null() {
        unsafe { LocalFree(descr as *mut c_void) };
    }
    if ok == 0 {
        return Err(format!("DPAPI 解密失败，{}", last_error()));
    }
    Ok(unsafe { DataBlob::take(out) })
}

/// 设置写盘用：非空 → `dpapi:<base64>`；空串保持空；加密失败回落原文（绝不让保存设置失败）。
pub fn encode_secret(plain: &str) -> String {
    if plain.is_empty() {
        return String::new();
    }
    match protect(plain.as_bytes()) {
        Ok(blob) => format!("dpapi:{}", base64::engine::general_purpose::STANDARD.encode(blob)),
        Err(e) => {
            eprintln!("[dpapi] 加密失败，回落明文写盘：{e}");
            plain.to_string()
        }
    }
}

/// 设置读入用：`dpapi:` 前缀 → 解码（失败/异机 → 空串，让用户重输）；无前缀 → 原文（兼容旧明文 settings.json 迁移）。
pub fn decode_secret(value: &str) -> String {
    let Some(b64) = value.strip_prefix("dpapi:") else {
        return value.to_string();
    };
    match base64::engine::general_purpose::STANDARD.decode(b64.trim()) {
        Ok(blob) => unprotect(&blob)
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .unwrap_or_default(),
        Err(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpapi_roundtrip() {
        let pw = "secretpw-测试🚀";
        let blob = protect(pw.as_bytes()).expect("protect");
        assert_ne!(blob.len(), 0);
        assert_eq!(unprotect(&blob).expect("unprotect"), pw.as_bytes());
    }

    #[test]
    fn encode_uses_prefix_and_decodes_back() {
        let enc = encode_secret("pw123");
        assert!(enc.starts_with("dpapi:"), "应带前缀，得到 {enc}");
        assert_eq!(decode_secret(&enc), "pw123");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(encode_secret(""), "");
        assert_eq!(decode_secret(""), "");
    }

    #[test]
    fn legacy_plaintext_passes_through() {
        // 旧版 settings.json 是明文 → 直接原样用（迁移路径）
        assert_eq!(decode_secret("old-plain-pw"), "old-plain-pw");
    }

    #[test]
    fn corrupt_or_foreign_blob_decodes_to_empty() {
        // 异机密文/损坏值 → 空串（用户重输），不把垃圾当密码
        assert_eq!(decode_secret("dpapi:@@not-valid-base64@@"), "");
        assert_eq!(decode_secret("dpapi:AAAA"), "");
    }
}
