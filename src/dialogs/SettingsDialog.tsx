import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { WebDavForm } from "./WebDavDialog";
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
  exportVaultSicv,
  importCsv,
  importSafeincloudXml,
  importVaultBytes,
  renameVault,
  webdavPull,
  webdavSync,
} from "../api";
import type { BackupInfo, ThemeMode } from "../types";
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

const THEMES: { key: ThemeMode; label: string; icon: string }[] = [
  { key: "system", label: "跟随系统", icon: "🖥" },
  { key: "light", label: "浅色", icon: "☀️" },
  { key: "dark", label: "深色", icon: "🌙" },
];

export function SettingsDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("security");
  return (
    <Dialog title="设置" onClose={onClose} showCloseButton={false} width={540}>
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
    if (newPw.length < 4) {
      setMsg({ ok: false, text: "新主密码至少 4 位" });
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
        <label className="field-caption">新主密码</label>
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

// ---------------------------------------------------------------- 同步与备份

function SyncTab({ ctx }: { ctx: AppCtx }) {
  const [backups, setBackups] = useState<BackupInfo[] | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [restoreTarget, setRestoreTarget] = useState<BackupInfo | null>(null);
  const [restorePw, setRestorePw] = useState("");
  const [confirmDel, setConfirmDel] = useState<string | null>(null);
  const [importText, setImportText] = useState("");

  const cfg = ctx.settings.webdav;

  const refreshBackups = async () => {
    try {
      setBackups(await backupList());
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    }
  };

  const doSync = async (kind: "sync" | "pull") => {
    setBusy(kind);
    setMsg(null);
    try {
      const s = kind === "sync" ? await webdavSync(cfg, "DETECT") : await webdavPull(cfg);
      await ctx.refreshFromBackend();
      setMsg({ ok: true, text: `${s.message || "同步完成"}（${s.direction}，冲突 ${s.conflicts}）` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const createBackup = async () => {
    setBusy("create");
    setMsg(null);
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
    setMsg(null);
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

  return (
    <>
      <div className="section-cap">WebDAV</div>
      <WebDavForm ctx={ctx} />
      <div className="spacer-10" />
      <div className="row">
        <button className="btn primary sm" disabled={busy !== null} onClick={() => void doSync("sync")}>
          {busy === "sync" ? "同步中…" : "立即同步"}
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => void doSync("pull")}>
          {busy === "pull" ? "拉取中…" : "立即拉取"}
        </button>
      </div>

      <div className="hr" />
      <div className="section-cap">本地备份</div>
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
      <div className="row">
        <button className="btn sm" disabled={busy !== null} onClick={() => void createBackup()}>
          {busy === "create" ? "创建中…" : "创建备份"}
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => void refreshBackups()}>
          刷新列表
        </button>
      </div>
      <div className="spacer-10" />
      {backups === null && <div className="muted small">点击“刷新列表”查看备份</div>}
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

      <div className="hr" />
      <div className="section-cap">从文本导入（CSV）</div>
      <textarea
        className="input"
        rows={3}
        placeholder="粘贴 CSV 文本…"
        value={importText}
        onChange={(e) => setImportText(e.target.value)}
      />
      <div className="spacer-10" />
      <button
        className="btn sm primary"
        disabled={!importText.trim() || busy !== null}
        onClick={async () => {
          setBusy("import");
          try {
            const n = await importCsv(importText);
            await ctx.refreshFromBackend();
            setImportText("");
            setMsg({ ok: true, text: `成功导入 ${n} 条` });
          } catch (e) {
            setMsg({ ok: false, text: String(e) });
          } finally {
            setBusy(null);
          }
        }}
      >
        导入 CSV
      </button>
      {msg && <div className={msg.ok ? "ok-text" : "err-text"}>{msg.text}</div>}
    </>
  );
}

// ---------------------------------------------------------------- 密码库

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
    if (!newName.trim() || newPw.length < 4) {
      setMsg({ ok: false, text: "请填写名称和至少 4 位的主密码" });
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
      <div className="section-cap">当前密码库</div>
      <div className="setting-row">
        <div className="setting-text">
          <div className="setting-title">{vault.name}</div>
          <div className="setting-sub">条目 {ctx.vault?.items.length ?? 0} 个 · 标签 {ctx.vault?.labels.length ?? 0} 个</div>
        </div>
        <button
          className="btn sm"
          onClick={() => {
            onClose();
            ctx.gotoSelect();
          }}
        >
          切换
        </button>
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
        <label className="field-caption">主密码（至少 4 位）</label>
        <input className="input" type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)} />
      </div>
      <button className="btn sm primary" disabled={busy} onClick={() => void create()}>
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

// ---------------------------------------------------------------- 显示与搜索

function DisplayTab({ ctx }: { ctx: AppCtx }) {
  return (
    <>
      <div className="section-cap">主题</div>
      <div className="choice-grid">
        {THEMES.map((t) => (
          <button
            key={t.key}
            className={`choice-card${ctx.settings.theme === t.key ? " active" : ""}`}
            onClick={() => void ctx.applySettings({ ...ctx.settings, theme: t.key })}
          >
            <span className="big">{t.icon}</span>
            <span>{t.label}</span>
          </button>
        ))}
      </div>

      <div className="hr" />
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

// ---------------------------------------------------------------- 导入与导出

function TransferTab({ ctx }: { ctx: AppCtx }) {
  const [csvIn, setCsvIn] = useState("");
  const [xmlIn, setXmlIn] = useState("");
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

  // 加密 .sicv：与手机端同格式，拷回手机 / 同步盘均可直接使用
  const doExportSicv = () =>
    void run("exp-sicv", async () => {
      const b64 = await exportVaultSicv();
      const name = (ctx.vault?.name ?? "vault").replace(/[\\/:*?"<>|]/g, "_");
      downloadBase64(`${name}.sicv`, b64);
      setMsg({ ok: true, text: `已导出 ${name}.sicv（加密，可拷回手机）` });
    });

  return (
    <>
      <div className="section-cap">导出</div>
      <div className="row" style={{ flexWrap: "wrap" }}>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("csv")}>
          导出 CSV
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("xml")}>
          导出 SafeInCloud XML
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("json")}>
          导出明文 JSON（不安全）
        </button>
        <button className="btn sm" disabled={busy !== null || !ctx.vault} onClick={doExportSicv}>
          导出 .sicv（加密，可拷回手机）
        </button>
      </div>

      <div className="hr" />
      <div className="section-cap">导入 CSV</div>
      <textarea
        className="input"
        rows={4}
        placeholder={"title,username,password,url,notes\n…"}
        value={csvIn}
        onChange={(e) => setCsvIn(e.target.value)}
      />
      <div className="spacer-10" />
      <button
        className="btn sm primary"
        disabled={busy !== null || !csvIn.trim()}
        onClick={() =>
          void run("imp-csv", async () => {
            const n = await importCsv(csvIn);
            await ctx.refreshFromBackend();
            setCsvIn("");
            setMsg({ ok: true, text: `成功导入 ${n} 条` });
          })
        }
      >
        {busy === "imp-csv" ? "导入中…" : "导入 CSV"}
      </button>

      <div className="hr" />
      <div className="section-cap">导入 SafeInCloud XML</div>
      <textarea
        className="input"
        rows={4}
        placeholder="<vault>…</vault>"
        value={xmlIn}
        onChange={(e) => setXmlIn(e.target.value)}
      />
      <div className="spacer-10" />
      <button
        className="btn sm primary"
        disabled={busy !== null || !xmlIn.trim()}
        onClick={() =>
          void run("imp-xml", async () => {
            const n = await importSafeincloudXml(xmlIn);
            await ctx.refreshFromBackend();
            setXmlIn("");
            setMsg({ ok: true, text: `成功导入 ${n} 条` });
          })
        }
      >
        {busy === "imp-xml" ? "导入中…" : "导入 XML"}
      </button>

      <div className="hr" />
      <div className="section-cap">导入 .sicv 备份</div>
      <div className="row" style={{ marginBottom: 8 }}>
        <input
          className="input grow"
          type="password"
          placeholder="备份主密码（未加密留空）"
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
      <SicvPicker
        disabled={busy !== null}
        onFile={(f) =>
          void run("imp-sicv", async () => {
            const buf = await f.arrayBuffer();
            const meta = await importVaultBytes(
              arrayBufferToBase64(buf),
              sicvPw,
              sicvName.trim() || f.name.replace(/\.sicv$/i, "") || "导入的密码库",
            );
            await ctx.refreshFromBackend();
            setSicvPw("");
            setSicvName("");
            setMsg({ ok: true, text: `已导入密码库「${meta.name}」并切换过去` });
          })
        }
      />

      {msg && <div className="ok-text">{msg.text}</div>}
      {err && <div className="err-text">{err}</div>}
    </>
  );
}

function SicvPicker({ disabled, onFile }: { disabled: boolean; onFile: (f: File) => void }) {
  return (
    <label className="btn sm" style={{ display: "inline-block", cursor: "pointer" }}>
      选择 .sicv 文件…
      <input
        type="file"
        accept=".sicv"
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
