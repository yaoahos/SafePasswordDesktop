import type { AppSettings, Vault, VaultMeta } from "./types";

export type ToastKind = "info" | "success" | "error";

/** 左栏状态筛选 */
export type SectionFilter = "ALL" | "FAVORITES" | "TRASH";

/** App.tsx 提供给各组件/弹窗的共享能力。 */
export interface AppCtx {
  vault: Vault | null;
  vaults: VaultMeta[];
  settings: AppSettings;
  notify: (msg: string, kind?: ToastKind) => void;
  /** 更新本地 settings 并持久化到后端（失败只提示，不抛出） */
  applySettings: (next: AppSettings) => Promise<void>;
  setVault: (v: Vault | null) => void;
  refreshVaults: () => Promise<void>;
  /** 从后端重新拉取 current_vault + vaults（导入/恢复/同步后用） */
  refreshFromBackend: () => Promise<void>;
  /** close_vault() 并回到解锁页 */
  lockNow: () => void;
  /** 回到密码库选择页（close_vault + 重新解锁） */
  gotoSelect: () => void;
  selectItemId: string | null;
  setSelectItemId: (id: string | null) => void;
  searchPasswords: boolean;
  setSearchPasswords: (v: boolean) => void;
}
