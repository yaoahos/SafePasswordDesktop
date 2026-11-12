//! 明文模型，逐字段对应 Kotlin core/model/Model.kt。
//!
//! 字节兼容要点：
//! - wire 名 = Kotlin 属性名（camelCase，无 @SerialName）→ struct 用 rename_all = "camelCase"
//! - encodeDefaults=true → 所有字段始终写出（含 deletedAt: null）
//! - ignoreUnknownKeys=true → 解析时忽略未知字段（serde 默认行为）
//! - 缺字段用默认值（逐字段 #[serde(default)]），Field.sensitive 的默认值依赖 type，
//!   故 Field 手写 Deserialize
//! - 枚举 wire 值 = 变体名大写（ACTIVE / TEXT / MULTILINE …）

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 墓碑默认保留 90 天，对应 `Vault.TOMBSTONE_TTL_MS`。
pub const TOMBSTONE_TTL_MS: i64 = 90 * 24 * 60 * 60 * 1000;

fn default_notes_label() -> String {
    "备注".to_string()
}

fn default_mime() -> String {
    "application/octet-stream".to_string()
}

fn default_vault_name() -> String {
    "我的密码库".to_string()
}

fn default_schema() -> i32 {
    1
}

fn default_notes_position() -> i32 {
    -1
}

/// 0xFF0F766E.toInt()
pub fn default_label_color() -> i32 {
    -15_763_858
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[derive(Default)]
pub enum ItemStatus {
    #[default]
    Active,
    Archived,
    Trashed,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[derive(Default)]
pub enum FieldType {
    #[default]
    Text,
    Password,
    /// 与密码一样加密、掩码显示，但不参与安全检查。
    Secret,
    Url,
    Email,
    Phone,
    Multiline,
    /// 文件：单个 ≤50KB 小文件，value 存 base64，file_name / file_mime 存元数据。
    File,
}


#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    #[serde(default = "new_id")]
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(rename = "type", default)]
    pub r#type: FieldType,
    pub sensitive: bool,
    /// 密码/秘密字段的历史值（新的在前）；空数组不写进 JSON，保持与旧格式逐字节一致。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<FieldHistoryEntry>,
    /// FILE 字段的文件名；空值不写进 JSON（与 Kotlin @EncodeDefault(NEVER) 一致）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    /// FILE 字段的 MIME 类型；空值不写进 JSON。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_mime: String,
}

/// 密码/秘密字段被改掉之前的值。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldHistoryEntry {
    #[serde(default)]
    pub value: String,
    #[serde(default = "now_ms")]
    pub changed_at: i64,
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            #[serde(default = "new_id")]
            id: String,
            label: String,
            #[serde(default)]
            value: String,
            #[serde(rename = "type", default)]
            r#type: FieldType,
            sensitive: Option<bool>,
            #[serde(default)]
            history: Vec<FieldHistoryEntry>,
            #[serde(default)]
            file_name: String,
            #[serde(default)]
            file_mime: String,
        }
        let raw = Raw::deserialize(deserializer)?;
        // Kotlin: sensitive = type == PASSWORD || type == SECRET（缺省时按 type 推导）
        let sensitive = raw
            .sensitive
            .unwrap_or(matches!(raw.r#type, FieldType::Password | FieldType::Secret));
        Ok(Field {
            id: raw.id,
            label: raw.label,
            value: raw.value,
            r#type: raw.r#type,
            sensitive,
            history: raw.history,
            file_name: raw.file_name,
            file_mime: raw.file_mime,
        })
    }
}

