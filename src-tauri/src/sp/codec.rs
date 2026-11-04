//! 对齐 Android VaultCodec.kt：Vault <-> JSON / CSV（含公式注入防护）。
#![allow(dead_code)]

use crate::sp::model::{FieldType, ItemStatus, Label, LabelSpec, Vault, VaultItem};
use crate::sp::model::now_ms;

pub fn to_json(vault: &Vault) -> Result<String, String> {
    serde_json::to_string(vault).map_err(|e| e.to_string())
}

pub fn from_json(text: &str) -> Result<Vault, String> {
    serde_json::from_str(text).map_err(|e| e.to_string())
}

pub fn to_csv(vault: &Vault) -> String {
    let mut builder = String::new();
    // 前六列是与手机端/Excel 对齐的常用列；后三列是给本应用回导用的元数据，
    // 别的工具忽略即可，这样 CSV 往返不再丢标签颜色/排序与字段类型。
    builder.push_str("标题,用户名,密码,网址,备注,标签,标签颜色,标签顺序,附加字段\r\n");
    for item in vault.items.iter().filter(|i| i.status != ItemStatus::Trashed) {
        let username = item
            .fields
            .iter()
            .find(|f| f.label == "用户名" || f.label == "邮箱" || f.label == "账号")
            .map(|f| f.value.clone())
            .unwrap_or_default();
        let password = item.primary_password().unwrap_or_default();
        let labels: Vec<&Label> = item
            .label_ids
            .iter()
            .filter_map(|id| vault.label(id))
            .collect();
        let label_names = labels
            .iter()
            .map(|l| l.name.clone())
            .collect::<Vec<_>>()
            .join("|");
        let label_colors = labels
            .iter()
            .map(|l| format!("{}={}", l.name, LabelSpec::color_hex(l.color)))
            .collect::<Vec<_>>()
            .join("|");
        let label_orders = labels
            .iter()
            .map(|l| format!("{}={}", l.name, l.sort_order))
            .collect::<Vec<_>>()
            .join("|");
        let extra = serde_json::to_string(&item.fields).unwrap_or_default();
        let row = [
            item.title.clone(),
            username,
            password,
            item.url.clone(),
            item.notes.clone(),
            label_names,
            label_colors,
            label_orders,
            extra,
        ];
        builder.push_str(&row.iter().map(|s| escape_csv(s)).collect::<Vec<_>>().join(","));
        builder.push_str("\r\n");
    }
    builder
}

