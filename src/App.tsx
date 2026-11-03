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
  importVaultBytes,
  webdavSync,
  webdavPullIfChanged,
} from "./api";
import type { AppSettings, SortMode, Vault, VaultItem, VaultMeta } from "./types";
import type { AppCtx, SectionFilter, ToastKind } from "./app-context";
import { AuthScreens } from "./components/AuthScreens";
import { ItemDetail } from "./components/ItemDetail";
import { ItemList } from "./components/ItemList";
import { Sidebar } from "./components/Sidebar";
import { TopBar, type TopAction } from "./components/TopBar";
import { AboutDialog } from "./dialogs/AboutDialog";
import { GeneratorDialog } from "./dialogs/GeneratorDialog";
import { LabelManagerDialog } from "./dialogs/LabelManagerDialog";
import { LockDialog } from "./dialogs/LockDialog";
import { SecurityDialog } from "./dialogs/SecurityDialog";
import { SettingsDialog } from "./dialogs/SettingsDialog";
import { ThemeDialog } from "./dialogs/ThemeDialog";
import { duplicateItem, itemMatches, newItemFromTemplate } from "./utils";

type DialogKind = TopAction | "LABELS" | null;

const DEFAULT_SETTINGS: AppSettings = {
  theme: "system",
  accent: "#1677FF",
  autoLockMinutes: 5,
  hibpApiKey: "",
  webdav: { baseUrl: "", username: "", password: "", remoteDir: "/SafePassword", enabled: false },
  autoBackupDays: 1,
  lastBackupAt: 0,
  autoSync: true,
  pullIntervalSeconds: 60,
  searchPasswords: false,
  defaultLabelId: null,
  lastVaultId: null,
};

