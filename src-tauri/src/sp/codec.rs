//! 对齐 Android VaultCodec.kt：Vault <-> JSON / CSV（含公式注入防护）。
#![allow(dead_code)]

use crate::sp::model::{FieldType, ItemStatus, Vault, VaultItem};
use crate::sp::model::now_ms;

pub fn to_json(vault: &Vault) -> Result<String, String> {
    serde_json::to_string(vault).map_err(|e| e.to_string())
}

pub fn from_json(text: &str) -> Result<Vault, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

pub fn to_csv(vault: &Vault) -> String {
    let mut builder = String::new();
    builder.push_str("标题,用户名,密码,网址,备注,标签\r\n");
    for item in vault.items.iter().filter(|i| i.status != ItemStatus::Trashed) {
        let username = item
            .fields
            .iter()
            .find(|f| f.label == "用户名" || f.label == "邮箱" || f.label == "账号")
            .map(|f| f.value.clone())
            .unwrap_or_default();
        let password = item.primary_password().unwrap_or_default();
        let labels = item
            .label_ids
            .iter()
            .filter_map(|id| vault.label(id).map(|l| l.name.clone()))
            .collect::<Vec<_>>()
            .join("|");
        let row = [
            item.title.clone(),
            username,
            password,
            item.url.clone(),
            item.notes.clone(),
            labels,
        ];
        builder.push_str(&row.iter().map(|s| escape_csv(s)).collect::<Vec<_>>().join(","));
        builder.push_str("\r\n");
    }
    builder
}

pub fn items_from_csv(text: &str) -> Vec<VaultItem> {
    let rows = parse_csv(text);
    if rows.is_empty() {
        return Vec::new();
    }
    let header: Vec<String> = rows[0].iter().map(|h| normalize_header(h)).collect();
    let title_index = header.iter().position(|h| h == "title").map(|i| i as i32).unwrap_or(-1);
    let username_index = header.iter().position(|h| h == "username").map(|i| i as i32).unwrap_or(-1);
    let password_index = header.iter().position(|h| h == "password").map(|i| i as i32).unwrap_or(-1);
    let url_index = header.iter().position(|h| h == "url").map(|i| i as i32).unwrap_or(-1);
    let notes_index = header.iter().position(|h| h == "notes").map(|i| i as i32).unwrap_or(-1);
    if title_index < 0 && username_index < 0 && password_index < 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for row in rows.iter().skip(1) {
        let cell = |index: i32| -> String {
            if index >= 0 && (index as usize) < row.len() {
                row[index as usize].trim().to_string()
            } else {
                String::new()
            }
        };
        let title_cell = cell(title_index);
        let username_cell = cell(username_index);
        let password_cell = cell(password_index);
        if username_cell.trim().is_empty()
            && password_cell.trim().is_empty()
            && title_cell.trim().is_empty()
        {
            continue;
        }
        let title = if title_cell.trim().is_empty() {
            if username_cell.trim().is_empty() {
                "导入的账号".to_string()
            } else {
                username_cell.clone()
            }
        } else {
            title_cell
        };
        let mut fields = Vec::new();
        if !username_cell.trim().is_empty() {
            fields.push(crate::sp::model::Field {
                label: "用户名".to_string(),
                value: username_cell,
                r#type: FieldType::Text,
                ..Default::default()
            });
        }
        if !password_cell.trim().is_empty() {
            fields.push(crate::sp::model::Field {
                label: "密码".to_string(),
                value: password_cell,
                r#type: FieldType::Password,
                sensitive: true,
                ..Default::default()
            });
        }
        let now = now_ms();
        out.push(VaultItem {
            template_id: "login".to_string(),
            title,
            fields,
            notes: cell(notes_index),
            url: cell(url_index),
            created_at: now,
            updated_at: now,
            ..Default::default()
        });
    }
    out
}

fn normalize_header(header: &str) -> String {
    let value = header
        .trim()
        .to_lowercase()
        .replace(' ', "")
        .replace('_', "");
    match value.as_str() {
        "title" | "name" | "标题" | "名称" => "title".to_string(),
        "username" | "user" | "login" | "账号" | "用户名" | "邮箱" => "username".to_string(),
        "password" | "pass" | "密码" => "password".to_string(),
        "url" | "website" | "网址" | "链接" => "url".to_string(),
        "notes" | "note" | "备注" | "注释" => "notes".to_string(),
        other => other.to_string(),
    }
}

