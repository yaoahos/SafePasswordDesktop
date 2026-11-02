// 与 src-tauri/src/sp/model.rs 等 serde camelCase 结构一一对应。

export interface VaultMeta {
  id: string;
  name: string;
  fileName: string;
  createdAt: number;
  lastOpenedAt: number;
  lastSyncedAt: number;
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
  tombstones: Tombstone[];
}

export type ItemStatus = "ACTIVE" | "ARCHIVED" | "TRASHED";

export type FieldType = "TEXT" | "PASSWORD" | "SECRET" | "URL" | "EMAIL" | "PHONE" | "MULTILINE";

export interface Field {
  id: string;
  label: string;
  value: string;
  type: FieldType;
  sensitive: boolean;
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
  autoLockMinutes: number;
  hibpApiKey: string;
  webdav: WebDavConfig;
}

export type SyncStrategy = "DETECT" | "KEEP_LOCAL" | "KEEP_REMOTE" | "MERGE";

export interface SyncSummary {
  direction: "NONE" | "UPLOADED" | "DOWNLOADED" | "MERGED" | "CONFLICT";
  conflicts: number;
  attachmentsUploaded: number;
  attachmentsDownloaded: number;
  message: string;
}
