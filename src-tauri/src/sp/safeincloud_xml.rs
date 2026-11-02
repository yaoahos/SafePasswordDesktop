//! 对齐 Android SafeInCloudXml.kt：原版 SafeInCloud XML 导出解析/导入。
//!
//! 解析用 quick-xml（不解析外部实体，天然防 XXE）；导出手工序列化以完全
//! 控制属性顺序与转义（Transformer 的输出只要求 XML 合法，不做字节兼容）。
#![allow(dead_code)]

use std::collections::HashMap;

use crate::sp::model::{Field, FieldType, ItemStatus, Vault, VaultItem, now_ms};
use crate::sp::model::TemplateIds;

#[derive(Debug)]
pub struct SafeInCloudCard {
    pub item: VaultItem,
    pub label_names: Vec<String>,
}

/// 与 Kotlin 一致的最小 DOM 节点。
#[derive(Debug, Default)]
struct XmlEl {
    name: String,
    attrs: Vec<(String, String)>,
    text: String,
    children: Vec<XmlEl>,
}

impl XmlEl {
    fn attr(&self, name: &str) -> String {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }
    fn child_els(&self, tag_ci: &str) -> Vec<&XmlEl> {
        self.children
            .iter()
            .filter(|c| c.name.eq_ignore_ascii_case(tag_ci))
            .collect()
    }
    fn first_child(&self, tag_ci: &str) -> Option<&XmlEl> {
        self.children
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(tag_ci))
    }
    fn first_desc(&self, tag_ci: &str) -> Option<&XmlEl> {
        for child in &self.children {
            if child.name.eq_ignore_ascii_case(tag_ci) {
                return Some(child);
            }
            if let Some(found) = child.first_desc(tag_ci) {
                return Some(found);
            }
        }
        None
    }
}

fn is_true(value: &str) -> bool {
    value.eq_ignore_ascii_case("true") || value == "1"
}

pub fn looks_like_xml(text: &str) -> bool {
    text.trim_start_matches(['\u{FEFF}', ' ', '\n', '\r', '\t'])
        .starts_with('<')
}

pub fn parse(text: &str) -> Result<Vec<SafeInCloudCard>, String> {
    let root = parse_document(text)?;
    if !root.name.eq_ignore_ascii_case("database") {
        return Err("不是 SafeInCloud 导出的 XML（根节点应为 database）".to_string());
    }
    let mut labels: HashMap<String, String> = HashMap::new();
    for label in root.child_els("label") {
        let id = label.attr("id").trim().to_string();
        let name = label.attr("name").trim().to_string();
        if id.is_empty() || name.is_empty() {
            continue;
        }
        labels.insert(id, name);
    }
    let mut cards = Vec::new();
    for card in root.child_els("card") {
        if let Some(parsed) = parse_card(card, &labels) {
            cards.push(parsed);
        }
    }
    Ok(cards)
}

fn parse_document(text: &str) -> Result<XmlEl, String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let source = text.trim_start_matches('\u{FEFF}');
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);

    let mut stack: Vec<XmlEl> = Vec::new();
    let mut root: Option<XmlEl> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                stack.push(XmlEl {
                    name,
                    attrs: collect_attrs(&e),
                    ..Default::default()
                });
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                let el = XmlEl {
                    name,
                    attrs: collect_attrs(&e),
                    ..Default::default()
                };
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(el);
                } else {
                    root = Some(el);
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(top) = stack.last_mut() {
                    let raw = String::from_utf8_lossy(t.as_ref()).into_owned();
                    top.text.push_str(&unescape_xml(&raw));
                }
            }
            Ok(Event::CData(c)) => {
                if let Some(top) = stack.last_mut() {
                    let raw = String::from_utf8_lossy(c.as_ref()).into_owned();
                    top.text.push_str(&raw);
                }
            }
            Ok(Event::End(_)) => {
                if let Some(el) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(el);
                    } else {
                        root = Some(el);
                    }
                }
            }
            // Kotlin 关闭了 doctype（disallow-doctype-decl），保持同样拒绝
            Ok(Event::DocType(_)) => {
                return Err("XML 解析失败：不允许 DOCTYPE 声明".to_string());
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(format!("XML 解析失败：{}", e)),
        }
    }

    root.ok_or_else(|| "XML 没有根节点".to_string())
}