/// 解析 CSV：返回 (条目, 标签信息) —— 标签颜色/排序与字段类型都从扩展列还原。
pub fn items_from_csv(text: &str) -> Vec<(VaultItem, Vec<LabelSpec>)> {
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
    let labels_index = header.iter().position(|h| h == "labels").map(|i| i as i32).unwrap_or(-1);
    let colors_index = header.iter().position(|h| h == "labelcolors").map(|i| i as i32).unwrap_or(-1);
    let orders_index = header.iter().position(|h| h == "labelorders").map(|i| i as i32).unwrap_or(-1);
    let fields_index = header.iter().position(|h| h == "fields").map(|i| i as i32).unwrap_or(-1);
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
        let colors = parse_pairs(&cell(colors_index));
        let orders = parse_pairs(&cell(orders_index));
        let specs: Vec<LabelSpec> = split_labels(&cell(labels_index))
            .into_iter()
            .map(|name| LabelSpec {
                color: colors
                    .iter()
                    .find(|(key, _)| key == &name)
                    .and_then(|(_, value)| LabelSpec::parse_color(value)),
                sort_order: orders
                    .iter()
                    .find(|(key, _)| key == &name)
                    .and_then(|(_, value)| value.trim().parse::<i32>().ok()),
                name,
            })
            .collect();

        let mut fields = deserialize_fields(&cell(fields_index));
        if fields.is_empty() {
            if !username_cell.trim().is_empty() {
                fields.push(crate::sp::model::Field {
                    label: "用户名".to_string(),
                    value: username_cell.clone(),
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
        }
        let now = now_ms();
        out.push((
            VaultItem {
                template_id: "login".to_string(),
                title,
                fields,
                notes: cell(notes_index),
                url: cell(url_index),
                created_at: now,
                updated_at: now,
                ..Default::default()
            },
            specs,
        ));
    }
    out
}

/// `a=1|b=2` → [(a,1),(b,2)]；名字里带 `=` 的部分以最后一个 `=` 为分隔。
fn parse_pairs(text: &str) -> Vec<(String, String)> {
    text.split('|')
        .filter_map(|part| {
            let (key, value) = part.rsplit_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            Some((key.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn split_labels(text: &str) -> Vec<String> {
    text.split('|')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn deserialize_fields(text: &str) -> Vec<crate::sp::model::Field> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    serde_json::from_str::<Vec<crate::sp::model::Field>>(text).unwrap_or_default()
}

fn normalize_header(header: &str) -> String {
    let value = header
        .trim()
        .to_lowercase()
        .replace([' ', '_'], "");
    match value.as_str() {
        "title" | "name" | "标题" | "名称" => "title".to_string(),
        "username" | "user" | "login" | "账号" | "用户名" | "邮箱" => "username".to_string(),
        "password" | "pass" | "密码" => "password".to_string(),
        "url" | "website" | "网址" | "链接" => "url".to_string(),
        "notes" | "note" | "备注" | "注释" => "notes".to_string(),
        "标签" | "标签名" | "labels" | "label" => "labels".to_string(),
        "标签颜色" | "labelcolors" | "labelcolor" => "labelcolors".to_string(),
        "标签顺序" | "标签排序" | "labelorders" | "labelorder" => "labelorders".to_string(),
        "附加字段" | "fields" | "fieldjson" => "fields".to_string(),
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
    use crate::sp::model::{Field, ItemStatus, Label, VaultItem};

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
        assert!(csv.starts_with("标题,用户名,密码,网址,备注,标签,标签颜色,标签顺序,附加字段\r\n"));
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
        let first = &items[0].0;
        assert_eq!(first.title, "GitHub");
        assert_eq!(first.fields.len(), 2);
        assert_eq!(first.fields[0].label, "用户名");
        assert_eq!(first.fields[0].value, "alice@example.com");
        assert_eq!(first.fields[1].label, "密码");
        assert!(first.fields[1].sensitive);
        assert_eq!(first.url, "https://github.com");
        assert_eq!(first.notes, "工作账号");
        // 第二行：无 title → 回落 username(空) → "导入的账号"
        assert_eq!(items[1].0.title, "导入的账号");
        assert_eq!(items[1].0.fields.len(), 1);
        assert_eq!(items[1].0.fields[0].value, "onlypassword");
    }

    #[test]
    fn csv_import_header_variants_and_guard() {
        // 中文表头变体
        let text = "名称,账号,密码\r\n某站,bob,pw1\r\n";
        let items = items_from_csv(text);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].0.title, "某站");
        assert_eq!(items[0].0.fields[0].value, "bob");
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

    #[test]
    fn csv_roundtrip_keeps_label_style_and_field_types() {
        let mut vault = Vault::default();
        let color: i32 = 0xFF33_66CCu32 as i32;
        vault.labels.push(Label {
            id: "l1".to_string(),
            name: "工作".to_string(),
            color,
            sort_order: 3,
        });
        let mut item = VaultItem {
            title: "站点".to_string(),
            url: "https://a.example".to_string(),
            ..Default::default()
        };
        item.status = ItemStatus::Active;
        item.label_ids.push("l1".to_string());
        item.fields.push(Field {
            label: "密码".to_string(),
            value: "pw".to_string(),
            r#type: FieldType::Password,
            sensitive: true,
            ..Default::default()
        });
        item.fields.push(Field {
            label: "秘密".to_string(),
            value: "s3cr3t".to_string(),
            r#type: FieldType::Secret,
            sensitive: true,
            ..Default::default()
        });
        vault.items.push(item);

        let csv = to_csv(&vault);
        let pairs = items_from_csv(&csv);
        assert_eq!(pairs.len(), 1);
        let (parsed, specs) = &pairs[0];
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "工作");
        assert_eq!(specs[0].color, Some(color));
        assert_eq!(specs[0].sort_order, Some(3));
        // 附加字段列还原了全部字段与类型（含秘密字段）
        assert_eq!(parsed.fields.len(), 2);
        assert_eq!(parsed.fields[0].r#type, FieldType::Password);
        assert_eq!(parsed.fields[1].label, "秘密");
        assert_eq!(parsed.fields[1].r#type, FieldType::Secret);
        assert_eq!(parsed.fields[1].value, "s3cr3t");
    }

    #[test]
    fn csv_without_extra_columns_still_imports() {
        // 只认核心列的旧文件（手机端/Excel 手工整理）仍可导入
        let csv = "标题,用户名,密码,标签\r\n站点,bob,pw,工作|私人\r\n";
        let pairs = items_from_csv(csv);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0.fields.len(), 2);
        assert_eq!(pairs[0].1.len(), 2);
        assert_eq!(pairs[0].1[1].name, "私人");
        assert_eq!(pairs[0].1[1].color, None);
    }
}