const SECTION_TITLES: Record<SectionFilter, string> = {
  ALL: "全部",
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
  /** 新建条目草稿：不落库，手动保存才写入（v1 项 6） */
  const [draftNew, setDraftNew] = useState<VaultItem | null>(null);
  /** 从设置页「切换」进入解锁页时要预选密码库 */
  const [preselectVaultId, setPreselectVaultId] = useState<string | null>(null);
  const [toast, setToast] = useState<{ id: number; msg: string; kind: ToastKind } | null>(null);
  const [sortMode, setSortMode] = useState<SortMode>("CREATED");
  const [sortAsc, setSortAsc] = useState(false);

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

  // 去掉网页感右键菜单（刷新/打印/另存为…）；输入框内保留系统菜单以便粘贴（v1 项 0）
  useEffect(() => {
    const onCtx = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA")) return;
      e.preventDefault();
    };
    document.addEventListener("contextmenu", onCtx);
    return () => document.removeEventListener("contextmenu", onCtx);
  }, []);

  // ------------------------------------------------------------ 主题

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const dark = settings.theme === "dark" || (settings.theme === "system" && mq.matches);
      document.documentElement.dataset.theme = dark ? "dark" : "light";
      // 强调色（对齐手机端 ThemeScreen：预设/自定义色覆盖 --primary）
      const accent = /^#[0-9a-fA-F]{6}$/.test(settings.accent || "") ? settings.accent : "#1677FF";
      const root = document.documentElement.style;
      root.setProperty("--primary", accent);
      root.setProperty("--primary-soft", `${accent}1F`);
    };
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [settings.theme, settings.accent]);

  const applySettings = useCallback(async (next: AppSettings) => {
    setSettings(next);
    try {
      await setSettingsApi(next);
    } catch (e) {
      notify(`保存设置失败：${String(e)}`, "error");
    }
  }, [notify]);

  const setSearchPasswords = useCallback(
    (v: boolean) => {
      void applySettings({ ...settings, searchPasswords: v });
    },
    [applySettings, settings],
  );

  /** 对齐手机端 setSortMode：同模式翻转方向；切模式则标题升序、时间类降序（不持久化） */
  const changeSort = useCallback(
    (m: SortMode) => {
      if (m === sortMode) setSortAsc(!sortAsc);
      else {
        setSortMode(m);
        setSortAsc(m === "TITLE");
      }
    },
    [sortMode, sortAsc],
  );

  // 开库时应用“默认展示”标签（对齐手机端 filterLabelId = defaultLabelIdFor）
  useEffect(() => {
    if (!vault) return;
    const d = settings.defaultLabelId;
    setLabelFilter(d && vault.labels.some((l) => l.id === d) ? d : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vault?.id]);

  // 设置里改“默认展示”时立即应用（对齐手机端 setDefaultLabel）
  useEffect(() => {
    if (!vault) return;
    const d = settings.defaultLabelId;
    if (d && !vault.labels.some((l) => l.id === d)) return; // 失效 id 忽略
    setLabelFilter(d);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [settings.defaultLabelId]);

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
    setPreselectVaultId(null);
    void closeVault().catch(() => undefined);
  }, []);

  const gotoSelect = useCallback((vaultId?: string) => {
    setPreselectVaultId(vaultId ?? null);
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
        setPreselectVaultId(null);
        setSelectItemId(v.items.find((i) => i.status !== "TRASHED")?.id ?? null);
        setSection("ALL");
        setLabelFilter(null);
        setQuery("");
        void listVaults().then(setVaults).catch(() => undefined);
        // 解锁后检查一次云端更新（对齐手机端：解锁后与回到前台各检查一次）
        void (async () => {
          try {
            const cfg = settings.webdav;
            if (!cfg.enabled || !/^https?:\/\//i.test(cfg.baseUrl)) return;
            if (settings.pullIntervalSeconds <= 0) return;
            const r = await webdavPullIfChanged(cfg);
            if (r) await refreshFromBackend();
          } catch {
            /* 静默失败，对齐手机端 runCatching */
          }
        })();
        return null;
      } catch (e) {
        return String(e);
      }
    },
    [settings.webdav, settings.pullIntervalSeconds, refreshFromBackend],
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

  // ------------------------------------------------------------ 从本地恢复（对齐手机端 UnlockScreen）

  const handleRestoreLocal = useCallback(
    async (fileName: string, base64Data: string, password: string): Promise<string | null> => {
      try {
        const base = fileName.replace(/\.(sicv|spv)$/i, "");
        await importVaultBytes(base64Data, password, base || "恢复的密码库");
        await refreshFromBackend();
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

  // ------------------------------------------------------------ 自动同步（保存后延迟上传，对齐手机端 scheduleSync）

  const syncSigRef = useRef<string>("");
  useEffect(() => {
    if (!vault) {
      syncSigRef.current = "";
      return;
    }
    const sig = `${vault.updatedAt}:${vault.revision}`;
    const prev = syncSigRef.current;
    syncSigRef.current = sig;
    if (!prev || prev === sig) return; // 首次装载或无变化：不触发
    const cfg = settings.webdav;
    if (!settings.autoSync || !cfg.enabled || !/^https?:\/\//i.test(cfg.baseUrl)) return;
    const t = window.setTimeout(() => {
      void (async () => {
        try {
          await webdavSync(cfg, "KEEP_LOCAL"); // 本地修改优先，对齐手机端 doSync 默认策略
          await refreshFromBackend();
        } catch (e) {
          notify(`自动同步失败：${String(e)}`, "error");
        }
      })();
    }, 1500); // 对齐手机端 scheduleSync 的 1500ms 防抖
    return () => window.clearTimeout(t);
  }, [vault, settings.autoSync, settings.webdav, refreshFromBackend, notify]);

  // ------------------------------------------------------------ 自动拉取（对齐手机端 restartPullLoop）

  useEffect(() => {
    const secs = settings.pullIntervalSeconds;
    const cfg = settings.webdav;
    if (!vault || secs <= 0 || !cfg.enabled || !/^https?:\/\//i.test(cfg.baseUrl)) return;
    const iv = window.setInterval(() => {
      void (async () => {
        try {
          const r = await webdavPullIfChanged(cfg);
          if (r) await refreshFromBackend(); // 有更新才刷新（静默，对齐手机端 silent pull）
        } catch (e) {
          // 对齐手机端 runCatching：拉取失败不打扰用户
          console.warn("自动拉取失败", e);
        }
      })();
    }, secs * 1000);
    return () => window.clearInterval(iv);
  }, [vault, settings.pullIntervalSeconds, settings.webdav, refreshFromBackend]);

  // ------------------------------------------------------------ 手动同步（顶栏"同步"直接执行，对齐手机端 doSync）

  const [syncState, setSyncState] = useState<{ kind: "working" | "ok" | "err"; text: string } | null>(null);
  const syncStateTimer = useRef<number | null>(null);

  const flashSync = useCallback((kind: "ok" | "err", text: string) => {
    setSyncState({ kind, text });
    if (syncStateTimer.current !== null) window.clearTimeout(syncStateTimer.current);
    // 对齐手机端：状态文本 3 秒后自动消失
    syncStateTimer.current = window.setTimeout(() => setSyncState(null), 3000);
  }, []);

  const runSync = useCallback(async () => {
    const cfg = settings.webdav;
    if (!cfg.enabled || !/^https?:\/\//i.test(cfg.baseUrl)) {
      notify("请先在设置 → 同步与备份中配置 WebDAV", "error");
      return;
    }
    setSyncState({ kind: "working", text: "同步中…" });
    try {
      // 与手机端一致：直接同步、本地优先，无需弹窗确认
      const r = await webdavSync(cfg, "KEEP_LOCAL");
      await refreshFromBackend();
      flashSync("ok", r.message);
    } catch (e) {
      flashSync("err", String(e));
    }
  }, [settings.webdav, refreshFromBackend, notify, flashSync]);

  // ------------------------------------------------------------ 条目操作

  const saveItem = useCallback(
    async (next: VaultItem) => {
      const v = await putItem(next);
      setVault(v);
      // 首次保存后草稿转为正式条目（对齐手机端保存后 existing!=null）
      setDraftNew((d) => (d && d.id === next.id ? null : d));
      setSelectItemId(next.id);
    },
    [],
  );

  /** 列表行内收藏：点击即生效（对齐手机端，不走草稿保存） */
  const toggleFavorite = useCallback(
    async (item: VaultItem) => {
      try {
        const v = await putItem({ ...item, favorite: !item.favorite, updatedAt: Date.now() });
        setVault(v);
      } catch (e) {
        notify(String(e), "error");
      }
    },
    [notify],
  );

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
    (templateId: string) => {
      if (!vault) return;
      // 只建草稿：不落库、不预填标题、不带备注（v1 项 6，对齐手机端"未保存的新建"）
      const item = newItemFromTemplate(templateId);
      item.title = "";
      item.notes = "";
      setDraftNew(item);
      setSelectItemId(null);
      setSection("ALL");
      setLabelFilter(null);
      setQuery("");
      setDialog(null);
    },
    [vault],
  );

  /** 新建草稿直接丢弃（未落库） */
  const discardDraft = useCallback(() => setDraftNew(null), []);

  // ------------------------------------------------------------ 过滤 / 选中

  const items = vault?.items ?? [];
  const filtered = useMemo(() => {
    const q = query.trim();
    const arr = items.filter((i) => {
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
      return itemMatches(i, q, settings.searchPasswords);
    });
    // 排序（对齐手机端 SortMode：标题拼音升序 / 最近更新 / 最近创建，时间类默认降序）
    if (sortMode === "TITLE") arr.sort((a, b) => a.title.localeCompare(b.title, "zh-Hans-CN"));
    else if (sortMode === "UPDATED") arr.sort((a, b) => a.updatedAt - b.updatedAt);
    else arr.sort((a, b) => a.createdAt - b.createdAt);
    if (!sortAsc) arr.reverse();
    // 收藏置顶（对齐手机端：收藏条目始终排最前，组内保持所选排序）
    arr.sort((a, b) => (a.favorite === b.favorite ? 0 : a.favorite ? -1 : 1));
    return arr;
  }, [items, section, labelFilter, query, settings.searchPasswords, sortMode, sortAsc]);

  // 选中项不在可见列表时，自动落到第一个（新建草稿不受影响）
  useEffect(() => {
    if (draftNew) return;
    if (filtered.length === 0) {
      if (selectItemId !== null) setSelectItemId(null);
      return;
    }
    if (!filtered.some((i) => i.id === selectItemId)) setSelectItemId(filtered[0].id);
  }, [filtered, selectItemId, draftNew]);

  const selected = useMemo(
    () => draftNew ?? filtered.find((i) => i.id === selectItemId) ?? null,
    [draftNew, filtered, selectItemId],
  );

  const sectionTitle = labelFilter
    ? (vault?.labels.find((l) => l.id === labelFilter)?.name ?? "")
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
    searchPasswords: settings.searchPasswords,
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
    return (
      <AuthScreens
        vaults={vaults}
        defaultVaultId={preselectVaultId ?? settings.lastVaultId}
        onUnlock={handleUnlock}
        onCreate={handleCreate}
        onRestoreLocal={handleRestoreLocal}
      />
    );
  }

  const closeDialog = () => setDialog(null);

  return (
    <div className="app-root">
      <TopBar
        onAction={(a) => {
          if (a === "SYNC") {
            void runSync();
            return;
          }
          setDialog(a);
        }}
      />
      <div className="main-row">
          <Sidebar
            vault={vaults.find((v) => v.id === vault.id) ?? { id: vault.id, name: vault.name, fileName: "", createdAt: vault.createdAt, lastOpenedAt: 0, lastSyncedAt: vault.lastSyncedAt }}
            itemCount={vault.items.filter((i) => i.status !== "TRASHED").length}
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
            onManageLabels={() => setDialog("LABELS")}
          />
          <div className="col-divider" />
          <ItemList
            items={filtered}
            allLabels={vault.labels}
            sectionTitle={sectionTitle}
            query={query}
            onQuery={setQuery}
            searchPasswords={settings.searchPasswords}
            sortMode={sortMode}
            sortAsc={sortAsc}
            onChangeSort={changeSort}
            selectedId={selectItemId}
            onSelect={(id) => {
              setDraftNew(null);
              setSelectItemId(id);
            }}
            onNew={() => createFromTemplate("login")}
            onToggleFavorite={(it) => void toggleFavorite(it)}
            syncText={syncState?.text ?? null}
          />
          <div className="col-divider" />
          {selected ? (
            <ItemDetail
              item={selected}
              labels={vault.labels}
              onSave={saveItem}
              onDuplicate={duplicateSelected}
              onDelete={draftNew ? async () => discardDraft() : trashSelected}
              onRestore={restoreSelected}
              onPurge={purgeSelected}
              notify={notify}
              isNew={!!draftNew}
              onSaveAndCreate={() => {
                void createFromTemplate("login");
              }}
            />
          ) : (
            <div className="detail">
              <div className="empty-hint" style={{ paddingTop: 120 }}>
                选择左侧条目查看详情，或点击“＋ 新建”创建
              </div>
            </div>
          )}
        </div>

      {dialog === "GENERATOR" && <GeneratorDialog notify={ctx.notify} onClose={closeDialog} />}
      {dialog === "SECURITY" && <SecurityDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "THEME" && <ThemeDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "SETTINGS" && <SettingsDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "ABOUT" && <AboutDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "LOCK" && <LockDialog ctx={ctx} onClose={closeDialog} />}
      {dialog === "LABELS" && <LabelManagerDialog ctx={ctx} onClose={closeDialog} />}

      {toast && <div className={`toast ${toast.kind}`}>{toast.msg}</div>}
    </div>
  );
}