fn collect_attrs(e: &quick_xml::events::BytesStart<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for attr in e.attributes().with_checks(false).flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
        let raw = String::from_utf8_lossy(&attr.value).into_owned();
        out.push((key, unescape_xml(&raw)));
    }
    out
}

fn unescape_xml(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        if let Some(semi) = tail.find(';') {
            let entity = &tail[..=semi];
            let replacement: Option<String> = match entity {
                "&amp;" => Some("&".to_string()),
                "&lt;" => Some("<".to_string()),
                "&gt;" => Some(">".to_string()),
                "&quot;" => Some("\"".to_string()),
                "&apos;" => Some("'".to_string()),
                _ => {
                    if entity.starts_with("&#x") || entity.starts_with("&#X") {
                        i32::from_str_radix(&entity[3..entity.len() - 1], 16)
                            .ok()
                            .and_then(|v| char::from_u32(v as u32))
                            .map(|c| c.to_string())
                    } else if entity.starts_with("&#") {
                        entity[2..entity.len() - 1]
                            .parse::<u32>()
                            .ok()
                            .and_then(char::from_u32)
                            .map(|c| c.to_string())
                    } else {
                        None
                    }
                }
            };
            match replacement {
                Some(text) => {
                    out.push_str(&text);
                    rest = &tail[semi + 1..];
                }
                None => {
                    out.push('&');
                    rest = &tail[1..];
                }
            }
        } else {
            out.push('&');
            rest = &tail[1..];
        }
    }
    out.push_str(rest);
    out
}

fn parse_card(card: &XmlEl, labels: &HashMap<String, String>) -> Option<SafeInCloudCard> {
    if is_true(&card.attr("template")) || is_true(&card.attr("deleted")) {
        return None;
    }

    let title_raw = card.attr("title").trim().to_string();
    let title = if title_raw.is_empty() {
        "未命名".to_string()
    } else {
        title_raw
    };

    let mut fields: Vec<Field> = Vec::new();
    let mut url = String::new();
    let mut note_parts: Vec<String> = Vec::new();

    for element in card.child_els("field") {
        let field_type_attr = element.attr("type").trim().to_lowercase();
        let raw_name = element.attr("name").trim().to_string();
        let value = element.text.trim().to_string();

        if raw_name.is_empty() && value.is_empty() && field_type_attr.is_empty() {
            continue;
        }
        let label = safe_label(&field_type_attr, &raw_name, &value);
        if label.trim().is_empty() {
            continue;
        }
        let field_type = safe_type(&field_type_attr, &raw_name);
        if field_type_attr == "notes" {
            if !value.is_empty() {
                note_parts.push(value);
            }
            continue;
        }
        if field_type == FieldType::Url && url.is_empty() && !value.is_empty() {
            url = value.clone();
        }
        fields.push(Field {
            label,
            value,
            r#type: field_type,
            sensitive: matches!(field_type, FieldType::Password | FieldType::Secret),
            ..Default::default()
        });
    }

    let notes_from_element = card
        .first_child("notes")
        .map(|n| n.text.trim().to_string())
        .unwrap_or_default();
    let has_notes_element = !notes_from_element.is_empty();
    if has_notes_element {
        note_parts.insert(0, notes_from_element.clone());
    }

    let mut label_names: Vec<String> = Vec::new();
    for element in card.child_els("label_id") {
        let id = element.text.trim().to_string();
        if id.is_empty() {
            continue;
        }
        let name = labels.get(&id).cloned().unwrap_or_else(|| format!("标签 {id}"));
        if !label_names.contains(&name) {
            label_names.push(name);
        }
    }

    let timestamp = match card.attr("time_stamp").trim().parse::<i64>() {
        // Kotlin: raw in 1..9_999_999_999 → 秒转毫秒
        Ok(raw) if (1..=9_999_999_999).contains(&raw) => raw * 1000,
        Ok(raw) => raw,
        Err(_) => now_ms(),
    };

    let template_id = if fields.iter().any(|f| f.label == "卡号") {
        TemplateIds::BANK_CARD
    } else if fields.iter().any(|f| f.label == "网络名称") {
        TemplateIds::WIFI
    } else if fields.iter().any(|f| f.label == "证件号") {
        TemplateIds::IDENTITY
    } else if fields.is_empty() && has_notes_element {
        TemplateIds::NOTE
    } else {
        TemplateIds::LOGIN
    };

    let custom_icon = card
        .first_desc("custom_icon")
        .map(|n| n.text.trim().to_string())
        .unwrap_or_default();
    let image_node = card
        .first_desc("image")
        .map(|n| n.text.trim().to_string())
        .unwrap_or_default();
    let raw_image = if custom_icon.is_empty() {
        image_node
    } else {
        custom_icon
    };
    let image = strip_base64(&raw_image);
    let symbol = card.attr("symbol").trim().to_string();

    let icon_key = if !image.is_empty() {
        "custom".to_string()
    } else if !symbol.is_empty() {
        symbol
    } else {
        String::new()
    };

    let item = VaultItem {
        template_id: template_id.to_string(),
        title,
        fields,
        notes: note_parts.join("\n\n"),
        url,
        label_ids: Vec::new(),
        favorite: is_true(&card.attr("star")),
        created_at: timestamp,
        updated_at: timestamp,
        icon_key,
        icon_data: image,
        ..Default::default()
    };
    Some(SafeInCloudCard { item, label_names })
}

