#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use safepassword::commands::{self, AppState};
use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 数据根目录：<appData>/com.yaoahos.safepassword.desktop
            //   vaults/<id>.spv   保险库（兼容历史 .sicv）
            //   backups/          备份
            //   settings.json      主题/HIBP key/WebDAV 配置
            let dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("."));
            let state = AppState::new(&dir).expect("初始化数据目录失败");
            app.manage(state);
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
            commands::webdav_pull_if_changed,
            commands::webdav_list_cloud,
            commands::webdav_get_vault,
            commands::import_json_items,
            commands::check_update,
            commands::update_download_and_install,
            commands::save_export,
        ])
        .run(tauri::generate_context!())
        .expect("error while running SafePassword");
}
