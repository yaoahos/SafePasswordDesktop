import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import "./styles.css";
import {
  closeVault,
  createVault,
  deleteItem,
  getSettings,
  listVaults,
  openVault,
  putItem,
  setSettings as setSettingsApi,
  currentVault,
} from "./api";
import type { AppSettings, Vault, VaultItem, VaultMeta } from "./types";
import type { AppCtx, SectionFilter, ToastKind } from "./app-context";
import { AuthScreens } from "./components/AuthScreens";
import { ItemDetail } from "./components/ItemDetail";
import { ItemList } from "./components/ItemList";
import { Sidebar } from "./components/Sidebar";
import { TopBar, type TopAction } from "./components/TopBar";
import { AboutDialog } from "./dialogs/AboutDialog";
import { GeneratorDialog } from "./dialogs/GeneratorDialog";
import { ImportDialog } from "./dialogs/ImportDialog";
import { LabelManagerDialog } from "./dialogs/LabelManagerDialog";
import { LockDialog } from "./dialogs/LockDialog";
import { SecurityDialog } from "./dialogs/SecurityDialog";
import { SettingsDialog } from "./dialogs/SettingsDialog";
import { SyncDialog } from "./dialogs/SyncDialog";
import { TemplatePickerDialog } from "./dialogs/TemplatePickerDialog";
import { ThemeDialog } from "./dialogs/ThemeDialog";
import { WebDavDialog } from "./dialogs/WebDavDialog";
import { duplicateItem, itemMatches, newItemFromTemplate } from "./utils";

type DialogKind = TopAction | "LABELS" | "TEMPLATE" | null;

const DEFAULT_SETTINGS: AppSettings = {
  theme: "system",
  autoLockMinutes: 5,
  hibpApiKey: "",
  webdav: { baseUrl: "", username: "", password: "", remoteDir: "/SafePassword", enabled: false },
};

const SECTION_TITLES: Record<SectionFilter, string> = {
  ALL: "全部条目",
  FAVORITES: "收藏",
  TRASH: "回收站",
};

