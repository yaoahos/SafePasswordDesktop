// 与 src-tauri/src/sp/model.rs 等 serde camelCase 结构一一对应。

export interface VaultMeta {
  id: string;
  name: string;
  fileName: string;
  createdAt: number;
  lastOpenedAt: number;
  lastSyncedAt: number;
  /** 云端文件名键（由内容密钥派生）：判断"云端某个库是不是本机的库"只看它，不看名字 */
  cloudKey?: string;
}

/** 云端密码库清单条目（对应 sp/cloud.rs CloudVaultEntry） */
export interface CloudVaultEntry {
  key: string;
  name: string;
  updatedAt: number;
  size: number;
}

export interface Vault {
  schema: number;
  id: string;
  name: string;
  items: VaultItem[];
  labels: Label[];
  createdAt: number;
  updatedAt: number;
  revision: number;
  lastSyncedAt: number;
  deviceId: string;
  contentKey: string;
  /** 云端文件名用的稳定键（由内容密钥派生）；空串表示尚未分配，JSON 中省略 */
  cloudKey?: string;
  tombstones: Tombstone[];
}

export type ItemStatus = "ACTIVE" | "ARCHIVED" | "TRASHED";

export type FieldType = "TEXT" | "PASSWORD" | "SECRET" | "TOTP" | "URL" | "EMAIL" | "PHONE" | "MULTILINE" | "FILE";

export interface FieldHistoryEntry {
  value: string;
  changedAt: number;
}

export interface Field {
  id: string;
  label: string;
  value: string;
  type: FieldType;
  sensitive: boolean;
  /** 密码/秘密字段的历史值（新的在前）；无历史时后端不下发该字段 */
  history?: FieldHistoryEntry[];
  /** FILE 字段的文件名 / MIME；空值时后端不下发 */
  fileName?: string;
  fileMime?: string;
}

export interface VaultItem {
  id: string;
  templateId: string;
  title: string;
  fields: Field[];
  notes: string;
  notesLabel: string;
  notesType: FieldType;
  notesPosition: number;
  url: string;
  labelIds: string[];
  favorite: boolean;
  status: ItemStatus;
  createdAt: number;
  updatedAt: number;
  deletedAt: number | null;
  attachments: AttachmentRef[];
  iconKey: string;
  iconData: string;
}

export interface Label {
  id: string;
  name: string;
  /** ARGB int32；转 CSS 用 labelColorCss() */
  color: number;
  sortOrder: number;
}

export interface Tombstone {
  id: string;
  deletedAt: number;
}

export interface AttachmentRef {
  id: string;
  name: string;
  mime: string;
  size: number;
  addedAt: number;
}

export interface GeneratorOptions {
  length?: number;
  lowercase?: boolean;
  uppercase?: boolean;
  digits?: boolean;
  symbols?: boolean;
  excludeAmbiguous?: boolean;
  requireEach?: boolean;
}

export interface StrengthResult {
  score: number;
  label: string;
  entropyBits: number;
  crackTime: string;
}

export interface SecurityReport {
  total: number;
  weak: VaultItem[];
  reused: VaultItem[];
  breached: VaultItem[];
  old: VaultItem[];
}

/** 检查更新结果（对齐手机端 AboutScreen「检查更新」） */
export interface UpdateInfo {
  current: string;
  latest: string;
  hasUpdate: boolean;
  url: string;
  assetUrl: string;
  assetSize: number;
  notes: string;
}

export interface BackupInfo {
  path: string;
  name: string;
  createdAt: number;
  size: number;
}

export interface WebDavConfig {
  baseUrl: string;
  username: string;
  password: string;
  remoteDir: string;
  enabled: boolean;
}

export type ThemeMode = "system" | "light" | "dark";

export interface AppSettings {
  theme: ThemeMode;
  /** 主题强调色（#RRGGBB，null/空 = 默认蓝） */
  accent: string;
  /** 自动锁定秒数（对齐手机端 autoLockSeconds）：-1 = 从不，0 = 切到后台立即锁定，>0 = 两段计时秒数 */
  autoLockSeconds: number;
  hibpApiKey: string;
  webdav: WebDavConfig;
  /** 0 = 关闭自动备份，否则每 N 天（对齐手机端） */
  autoBackupDays: number;
  /** 上次备份时间（epoch millis） */
  lastBackupAt: number;
  /** 保存后自动同步（对齐手机端 autoSync，默认开） */
  autoSync: boolean;
  /** 自动拉取云端更新间隔秒数（0 = 关闭，对齐 pullIntervalSeconds） */
  pullIntervalSeconds: number;
  /** 搜索是否匹配密码/机密字段（对齐 searchPasswords） */
  searchPasswords: boolean;
  /** 默认展示的标签 id（对齐 defaultLabelId，null = 全部条目） */
  defaultLabelId: string | null;
  /** 最近打开的密码库（对齐 lastVaultId） */
  lastVaultId: string | null;
  /** 启动时自动检查更新 */
  checkUpdatesOnLaunch: boolean;
}

export type SyncStrategy = "KEEP_LOCAL" | "MERGE";

/** 对齐手机端 SortMode（仅会话内，不持久化） */
export type SortMode = "TITLE" | "UPDATED" | "CREATED";

export interface SyncSummary {
  direction:
    | "NONE"
    | "UPLOADED"
    | "DOWNLOADED"
    | "MERGED"
    | "CONFLICT"
    | "REMOTE_DELETED"
    | "NAME_CONFLICT";
  conflicts: number;
  message: string;
}
