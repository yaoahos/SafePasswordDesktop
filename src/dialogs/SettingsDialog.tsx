import { useState } from "react";
import { Dialog } from "../components/Dialog";
import {
  backupCreate,
  backupDelete,
  backupList,
  backupRestore,
  changeMasterPassword,
  createVault,
  deleteVault,
  exportCsv,
  exportSafeincloudXml,
  exportVaultJson,
  exportVaultSpv,
  importCsv,
  importSafeincloudXml,
  importVaultBytes,
  importVaultJson,
  renameVault,
  webdavPull,
  webdavSync,
} from "../api";
import type { BackupInfo, WebDavConfig } from "../types";
import { arrayBufferToBase64, downloadBase64, downloadText, formatSize, formatTime } from "../utils";
import type { AppCtx } from "../app-context";

type Tab = "security" | "sync" | "vault" | "display" | "transfer";

const TABS: { key: Tab; label: string }[] = [
  { key: "security", label: "安全" },
  { key: "sync", label: "同步与备份" },
  { key: "vault", label: "密码库" },
  { key: "display", label: "显示与搜索" },
  { key: "transfer", label: "导入与导出" },
];

export function SettingsDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("security");
  return (
    <Dialog title="设置" onClose={onClose} width={560}>
      <div className="tabs" style={{ marginBottom: 8 }}>
        {TABS.map((t) => (
          <button key={t.key} className={`tab${tab === t.key ? " active" : ""}`} onClick={() => setTab(t.key)}>
            {t.label}
          </button>
        ))}
      </div>
      <div className="spacer-10" />
      {tab === "security" && <SecurityTab ctx={ctx} />}
      {tab === "sync" && <SyncTab ctx={ctx} />}
      {tab === "vault" && <VaultTab ctx={ctx} onClose={onClose} />}
      {tab === "display" && <DisplayTab ctx={ctx} />}
      {tab === "transfer" && <TransferTab ctx={ctx} />}
    </Dialog>
  );
}

// ---------------------------------------------------------------- 安全

function SecurityTab({ ctx }: { ctx: AppCtx }) {
  const [minutes, setMinutes] = useState(String(ctx.settings.autoLockMinutes));
  const [oldPw, setOldPw] = useState("");
  const [newPw, setNewPw] = useState("");
  const [confirmPw, setConfirmPw] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);

  const saveMinutes = async () => {
    const n = Number(minutes);
    if (!Number.isFinite(n) || n < 0) {
      setMsg({ ok: false, text: "请输入不小于 0 的分钟数（0 = 关闭自动锁定）" });
      return;
    }
    await ctx.applySettings({ ...ctx.settings, autoLockMinutes: Math.floor(n) });
    setMsg({ ok: true, text: `自动锁定已设为 ${Math.floor(n)} 分钟` });
  };

  const changePw = async () => {
    if (newPw.length < 8) {
      setMsg({ ok: false, text: "新主密码至少 8 位" });
      return;
    }
    if (newPw !== confirmPw) {
      setMsg({ ok: false, text: "两次输入的新主密码不一致" });
      return;
    }
    setBusy(true);
    try {
      await changeMasterPassword(oldPw, newPw);
      setMsg({ ok: true, text: "主密码已修改" });
      setOldPw("");
      setNewPw("");
      setConfirmPw("");
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <div className="section-cap">自动锁定</div>
      <div className="setting-row">
        <div className="setting-text">
          <div className="setting-title">空闲后自动锁定</div>
          <div className="setting-sub">鼠标/键盘闲置该时长后自动锁定密码库，0 = 关闭</div>
        </div>
        <input
          className="input"
          style={{ width: 74, textAlign: "center" }}
          type="number"
          min={0}
          value={minutes}
          onChange={(e) => setMinutes(e.target.value)}
          onBlur={() => void saveMinutes()}
        />
        <span className="muted small">分钟</span>
      </div>

      <div className="hr" />
      <div className="section-cap">修改主密码</div>
      <div className="form-row">
        <label className="field-caption">当前主密码</label>
        <input className="input" type="password" value={oldPw} onChange={(e) => setOldPw(e.target.value)} />
      </div>
      <div className="form-row">
        <label className="field-caption">新主密码（至少 8 位）</label>
        <input className="input" type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)} />
      </div>
      <div className="form-row">
        <label className="field-caption">确认新主密码</label>
        <input className="input" type="password" value={confirmPw} onChange={(e) => setConfirmPw(e.target.value)} />
      </div>
      <button className="btn primary" disabled={busy} onClick={() => void changePw()}>
        {busy ? "修改中…" : "修改主密码"}
      </button>
      {msg && <div className={msg.ok ? "ok-text" : "err-text"}>{msg.text}</div>}
    </>
  );
}