export default function App() {
  const [booted, setBooted] = useState(false);
  const [vaults, setVaults] = useState<VaultMeta[]>([]);
  const [vault, setVault] = useState<Vault | null>(null);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [dialog, setDialog] = useState<DialogKind>(null);
  const [section, setSection] = useState<SectionFilter>("ALL");
  const [labelFilter, setLabelFilter] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [selectItemId, setSelectItemId] = useState<string | null>(null);
  const [toast, setToast] = useState<{ id: number; msg: string; kind: ToastKind } | null>(null);
  const [searchPasswords, setSearchPasswordsState] = useState<boolean>(
    () => typeof localStorage !== "undefined" && localStorage.getItem("sp.searchPasswords") === "1",
  );

  // ------------------------------------------------------------ 基础加载

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const s = await getSettings();
        if (!cancelled && s) setSettings({ ...DEFAULT_SETTINGS, ...s, webdav: { ...DEFAULT_SETTINGS.webdav, ...s.webdav } });
      } catch {
        // 没有已保存设置时后端可能报错，使用默认值
      }
      try {
        const [lv, cv] = await Promise.all([listVaults(), currentVault()]);
        if (!cancelled) {
          setVaults(lv);
          setVault(cv);
        }
      } catch (e) {
        if (!cancelled) notify(String(e), "error");
      } finally {
        if (!cancelled) setBooted(true);
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ------------------------------------------------------------ toast

  const notify = useCallback((msg: string, kind: ToastKind = "info") => {
    setToast({ id: Date.now(), msg, kind });
  }, []);

  useEffect(() => {
    if (!toast) return;
    const t = window.setTimeout(() => setToast(null), 3500);
    return () => window.clearTimeout(t);
  }, [toast]);

  // ------------------------------------------------------------ 主题

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = settings.theme === "dark" || (settings.theme === "system" && mq.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [settings.theme]);

  const applySettings = useCallback(async (next: AppSettings) => {
    setSettings(next);
    try {
      await setSettingsApi(next);
    } catch (e) {
      notify(`保存设置失败：${String(e)}`, "error");
    }
  }, [notify]);

  const setSearchPasswords = useCallback((v: boolean) => {
    setSearchPasswordsState(v);
    try {
      localStorage.setItem("sp.searchPasswords", v ? "1" : "0");
    } catch {
      // localStorage 不可用时忽略
    }
  }, []);

  // ------------------------------------------------------------ 流程

  const refreshVaults = useCallback(async () => {
    try {
      setVaults(await listVaults());
    } catch (e) {
      notify(String(e), "error");
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const refreshFromBackend = useCallback(async () => {
    try {
      const [cv, lv] = await Promise.all([currentVault(), listVaults()]);
      setVault(cv);
      setVaults(lv);
    } catch (e) {
      notify(String(e), "error");
    }
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const lockNow = useCallback(() => {
    setVault(null);
    setSelectItemId(null);
    setDialog(null);
    void closeVault().catch(() => undefined);
  }, []);

  const gotoSelect = useCallback(() => {
    setVault(null);
    setSelectItemId(null);
    setDialog(null);
    void closeVault().catch(() => undefined);
    void listVaults()
      .then(setVaults)
      .catch(() => undefined);
  }, []);

  const handleUnlock = useCallback(
    async (id: string, password: string): Promise<string | null> => {
      try {
        const v = await openVault(id, password);
        setVault(v);
        setSelectItemId(v.items.find((i) => i.status !== "TRASHED")?.id ?? null);
        setSection("ALL");
        setLabelFilter(null);
        setQuery("");
        void listVaults().then(setVaults).catch(() => undefined);
        return null;
      } catch (e) {
        return String(e);
      }
    },
    [],
  );

  const handleCreate = useCallback(
    async (name: string, password: string): Promise<string | null> => {
      try {
        await createVault(name, password);
        await refreshFromBackend();
        // create_vault 已切换 current，这里再把条目选中状态理顺
        setSelectItemId(null);
        setSection("ALL");
        setLabelFilter(null);
        setQuery("");
        return null;
      } catch (e) {
        return String(e);
      }
    },
    [refreshFromBackend],
  );

  // ------------------------------------------------------------ 自动锁定（闲置计时）

  const idleRef = useRef<number | null>(null);
  useEffect(() => {
    if (!vault || settings.autoLockMinutes <= 0) return;
    const ms = settings.autoLockMinutes * 60_000;
    const reset = () => {
      if (idleRef.current !== null) window.clearTimeout(idleRef.current);
      idleRef.current = window.setTimeout(() => {
        lockNow();
        notify("已根据闲置时间自动锁定", "info");
      }, ms);
    };
    reset();
    const events: (keyof WindowEventMap)[] = ["mousemove", "mousedown", "keydown", "wheel", "touchstart"];
    for (const ev of events) window.addEventListener(ev, reset, { passive: true });
    return () => {
      if (idleRef.current !== null) window.clearTimeout(idleRef.current);
      for (const ev of events) window.removeEventListener(ev, reset);
    };
  }, [vault, settings.autoLockMinutes, lockNow, notify]);

  // ------------------------------------------------------------ 条目操作

  const saveItem = useCallback(
    async (next: VaultItem) => {
      const v = await putItem(next);
      setVault(v);
    },
    [],
  );

  const toggleFavorite = useCallback(async () => {
    if (!vault || !selectItemId) return;
    const item = vault.items.find((i) => i.id === selectItemId);
    if (!item) return;
    const v = await putItem({ ...item, favorite: !item.favorite, updatedAt: Date.now() });
    setVault(v);
  }, [vault, selectItemId]);

  const duplicateSelected = useCallback(async () => {
    if (!vault || !selectItemId) return;
    const item = vault.items.find((i) => i.id === selectItemId);
    if (!item) return;
    const copy = duplicateItem(item);
    const v = await putItem(copy);
    setVault(v);
    setSelectItemId(copy.id);
    notify("已创建副本", "success");
  }, [vault, selectItemId, notify]);

  /** 非回收站条目 → 移回收站（后端 delete_item 是永久删除，软删需自己置状态） */
  const trashSelected = useCallback(async () => {
    if (!vault || !selectItemId) return;
    const item = vault.items.find((i) => i.id === selectItemId);
    if (!item) return;
    const v = await putItem({ ...item, status: "TRASHED", deletedAt: Date.now(), updatedAt: Date.now() });
    setVault(v);
    setSelectItemId(null);
    notify("已移入回收站", "success");
  }, [vault, selectItemId, notify]);

  const restoreSelected = useCallback(async () => {
    if (!vault || !selectItemId) return;
    const item = vault.items.find((i) => i.id === selectItemId);
    if (!item) return;
    const v = await putItem({ ...item, status: "ACTIVE", deletedAt: null, updatedAt: Date.now() });
    setVault(v);
    notify("已恢复", "success");
  }, [vault, selectItemId, notify]);

  const purgeSelected = useCallback(async () => {
    if (!vault || !selectItemId) return;
    const v = await deleteItem(selectItemId);
    setVault(v);
    setSelectItemId(null);
    notify("已彻底删除", "success");
  }, [vault, selectItemId, notify]);

  const createFromTemplate = useCallback(
    async (templateId: string) => {
      if (!vault) return;
      const item = newItemFromTemplate(templateId);
      item.title = "新条目";
      const v = await putItem(item);
      setVault(v);
      setSelectItemId(item.id);
      setSection("ALL");
      setLabelFilter(null);
      setDialog(null);
    },
    [vault],
  );

  // ------------------------------------------------------------ 过滤 / 选中

  const items = vault?.items ?? [];
  const filtered = useMemo(() => {
    const q = query.trim();
    return items.filter((i) => {
      if (labelFilter) {
        if (i.status === "TRASHED") return false;
        if (!i.labelIds.includes(labelFilter)) return false;
      } else if (section === "TRASH") {
        if (i.status !== "TRASHED") return false;
      } else if (section === "FAVORITES") {
        if (i.status === "TRASHED" || !i.favorite) return false;
      } else if (i.status === "TRASHED") {
        return false;
      }
      return itemMatches(i, q, searchPasswords);
    });
  }, [items, section, labelFilter, query, searchPasswords]);

  // 选中项不在可见列表时，自动落到第一个
  useEffect(() => {
    if (filtered.length === 0) {
      if (selectItemId !== null) setSelectItemId(null);
      return;
    }
    if (!filtered.some((i) => i.id === selectItemId)) setSelectItemId(filtered[0].id);
  }, [filtered, selectItemId]);

  const selected = useMemo(
    () => filtered.find((i) => i.id === selectItemId) ?? null,
    [filtered, selectItemId],
  );

  const sectionTitle = labelFilter
    ? `标签：${vault?.labels.find((l) => l.id === labelFilter)?.name ?? ""}`
    : SECTION_TITLES[section];

  // ------------------------------------------------------------ 上下文

  const ctx: AppCtx = {
    vault,
    vaults,
    settings,
    notify,
    applySettings,
    setVault,
    refreshVaults,
    refreshFromBackend,
    lockNow,
    gotoSelect,
    selectItemId,
    setSelectItemId,
    searchPasswords,
    setSearchPasswords,
  };

  // ------------------------------------------------------------ 渲染

  if (!booted) {
    return (
      <div className="loading-wrap">
        <div className="spinner" />
        <div>正在加载…</div>
      </div>
    );
  }

  if (!vault) {
    return <AuthScreens vaults={vaults} onUnlock={handleUnlock} onCreate={handleCreate} />;
  }

  const closeDialog = () => setDialog(null);

  return (
    <div className="app-root">
      <TopBar onAction={(a) => setDialog(a)} />
      <div className="main-row">
          <Sidebar
            vault={vaults.find((v) => v.id === vault.id) ?? { id: vault.id, name: vault.name, fileName: "", createdAt: vault.createdAt, lastOpenedAt: 0, lastSyncedAt: vault.lastSyncedAt }}
            labels={vault.labels}
            section={section}
            labelFilter={labelFilter}
            onSection={(s) => {
              setSection(s);
              setLabelFilter(null);
            }}
            onLabelFilter={(id) => {
              setLabelFilter(id);
              if (id) setSection("ALL");
            }}
            onSwitchVault={gotoSelect}
            onManageLabels={() => setDialog("LABELS")}
            onLock={() => setDialog("LOCK")}
          />
          <div className="col-divider" />
          <ItemList
            items={filtered}
            allLabels={vault.labels}
            sectionTitle={sectionTitle}
            query={query}
            onQuery={setQuery}
            searchPasswords={searchPasswords}
            selectedId={selectItemId}
            onSelect={setSelectItemId}
            onNew={() => setDialog("TEMPLATE")}
          />
          <div className="col-divider" />
          {selected ? (
            <ItemDetail
              item={selected}
              labels={vault.labels}
              onSave={saveItem}
              onVault={setVault}
              onToggleFavorite={toggleFavorite}
              onDuplicate={duplicateSelected}
              onDelete={trashSelected}
              onRestore={restoreSelected}
              onPurge={purgeSelected}
              notify={notify}
            />
          ) : (
            <div className="detail">
              <div className="empty-hint" style={{ paddingTop: 120 }}>
                选择左侧条目查看详情，或点击“＋ 新建”创建
              </div>
            </div>
          )}
        </div>

      {dialog === "GENERATOR" && <GeneratorDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "SECURITY" && <SecurityDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "WEBDAV" && <WebDavDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "THEME" && <ThemeDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "IMPORT" && <ImportDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "SETTINGS" && <SettingsDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "ABOUT" && <AboutDialog onClose={closeDialog} />}
      {dialog === "SYNC" && <SyncDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "LOCK" && <LockDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "LABELS" && <LabelManagerDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "TEMPLATE" && (
        <TemplatePickerDialog onPick={(id) => void createFromTemplate(id)} onClose={closeDialog} />
      )}

      {toast && <div className={`toast ${toast.kind}`}>{toast.msg}</div>}
    </div>
  );
}
