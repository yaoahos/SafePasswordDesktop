import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/icons";
import {
  backupCreate,
  backupDelete,
  backupList,
  backupRestore,
  changeMasterPassword,
  cloudKey,
  createVault,
  deleteVault,
  exportCsv,
  exportXml,
  exportVaultJson,
  exportVaultSpv,
  importCsv,
  importJsonItems,
  importXml,
  importVaultBytes,
  renameVault,
  saveExport,
  webdavDeleteCloud,
  webdavGetVault,
  webdavListCloud,
  webdavPull,
  webdavRestoreToCloud,
  webdavSync,
} from "../api";
import type { BackupInfo, CloudVaultEntry, VaultMeta, WebDavConfig } from "../types";
import { arrayBufferToBase64, exportStamp, formatSize, formatTime } from "../utils";
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
  /** 提示显示在触发操作的分区底部（sync=云端同步 / backup=本地备份） */
  const [scope, setScope] = useState<"sync" | "backup">("sync");
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
    setScope("sync");
    setBusy("sync");
    setMsg(null);
    try {
      await saveCfg();
      const s = await webdavSync(cfg, "MERGE");
      await ctx.refreshFromBackend();
      setMsg({ ok: true, text: `${s.message || "同步完成"}（${s.direction}）` });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const doPull = async () => {
    setScope("sync");
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
    setScope("backup");
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
    setScope("backup");
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
    setScope("backup");
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
      {msg && scope === "sync" && (
        <div
          className="setting-sub"
          style={{ marginTop: 6, color: msg.ok ? "var(--primary)" : "var(--danger)", fontWeight: 600 }}
        >
          {msg.text}
        </div>
      )}

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
      <div className="row" style={{ marginTop: 8, justifyContent: "flex-end", gap: 8 }}>
        <button
          className="btn"
          disabled={busy !== null}
          onClick={() => {
            setScope("backup");
            setMsg(null);
            void refreshBackups();
          }}
        >
          刷新
        </button>
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
        {msg && scope === "backup" && (
          <div
            className="setting-sub"
            style={{ marginTop: 6, color: msg.ok ? "var(--primary)" : "var(--danger)", fontWeight: 600 }}
          >
            {msg.text}
          </div>
        )}
      </div>
    </>
  );
}

// ---------------------------------------------------------------- 密码库（当前库用主题色标识，列表改下拉）