// ---------------------------------------------------------------- 同步与备份（对齐移动端 WebDAV 页）

function SyncTab({ ctx }: { ctx: AppCtx }) {
  const [cfg, setCfg] = useState<WebDavConfig>({ ...ctx.settings.webdav });
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [backups, setBackups] = useState<BackupInfo[] | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<BackupInfo | null>(null);
  const [restorePw, setRestorePw] = useState("");
  const [confirmDel, setConfirmDel] = useState<string | null>(null);
  const [confirmPull, setConfirmPull] = useState(false);

  const patch = (p: Partial<WebDavConfig>) => setCfg((c) => ({ ...c, ...p }));

  const saveCfg = async () => {
    await ctx.applySettings({ ...ctx.settings, webdav: cfg });
  };

  const doSaveAndSync = async () => {
    setBusy("sync");
    setMsg(null);
    try {
      await saveCfg();
      const s = await webdavSync(cfg, "KEEP_LOCAL");
      await ctx.refreshFromBackend();
      setMsg({ ok: true, text: `${s.message || "同步完成"}（${s.direction}）` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const doPull = async () => {
    setBusy("pull");
    setMsg(null);
    try {
      await saveCfg();
      const s = await webdavPull(cfg);
      await ctx.refreshFromBackend();
      setMsg({ ok: true, text: `${s.message || "已拉取云端数据"}（${s.direction}）` });
      setConfirmPull(false);
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const refreshBackups = async () => {
    try {
      setBackups(await backupList());
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const createBackup = async () => {
    setBusy("create");
    try {
      const b = await backupCreate();
      setMsg({ ok: true, text: `备份已创建：${b.name}` });
      await refreshBackups();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const doRestore = async () => {
    if (!restoreTarget) return;
    setBusy("restore");
    try {
      await backupRestore(restoreTarget.path, restorePw);
      await ctx.refreshFromBackend();
      setMsg({ ok: true, text: "备份已恢复" });
      setRestoreTarget(null);
      setRestorePw("");
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const doDeleteBackup = async (b: BackupInfo) => {
    setBusy(`del-${b.path}`);
    try {
      await backupDelete(b.path);
      await refreshBackups();
      setConfirmDel(null);
      setMsg({ ok: true, text: "备份已删除" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const lastSync = ctx.vault?.lastSyncedAt ?? 0;

  return (
    <>
      <div className="settings-group">
        <div className="settings-group-title">
          <span className="settings-group-dot" /> 云端同步（WebDAV）
        </div>
        <div className="info-card">
        <span className="info-ico">☁</span>
        <div>
          <div className="setting-title">只同步加密文件</div>
          <div className="setting-sub">WebDAV 服务器只看到密文；主密码和明文不会上传。</div>
        </div>
      </div>

      <div className="form-row">
        <label className="field-caption">服务器地址</label>
        <input
          className="input"
          placeholder="https://dav.jianguoyun.com/dav/"
          value={cfg.baseUrl}
          onChange={(e) => patch({ baseUrl: e.target.value })}
        />
      </div>
      <div className="form-row">
        <label className="field-caption">用户名</label>
        <input className="input" value={cfg.username} onChange={(e) => patch({ username: e.target.value })} />
      </div>
      <div className="form-row">
        <label className="field-caption">密码 / 应用密码</label>
        <input
          className="input"
          type="password"
          value={cfg.password}
          onChange={(e) => patch({ password: e.target.value })}
        />
      </div>
      <div className="form-row">
        <label className="field-caption">远程目录</label>
        <input
          className="input"
          placeholder="/SafePassword"
          value={cfg.remoteDir}
          onChange={(e) => patch({ remoteDir: e.target.value })}
        />
      </div>

      <div className="setting-row">
        <div className="setting-text">
          <div className="setting-title">保存后自动同步</div>
          <div className="setting-sub">
            {lastSync > 0 ? `上次同步：${formatTime(lastSync)}` : "上次同步：从未"}
          </div>
        </div>
        <input
          type="checkbox"
          checked={cfg.enabled && ctx.settings.autoSync}
          onChange={(e) => {
            const on = e.target.checked;
            patch({ enabled: on });
            void ctx.applySettings({ ...ctx.settings, autoSync: on, webdav: { ...cfg, enabled: on } });
          }}
        />
      </div>

      <div className="hr" />
      <div className="section-cap">自动拉取云端更新</div>
      <div className="seg">
        {[0, 30, 60, 300].map((v) => (
          <button
            key={v}
            className={ctx.settings.pullIntervalSeconds === v ? "active" : ""}
            onClick={() => void ctx.applySettings({ ...ctx.settings, pullIntervalSeconds: v })}
          >
            {v === 0 ? "关闭" : v < 60 ? `${v} 秒` : `${v / 60} 分钟`}
          </button>
        ))}
      </div>
      <div className="setting-sub" style={{ marginTop: 4 }}>
        解锁后与回到前台时也会各检查一次；只有云端变化时才下载。
      </div>

      <div className="spacer-10" />
      <div className="row" style={{ justifyContent: "flex-end", gap: 8 }}>
        <button className="btn" disabled={busy !== null} onClick={() => setConfirmPull(true)}>
          拉取云端数据（覆盖本地）
        </button>
        <button className="btn" disabled={busy !== null} onClick={() => void doSaveAndSync()}>
          {busy === "sync" ? "同步中…" : "保存并立即同步"}
        </button>
      </div>
      <div className="setting-sub" style={{ marginTop: 6 }}>
        坚果云用户请使用应用密码；远端建议使用 HTTPS。
      </div>

      {confirmPull && (
        <div className="confirm-box">
          <div className="small">将用云端密码库覆盖本地密码库。本地尚未同步的修改会丢失，是否继续？</div>
          <div className="row" style={{ marginTop: 8, justifyContent: "flex-end" }}>
            <button className="btn sm" onClick={() => setConfirmPull(false)}>
              取消
            </button>
            <button className="btn sm danger" disabled={busy !== null} onClick={() => void doPull()}>
              覆盖本地
            </button>
          </div>
        </div>
      )}

      </div>

      <div className="spacer-10" />

      <div className="settings-group">
        <div className="settings-group-title">
          <span className="settings-group-dot" /> 本地备份
        </div>
        <div className="form-row">
        <label className="field-caption">自动备份</label>
        <div className="seg">
          {[
            { v: 0, label: "关闭" },
            { v: 1, label: "每天" },
            { v: 3, label: "每 3 天" },
            { v: 7, label: "每 7 天" },
            { v: 30, label: "每 30 天" },
          ].map((o) => (
            <button
              key={o.v}
              className={ctx.settings.autoBackupDays === o.v ? "active" : ""}
              onClick={() => void ctx.applySettings({ ...ctx.settings, autoBackupDays: o.v })}
            >
              {o.label}
            </button>
          ))}
        </div>
      </div>
      {ctx.settings.lastBackupAt > 0 && (
        <div className="muted small" style={{ marginTop: 6 }}>
          上次备份：{formatTime(ctx.settings.lastBackupAt)}
        </div>
      )}
      <div className="row" style={{ marginTop: 8, justifyContent: "flex-end" }}>
        <button className="btn" disabled={busy !== null} onClick={() => void createBackup()}>
          {busy === "create" ? "创建中…" : "立即创建备份"}
        </button>
      </div>
      <div className="spacer-10" />
      {backups === null && <div className="muted small">打开此页会自动列出备份；没有则可点“立即创建备份”。</div>}
      {backups !== null && backups.length === 0 && <div className="muted small">暂无备份</div>}
      {backups?.map((b) => (
        <div key={b.path}>
          <div className="mini-row">
            <span className="grow">
              <b>{b.name}</b>
              <div className="muted small">
                {formatTime(b.createdAt)} · {formatSize(b.size)}
              </div>
            </span>
            <button
              className="link-btn"
              onClick={() => {
                setRestoreTarget(restoreTarget?.path === b.path ? null : b);
                setRestorePw("");
              }}
            >
              恢复
            </button>
            <button
              className="link-btn"
              style={{ color: "var(--danger)" }}
              onClick={() => setConfirmDel(confirmDel === b.path ? null : b.path)}
            >
              删除
            </button>
          </div>
          {confirmDel === b.path && (
            <div className="row" style={{ paddingLeft: 10 }}>
              <span className="small" style={{ color: "var(--danger)" }}>
                确认删除该备份？
              </span>
              <button className="btn sm danger" disabled={busy !== null} onClick={() => void doDeleteBackup(b)}>
                确认删除
              </button>
              <button className="btn sm" onClick={() => setConfirmDel(null)}>
                取消
              </button>
            </div>
          )}
          {restoreTarget?.path === b.path && (
            <div className="row" style={{ paddingLeft: 10 }}>
              <input
                className="input"
                type="password"
                style={{ width: 200, height: 32 }}
                placeholder="该备份的主密码"
                value={restorePw}
                onChange={(e) => setRestorePw(e.target.value)}
              />
              <button className="btn sm primary" disabled={busy !== null || !restorePw} onClick={() => void doRestore()}>
                确认恢复
              </button>
            </div>
          )}
        </div>
      ))}
      </div>
      {msg && <div className={msg.ok ? "ok-text" : "err-text"}>{msg.text}</div>}
    </>
  );
}

// ---------------------------------------------------------------- 密码库（当前库用主题色标识，列表改下拉）

function VaultTab({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const vault = ctx.vault;
  const [newName, setNewName] = useState("");
  const [newPw, setNewPw] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [confirmDel, setConfirmDel] = useState(false);

  if (!vault) return null;

  const rename = async () => {
    const trimmed = newName.trim();
    if (!trimmed) {
      setMsg({ ok: false, text: "请输入新名称" });
      return;
    }
    try {
      await renameVault(vault.id, trimmed);
      await ctx.refreshFromBackend();
      setMsg({ ok: true, text: "密码库已改名" });
      setNewName("");
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const create = async () => {
    if (!newName.trim() || newPw.length < 8) {
      setMsg({ ok: false, text: "请填写名称和至少 8 位的主密码" });
      return;
    }
    setBusy(true);
    try {
      await createVault(newName.trim(), newPw);
      await ctx.refreshFromBackend();
      setMsg({ ok: true, text: "已创建并切换到新密码库" });
      setNewName("");
      setNewPw("");
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!confirmDel) {
      setConfirmDel(true);
      return;
    }
    setBusy(true);
    try {
      await deleteVault(vault.id);
      setMsg({ ok: true, text: "密码库已删除" });
      ctx.gotoSelect();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(false);
      setConfirmDel(false);
    }
  };

  return (
    <>
      <div className="section-cap">密码库</div>
      <select
        className="input"
        value={vault.id}
        onChange={(e) => {
          if (e.target.value === vault.id) return;
          ctx.gotoSelect();
          onClose();
        }}
      >
        {ctx.vaults.map((v) => (
          <option key={v.id} value={v.id}>
            {v.id === vault.id ? `${v.name}（当前）` : v.name}
          </option>
        ))}
      </select>
      <div className="vault-current" style={{ marginTop: 8 }}>
        <span className="vault-dot" />
        <span className="grow">
          <b style={{ color: "var(--primary)" }}>{vault.name}</b>
          <div className="muted small">
            条目 {vault.items.length} 个 · 标签 {vault.labels.length} 个 · 上次打开 {formatTime(vault.createdAt)}
          </div>
        </span>
      </div>
      <div className="setting-sub" style={{ marginTop: 6 }}>
        每个密码库有独立的主密码和文件。切换后请在解锁页输入该库主密码。
      </div>

      <div className="hr" />
      <div className="section-cap">改名</div>
      <div className="row">
        <input
          className="input grow"
          placeholder="新名称"
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
        />
        <button className="btn sm" onClick={() => void rename()}>
          重命名
        </button>
      </div>

      <div className="hr" />
      <div className="section-cap">新建密码库</div>
      <div className="form-row">
        <label className="field-caption">名称</label>
        <input className="input" value={newName} onChange={(e) => setNewName(e.target.value)} />
      </div>
      <div className="form-row">
        <label className="field-caption">主密码（至少 8 位）</label>
        <input className="input" type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)} />
      </div>
      <button className="btn primary tall block" disabled={busy} onClick={() => void create()}>
        新建并切换
      </button>

      <div className="hr" />
      <div className="section-cap" style={{ color: "var(--danger)" }}>
        删除密码库
      </div>
      <div className="row">
        <button className="btn sm danger" disabled={busy} onClick={() => void remove()}>
          {confirmDel ? "再次点击确认删除" : "删除当前密码库"}
        </button>
        {confirmDel && (
          <button className="btn sm" onClick={() => setConfirmDel(false)}>
            取消
          </button>
        )}
      </div>
      {msg && <div className={msg.ok ? "ok-text" : "err-text"}>{msg.text}</div>}
    </>
  );
}

// ---------------------------------------------------------------- 显示与搜索（主题已移到顶部“主题”页）

function DisplayTab({ ctx }: { ctx: AppCtx }) {
  return (
    <>
      <div className="section-cap">默认展示</div>
      <select
        className="input"
        value={ctx.settings.defaultLabelId ?? ""}
        onChange={(e) => void ctx.applySettings({ ...ctx.settings, defaultLabelId: e.target.value || null })}
      >
        <option value="">全部条目</option>
        {(ctx.vault?.labels ?? []).map((l) => (
          <option key={l.id} value={l.id}>
            {l.name}
          </option>
        ))}
      </select>
      <div className="setting-sub" style={{ marginTop: 4 }}>
        打开密码库时自动按此标签筛选；选择“全部条目”则不筛选。
      </div>

      <div className="hr" />
      <div className="section-cap">搜索框行为</div>
      <label className="check-row">
        <input
          type="checkbox"
          checked={ctx.searchPasswords}
          onChange={(e) => ctx.setSearchPasswords(e.target.checked)}
        />
        搜索时包含敏感字段（密码 / 机密值）
      </label>
      <div className="setting-sub" style={{ marginTop: 4 }}>
        关闭时搜索只匹配标题、账号、网址与备注，不会泄露密码内容。
      </div>
    </>
  );
}

// ---------------------------------------------------------------- 导入与导出（只允许文件选择，不提供文本输入）

function TransferTab({ ctx }: { ctx: AppCtx }) {
  const [jsonPw, setJsonPw] = useState("");
  const [jsonName, setJsonName] = useState("");
  const [sicvPw, setSicvPw] = useState("");
  const [sicvName, setSicvName] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const run = async (key: string, fn: () => Promise<void>) => {
    setBusy(key);
    setErr(null);
    try {
      await fn();
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const doExport = (kind: "csv" | "xml" | "json") =>
    void run(`exp-${kind}`, async () => {
      const text =
        kind === "csv" ? await exportCsv() : kind === "xml" ? await exportSafeincloudXml() : await exportVaultJson();
      const ext = kind === "csv" ? "csv" : kind === "xml" ? "xml" : "json";
      const name = (ctx.vault?.name ?? "vault").replace(/[\\/:*?"<>|]/g, "_");
      downloadText(`${name}.${ext}`, text, kind === "csv" ? "text/csv;charset=utf-8" : "application/json");
      setMsg({ ok: true, text: `已导出 ${name}.${ext}` });
    });

  const doExportSicv = () =>
    void run("exp-spv", async () => {
      const b64 = await exportVaultSpv();
      const name = (ctx.vault?.name ?? "vault").replace(/[\\/:*?"<>|]/g, "_");
      downloadBase64(`${name}.spv`, b64);
      setMsg({ ok: true, text: `已导出 ${name}.spv（加密，可拷回手机）` });
    });

  const importTextFile = (f: File) =>
    void run("imp-file", async () => {
      const text = await f.text();
      const ext = (f.name.split(".").pop() ?? "").toLowerCase();
      if (ext === "csv") {
        const n = await importCsv(text);
        await ctx.refreshFromBackend();
        setMsg({ ok: true, text: `成功导入 ${n} 条（CSV）` });
      } else if (ext === "xml") {
        const n = await importSafeincloudXml(text);
        await ctx.refreshFromBackend();
        setMsg({ ok: true, text: `成功导入 ${n} 条（XML）` });
      } else if (ext === "json") {
        if (!jsonPw) throw new Error("导入 JSON 需要先填写新库主密码");
        const meta = await importVaultJson(text, jsonPw, jsonName.trim() || "导入的密码库");
        await ctx.refreshFromBackend();
        setJsonPw("");
        setJsonName("");
        setMsg({ ok: true, text: `已导入密码库「${meta.name}」并切换过去` });
      } else {
        throw new Error("不支持的文件类型（仅支持 .csv / .xml / .json）");
      }
    });

  const importSicv = (f: File) =>
    void run("imp-sicv", async () => {
      const buf = await f.arrayBuffer();
      const meta = await importVaultBytes(
        arrayBufferToBase64(buf),
        sicvPw,
        sicvName.trim() || f.name.replace(/\.(sicv|spv)$/i, "") || "导入的密码库",
      );
      await ctx.refreshFromBackend();
      setSicvPw("");
      setSicvName("");
      setMsg({ ok: true, text: `已导入密码库「${meta.name}」并切换过去` });
    });

  return (
    <>
      <div className="section-cap">导出</div>
      <button className="btn primary tall block" disabled={busy !== null || !ctx.vault} onClick={doExportSicv}>
        {busy === "exp-spv" ? "导出中…" : "导出加密备份（.spv，可拷回手机）"}
      </button>
      <div className="setting-sub" style={{ marginTop: 6 }}>
        加密备份与手机端同格式，可直接放回手机或 WebDAV；下面的导出为明文格式，注意保管。
      </div>
      <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("csv")}>
          导出 CSV
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("xml")}>
          导出 XML
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("json")}>
          导出明文 JSON（不安全）
        </button>
      </div>

      <div className="hr" />
      <div className="section-cap">导入条目（CSV / XML / 明文 JSON）</div>
      <div className="row" style={{ marginBottom: 8 }}>
        <input
          className="input grow"
          type="password"
          placeholder="新库主密码（仅 JSON 导入需要）"
          value={jsonPw}
          onChange={(e) => setJsonPw(e.target.value)}
        />
        <input
          className="input grow"
          placeholder="库名称（默认：导入的密码库）"
          value={jsonName}
          onChange={(e) => setJsonName(e.target.value)}
        />
      </div>
      <FilePicker
        label="选择文件导入…"
        accept=".csv,.xml,.json"
        disabled={busy !== null}
        onFile={importTextFile}
      />
      <div className="setting-sub" style={{ marginTop: 6 }}>
        CSV / XML 导入到当前密码库；JSON 会新建一个密码库并切换过去。
      </div>

      <div className="hr" />
      <div className="section-cap">导入加密备份（.spv，兼容旧版 .sicv）</div>
      <div className="row" style={{ marginBottom: 8 }}>
        <input
          className="input grow"
          type="password"
          placeholder="备份主密码"
          value={sicvPw}
          onChange={(e) => setSicvPw(e.target.value)}
        />
        <input
          className="input grow"
          placeholder="库名称（默认取文件名）"
          value={sicvName}
          onChange={(e) => setSicvName(e.target.value)}
        />
      </div>
      <FilePicker
        label="选择加密备份文件…"
        accept=".sicv,.spv"
        disabled={busy !== null}
        onFile={importSicv}
      />

      {msg && <div className="ok-text">{msg.text}</div>}
      {err && <div className="err-text">{err}</div>}
    </>
  );
}

function FilePicker({
  label,
  accept,
  disabled,
  onFile,
}: {
  label: string;
  accept: string;
  disabled: boolean;
  onFile: (f: File) => void;
}) {
  return (
    <label className="btn sm" style={{ display: "inline-block", cursor: "pointer" }}>
      {label}
      <input
        type="file"
        accept={accept}
        style={{ display: "none" }}
        disabled={disabled}
        onChange={(e) => {
          const f = e.target.files?.[0];
          if (f) onFile(f);
          e.target.value = "";
        }}
      />
    </label>
  );
}