impl Default for Field {
    fn default() -> Self {
        Field {
            id: new_id(),
            label: String::new(),
            value: String::new(),
            r#type: FieldType::Text,
            sensitive: false,
            history: Vec::new(),
            file_name: String::new(),
            file_mime: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentRef {
    #[serde(default = "new_id")]
    pub id: String,
    pub name: String,
    #[serde(default = "default_mime")]
    pub mime: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default = "now_ms")]
    pub added_at: i64,
}

impl Default for AttachmentRef {
    fn default() -> Self {
        AttachmentRef {
            id: new_id(),
            name: String::new(),
            mime: default_mime(),
            size: 0,
            added_at: now_ms(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    #[serde(default = "new_id")]
    pub id: String,
    pub name: String,
    /// ARGB color integer.
    #[serde(default = "default_label_color")]
    pub color: i32,
    #[serde(default)]
    pub sort_order: i32,
}

impl Default for Label {
    fn default() -> Self {
        Label {
            id: new_id(),
            name: String::new(),
            color: default_label_color(),
            sort_order: 0,
        }
    }
}

/// 导入时随条目带进来的标签信息：名字必有，颜色/排序可选。
///
/// JSON 直接带的是完整 `Label`，CSV/XML 只有名字与可选的 `color`/`sort_order`，
/// 统一先落到这里，等写入库时再按名字去重、缺省补默认值。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LabelSpec {
    pub name: String,
    pub color: Option<i32>,
    pub sort_order: Option<i32>,
}

impl LabelSpec {
    pub fn named(name: impl Into<String>) -> Self {
        LabelSpec {
            name: name.into(),
            color: None,
            sort_order: None,
        }
    }

    /// `#AARRGGBB` / `#RRGGBB`（大小写均可，`#` 可省）→ ARGB 整数。
    pub fn parse_color(text: &str) -> Option<i32> {
        let raw = text.trim().trim_start_matches('#');
        match raw.len() {
            8 => u32::from_str_radix(raw, 16).ok().map(|v| v as i32),
            6 => u32::from_str_radix(raw, 16)
                .ok()
                .map(|v| (0xFF00_0000u32 | v) as i32),
            _ => None,
        }
    }

    /// ARGB 整数 → `#AARRGGBB`（导出 CSV/XML 用，必要时可无损解析回来）。
    pub fn color_hex(color: i32) -> String {
        format!("#{:08X}", color as u32)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultItem {
    #[serde(default = "new_id")]
    pub id: String,
    #[serde(default = "default_template_id")]
    pub template_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub notes: String,
    #[serde(default = "default_notes_label")]
    pub notes_label: String,
    #[serde(default)]
    pub notes_type: FieldType,
    #[serde(default = "default_notes_position")]
    pub notes_position: i32,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub label_ids: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub status: ItemStatus,
    #[serde(default = "now_ms")]
    pub created_at: i64,
    #[serde(default = "now_ms")]
    pub updated_at: i64,
    #[serde(default)]
    pub deleted_at: Option<i64>,
    #[serde(default)]
    pub attachments: Vec<AttachmentRef>,
    #[serde(default)]
    pub icon_key: String,
    #[serde(default)]
    pub icon_data: String,
}

fn default_template_id() -> String {
    "login".to_string()
}

impl Default for VaultItem {
    fn default() -> Self {
        VaultItem {
            id: new_id(),
            template_id: default_template_id(),
            title: String::new(),
            fields: Vec::new(),
            notes: String::new(),
            notes_label: default_notes_label(),
            notes_type: FieldType::Multiline,
            notes_position: -1,
            url: String::new(),
            label_ids: Vec::new(),
            favorite: false,
            status: ItemStatus::Active,
            created_at: now_ms(),
            updated_at: now_ms(),
            deleted_at: None,
            attachments: Vec::new(),
            icon_key: String::new(),
            icon_data: String::new(),
        }
    }
}

fn first_line(s: &str) -> &str {
    let idx = s.find(['\r', '\n']).unwrap_or(s.len());
    &s[..idx]
}

impl VaultItem {
    pub fn primary_password(&self) -> Option<String> {
        self.fields
            .iter()
            .find(|f| f.r#type == FieldType::Password && !f.value.trim().is_empty())
            .map(|f| f.value.clone())
    }

    pub fn primary_username(&self) -> Option<String> {
        let non_empty: Vec<&Field> = self
            .fields
            .iter()
            .filter(|f| !f.value.trim().is_empty())
            .collect();
        let exact = |labels: &[&str]| -> Option<String> {
            non_empty
                .iter()
                .find(|f| labels.iter().any(|l| f.label.eq_ignore_ascii_case(l)))
                .map(|f| f.value.clone())
        };

        exact(&["账号", "帐号", "账户"])
            .or_else(|| exact(&["用户名", "登录名", "登录账号"]))
            .or_else(|| exact(&["邮箱", "电子邮件", "Email", "e-mail"]))
            .or_else(|| {
                non_empty
                    .iter()
                    .find(|f| {
                        let label = &f.label;
                        label.contains("账号")
                            || label.contains("帐号")
                            || label.contains("账户")
                            || label.contains("用户名")
                            || label.contains("邮箱")
                            || label.contains("邮件")
                            || label.to_lowercase().contains("email")
                            || label.contains("登录")
                    })
                    .map(|f| f.value.clone())
            })
            .or_else(|| exact(&["卡号", "证件号"]))
    }

    pub fn subtitle(&self) -> String {
        self.primary_username()
            .or_else(|| {
                if self.url.trim().is_empty() {
                    None
                } else {
                    Some(self.url.clone())
                }
            })
            .unwrap_or_else(|| first_line(&self.notes).to_string())
    }

    pub fn copy_as_conflict_copy(&self) -> VaultItem {
        VaultItem {
            id: new_id(),
            title: {
                let base = if self.title.trim().is_empty() {
                    "未命名".to_string()
                } else {
                    self.title.clone()
                };
                format!("{base}（冲突副本）")
            },
            favorite: false,
            status: ItemStatus::Active,
            created_at: now_ms(),
            updated_at: now_ms(),
            deleted_at: None,
            fields: self
                .fields
                .iter()
                .map(|f| Field {
                    id: new_id(),
                    ..f.clone()
                })
                .collect(),
            ..self.clone()
        }
    }

    pub fn copy_as_duplicate(&self) -> VaultItem {
        VaultItem {
            id: new_id(),
            title: {
                let base = if self.title.trim().is_empty() {
                    "未命名".to_string()
                } else {
                    self.title.clone()
                };
                format!("{base} 副本")
            },
            favorite: false,
            status: ItemStatus::Active,
            created_at: now_ms(),
            updated_at: now_ms(),
            deleted_at: None,
            fields: self
                .fields
                .iter()
                .map(|f| Field {
                    id: new_id(),
                    ..f.clone()
                })
                .collect(),
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tombstone {
    pub id: String,
    #[serde(default = "now_ms")]
    pub deleted_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vault {
    #[serde(default = "default_schema")]
    pub schema: i32,
    #[serde(default = "new_id")]
    pub id: String,
    #[serde(default = "default_vault_name")]
    pub name: String,
    #[serde(default)]
    pub items: Vec<VaultItem>,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default = "now_ms")]
    pub created_at: i64,
    #[serde(default = "now_ms")]
    pub updated_at: i64,
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub last_synced_at: i64,
    #[serde(default = "new_id")]
    pub device_id: String,
    #[serde(default)]
    pub content_key: String,
    #[serde(default)]
    pub tombstones: Vec<Tombstone>,
}

impl Default for Vault {
    fn default() -> Self {
        Vault {
            schema: 1,
            id: new_id(),
            name: default_vault_name(),
            items: Vec::new(),
            labels: Vec::new(),
            created_at: now_ms(),
            updated_at: now_ms(),
            revision: 0,
            last_synced_at: 0,
            device_id: new_id(),
            content_key: String::new(),
            tombstones: Vec::new(),
        }
    }
}

impl Vault {
    pub fn item(&self, id: &str) -> Option<&VaultItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn item_mut(&mut self, id: &str) -> Option<&mut VaultItem> {
        self.items.iter_mut().find(|i| i.id == id)
    }

    pub fn label(&self, id: &str) -> Option<&Label> {
        self.labels.iter().find(|l| l.id == id)
    }

    pub fn tombstone(&self, id: &str) -> Option<&Tombstone> {
        self.tombstones.iter().find(|t| t.id == id)
    }

    pub fn active_items(&self) -> Vec<&VaultItem> {
        self.items
            .iter()
            .filter(|i| i.status == ItemStatus::Active)
            .collect()
    }

    pub fn archived_items(&self) -> Vec<&VaultItem> {
        self.items
            .iter()
            .filter(|i| i.status == ItemStatus::Archived)
            .collect()
    }

    pub fn trashed_items(&self) -> Vec<&VaultItem> {
        self.items
            .iter()
            .filter(|i| i.status == ItemStatus::Trashed)
            .collect()
    }

    /// 永久删除条目并留下墓碑，避免它在下一次同步时从云端复活。
    pub fn purge_item(&mut self, item_id: &str, at: Option<i64>) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != item_id);
        let removed = self.items.len() != before;
        if removed {
            self.record_deletion(item_id, at.unwrap_or_else(now_ms));
        }
        removed
    }

    pub fn record_deletion(&mut self, item_id: &str, at: i64) {
        self.tombstones.retain(|t| t.id != item_id);
        self.tombstones.push(Tombstone {
            id: item_id.to_string(),
            deleted_at: at,
        });
    }

    /// 清理过期墓碑：默认保留 90 天，足以覆盖长期离线的设备。
    pub fn purge_old_tombstones(&mut self, before: Option<i64>) {
        let before = before.unwrap_or_else(|| now_ms() - TOMBSTONE_TTL_MS);
        self.tombstones.retain(|t| t.deleted_at >= before);
    }

    pub fn touch(&mut self) {
        self.updated_at = now_ms();
        self.revision += 1;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMeta {
    pub id: String,
    pub name: String,
    pub file_name: String,
    #[serde(default = "now_ms")]
    pub created_at: i64,
    #[serde(default)]
    pub last_opened_at: i64,
    #[serde(default)]
    pub last_synced_at: i64,
    /// 云端文件名键（由内容密钥派生）：用于判断"云端某个库是不是本机这个库"。
    /// 同名但键不同的，是另一个库，不能算"已在本机"。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub cloud_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VaultIndex {
    #[serde(default)]
    pub vaults: Vec<VaultMeta>,
}

/// 对齐 TemplateCatalog.kt 的模板 id 常量。
pub struct TemplateIds;
impl TemplateIds {
    pub const LOGIN: &'static str = "login";
    pub const BANK_CARD: &'static str = "bank_card";
    pub const EMAIL: &'static str = "email";
    pub const WIFI: &'static str = "wifi";
    pub const IDENTITY: &'static str = "identity";
    pub const NOTE: &'static str = "note";
    pub const CUSTOM: &'static str = "custom";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_defaults_follow_kotlin() {
        let f: Field = serde_json::from_str(r#"{"label":"账号"}"#).unwrap();
        assert_eq!(f.value, "");
        assert_eq!(f.r#type, FieldType::Text);
        assert!(!f.sensitive);
        assert_eq!(f.id.len(), 36);

        // 缺 sensitive 时按 type 推导
        let f2: Field = serde_json::from_str(r#"{"label":"密码","type":"PASSWORD"}"#).unwrap();
        assert!(f2.sensitive);
        let f3: Field = serde_json::from_str(r#"{"label":"密钥","type":"SECRET"}"#).unwrap();
        assert!(f3.sensitive);
        let f4: Field = serde_json::from_str(r#"{"label":"网址","type":"URL"}"#).unwrap();
        assert!(!f4.sensitive);

        // 显式 sensitive 覆盖 type 推导
        let f5: Field =
            serde_json::from_str(r#"{"label":"卡号","type":"TEXT","sensitive":true}"#).unwrap();
        assert!(f5.sensitive);
    }

    #[test]
    fn field_history_roundtrip_and_omitted_when_empty() {
        let f: Field = serde_json::from_str(r#"{"label":"密码","type":"PASSWORD"}"#).unwrap();
        assert!(f.history.is_empty());
        // 没有历史时不写 history 键，保持与旧格式一致
        let json = serde_json::to_string(&f).unwrap();
        assert!(!json.contains("history"), "{json}");

        let with_history: Field = serde_json::from_str(
            r#"{"label":"密码","type":"PASSWORD","history":[{"value":"old","changedAt":123}]}"#,
        )
        .unwrap();
        assert_eq!(with_history.history.len(), 1);
        assert_eq!(with_history.history[0].value, "old");
        assert_eq!(with_history.history[0].changed_at, 123);
        let back = serde_json::to_string(&with_history).unwrap();
        assert!(back.contains(r#""history":[{"value":"old","changedAt":123}]"#), "{back}");
    }

    #[test]
    fn missing_label_is_an_error_like_kotlin() {
        assert!(serde_json::from_str::<Field>(r#"{"value":"x"}"#).is_err());
    }

    #[test]
    fn enum_wire_values_match_kotlin() {
        assert_eq!(serde_json::to_string(&FieldType::Text).unwrap(), "\"TEXT\"");
        assert_eq!(
            serde_json::to_string(&FieldType::Multiline).unwrap(),
            "\"MULTILINE\""
        );
        assert_eq!(
            serde_json::to_string(&FieldType::Secret).unwrap(),
            "\"SECRET\""
        );
        assert_eq!(serde_json::to_string(&FieldType::File).unwrap(), "\"FILE\"");
        assert_eq!(
            serde_json::to_string(&ItemStatus::Archived).unwrap(),
            "\"ARCHIVED\""
        );
    }

    #[test]
    fn file_fields_roundtrip_and_omitted_when_empty() {
        // 空文件元数据不写键，与 Kotlin @EncodeDefault(NEVER) 一致
        let f: Field = serde_json::from_str(r#"{"label":"附件","type":"FILE"}"#).unwrap();
        assert_eq!(f.r#type, FieldType::File);
        assert!(!f.sensitive);
        assert!(f.file_name.is_empty());
        let json = serde_json::to_string(&f).unwrap();
        assert!(!json.contains("fileName"), "{json}");
        assert!(!json.contains("fileMime"), "{json}");

        // 有文件元数据时按 camelCase 写出并可回读
        let with_file: Field = serde_json::from_str(
            r#"{"label":"附件","type":"FILE","value":"aGk=","fileName":"hi.txt","fileMime":"text/plain"}"#,
        )
        .unwrap();
        assert_eq!(with_file.file_name, "hi.txt");
        assert_eq!(with_file.file_mime, "text/plain");
        let back = serde_json::to_string(&with_file).unwrap();
        assert!(back.contains(r#""fileName":"hi.txt""#), "{back}");
        assert!(back.contains(r#""fileMime":"text/plain""#), "{back}");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let v: Vault = serde_json::from_str(
            r#"{"schema":1,"futureField":{"x":1},"items":[{"title":"t","zzz":true}]}"#,
        )
        .unwrap();
        assert_eq!(v.schema, 1);
        assert_eq!(v.items.len(), 1);
        assert_eq!(v.items[0].title, "t");
    }

    #[test]
    fn vault_json_wire_names_are_camel_case() {
        let mut vault = Vault::default();
        let mut item = VaultItem {
            title: "招商银行".into(),
            ..VaultItem::default()
        };
        item.deleted_at = None;
        item.label_ids = vec!["lbl1".into()];
        vault.items.push(item);
        let json = serde_json::to_string(&vault).unwrap();
        for key in [
            "schema",
            "templateId",
            "notesLabel",
            "notesType",
            "notesPosition",
            "labelIds",
            "createdAt",
            "updatedAt",
            "deletedAt",
            "iconKey",
            "iconData",
            "contentKey",
            "tombstones",
            "lastSyncedAt",
            "deviceId",
        ] {
            assert!(json.contains(&format!("\"{key}\"")), "缺少 {key}：{json}");
        }
        assert!(
            json.contains("\"deletedAt\":null"),
            "encodeDefaults 要求显式写出 null：{json}"
        );
        assert!(json.contains("\"notesType\":\"MULTILINE\""), "{json}");
    }

    #[test]
    fn minimal_vault_decode_uses_kotlin_defaults() {
        let v: Vault = serde_json::from_str(r#"{"name":"测试库"}"#).unwrap();
        assert_eq!(v.schema, 1);
        assert_eq!(v.name, "测试库");
        assert!(v.items.is_empty());
        assert_eq!(v.revision, 0);
        assert_eq!(v.last_synced_at, 0);
        assert!(v.content_key.is_empty());
        assert_eq!(v.id.len(), 36);
    }

    #[test]
    fn subtitle_and_primary_username_logic() {
        let item = VaultItem {
            fields: vec![
                Field {
                    label: "账号".into(),
                    value: "user@example.com".into(),
                    ..Field::default()
                },
                Field {
                    label: "密码".into(),
                    value: "p@ss".into(),
                    r#type: FieldType::Password,
                    sensitive: true,
                    ..Field::default()
                },
            ],
            ..VaultItem::default()
        };
        assert_eq!(item.primary_username().as_deref(), Some("user@example.com"));
        assert_eq!(item.primary_password().as_deref(), Some("p@ss"));
        assert_eq!(item.subtitle(), "user@example.com");

        // 无账号时回落到 url，再到备注首行
        let item2 = VaultItem {
            url: "https://example.com".into(),
            ..VaultItem::default()
        };
        assert_eq!(item2.subtitle(), "https://example.com");
        let item3 = VaultItem {
            notes: "第一行\n第二行".into(),
            ..VaultItem::default()
        };
        assert_eq!(item3.subtitle(), "第一行");

        // 忽略大小写的 Email 匹配
        let item4 = VaultItem {
            fields: vec![Field {
                label: "EMAIL".into(),
                value: "a@b.c".into(),
                r#type: FieldType::Email,
                ..Field::default()
            }],
            ..VaultItem::default()
        };
        assert_eq!(item4.primary_username().as_deref(), Some("a@b.c"));
    }

    #[test]
    fn tombstone_helpers() {
        let mut v = Vault::default();
        let item = VaultItem::default();
        let id = item.id.clone();
        v.items.push(item);
        assert!(v.purge_item(&id, Some(1_000)));
        assert!(v.items.is_empty());
        assert_eq!(v.tombstones.len(), 1);
        assert_eq!(v.tombstones[0].deleted_at, 1_000);
        // 重复删除不产生重复墓碑
        let fresh = now_ms();
        v.record_deletion(&id, fresh);
        assert_eq!(v.tombstones.len(), 1);
        assert_eq!(v.tombstones[0].deleted_at, fresh);

        // 过期墓碑清理（默认 90 天）
        v.record_deletion("old-id", now_ms() - TOMBSTONE_TTL_MS - 1);
        assert_eq!(v.tombstones.len(), 2);
        v.purge_old_tombstones(None);
        assert_eq!(v.tombstones.len(), 1);
        assert_eq!(v.tombstones[0].id, id);

        // touch：revision+1、updatedAt 更新
        let before = v.revision;
        v.touch();
        assert_eq!(v.revision, before + 1);
    }
}
