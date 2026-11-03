//! 运行时目录与窗口位置记忆。
//!
//! 数据根目录优先放在**安装目录下的 `data/`**（便携语义，卸载时随安装目录一起清掉），
//! 装到 `Program Files` 之类不可写位置时回落到 `%APPDATA%\<identifier>`。
//! 首次从旧目录升级时把 `vaults/`、`backups/`、`settings.json` 等整体搬过去，成功后删除旧目录。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, LogicalSize, Manager, PhysicalPosition, WindowEvent};

/// 旧版本的数据目录名（`%APPDATA%\<identifier>`）。
pub const LEGACY_DIR_NAME: &str = "com.yaoahos.safepassword.desktop";

fn probe_writable(dir: &Path) -> bool {
    let probe = dir.join(".write-probe");
    match fs::write(&probe, b"ok") {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 解析数据根目录：`<exe 所在目录>/data`，不可写时回落到旧的应用数据目录。
pub fn data_root(legacy: Option<&Path>) -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let dir = parent.join("data");
            if fs::create_dir_all(&dir).is_ok() && probe_writable(&dir) {
                if let Some(old) = legacy {
                    migrate_legacy(old, &dir);
                }
                return dir;
            }
        }
    }
    legacy.map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if src.is_dir() {
            copy_tree(&src, &dst)?;
        } else {
            fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

fn count_vaults(dir: &Path) -> usize {
    fs::read_dir(dir.join("vaults"))
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().ends_with(".spv"))
                .count()
        })
        .unwrap_or(0)
}

fn total_bytes(dir: &Path) -> u64 {
    let mut sum = 0u64;
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                sum += total_bytes(&path);
            } else if let Ok(meta) = entry.metadata() {
                sum += meta.len();
            }
        }
    }
    sum
}

/// 旧目录 → 新目录的一次性搬迁；新目录已有库、或拷贝校验不通过时保持原状（宁可不清）。
fn migrate_legacy(old: &Path, new: &Path) {
    if !old.is_dir() || !new.is_dir() {
        return;
    }
    if new.join("vaults").join("index.json").exists() {
        return;
    }
    if copy_tree(old, new).is_err() {
        return;
    }
    // 库文件数量与总体积都对得上才删旧目录，避免拷贝不完整时把数据删掉
    if count_vaults(old) == count_vaults(new) && total_bytes(old) == total_bytes(new) {
        let _ = fs::remove_dir_all(old);
    }
}

/// 窗口位置/尺寸（位置用物理像素、尺寸用逻辑像素，与当前 DPI 下所见一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub maximized: bool,
}

static PENDING: Mutex<Option<WindowState>> = Mutex::new(None);
static SAVED: AtomicBool = AtomicBool::new(false);

fn read_state(path: &Path) -> Option<WindowState> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn write_state(path: &Path, state: &WindowState) {
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, text);
    }
}

/// 记录最新窗口状态，交给后台线程节流落盘（拖拽时会连发几十个事件）。
fn remember(window: &tauri::WebviewWindow, event: &WindowEvent) {
    let maximized = window.is_maximized().unwrap_or(false);
    let scale = window.scale_factor().unwrap_or(1.0);
    let mut guard = match PENDING.lock() {
        Ok(g) => g,
        Err(_) => return,
    };
    let mut state = guard.clone().unwrap_or(WindowState {
        x: 0,
        y: 0,
        width: 1280,
        height: 820,
        maximized,
    });
    match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => {
            if maximized {
                state.maximized = true;
                *guard = Some(state);
                SAVED.store(false, Ordering::Relaxed);
                return;
            }
            if let Ok(size) = window.inner_size() {
                if size.width > 0 && size.height > 0 {
                    state.width = (size.width as f64 / scale).round() as u32;
                    state.height = (size.height as f64 / scale).round() as u32;
                }
            }
            if let Ok(pos) = window.outer_position() {
                state.x = pos.x;
                state.y = pos.y;
            }
            state.maximized = false;
        }
        _ => return,
    }
    *guard = Some(state);
    SAVED.store(false, Ordering::Relaxed);
}

fn position_on_screen(window: &tauri::WebviewWindow, state: &WindowState) -> bool {
    let monitors = match window.available_monitors() {
        Ok(m) if !m.is_empty() => m,
        _ => return true,
    };
    monitors.iter().any(|m| {
        let pos = m.position();
        let size = m.size();
        let left = pos.x;
        let top = pos.y;
        let right = left + size.width as i32;
        let bottom = top + size.height as i32;
        // 至少要有一部分窗口落在该显示器里（留 40px 容错）
        state.x + 40 < right
            && state.x + state.width as i32 > left + 40
            && state.y + 40 < bottom
            && state.y + state.height as i32 > top + 40
    })
}

/// 启动时恢复上次的窗口位置/尺寸；没有记录就交给 `tauri.conf.json` 的 `center: true`。
pub fn attach_window_state(app: &AppHandle, path: PathBuf) {
    let window = match app.get_webview_window("main") {
        Some(w) => w,
        None => return,
    };

    if let Some(saved) = read_state(&path) {
        if saved.width >= 640 && saved.height >= 480 {
            let _ = window.set_size(LogicalSize::new(saved.width as f64, saved.height as f64));
            if position_on_screen(&window, &saved) {
                let _ = window.set_position(PhysicalPosition::new(saved.x, saved.y));
            }
            if saved.maximized {
                let _ = window.maximize();
            }
            if let Ok(mut guard) = PENDING.lock() {
                *guard = Some(WindowState {
                    maximized: saved.maximized,
                    ..saved
                });
            }
        }
    }

    // 记录当前几何作为基线：即使一次都不拖动，也会落一份 window.json
    if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
        let scale = window.scale_factor().unwrap_or(1.0);
        if let Ok(mut guard) = PENDING.lock() {
            *guard = Some(WindowState {
                x: pos.x,
                y: pos.y,
                width: (size.width as f64 / scale).round() as u32,
                height: (size.height as f64 / scale).round() as u32,
                maximized: window.is_maximized().unwrap_or(false),
            });
        }
        SAVED.store(false, Ordering::Relaxed);
    }

    let tracked = window.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::Moved(_) | WindowEvent::Resized(_) => remember(&tracked, event),
        _ => {}
    });

    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(1000));
        if SAVED.load(Ordering::Relaxed) {
            continue;
        }
        let snapshot = PENDING.lock().ok().and_then(|g| g.clone());
        if let Some(state) = snapshot {
            write_state(&path, &state);
            SAVED.store(true, Ordering::Relaxed);
        }
    });
}
