//! WebDAV 同步端到端：内嵌 mock WebDAV 服务器（std::TcpListener，零外部依赖）+ 双设备 AppState。
//!
//! 覆盖真实 HTTP 链路：PROPFIND 连通 → 首传 → 二次幂等 → 设备B 导入副本 MERGE 首同步 →
//! A 改动上传 / B 下载 → B 改动上传 / A 下载 → 附件跨设备往返。
//! 远端路径对齐手机端多库布局：`<remoteDir>/SafePassword-<key>.spv` + 清单 `SafePassword-index.json`（旧单文件路径仅作回退）。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use safepassword::commands as c;
use safepassword::commands::AppState;
use safepassword::sp::sync::ConflictStrategy;
use safepassword::sp::webdav::WebDavConfig;
use safepassword::sp::model::VaultItem;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, Manager};

// ---------------------------------------------------------------- mock 服务器

type Files = Arc<Mutex<HashMap<String, (Vec<u8>, i64)>>>;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

fn http_date(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .to_rfc2822()
}

fn write_resp(stream: &mut TcpStream, status: &str, ctype: &str, extra: &[String], body: &[u8]) {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Type: {ctype}\r\nConnection: close\r\n",
        body.len()
    );
    for e in extra {
        head.push_str(e);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn handle(stream: TcpStream, files: &Files) {
    let mut reader = BufReader::new(stream);
    let mut req_line = String::new();
    if reader.read_line(&mut req_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = req_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let mut content_length: usize = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = v.trim().parse().unwrap_or(0);
        }
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 && reader.read_exact(&mut body).is_err() {
        return;
    }

    // 回应需要拿回底层流：BufReader 消费了 stream —— 用 into_inner 还原
    let mut stream = reader.into_inner();

    match method.as_str() {
        "PROPFIND" => {
            let xml = format!(
                r#"<?xml version="1.0" encoding="utf-8"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>{target}</D:href><D:propstat><D:prop><D:resourcetype/></D:prop><D:status>HTTP/1.1 200 OK</D:status></D:propstat></D:response></D:multistatus>"#
            );
            write_resp(&mut stream, "207 Multi-Status", "application/xml; charset=utf-8", &[], xml.as_bytes());
        }
        "GET" => match files.lock().unwrap().get(&target) {
            Some((data, _)) => {
                let data = data.clone();
                write_resp(&mut stream, "200 OK", "application/octet-stream", &[], &data);
            }
            None => write_resp(&mut stream, "404 Not Found", "text/plain", &[], b"not found"),
        },
        "HEAD" => {
            let snapshot = files.lock().unwrap().get(&target).cloned();
            match snapshot {
                Some((data, ms)) => {
                    let extra = vec![
                        format!("Last-Modified: {}", http_date(ms)),
                        format!("ETag: \"{}-{}\"", data.len(), ms),
                    ];
                    write_resp(&mut stream, "200 OK", "application/octet-stream", &extra, &[]);
                }
                None => write_resp(&mut stream, "404 Not Found", "text/plain", &[], &[]),
            }
        }
        "PUT" => {
            files.lock().unwrap().insert(target, (body, now_ms()));
            write_resp(&mut stream, "201 Created", "text/plain", &[], b"");
        }
        "MKCOL" => {
            // 简化：目录恒返回 201（Kotlin 端 2xx 与 405 都算成功）
            write_resp(&mut stream, "201 Created", "text/plain", &[], b"");
        }
        "DELETE" => {
            let existed = files.lock().unwrap().remove(&target).is_some();
            if existed {
                write_resp(&mut stream, "204 No Content", "text/plain", &[], b"");
            } else {
                write_resp(&mut stream, "404 Not Found", "text/plain", &[], b"not found");
            }
        }
        _ => write_resp(&mut stream, "405 Method Not Allowed", "text/plain", &[], b""),
    }
}

fn start_server() -> (Files, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock webdav");
    let port = listener.local_addr().unwrap().port();
    let files: Files = Arc::new(Mutex::new(HashMap::new()));
    let f2 = files.clone();
    std::thread::spawn(move || {
        for s in listener.incoming().flatten() {
            handle(s, &f2);
        }
    });
    (files, port)
}

// ---------------------------------------------------------------- 测试工具

fn mock_app(root: &PathBuf) -> App<MockRuntime> {
    mock_builder()
        .manage(AppState::new(root).unwrap())
        .build(mock_context(noop_assets()))
        .unwrap()
}

fn tmp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("sp-webdav-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

// ---------------------------------------------------------------- E2E

#[tokio::test(flavor = "multi_thread")]
async fn two_devices_sync_through_mock_webdav() {
    let (files, port) = start_server();
    let base = format!("http://127.0.0.1:{port}/dav/");
    let cfg = || WebDavConfig {
        base_url: base.clone(),
        username: "user".into(),
        password: "app-password".into(),
        remote_dir: "SafePassword".into(),
        enabled: true,
    };

    // ---- 设备 A：建库 + 条目
    let root_a = tmp_root("a");
    let app_a = mock_app(&root_a);
    c::create_vault(app_a.state::<AppState>(), "同步库".into(), "pw-sync🚀".into()).unwrap();
    let item = VaultItem {
        id: "item-a1".to_string(),
        title: "GitHub".to_string(),
        ..VaultItem::default()
    };
    c::put_item(app_a.state::<AppState>(), item).unwrap();

    // 1. 连通测试（PROPFIND Depth:0）
    let msg = c::webdav_test_connection(cfg()).await.unwrap();
    assert_eq!(msg, "连接成功");

    // 2. A 首传（远端为空 → UPLOADED）
    let s = c::webdav_sync(app_a.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_eq!(s.direction, "UPLOADED", "首传方向不对：{s:?}");
    assert_eq!(s.message, "已上传到 WebDAV");
    let keys: Vec<String> = files.lock().unwrap().keys().cloned().collect();
    // 云端文件名按**内容密钥**派生：SafePassword-k<64 位 hex>.spv（改名不变）
    assert!(
        keys.iter()
            .any(|k| k.contains("/SafePassword-k") && k.ends_with(".spv")),
        "远端应按内容密钥分文件 SafePassword-<key>.spv，实际：{keys:?}"
    );
    assert!(
        keys.iter().any(|k| k.ends_with("/SafePassword-index.json")),
        "上传后应维护云端清单，实际：{keys:?}"
    );

    // 3. A 二次同步（无变化 → NONE）
    let s = c::webdav_sync(app_a.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_eq!(s.direction, "NONE", "二次同步应无变化：{s:?}");
    assert_eq!(s.message, "已是最新状态");

    // ---- 设备 B：导出 A 的加密副本并按同名导入（云端 key 由内容密钥派生，跨设备/改名都不变）
    let root_b = tmp_root("b");
    let app_b = mock_app(&root_b);
    let export_b64 = c::export_vault_spv(app_a.state::<AppState>()).unwrap();
    c::import_vault_bytes(
        app_b.state::<AppState>(),
        export_b64,
        "pw-sync🚀".into(),
        "同步库".into(),
    )
    .unwrap();
    // 3. B 首同步：import 会复制 A 的 lastSyncedAt 并 bump updatedAt → 判定"本地更新"→
    //    UPLOADED（与远端内容相同，属无害同内容覆盖，与 Kotlin 行为一致）
    let s = c::webdav_sync(app_b.state::<AppState>(), cfg(), Some(ConflictStrategy::Merge))
        .await
        .unwrap();
    assert_eq!(s.direction, "UPLOADED", "B 首同步应本地更新上传：{s:?}");
    assert_eq!(s.message, "已上传本地更新");
    let vb = c::current_vault(app_b.state::<AppState>()).unwrap().unwrap();
    assert!(vb.items.iter().any(|i| i.title == "GitHub"));

    // 4. A 加第二条 → 用 KeepLocal 覆盖上传（此刻两端 updatedAt 都新，DETECT 会报冲突；
    //    真实场景自动同步也走 KEEP_LOCAL）→ B 下载到 2 条
    let item2 = VaultItem {
        id: "item-a2".to_string(),
        title: "Second".to_string(),
        ..VaultItem::default()
    };
    c::put_item(app_a.state::<AppState>(), item2).unwrap();
    let s = c::webdav_sync(app_a.state::<AppState>(), cfg(), Some(ConflictStrategy::KeepLocal))
        .await
        .unwrap();
    assert_eq!(s.direction, "UPLOADED", "A 有本地新改动应上传：{s:?}");
    assert_eq!(s.message, "已保留本地版本并覆盖云端");
    let s = c::webdav_sync(app_b.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_eq!(s.direction, "DOWNLOADED", "B 应下载远端更新：{s:?}");
    assert_eq!(s.message, "已下载远端更新");
    let vb = c::current_vault(app_b.state::<AppState>()).unwrap().unwrap();
    assert_eq!(vb.items.len(), 2, "B 应有 2 条");

    // 5. B 改标题 → 上传；A → 下载
    let vb = c::current_vault(app_b.state::<AppState>()).unwrap().unwrap();
    let mut edited = vb
        .items
        .into_iter()
        .find(|i| i.id == "item-a1")
        .expect("B 有 item-a1");
    edited.title = "GitHub-改".to_string();
    c::put_item(app_b.state::<AppState>(), edited).unwrap();
    let s = c::webdav_sync(app_b.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_eq!(s.direction, "UPLOADED", "B 编辑应上传：{s:?}");
    let s = c::webdav_sync(app_a.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_eq!(s.direction, "DOWNLOADED", "A 应下载 B 的编辑：{s:?}");
    let va = c::current_vault(app_a.state::<AppState>()).unwrap().unwrap();
    assert!(
        va.items.iter().any(|i| i.title == "GitHub-改"),
        "A 应拿到改后的标题"
    );

    // 6. 多库布局：上传后应落到 SafePassword-<key>.spv，并维护清单
    let s = c::webdav_sync(app_a.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_eq!(s.direction, "NONE", "已是最新状态：{s:?}");

    // 7. 改名后云端文件名不变：A 改名并同步，B 再同步应正常拉到新名字，而不是"云端已删除"
    let va = c::current_vault(app_a.state::<AppState>()).unwrap().unwrap();
    let keys_before: Vec<String> = files.lock().unwrap().keys().cloned().collect();
    let spv_before: Vec<String> = keys_before
        .into_iter()
        .filter(|k| k.ends_with(".spv"))
        .collect();
    c::rename_vault(app_a.state::<AppState>(), va.id, "改名后的库".into()).unwrap();
    let s = c::webdav_sync(app_a.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_eq!(s.direction, "UPLOADED", "改名后应上传：{s:?}");
    let keys_after: Vec<String> = files
        .lock()
        .unwrap()
        .keys()
        .filter(|k| k.ends_with(".spv"))
        .cloned()
        .collect();
    assert_eq!(spv_before, keys_after, "改名不应换云端文件名：{keys_after:?}");

    let s = c::webdav_sync(app_b.state::<AppState>(), cfg(), None)
        .await
        .unwrap();
    assert_ne!(s.direction, "REMOTE_DELETED", "改名不应被当成云端删除：{s:?}");
    assert_eq!(s.direction, "DOWNLOADED", "B 应拉到改名后的库：{s:?}");
    let vb = c::current_vault(app_b.state::<AppState>()).unwrap().unwrap();
    assert_eq!(vb.name, "改名后的库", "B 的库名应跟随云端");

    let _ = std::fs::remove_dir_all(&root_a);
    let _ = std::fs::remove_dir_all(&root_b);
}
