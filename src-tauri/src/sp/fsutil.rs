//! 原子写，对应 Kotlin core/io/AtomicFiles.kt：
//! 写 tmp → rename（最多 3 次，间隔 30ms*(n+1)）→ 退化 copy+delete 兜底。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn tmp_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}

pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let tmp = tmp_path(path);
    fs::write(&tmp, data)?;

    let mut rename_err = None;
    for attempt in 0..3 {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) => {
                rename_err = Some(e);
                thread::sleep(Duration::from_millis(30 * (attempt as u64 + 1)));
            }
        }
    }

    // 兜底：复制 + 删除 tmp（rename 失败通常因为目标被占用）
    match fs::copy(&tmp, path) {
        Ok(_) => {
            let _ = fs::remove_file(&tmp);
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(rename_err.unwrap_or(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_creates_overwrites_and_leaves_no_tmp() {
        let dir = std::env::temp_dir().join(format!("sp-fsutil-{}", uuid::Uuid::new_v4()));
        let path = dir.join("sub").join("a.sicv");
        write_atomic(&path, b"one").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"one");
        write_atomic(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert!(!tmp_path(&path).exists(), "tmp 文件应已被清理");
        fs::remove_dir_all(&dir).ok();
    }
}
