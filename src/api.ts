import { invoke } from "@tauri-apps/api/core";
import type {
  AppSettings,
  BackupInfo,
  CloudVaultEntry,
  GeneratorOptions,
  SecurityReport,
  StrengthResult,
  SyncStrategy,
  SyncSummary,
  UpdateInfo,
  Vault,
  VaultItem,
  VaultMeta,
  WebDavConfig,
} from "./types";

// 后端命令层：src-tauri/src/commands.rs（参数名 camelCase，Rust 自动转换）

export const listVaults = () => invoke<VaultMeta[]>("list_vaults");

export const createVault = (name: string, password: string) =>
  invoke<VaultMeta>("create_vault", { name, password });

export const openVault = (id: string, password: string) =>
  invoke<Vault>("open_vault", { id, password });

export const closeVault = () => invoke<void>("close_vault");

export const currentVault = () => invoke<Vault | null>("current_vault");

export const deleteVault = (id: string) => invoke<void>("delete_vault", { id });

export const renameVault = (id: string, name: string) =>
  invoke<void>("rename_vault", { id, name });

export const updateVault = (vault: Vault) => invoke<void>("update_vault", { vault });

export const putItem = (item: VaultItem) => invoke<Vault>("put_item", { item });

export const deleteItem = (id: string) => invoke<Vault>("delete_item", { id });

export const changeMasterPassword = (oldPassword: string, newPassword: string) =>
  invoke<void>("change_master_password", { oldPassword, newPassword });

export const importVaultBytes = (base64Data: string, password: string, name: string) =>
  invoke<VaultMeta>("import_vault_bytes", { base64Data, password, name });

export const importVaultJson = (json: string, password: string, name: string) =>
  invoke<VaultMeta>("import_vault_json", { json, password, name });

export const generatePassword = (options?: GeneratorOptions) =>
  invoke<string>("generate_password", { options });

export const generatePassphrase = (
  wordCount?: number,
  separator?: string,
  capitalize?: boolean,
  includeNumber?: boolean,
) =>
  invoke<string>("generate_passphrase", { wordCount, separator, capitalize, includeNumber });

export const evaluateStrength = (password: string) =>
  invoke<StrengthResult>("evaluate_strength", { password });

export const securityReport = () => invoke<SecurityReport>("security_report");

export const securityReportFull = () => invoke<SecurityReport>("security_report_full");

export const checkPasswordBreach = (password: string) =>
  invoke<number>("check_password_breach", { password });

export const exportCsv = () => invoke<string>("export_csv");

export const exportXml = () => invoke<string>("export_xml");

export const exportVaultJson = () => invoke<string>("export_vault_json");

/** 弹出系统保存对话框并写入（base64 内容，返回实际保存路径；取消返回 null） */
export const saveExport = (defaultName: string, base64Data: string) =>
  invoke<string | null>("save_export", { defaultName, base64Data });

export const importCsv = (text: string) => invoke<number>("import_csv", { text });

export const importXml = (text: string) =>
  invoke<number>("import_xml", { text });

/** 导入明文 JSON 里的条目到当前密码库（不新建库，对齐手机端「导入条目」） */
export const importJsonItems = (json: string) => invoke<number>("import_json_items", { json });

export const backupCreate = () => invoke<BackupInfo>("backup_create");

export const backupList = () => invoke<BackupInfo[]>("backup_list");

export const backupRestore = (path: string, password: string) =>
  invoke<Vault>("backup_restore", { path, password });

export const backupDelete = (path: string) => invoke<void>("backup_delete", { path });

export const getSettings = () => invoke<AppSettings>("get_settings");

export const setSettings = (settings: AppSettings) => invoke<void>("set_settings", { settings });

export const webdavTestConnection = (config: WebDavConfig) =>
  invoke<string>("webdav_test_connection", { config });

export const webdavSync = (config: WebDavConfig, strategy?: SyncStrategy) =>
  invoke<SyncSummary>("webdav_sync", { config, strategy });

export const webdavPull = (config: WebDavConfig) => invoke<SyncSummary>("webdav_pull", { config });

/** 自动拉取：先 HEAD 比对，云端没变化时返回 null（对齐手机端 pullIfRemoteChanged） */
export const webdavPullIfChanged = (config: WebDavConfig) =>
  invoke<SyncSummary | null>("webdav_pull_if_changed", { config });

/** 导出加密备份（.spv，Base64；前端转 Blob 下载，可拷回手机） */
export const exportVaultSpv = () => invoke<string>("export_vault_spv");

/** 云端密码库清单（读取 WebDAV 上的 index 文件） */
export const webdavListCloud = (config: WebDavConfig) =>
  invoke<CloudVaultEntry[]>("webdav_list_cloud", { config });

/** 下载指定云端密码库密文（Base64；找不到返回 null） */
export const webdavGetVault = (config: WebDavConfig, key: string) =>
  invoke<string | null>("webdav_get_vault", { config, key });

// ---- 检查更新 / 软件内自更新（对齐手机端 AboutScreen）----

export const checkUpdate = (appVersion: string) =>
  invoke<UpdateInfo>("check_update", { appVersion });

export const updateDownloadAndInstall = (assetUrl: string) =>
  invoke<string>("update_download_and_install", { assetUrl });
