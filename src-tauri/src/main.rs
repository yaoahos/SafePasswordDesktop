#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use safepassword::commands::{self, AppState};
use safepassword::runtime;
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 数据根目录：<安装目录>/data（便携语义；不可写时回落旧的应用数据目录）
            //   vaults/<id>.spv   保险库（容器 magic "SPV"，version 2）
            //   backups/          备份
            //   settings.json      主题/HIBP key/WebDAV 配置
            //   window.json        上次窗口位置与尺寸
            let legacy = app.path().app_data_dir().ok();
            let dir = runtime::data_root(legacy.as_deref());
            let state = AppState::new(&dir).expect("初始化数据目录失败");
            app.manage(state);
            runtime::attach_window_state(app.handle(), dir.join("window.json"));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_vaults,
            commands::create_vault,
            commands::open_vault,
            commands::close_vault,
            commands::current_vault,
            commands::delete_vault,
            commands::rename_vault,
            commands::update_vault,
            commands::put_item,
            commands::delete_item,
            commands::delete_field_history,
            commands::change_master_password,
            commands::import_vault_bytes,
            commands::import_vault_json,
            commands::generate_password,
            commands::generate_passphrase,
            commands::evaluate_strength,
            commands::security_report,
            commands::security_report_full,
            commands::check_password_breach,
            commands::check_email_breaches,
            commands::export_csv,
            commands::export_xml,
            commands::export_vault_json,
            commands::export_vault_spv,
            commands::import_csv,
            commands::import_xml,
            commands::backup_create,
            commands::backup_list,
            commands::backup_restore,
            commands::backup_delete,
            commands::get_settings,
            commands::set_settings,
            commands::webdav_test_connection,
            commands::webdav_sync,
            commands::webdav_pull,
            commands::webdav_restore_to_cloud,
            commands::webdav_pull_if_changed,
            commands::webdav_list_cloud,
            commands::webdav_get_vault,
            commands::webdav_delete_cloud,
            commands::cloud_key,
            commands::import_json_items,
            commands::check_update,
            commands::update_download_and_install,
            commands::save_export,
            commands::pick_any_file,
            commands::capture_screen_png,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SafePassword");
}