fn escape_csv(value: &str) -> String {
    let text = neutralize_formula(value);
    if text.contains(',') || text.contains('"') || text.contains('\n') || text.contains('\r') {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

/// Excel / WPS 会把以 = + - @ 开头（以及 Tab、CR 开头）的单元格当公式执行，
/// 导出的 CSV 一旦被打开就可能触发外部请求或命令。加单引号前缀强制按文本处理。
fn neutralize_formula(value: &str) -> String {
    let first = match value.chars().next() {
        Some(c) => c,
        None => return value.to_string(),
    };
    if matches!(first, '=' | '+' | '-' | '@' | '\t' | '\r') {
        format!("'{value}")
    } else {
        value.to_string()
    }
}

/// Kotlin VaultCodec.parseCsv 的逐字符状态机。
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => in_quotes = false,
                _ => field.push(ch),
            }
        } else {
            match ch {
                '"' => in_quotes = true,
                ',' => {
                    row.push(std::mem::take(&mut field));
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    if row.iter().any(|c| !c.trim().is_empty()) {
                        rows.push(std::mem::take(&mut row));
                    }
                    row = Vec::new();
                }
                '\r' => {}
                _ => field.push(ch),
            }
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        if row.iter().any(|c| !c.trim().is_empty()) {
            rows.push(row);
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sp::model::{Field, VaultItem};

    #[test]
    fn csv_header_and_escape() {
        let mut vault = Vault::default();
        let mut item = VaultItem {
            title: "普通条目".to_string(),
            url: "https://example.com".to_string(),
            ..Default::default()
        };
        item.fields.push(Field {
            label: "用户名".to_string(),
            value: "alice".to_string(),
            r#type: FieldType::Text,
            ..Default::default()
        });
        item.fields.push(Field {
            label: "密码".to_string(),
            value: "p@ss".to_string(),
            r#type: FieldType::Password,
            sensitive: true,
            ..Default::default()
        });
        vault.items.push(item);
        // 公式前缀 + 逗号
        let mut item2 = VaultItem {
            title: "=HYPERLINK(\"http://evil\")".to_string(),
            notes: "line1\nline2, with comma".to_string(),
            ..Default::default()
        };
        item2.status = ItemStatus::Active;
        vault.items.push(item2);
        // TRASHED 不导出
        vault.items.push(VaultItem {
            title: "已删除".to_string(),
            status: ItemStatus::Trashed,
            ..Default::default()
        });
        let csv = to_csv(&vault);
        assert!(csv.starts_with("标题,用户名,密码,网址,备注,标签\r\n"));
        assert!(csv.contains("普通条目,alice,p@ss,https://example.com,,"));
        // 公式防护：' 前缀 + 引号包裹
        assert!(csv.contains("'=HYPERLINK(\"\"http://evil\"\")"));
        assert!(csv.contains("\"line1\nline2, with comma\""));
        assert!(!csv.contains("已删除"));
        // 每行以 \r\n 结尾
        assert!(csv.ends_with("\r\n"));
    }

    #[test]
    fn csv_import_maps_columns() {
        let text = "标题,用户名,密码,网址,备注\r\n\
                    GitHub,alice@example.com,secret123,https://github.com,工作账号\r\n\
                    ,,onlypassword,,\r\n\
                    \r\n";
        let items = items_from_csv(text);
        assert_eq!(items.len(), 2);
        let first = &items[0];
        assert_eq!(first.title, "GitHub");
        assert_eq!(first.fields.len(), 2);
        assert_eq!(first.fields[0].label, "用户名");
        assert_eq!(first.fields[0].value, "alice@example.com");
        assert_eq!(first.fields[1].label, "密码");
        assert!(first.fields[1].sensitive);
        assert_eq!(first.url, "https://github.com");
        assert_eq!(first.notes, "工作账号");
        // 第二行：无 title → 回落 username(空) → "导入的账号"
        assert_eq!(items[1].title, "导入的账号");
        assert_eq!(items[1].fields.len(), 1);
        assert_eq!(items[1].fields[0].value, "onlypassword");
    }

    #[test]
    fn csv_import_header_variants_and_guard() {
        // 中文表头变体
        let text = "名称,账号,密码\r\n某站,bob,pw1\r\n";
        let items = items_from_csv(text);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "某站");
        assert_eq!(items[0].fields[0].value, "bob");
        // 没有 title/username/password 列 → 空
        let other = "a,b\r\n1,2\r\n";
        assert!(items_from_csv(other).is_empty());
        // 全空行 → 空
        assert!(items_from_csv("").is_empty());
    }

    #[test]
    fn parse_csv_quotes_state_machine() {
        let rows = parse_csv("a,\"b,c\",\"d\"\"e\"\r\n\"multi\nline\",f,g");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["a", "b,c", "d\"e"]);
        assert_eq!(rows[1], vec!["multi\nline", "f", "g"]);
    }
}