/// Kotlin: substringAfter("base64,", rawImage).filterNot { it.isWhitespace() }
fn strip_base64(raw: &str) -> String {
    let after = match raw.find("base64,") {
        Some(pos) => &raw[pos + "base64,".len()..],
        None => raw,
    };
    after.chars().filter(|c| !c.is_whitespace()).collect()
}

fn safe_type(type_name: &str, raw_name: &str) -> FieldType {
    match type_name {
        "password" | "cvv" | "pin" => FieldType::Password,
        "secret" => FieldType::Secret,
        "url" | "website" => FieldType::Url,
        "email" | "e-mail" => FieldType::Email,
        "phone" | "tel" => FieldType::Phone,
        "multiline" | "notes" | "note" => FieldType::Multiline,
        "login" | "username" | "user" => {
            if raw_name.to_lowercase().contains("email") || raw_name.contains("邮箱") {
                FieldType::Email
            } else {
                FieldType::Text
            }
        }
        _ => {
            let lower = raw_name.to_lowercase();
            if raw_name.contains("网址")
                || raw_name == "地址"
                || lower.contains("url")
                || lower.contains("website")
            {
                FieldType::Url
            } else {
                FieldType::Text
            }
        }
    }
}

fn safe_label(type_name: &str, raw_name: &str, value: &str) -> String {
    let normalized = raw_name.trim().to_lowercase();
    let generic = |names: &[&str]| normalized.is_empty() || names.iter().any(|n| normalized == *n);

    match type_name {
        "login" | "username" | "user" => {
            if normalized.contains("email") || normalized.contains("邮箱") {
                "邮箱".to_string()
            } else if generic(&["login", "username", "user", "account", "账号", "帐号", "登录名", "登录账号"]) {
                "用户名".to_string()
            } else {
                raw_name.to_string()
            }
        }
        "password" => {
            if generic(&["password", "pass", "pwd", "密码"]) {
                "密码".to_string()
            } else {
                raw_name.to_string()
            }
        }
        "secret" => {
            if generic(&["secret", "秘密"]) {
                "秘密".to_string()
            } else {
                raw_name.to_string()
            }
        }
        "url" | "website" => {
            if generic(&["website", "url", "web site", "网址"]) {
                "网址".to_string()
            } else {
                raw_name.to_string()
            }
        }
        "email" | "e-mail" => "邮箱".to_string(),
        "phone" | "tel" => {
            if generic(&["phone", "tel", "telephone", "电话"]) {
                "电话".to_string()
            } else {
                raw_name.to_string()
            }
        }
        "card_number" => "卡号".to_string(),
        "card_holder" => "持卡人".to_string(),
        "expiry" | "expiration" => "有效期".to_string(),
        "cvv" => "CVV".to_string(),
        "bank" => "银行".to_string(),
        "network_name" | "ssid" => "网络名称".to_string(),
        "identity_number" => "证件号".to_string(),
        "notes" | "note" => "备注".to_string(),
        _ => {
            if !raw_name.is_empty() {
                raw_name.to_string()
            } else if value.is_empty() {
                String::new()
            } else {
                let replaced = type_name.replace('_', " ");
                if replaced.is_empty() {
                    "自定义字段".to_string()
                } else {
                    replaced
                }
            }
        }
    }
}