function VaultTab({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const vault = ctx.vault;
  const cfg = ctx.settings.webdav;
  const [newName, setNewName] = useState("");
  const [newPw, setNewPw] = useState("");
  const [renameTo, setRenameTo] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  /** 提示显示在触发操作的分区底部（local/cloud/new/rename） */
  const [scope, setScope] = useState<"local" | "cloud" | "new" | "rename">("local");
  const [busy, setBusy] = useState<string | null>(null);
  const [delId, setDelId] = useState<string | null>(null);
  const [cloud, setCloud] = useState<CloudVaultEntry[] | null>(null);
  const [cloudErr, setCloudErr] = useState<string | null>(null);
  const [cloudDel, setCloudDel] = useState<string | null>(null);
  const [dlKey, setDlKey] = useState<string | null>(null);
  const [dlPw, setDlPw] = useState("");

  const localNames = ctx.vaults.map((v) => v.name);

  const rename = async () => {
    if (!vault) return;
    const trimmed = renameTo.trim();
    setScope("rename");
    if (!trimmed) {
      setMsg({ ok: false, text: "请输入新名称" });
      return;
    }
    setBusy("rename");
    try {
      const oldKey = await cloudKey(vault.name);
      await renameVault(vault.id, trimmed);
      // 云端文件名由内容密钥派生：改名**不换文件**，只需把清单里的库名更新成新名字。
      // 只有在旧版按键名派生（键真的变了）时才需要清掉旧键文件。
      let cloudSynced = false;
      if (cfg.baseUrl.trim() && cfg.enabled) {
        try {
          await webdavRestoreToCloud(cfg);
          const newKey = await cloudKey(trimmed);
          if (oldKey && newKey && newKey !== oldKey) await webdavDeleteCloud(cfg, oldKey);
          cloudSynced = true;
        } catch {
          /* 云端清理失败不影响本地改名 */
        }
      }
      await ctx.refreshFromBackend();
      setRenameTo("");
      setMsg({ ok: true, text: cloudSynced ? "密码库已改名，云端库名已同步" : "密码库已改名" });
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const create = async () => {
    setScope("new");
    if (!newName.trim() || newPw.length < 8) {
      setMsg({ ok: false, text: "请填写名称和至少 8 位的主密码" });
      return;
    }
    setBusy("create");
    try {
      await createVault(newName.trim(), newPw);
      await ctx.refreshFromBackend();
      setNewName("");
      setNewPw("");
      setMsg({ ok: true, text: "已创建并切换到新密码库" });
      onClose();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const removeLocal = async (m: VaultMeta) => {
    setScope("local");
    setBusy(`del-${m.id}`);
    try {
      await deleteVault(m.id);
      const wasCurrent = vault?.id === m.id;
      setDelId(null);
      setMsg({ ok: true, text: `已删除「${m.name}」` });
      if (wasCurrent) {
        onClose();
        ctx.gotoSelect();
      } else {
        await ctx.refreshFromBackend();
      }
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  /** 删除云端密码库（密文 + 清单条目） */
  const removeCloud = async (c: CloudVaultEntry) => {
    setScope("cloud");
    setBusy(`cdel-${c.key}`);
    try {
      await webdavDeleteCloud(cfg, c.key);
      setCloudDel(null);
      setMsg({ ok: true, text: `已删除云端密码库「${c.name}」` });
      await refreshCloud();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const refreshCloud = async () => {
    setScope("cloud");
    if (!cfg.baseUrl.trim()) {
      setCloudErr("请先在「同步与备份」里填写 WebDAV 服务器地址");
      return;
    }
    setBusy("cloud");
    setCloudErr(null);
    try {
      setCloud(await webdavListCloud(cfg));
    } catch (e) {
      setCloudErr(String(e));
    } finally {
      setBusy(null);
    }
  };

  const downloadCloud = async (c: CloudVaultEntry) => {
    setScope("cloud");
    if (dlPw.length < 8) {
      setMsg({ ok: false, text: "请输入该密码库的主密码（至少 8 位）" });
      return;
    }
    setBusy(`dl-${c.key}`);
    try {
      const b64 = await webdavGetVault(cfg, c.key);
      if (!b64) throw new Error("云端没有找到该密码库文件");
      const meta = await importVaultBytes(b64, dlPw, c.name);
      await ctx.refreshFromBackend();
      setDlKey(null);
      setDlPw("");
      setMsg({ ok: true, text: `已下载并切换到「${meta.name}」` });
      onClose();
    } catch (e) {
      setMsg({ ok: false, text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <>
      <div className="settings-group">
        <div className="settings-group-title">
          <span className="settings-group-dot" /> 本地密码库
        </div>
        <div className="setting-sub" style={{ marginBottom: 8 }}>
          每个密码库有独立的主密码和文件；切换后需要在解锁页输入该库主密码。
        </div>
        {ctx.vaults.map((m) => {
          const current = m.id === vault?.id;
          return (
            <div className="vault-row" key={m.id}>
              <div className="grow">
                <div className="vault-row-name">
                  {current && <span className="vault-dot" />}
                  <b style={current ? { color: "var(--primary)" } : undefined}>{m.name}</b>
                  {current && <span className="vault-badge">当前</span>}
                </div>
                <div className="muted small">
                  上次打开 {m.lastOpenedAt > 0 ? formatTime(m.lastOpenedAt) : "从未"}
                </div>
              </div>
              {!current && (
                <button
                  className="btn sm"
                  onClick={() => {
                    ctx.gotoSelect(m.id);
                    onClose();
                  }}
                >
                  切换
                </button>
              )}
              {delId === m.id ? (
                <span className="row" style={{ gap: 6 }}>
                  <button
                    className="btn sm danger"
                    disabled={busy !== null}
                    onClick={() => void removeLocal(m)}
                  >
                    确认删除
                  </button>
                  <button className="btn sm" onClick={() => setDelId(null)}>
                    取消
                  </button>
                </span>
              ) : (
                <button className="icon-btn" title="删除" onClick={() => setDelId(m.id)}>
                  <Icon name="trash" size={16} />
                </button>
              )}
            </div>
          );
        })}
        {msg && scope === "local" && <div className={msg.ok ? "ok-text" : "err-text"}>{msg.text}</div>}
      </div>

      <div className="spacer-10" />
      <div className="settings-group">
        <div className="settings-group-title">
          <span className="settings-group-dot" /> 云端密码库
          <button className="btn sm" style={{ marginLeft: "auto" }} disabled={busy !== null} onClick={() => void refreshCloud()}>
            {busy === "cloud" ? "读取中…" : "刷新"}
          </button>
        </div>
        <div className="setting-sub" style={{ marginBottom: 8 }}>
          云端只保存密文；下载并切换时需要输入该库的主密码。
        </div>
        {cloudErr && <div className="err-text">{cloudErr}</div>}
        {cloud === null && !cloudErr && (
          <div className="muted small">点击「刷新」读取 WebDAV 上的密码库清单。</div>
        )}
        {cloud?.length === 0 && <div className="muted small">云端还没有密码库。</div>}
        {cloud?.map((c) => {
          const local = localNames.includes(c.name);
          return (
            <div className="vault-row" key={c.key}>
              <div className="grow">
                <div className="vault-row-name">
                  <Icon name="cloud" size={15} />
                  <b>{c.name}</b>
                  {local && <span className="vault-badge">已在本机</span>}
                </div>
                <div className="muted small">
                  {formatSize(c.size)} · 更新于 {formatTime(c.updatedAt)}
                </div>
              </div>
              {!local && dlKey !== c.key && (
                <button className="btn sm" onClick={() => setDlKey(c.key)}>
                  下载并切换
                </button>
              )}
              {dlKey === c.key && (
                <span className="row" style={{ gap: 6 }}>
                  <input
                    className="input"
                    type="password"
                    style={{ width: 160, height: 32 }}
                    placeholder="该库主密码"
                    value={dlPw}
                    onChange={(e) => setDlPw(e.target.value)}
                  />
                  <button
                    className="btn sm primary"
                    disabled={busy !== null}
                    onClick={() => void downloadCloud(c)}
                  >
                    确认
                  </button>
                  <button className="btn sm" onClick={() => setDlKey(null)}>
                    取消
                  </button>
                </span>
              )}
              {cloudDel === c.key ? (
                <span className="row" style={{ gap: 6 }}>
                  <button
                    className="btn sm danger"
                    disabled={busy !== null}
                    onClick={() => void removeCloud(c)}
                  >
                    确认删除
                  </button>
                  <button className="btn sm" onClick={() => setCloudDel(null)}>
                    取消
                  </button>
                </span>
              ) : (
                <button className="icon-btn" title="删除云端密码库" onClick={() => setCloudDel(c.key)}>
                  <Icon name="trash" size={16} />
                </button>
              )}
            </div>
          );
        })}
        {msg && scope === "cloud" && <div className={msg.ok ? "ok-text" : "err-text"}>{msg.text}</div>}
      </div>

      <div className="spacer-10" />
      <div className="settings-group">
        <div className="settings-group-title">
          <span className="settings-group-dot" /> 新建密码库
        </div>
        <div className="form-row">
          <label className="field-caption">名称</label>
          <input className="input" value={newName} onChange={(e) => setNewName(e.target.value)} />
        </div>
        <div className="form-row">
          <label className="field-caption">主密码（至少 8 位）</label>
          <input className="input" type="password" value={newPw} onChange={(e) => setNewPw(e.target.value)} />
        </div>
        <button className="btn primary tall block" disabled={busy !== null} onClick={() => void create()}>
          新建并切换
        </button>
        {msg && scope === "new" && <div className={msg.ok ? "ok-text" : "err-text"}>{msg.text}</div>}
      </div>

      {vault && (
        <>
          <div className="spacer-10" />
          <div className="settings-group">
            <div className="settings-group-title">
              <span className="settings-group-dot" /> 重命名当前库
            </div>
            <div className="row">
              <input
                className="input grow"
                placeholder="新名称"
                value={renameTo}
                onChange={(e) => setRenameTo(e.target.value)}
              />
              <button className="btn sm" onClick={() => void rename()}>
                重命名
              </button>
            </div>
            {msg && scope === "rename" && (
              <div className={msg.ok ? "ok-text" : "err-text"} style={{ marginTop: 6 }}>
                {msg.text}
              </div>
            )}
          </div>
        </>
      )}

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
  const [sicvPw, setSicvPw] = useState("");
  const [sicvName, setSicvName] = useState("");
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  /** 提示显示在触发操作的分区底部（export / import / spv） */
  const [tScope, setTScope] = useState<"export" | "import" | "spv">("export");

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

  const exportName = (ext: string) => {
    const name = (ctx.vault?.name ?? "vault").replace(/[\\/:*?"<>|]/g, "_");
    return `${name}-${exportStamp()}.${ext}`;
  };

  const doExport = (kind: "csv" | "xml" | "json") =>
    void run(`exp-${kind}`, async () => {
      setTScope("export");
      setErr(null);
      setMsg(null);
      const text =
        kind === "csv" ? await exportCsv() : kind === "xml" ? await exportXml() : await exportVaultJson();
      const ext = kind === "csv" ? "csv" : kind === "xml" ? "xml" : "json";
      const b64 = arrayBufferToBase64(new TextEncoder().encode(text).buffer as ArrayBuffer);
      const saved = await saveExport(exportName(ext), b64);
      setMsg(saved ? { ok: true, text: `已保存到 ${saved}` } : { ok: true, text: "已取消保存" });
    });

  const doExportSpv = () =>
    void run("exp-spv", async () => {
      setTScope("export");
      setErr(null);
      setMsg(null);
      const b64 = await exportVaultSpv();
      const saved = await saveExport(exportName("spv"), b64);
      setMsg(saved ? { ok: true, text: `已保存加密备份到 ${saved}` } : { ok: true, text: "已取消保存" });
    });

  const importTextFile = (f: File) =>
    void run("imp-file", async () => {
      setTScope("import");
      setErr(null);
      setMsg(null);
      const text = await f.text();
      const ext = (f.name.split(".").pop() ?? "").toLowerCase();
      if (ext === "csv") {
        const n = await importCsv(text);
        await ctx.refreshFromBackend();
        setMsg({ ok: true, text: `成功导入 ${n} 条（CSV）` });
      } else if (ext === "xml") {
        const n = await importXml(text);
        await ctx.refreshFromBackend();
        setMsg({ ok: true, text: `成功导入 ${n} 条（XML）` });
      } else if (ext === "json") {
        const n = await importJsonItems(text);
        await ctx.refreshFromBackend();
        setMsg({ ok: true, text: `成功导入 ${n} 条（JSON）` });
      } else {
        throw new Error("不支持的文件类型（仅支持 .csv / .xml / .json）");
      }
    });

  const spvReady = sicvName.trim().length > 0 && sicvPw.length > 0;

  const importSpv = (f: File) =>
    void run("imp-spv", async () => {
      setTScope("spv");
      setErr(null);
      setMsg(null);
      if (!sicvName.trim()) throw new Error("请先填写库名称");
      if (!sicvPw) throw new Error("请先填写主密码");
      const buf = await f.arrayBuffer();
      const meta = await importVaultBytes(
        arrayBufferToBase64(buf),
        sicvPw,
        sicvName.trim(),
      );
      await ctx.refreshFromBackend();
      setSicvPw("");
      setSicvName("");
      setMsg({ ok: true, text: `已导入密码库「${meta.name}」并切换过去` });
    });

  return (
    <>
      <div className="section-cap">导出</div>
      <button className="btn primary tall block" disabled={busy !== null || !ctx.vault} onClick={doExportSpv}>
        {busy === "exp-spv" ? "导出中…" : "导出加密备份（.spv）"}
      </button>
      <div className="setting-sub" style={{ marginTop: 6 }}>
        加密备份与手机端同一格式，可直接放回手机或通过 WebDAV 同步。
      </div>
      <div className="row" style={{ marginTop: 10, flexWrap: "wrap" }}>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("csv")}>
          导出 CSV
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("xml")}>
          导出 XML
        </button>
        <button className="btn sm" disabled={busy !== null} onClick={() => doExport("json")}>
          导出 JSON
        </button>
      </div>

      {msg && tScope === "export" && <div className="ok-text">{msg.text}</div>}
      {err && tScope === "export" && <div className="err-text">{err}</div>}

      <div className="hr" />
      <div className="section-cap">导入条目（CSV / XML / JSON）</div>
      <FilePicker
        label="选择文件导入…"
        accept=".csv,.xml,.json"
        disabled={busy !== null}
        onFile={importTextFile}
      />
      <div className="setting-sub" style={{ marginTop: 6 }}>
        导入的条目会添加到当前密码库。
      </div>
      {msg && tScope === "import" && <div className="ok-text">{msg.text}</div>}
      {err && tScope === "import" && <div className="err-text">{err}</div>}

      <div className="hr" />
      <div className="section-cap">导入加密备份（.spv）</div>
      <div className="form-row">
        <label className="field-caption">库名称</label>
        <input
          className="input"
          placeholder="导入后的密码库名称"
          value={sicvName}
          onChange={(e) => setSicvName(e.target.value)}
        />
      </div>
      <div className="form-row">
        <label className="field-caption">主密码</label>
        <input
          className="input"
          type="password"
          placeholder="该备份的主密码"
          value={sicvPw}
          onChange={(e) => setSicvPw(e.target.value)}
        />
      </div>
      <FilePicker
        label="选择加密备份文件…"
        accept=".spv,.sicv"
        disabled={busy !== null}
        blocked={!spvReady}
        onBlocked={() => {
          setTScope("spv");
          setErr("请先填写库名称与主密码，再选择备份文件");
        }}
        onFile={importSpv}
      />
      <div className={err && tScope === "spv" ? "err-text" : "setting-sub"} style={{ marginTop: 6 }}>
        {err && tScope === "spv"
          ? err
          : spvReady
            ? "选择备份文件后将导入并切换到该密码库。"
            : "请先填写库名称与主密码，再选择备份文件。"}
      </div>

      {msg && tScope === "spv" && <div className="ok-text">{msg.text}</div>}
    </>
  );
}
function FilePicker({
  label,
  accept,
  disabled,
  blocked,
  onBlocked,
  onFile,
}: {
  label: string;
  accept: string;
  disabled: boolean;
  /** 条件未满足时点击：给出提示而不是毫无反应 */
  blocked?: boolean;
  onBlocked?: () => void;
  onFile: (f: File) => void;
}) {
  return (
    <label
      className="btn sm"
      style={{ display: "inline-block", cursor: "pointer" }}
      onClick={(e) => {
        if (blocked) {
          e.preventDefault();
          onBlocked?.();
        }
      }}
    >
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
