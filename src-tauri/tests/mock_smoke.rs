//! mock app 构造冒烟：只建 mock_builder().manage(...).build()，不调用任何命令。

use std::path::PathBuf;

use safepassword::commands::AppState;
use tauri::test::{mock_builder, mock_context, noop_assets};

#[test]
fn mock_app_builds() {
    let root: PathBuf = std::env::temp_dir().join(format!("sp-mocksmoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let app = mock_builder()
        .manage(AppState::new(&root).unwrap())
        .build(mock_context(noop_assets()))
        .unwrap();
    drop(app);
    let _ = std::fs::remove_dir_all(&root);
}