fn export_type(field: &Field) -> &'static str {
    let label = field.label.to_lowercase();
    if label.contains("密码") {
        "password"
    } else if label.contains("卡号") {
        "card_number"
    } else if label.contains("持卡人") {
        "card_holder"
    } else if label.contains("有效期") {
        "expiry"
    } else if label.contains("cvv") {
        "cvv"
    } else if label.contains("银行") {
        "bank"
    } else if label.contains("网络名称") || label.contains("wifi") {
        "network_name"
    } else if label.contains("证件号") {
        "identity_number"
    } else if label.contains("用户名") || label.contains("账号") || label.contains("帐号") || label.contains("登录") {
        "login"
    } else if label.contains("邮箱") || field.r#type == FieldType::Email {
        "email"
    } else if label.contains("秘密") {
        "secret"
    } else if label.contains("电话") || field.r#type == FieldType::Phone {
        "phone"
    } else if label.contains("网址") || label.contains("url") || label.contains("website") || field.r#type == FieldType::Url {
        "website"
    } else {
        match field.r#type {
            FieldType::Password => "password",
            FieldType::Secret => "secret",
            FieldType::Email => "email",
            FieldType::Phone => "phone",
            FieldType::Url => "website",
            _ => "text",
        }
    }
}

fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_attr(text: &str) -> String {
    escape_text(text)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// 导出活动条目为 SafeInCloud XML（声明 + 缩进 2 空格）。
pub fn export(vault: &Vault) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n");
    out.push_str("<database>\n");

    let mut label_ids: HashMap<&str, String> = HashMap::new();
    for (index, label) in vault.labels.iter().enumerate() {
        let numeric = (index + 1).to_string();
        out.push_str(&format!(
            "  <label id=\"{}\" name=\"{}\"/>\n",
            numeric,
            escape_attr(&label.name)
        ));
        label_ids.insert(label.id.as_str(), numeric);
    }

    for item in vault.items.iter().filter(|i| i.status == ItemStatus::Active) {
        out.push_str("  <card");
        out.push_str(&format!(" id=\"{}\"", escape_attr(&item.id)));
        let title = if item.title.trim().is_empty() {
            "未命名"
        } else {
            item.title.as_str()
        };
        out.push_str(&format!(" title=\"{}\"", escape_attr(title)));
        out.push_str(&format!(" time_stamp=\"{}\"", item.updated_at / 1000));
        out.push_str(" template=\"false\"");
        if item.favorite {
            out.push_str(" star=\"true\"");
        }
        let has_url_field = item.fields.iter().any(|f| {
            f.r#type == FieldType::Url
                || f.label.eq_ignore_ascii_case("网址")
                || f.label.eq_ignore_ascii_case("地址")
                || f.label.to_lowercase().contains("website")
                || f.label.to_lowercase().contains("url")
        });
        let icon_key_attr = if item.icon_data.is_empty()
            && !item.icon_key.is_empty()
            && item.icon_key != "custom"
        {
            Some(item.icon_key.as_str())
        } else {
            None
        };
        if let Some(symbol) = icon_key_attr {
            out.push_str(&format!(" symbol=\"{}\"", escape_attr(symbol)));
        }
        out.push_str(">\n");

        for field in &item.fields {
            if field.label.eq_ignore_ascii_case("备注") {
                continue;
            }
            let name = if field.label.trim().is_empty() {
                "自定义字段"
            } else {
                &field.label
            };
            out.push_str(&format!(
                "    <field name=\"{}\" type=\"{}\">{}</field>\n",
                escape_attr(name),
                export_type(field),
                escape_text(&field.value)
            ));
        }
        if !item.url.trim().is_empty() && !has_url_field {
            out.push_str(&format!(
                "    <field name=\"website\" type=\"url\">{}</field>\n",
                escape_text(&item.url)
            ));
        }

        let mut seen: Vec<&str> = Vec::new();
        for label_id in &item.label_ids {
            let Some(mapped) = label_ids.get(label_id.as_str()) else {
                continue;
            };
            if seen.contains(&mapped.as_str()) {
                continue;
            }
            seen.push(mapped.as_str());
            out.push_str(&format!("    <label_id>{mapped}</label_id>\n"));
        }

        if !item.notes.trim().is_empty() {
            out.push_str(&format!("    <notes>{}</notes>\n", escape_text(&item.notes)));
        }

        if !item.icon_data.is_empty() {
            out.push_str(&format!(
                "    <custom_icon>{}</custom_icon>\n",
                escape_text(&strip_base64(&item.icon_data))
            ));
        }

        out.push_str("  </card>\n");
    }

    out.push_str("</database>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sp::model::Label;

    fn sample_xml() -> String {
        r#"<?xml version="1.0" encoding="UTF-8"?>
<database>
  <label id="1" name="银行"/>
  <label id="2" name="工作"/>
  <card id="c1" title="GitHub" time_stamp="1700000000" template="false" star="true">
    <field name="login" type="login">user@example.com</field>
    <field name="password" type="password">S3cret!pw</field>
    <field name="website" type="url">https://github.com</field>
    <label_id>2</label_id>
    <notes>开发账号</notes>
  </card>
  <card id="c2" title="招商银行" time_stamp="1700000100" template="false">
    <field name="卡号" type="card_number">6225880137654321</field>
    <field name="持卡人" type="card_holder">张三</field>
    <field name="CVV" type="cvv">123</field>
    <label_id>1</label_id>
    <label_id>9</label_id>
  </card>
  <card id="c3" title="跳过我" time_stamp="1" template="true"/>
  <card id="c4" title="也跳过" time_stamp="1" deleted="1"/>
  <card id="c5" title="网络WiFi" time_stamp="1700000200">
    <field name="网络名称" type="network_name">MyWiFi-5G</field>
    <field name="密码" type="password">wifi-pass-9</field>
  </card>
</database>"#
        .to_string()
    }

    #[test]
    fn looks_like_xml_handles_bom_and_space() {
        assert!(looks_like_xml("<database/>"));
        assert!(looks_like_xml("\u{FEFF}\n  <database/>"));
        assert!(!looks_like_xml("{\"json\": true}"));
    }

    #[test]
    fn parse_sample_cards() {
        let cards = parse(&sample_xml()).unwrap();
        assert_eq!(cards.len(), 3); // 5 张卡，template/deleted 两张被跳过

        let gh = &cards[0];
        // Kotlin 不读 XML 的 card id（SafeInCloudXml.kt:240 未传 id）→ VaultItem 默认 newId() 生成 UUID
        assert!(gh.item.id.contains('-'));
        assert_ne!(gh.item.id, "c1");
        assert_eq!(gh.item.title, "GitHub");
        assert_eq!(gh.item.created_at, 1_700_000_000_000); // 秒 → 毫秒
        assert_eq!(gh.item.updated_at, 1_700_000_000_000);
        assert!(gh.item.favorite);
        assert_eq!(gh.item.template_id, TemplateIds::LOGIN);
        assert_eq!(gh.item.notes, "开发账号");
        assert_eq!(gh.item.url, "https://github.com");
        assert_eq!(gh.label_names, vec!["工作"]);
        assert_eq!(gh.item.fields.len(), 3);
        assert_eq!(gh.item.fields[0].r#type, FieldType::Text); // login 且名字不含 email → TEXT
        assert_eq!(gh.item.fields[1].r#type, FieldType::Password);
        assert!(gh.item.fields[1].sensitive);
        assert_eq!(gh.item.fields[2].r#type, FieldType::Url);

        let bank = &cards[1];
        assert_eq!(bank.item.template_id, TemplateIds::BANK_CARD);
        // label 9 不存在 → "标签 9"
        assert_eq!(bank.label_names, vec!["银行", "标签 9"]);
        assert_eq!(bank.item.fields[2].label, "CVV");
        assert_eq!(bank.item.fields[2].r#type, FieldType::Password); // cvv → PASSWORD

        let wifi = &cards[2];
        assert_eq!(wifi.item.template_id, TemplateIds::WIFI);
        assert_eq!(wifi.item.fields[0].label, "网络名称");
        // safeType("network_name") 不在显式映射 → else 分支 → 非网址名 → TEXT
        assert_eq!(wifi.item.fields[0].r#type, FieldType::Text);
    }

    #[test]
    fn root_must_be_database() {
        let err = parse("<html><body/></html>").unwrap_err();
        assert!(err.contains("database"), "{err}");
        assert!(parse("").is_err());
    }

    #[test]
    fn doctype_rejected() {
        let xml = "<?xml version=\"1.0\"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///c:/windows/win.ini\">]><database><card title=\"&xxe;\"/></database>";
        let err = parse(xml).unwrap_err();
        assert!(err.contains("DOCTYPE"), "{err}");
    }

    #[test]
    fn timestamp_edge_values() {
        // 恰好等于上限 → 在 1..=9999999999 内 → ×1000
        let xml = "<database><card id=\"a\" title=\"t\" time_stamp=\"9999999999\"/></database>";
        let cards = parse(xml).unwrap();
        assert_eq!(cards[0].item.created_at, 9_999_999_999_000);
        // 0 不在 1.. 内 → 原样
        let xml0 = "<database><card id=\"a\" title=\"t\" time_stamp=\"0\"/></database>";
        assert_eq!(parse(xml0).unwrap()[0].item.created_at, 0);
        // 9 位 → ×1000
        let xml2 = "<database><card id=\"a\" title=\"t\" time_stamp=\"999999999\"/></database>";
        let cards2 = parse(xml2).unwrap();
        assert_eq!(cards2[0].item.created_at, 999_999_999_000);
        // 缺失/非法 → now
        let xml3 = "<database><card id=\"a\" title=\"t\"/></database>";
        let cards3 = parse(xml3).unwrap();
        let now = now_ms();
        assert!((cards3[0].item.created_at - now).abs() < 5_000);
    }

    #[test]
    fn label_and_field_helpers() {
        // safe_label 泛化名 → 标准名；自定义名保留
        assert_eq!(safe_label("login", "username", ""), "用户名");
        assert_eq!(safe_label("login", "Email", ""), "邮箱");
        assert_eq!(safe_label("login", "自定义账号名", ""), "自定义账号名");
        assert_eq!(safe_label("password", "", "x"), "密码"); // 空名走泛化
        assert_eq!(safe_label("unknown", "", ""), ""); // 空名+空值 → 空 → 字段被跳过
        assert_eq!(safe_label("unknown", "", "v"), "unknown"); // type.replace('_',' ') 后仍 "unknown"
        assert_eq!(safe_label("my_field", "", "v"), "my field");
        // safe_type 回落
        assert_eq!(safe_type("weird", "网址"), FieldType::Url);
        assert_eq!(safe_type("login", "user邮箱"), FieldType::Email);
        assert_eq!(safe_type("", ""), FieldType::Text);
    }

    #[test]
    fn export_roundtrip() {
        let mut vault = Vault::default();
        vault.labels.push(Label {
            id: "lbl-1".to_string(),
            name: "银行&<测试>".to_string(),
            ..Default::default()
        });
        let mut item = VaultItem {
            id: "item-1".to_string(),
            title: "我的<账号>\"引号\"".to_string(),
            notes: "备注1\n\n备注2".to_string(),
            url: "https://example.com/?a=1&b=2".to_string(),
            favorite: true,
            label_ids: vec!["lbl-1".to_string()],
            updated_at: 1_700_000_123_000,
            ..Default::default()
        };
        item.fields.push(Field {
            label: "用户名".to_string(),
            value: "alice&bob".to_string(),
            r#type: FieldType::Text,
            ..Default::default()
        });
        item.fields.push(Field {
            label: "密码".to_string(),
            value: "p<w>".to_string(),
            r#type: FieldType::Password,
            sensitive: true,
            ..Default::default()
        });
        item.fields.push(Field {
            label: "备注".to_string(), // 导出时被跳过（notes 单独导出）
            value: "隐藏".to_string(),
            r#type: FieldType::Multiline,
            ..Default::default()
        });
        vault.items.push(item);
        // TRASHED 不导出
        vault.items.push(VaultItem {
            id: "trashed".to_string(),
            status: ItemStatus::Trashed,
            ..Default::default()
        });
        // 归档也不导出（只导 ACTIVE）
        vault.items.push(VaultItem {
            id: "archived".to_string(),
            status: ItemStatus::Archived,
            ..Default::default()
        });

        let xml = export(&vault);
        assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>"));
        assert!(xml.contains("name=\"银行&amp;&lt;测试&gt;\""));
        assert!(xml.contains("title=\"我的&lt;账号&gt;&quot;引号&quot;\""));
        assert!(xml.contains("star=\"true\""));
        assert!(xml.contains("time_stamp=\"1700000123\"")); // 毫秒 → 秒
        assert!(!xml.contains("trashed"));
        assert!(!xml.contains("archived"));
        assert!(!xml.contains("隐藏")); // 备注字段跳过
        assert!(xml.contains("alice&amp;bob"));
        assert!(xml.contains("https://example.com/?a=1&amp;b=2"));

        let cards = parse(&xml).unwrap();
        assert_eq!(cards.len(), 1);
        let back = &cards[0];
        assert_eq!(back.item.title, "我的<账号>\"引号\"");
        assert_eq!(back.item.notes, "备注1\n\n备注2");
        assert_eq!(back.item.url, "https://example.com/?a=1&b=2");
        assert!(back.item.favorite);
        assert_eq!(back.label_names, vec!["银行&<测试>"]);
        assert_eq!(back.item.updated_at, 1_700_000_123_000);
        // 字段：用户名 + 密码 + 追加的 website（item.url 无对应字段时 Kotlin 也会追加）
        assert_eq!(back.item.fields.len(), 3);
        assert_eq!(back.item.fields[0].label, "用户名");
        assert_eq!(back.item.fields[1].r#type, FieldType::Password);
        assert_eq!(back.item.fields[2].label, "网址");
        assert_eq!(back.item.fields[2].r#type, FieldType::Url);
        // 只有 notes 元素，没有单独的 field notes
        assert!(!back.item.fields.iter().any(|f| f.label == "备注"));
    }

    #[test]
    fn export_only_url_field_sets_url_once() {
        let mut vault = Vault::default();
        let mut item = VaultItem {
            id: "u1".to_string(),
            title: "有网址字段".to_string(),
            url: "https://from-item-url".to_string(),
            ..Default::default()
        };
        item.fields.push(Field {
            label: "网址".to_string(),
            value: "https://from-field".to_string(),
            r#type: FieldType::Url,
            ..Default::default()
        });
        vault.items.push(item);
        let xml = export(&vault);
        // hasUrlField=true → 不追加 website 字段；exportType 对 Url 输出 "website"
        assert_eq!(xml.matches("type=\"website\"").count(), 1);
        let cards = parse(&xml).unwrap();
        assert_eq!(cards[0].item.fields[0].value, "https://from-field");
    }

    #[test]
    fn unescape_handles_entities() {
        assert_eq!(unescape_xml("a&amp;b&lt;c&gt;d&quot;e&apos;f"), "a&b<c>d\"e'f");
        assert_eq!(unescape_xml("&#65;&#x42;"), "AB");
        assert_eq!(unescape_xml("not an entity & here"), "not an entity & here");
    }
}
