//! 命令层端到端测试：模拟 UI 的完整调用流。
//!
//! 覆盖本轮新增的胶水逻辑（base64 + Mutex + store/attachment/backup 串联）：
//! 创建 → 加条目 → 附件加/读/删 → 导出 .sicv → 备份/恢复 → 改主密码 → 模拟重启重开 → 导入副本。
//!
//! State 通过 tauri::test 的 mock app 构造（StateManager 的 new/set 是 pub(crate），
//! 外部只能走 `mock_builder().manage(...)`；tauri/test 只开在 dev-dependencies，不影响 release）。

use std::fs;
use std::path::PathBuf;

use base64::Engine;
use safepassword::commands as c;
use safepassword::commands::AppState;
use safepassword::sp::container::VaultContainer;
use safepassword::sp::model::{Field, VaultItem};
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};

fn mock_app(root: &PathBuf) -> App<MockRuntime> {
    mock_builder()
        .manage(AppState::new(root).unwrap())
        .build(mock_context(noop_assets()))
        .unwrap()
}

fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

fn unb64(s: &str) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(s.trim())
        .expect("base64 decode")
}

fn tmp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("sp-cmdflow-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}

/// UI 主流程：建库 → 条目 → 附件 → 导出 .sicv → 备份/恢复 → 改密 → 重启重开 → 导入副本。
#[test]
fn ui_full_flow_roundtrip() {
    let root = tmp_root("flow");
    let app = mock_app(&root);

    // 1. 创建（空密码被拒）
    assert!(
        c::create_vault(app.state::<AppState>(), "t".into(), String::new()).is_err()
    );
    let meta = c::create_vault(
        app.state::<AppState>(),
        "命令层测试库".into(),
        "pw-主密码🚀".into(),
    )
    .unwrap();
    assert_eq!(c::list_vaults(app.state::<AppState>()).unwrap().len(), 1);
    assert!(c::current_vault(app.state::<AppState>()).unwrap().is_some());

    // 2. 加条目
    let mut item = VaultItem::default();
    item.id = "item-e2e-1".to_string();
    item.title = "GitHub".to_string();
    item.fields.push(Field {
        id: "f1".to_string(),
        label: "账号".to_string(),
        value: "alice@example.com".to_string(),
        ..Default::default()
    });
    let v = c::put_item(app.state::<AppState>(), item).unwrap();
    assert_eq!(v.items.len(), 1);

    // 3. 附件：加 → 读回字节比对 → 删（引用 + .sica 文件都消失）
    let payload = b"hello attachment \xE4\xB8\xAD\xE6\x96\x87 \xF0\x9F\x94\x90";
    let v = c::attachment_add(
        app.state::<AppState>(),
        "item-e2e-1".into(),
        "秘密.txt".into(),
        "text/plain".into(),
        b64(payload),
    )
    .unwrap();
    let att = v.items[0].attachments[0].clone();
    assert_eq!(att.name, "秘密.txt");
    let back = unb64(
        &c::attachment_read(app.state::<AppState>(), att.id.clone()).unwrap(),
    );
    assert_eq!(back, payload, "附件读回字节不一致");

    let v = c::attachment_remove(
        app.state::<AppState>(),
        "item-e2e-1".into(),
        att.id.clone(),
    )
    .unwrap();
    assert!(v.items[0].attachments.is_empty());
    assert!(
        !root
            .join("attachments")
            .join(&meta.id)
            .join(format!("{}.sica", att.id))
            .exists(),
        ".sica 文件应已删除"
    );

    // 4. 导出 .sicv → 独立容器打开（SICV 魔数 + 条目还在）
    let sicv = c::export_vault_sicv(app.state::<AppState>()).unwrap();
    let bytes = unb64(&sicv);
    assert_eq!(&bytes[..4], b"SICV");
    let c2 = VaultContainer::open(&bytes, "pw-主密码🚀").unwrap();
    assert_eq!(c2.vault.items.len(), 1);
    assert_eq!(c2.vault.items[0].title, "GitHub");

    // 5. 备份 → 列表 → 恢复（同 id 覆盖）
    // 创建时 maybe_auto_backup 已自动备 1 份；文件名时间戳是秒级精度，
    // 等 >1s 再手动备 1 份，避免同秒同名被 REPLACE 覆盖 → 应共 2 份。
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let info = c::backup_create(app.state::<AppState>()).unwrap();
    assert!(info.path.exists(), "备份文件应存在");
    let backups = c::backup_list(app.state::<AppState>()).unwrap();
    assert_eq!(backups.len(), 2, "1 份来自创建时自动备份 + 1 份手动备份");
    let restored = c::backup_restore(
        app.state::<AppState>(),
        info.path.to_string_lossy().to_string(),
        "pw-主密码🚀".into(),
    )
    .unwrap();
    assert_eq!(restored.items.len(), 1);

    // 6. 改主密码：旧密码被拒 → 改成功 → 旧密码失效、新密码可开
    assert!(
        c::change_master_password(app.state::<AppState>(), "wrong".into(), "new".into()).is_err()
    );
    c::change_master_password(
        app.state::<AppState>(),
        "pw-主密码🚀".into(),
        "新主密码123".into(),
    )
    .unwrap();
    let id = meta.id.clone();
    c::close_vault(app.state::<AppState>()).unwrap();
    assert!(
        c::open_vault(app.state::<AppState>(), id.clone(), "pw-主密码🚀".into()).is_err(),
        "旧主密码应失效"
    );
    let v = c::open_vault(app.state::<AppState>(), id.clone(), "新主密码123".into()).unwrap();
    assert_eq!(v.items.len(), 1);

    // 7. 模拟重启：全新 AppState 打开
    let app2 = mock_app(&root);
    let v2 = c::open_vault(app2.state::<AppState>(), id.clone(), "新主密码123".into()).unwrap();
    assert_eq!(v2.items[0].title, "GitHub");
    assert!(v2.items[0].attachments.is_empty());

    // 8. 导入第 4 步导出的 .sicv（旧口令加密）→ 新 id 新库
    let m2 = c::import_vault_bytes(
        app2.state::<AppState>(),
        sicv,
        "pw-主密码🚀".into(),
        "导入副本".into(),
    )
    .unwrap();
    let v3 = c::current_vault(app2.state::<AppState>()).unwrap().unwrap();
    assert_eq!(v3.items.len(), 1);
    assert_ne!(m2.id, id);

    // 9. 设置持久化
    let mut st = c::get_settings(app2.state::<AppState>()).unwrap();
    st.theme = "dark".to_string();
    c::set_settings(app2.state::<AppState>(), st).unwrap();
    let app3 = mock_app(&root);
    assert_eq!(
        c::get_settings(app3.state::<AppState>()).unwrap().theme,
        "dark"
    );

    let _ = fs::remove_dir_all(&root);
}

/// 导出类命令产出正确格式；生成器/强度/安全报告走通。
#[test]
fn export_and_util_commands() {
    let root = tmp_root("export");
    let app = mock_app(&root);
    c::create_vault(app.state::<AppState>(), "导出库".into(), "pw123456".into()).unwrap();

    let mut item = VaultItem::default();
    item.id = "item-x-1".to_string();
    item.title = "招商银行".to_string();
    item.fields.push(Field {
        id: "f1".to_string(),
        label: "卡号".to_string(),
        value: "6225880137654321".to_string(),
        ..Default::default()
    });
    c::put_item(app.state::<AppState>(), item).unwrap();

    let csv = c::export_csv(app.state::<AppState>()).unwrap();
    assert!(csv.starts_with("标题,"), "CSV 表头不对: {csv}");
    assert!(csv.contains("招商银行"));
    let xml = c::export_safeincloud_xml(app.state::<AppState>()).unwrap();
    assert!(xml.contains("<database"), "SafeInCloud XML 根节点缺失");
    assert!(xml.contains("招商银行"));
    let json = c::export_vault_json(app.state::<AppState>()).unwrap();
    assert!(json.contains("\"items\""));

    // 生成器 / 强度 / 安全报告（无网络）
    let pw = c::generate_password(None);
    assert_eq!(pw.len(), 20);
    let pp = c::generate_passphrase(None, None, None, None);
    assert!(!pp.is_empty());
    let s = c::evaluate_strength("password".to_string());
    assert_eq!(s.score, 0, "常见弱口令应最低分");
    let report = c::security_report(app.state::<AppState>()).unwrap();
    assert_eq!(report.total, 1, "应统计到 1 个有密码的活跃条目");

    let _ = fs::remove_dir_all(&root);
}

/// settings.json 敏感字段 DPAPI 落盘：内存明文、磁盘密文、重启回读一致。
#[test]
fn settings_secrets_are_dpapi_on_disk() {
    let root = tmp_root("dpapi");
    let app = mock_app(&root);

    // 设置 WebDAV 密码 + HIBP key（内存里是明文）
    let mut s = c::get_settings(app.state::<AppState>()).unwrap();
    s.webdav.password = "明文口令🚀".to_string();
    s.hibp_api_key = "hibp-key-123".to_string();
    c::set_settings(app.state::<AppState>(), s).unwrap();

    // 1) 磁盘必须是 dpapi: 密文，明文绝不出现在文件里
    let raw = fs::read_to_string(root.join("settings.json")).unwrap();
    assert!(raw.contains("dpapi:"), "敏感字段应为 DPAPI 密文，实际：{raw}");
    assert!(!raw.contains("明文口令"), "WebDAV 明文口令不得出现在磁盘");
    assert!(!raw.contains("hibp-key-123"), "HIBP key 不得出现在磁盘");

    // 2) 同进程读回仍是明文
    let back = c::get_settings(app.state::<AppState>()).unwrap();
    assert_eq!(back.webdav.password, "明文口令🚀");
    assert_eq!(back.hibp_api_key, "hibp-key-123");

    // 3) 模拟重启：新 AppState 从同一磁盘读，回读一致（旧明文迁移路径由 dpapi 单测覆盖）
    let app2 = mock_app(&root);
    let back2 = c::get_settings(app2.state::<AppState>()).unwrap();
    assert_eq!(back2.webdav.password, "明文口令🚀");
    assert_eq!(back2.hibp_api_key, "hibp-key-123");

    let _ = fs::remove_dir_all(&root);
}
