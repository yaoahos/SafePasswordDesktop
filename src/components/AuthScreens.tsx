import { useRef, useState } from "react";
import type { CloudVaultEntry, VaultMeta, WebDavConfig } from "../types";
import { evaluateStrength, importVaultBytes, webdavGetVault, webdavListCloud } from "../api";
import { arrayBufferToBase64, formatTime } from "../utils";
import { Icon } from "./icons";
import { AppTitleBar } from "./TopBar";

type Mode = "empty" | "select" | "create" | "restore" | "cloud";

/** 强度条颜色（对齐手机端 StrengthBar 分段） */
const STRENGTH_COLORS = ["#EF4444", "#F59E0B", "#EAB308", "#22C55E", "#16A34A"];

export function AuthScreens({
  vaults,
  defaultVaultId,
  onUnlock,
  onCreate,
  onRestoreLocal,
  webdav,
  onCloudRestored,
}: {
  vaults: VaultMeta[];
  /** 最近打开的密码库（对齐手机端 lastVaultId 预选） */
  defaultVaultId?: string | null;
  /** 成功返回 null，失败返回后端错误文案 */
  onUnlock: (id: string, password: string) => Promise<string | null>;
  onCreate: (name: string, password: string) => Promise<string | null>;
  /** 从本地恢复 .sicv/.spv（成功返回 null） */
  onRestoreLocal?: (fileName: string, base64Data: string, password: string) => Promise<string | null>;
  /** WebDAV 配置（用于无本地库时从云端恢复） */
  webdav?: WebDavConfig;
  /** 从云端恢复成功后刷新库列表 */
  onCloudRestored?: () => Promise<void>;
}) {
  const [mode, setMode] = useState<Mode>(vaults.length === 0 ? "empty" : "select");
  const [selected, setSelected] = useState<string>(
    (defaultVaultId && vaults.some((v) => v.id === defaultVaultId) ? defaultVaultId : vaults[0]?.id) ?? "",
  );
  const [password, setPassword] = useState("");
  const [showPw, setShowPw] = useState(false);
  const [name, setName] = useState("");
  const [confirm, setConfirm] = useState("");
  const [master, setMaster] = useState("");
  const [strength, setStrength] = useState<{ score: number; label: string } | null>(null);
  const [restoreFile, setRestoreFile] = useState<File | null>(null);
  const [restoreData, setRestoreData] = useState("");
  const [restorePw, setRestorePw] = useState("");
  const [cloudList, setCloudList] = useState<CloudVaultEntry[] | null>(null);
  const [cloudKey, setCloudKey] = useState("");
  const [cloudPw, setCloudPw] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  const doUnlock = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const err = await onUnlock(selected, password);
      if (err) setError(err);
      else setPassword("");
    } finally {
      setBusy(false);
    }
  };

  const doCreate = async () => {
    if (!name.trim()) {
      setError("请输入密码库名称");
      return;
    }
    if (master.length < 8) {
      setError("主密码至少 8 位");
      return;
    }
    if (master !== confirm) {
      setError("两次输入的主密码不一致");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const err = await onCreate(name.trim(), master);
      if (err) setError(err);
      else {
        setName("");
        setMaster("");
        setConfirm("");
        setStrength(null);
      }
    } finally {
      setBusy(false);
    }
  };

  const onMasterChange = (v: string) => {
    setMaster(v);
    if (!v) {
      setStrength(null);
      return;
    }
    void evaluateStrength(v)
      .then((r) => setStrength({ score: r.score, label: r.label }))
      .catch(() => setStrength(null));
  };

  const pickRestore = async (f: File) => {
    setError(null);
    try {
      const buf = await f.arrayBuffer();
      setRestoreFile(f);
      setRestoreData(arrayBufferToBase64(buf));
    } catch {
      setError("无法读取所选文件");
    }
  };

  const doRestore = async () => {
    if (!restoreFile || !restoreData) {
      setError("请先选择保险库文件（.spv）");
      return;
    }
    if (!onRestoreLocal) {
      setError("当前版本未启用本地恢复");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const err = await onRestoreLocal(restoreFile.name, restoreData, restorePw);
      if (err) setError(err);
      else {
        setRestoreFile(null);
        setRestoreData("");
        setRestorePw("");
      }
    } finally {
      setBusy(false);
    }
  };

  const cloudConfigured =
    !!webdav && /^https?:\/\//i.test((webdav.baseUrl || "").trim()) && !!webdav.username.trim();

  const openCloud = async () => {
    setMode("cloud");
    setError(null);
    setCloudList(null);
    setCloudKey("");
    setCloudPw("");
    if (!webdav || !cloudConfigured) return;
    setBusy(true);
    try {
      setCloudList(await webdavListCloud(webdav));
    } catch (e) {
      setCloudList([]);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doCloudRestore = async () => {
    if (!webdav) return;
    if (!cloudKey) {
      setError("请选择要恢复的云端密码库");
      return;
    }
    if (!cloudPw) {
      setError("请输入该密码库的主密码");
      return;
    }
    const entry = cloudList?.find((c) => c.key === cloudKey);
    setBusy(true);
    setError(null);
    try {
      const b64 = await webdavGetVault(webdav, cloudKey);
      if (!b64) throw new Error("云端没有找到该密码库文件");
      await importVaultBytes(b64, cloudPw, entry?.name ?? "");
      await onCloudRestored?.();
      setCloudPw("");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const selectedMeta = vaults.find((v) => v.id === selected);

  return (
    <div className="auth-wrap">
      <AppTitleBar />
      <div className="auth-center">
        <div className="auth-panel">
        <div className="auth-hero">
          <img className="auth-appicon" src="/icon.png" alt="" />
          <div className="auth-appname">SafePassword</div>
          <div className="auth-appsub">AES-256 加密 · 仅保存在你的设备与自己的云</div>
        </div>

        {mode === "empty" && (
          <>
            <div className="auth-empty-title">还没有密码库</div>
            <div className="auth-empty-sub">创建一个全新的密码库，或从本地备份恢复。</div>
            <button className="btn primary block tall" onClick={() => setMode("create")}>
              创建密码库
            </button>
            <div className="spacer-10" />
            <div className="row" style={{ justifyContent: "center", gap: 18 }}>
              <button className="link-btn" onClick={() => setMode("restore")}>
                从本地恢复
              </button>
              <button className="link-btn" onClick={() => void openCloud()}>
                从云端恢复
              </button>
            </div>
          </>
        )}

        {mode === "select" && (
          <>
            {vaults.length > 1 && (
              <div className="vault-chips">
                {vaults.map((v) => (
                  <button
                    key={v.id}
                    className={`vault-chip${selected === v.id ? " active" : ""}`}
                    onClick={() => {
                      setSelected(v.id);
                      setError(null);
                    }}
                  >
                    <Icon name="lock" size={13} /> {v.name}
                  </button>
                ))}
              </div>
            )}
            <div className="auth-vault-name">{selectedMeta?.name ?? "选择密码库"}</div>
            <div className="auth-vault-meta">
              上次打开 {formatTime(selectedMeta?.lastOpenedAt ?? 0)}
            </div>
            <div className="form-row">
              <label className="field-caption">主密码</label>
              <div className="input-wrap">
                <input
                  className="input with-eye"
                  type={showPw ? "text" : "password"}
                  autoFocus
                  value={password}
                  placeholder="输入主密码解锁"
                  onChange={(e) => setPassword(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") void doUnlock();
                  }}
                />
                <button
                  className="icon-btn in-frame"
                  title={showPw ? "隐藏" : "显示"}
                  onClick={() => setShowPw((v) => !v)}
                >
                  <Icon name={showPw ? "eye-off" : "eye"} size={17} />
                </button>
              </div>
            </div>
            <button className="btn primary block tall" disabled={busy || !selected} onClick={() => void doUnlock()}>
              {busy ? "解锁中…" : "解锁"}
            </button>
          </>
        )}

        {mode === "create" && (
          <>
            <div className="field-caption">创建密码库</div>
            <div className="form-row">
              <label className="field-caption">名称</label>
              <input
                className="input"
                value={name}
                placeholder="例如：个人密码库"
                onChange={(e) => setName(e.target.value)}
              />
            </div>
            <div className="form-row">
              <label className="field-caption">主密码</label>
              <input
                className="input"
                type="password"
                value={master}
                placeholder="主密码（至少 8 位）"
                onChange={(e) => onMasterChange(e.target.value)}
              />
            </div>
            {strength && (
              <div className="pw-meter">
                <div className="pw-bar">
                  <div
                    className="pw-bar-fill"
                    style={{
                      width: `${((strength.score + 1) / 5) * 100}%`,
                      background: STRENGTH_COLORS[Math.min(strength.score, 4)],
                    }}
                  />
                </div>
                <span className="muted small">强度：{strength.label}</span>
              </div>
            )}
            <div className="form-row">
              <label className="field-caption">确认主密码</label>
              <input
                className="input"
                type="password"
                value={confirm}
                placeholder="再次输入主密码"
                onChange={(e) => setConfirm(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void doCreate();
                }}
              />
            </div>
            {confirm && confirm !== master && <div className="setting-sub">两次输入不一致</div>}
            <button className="btn primary block tall" disabled={busy} onClick={() => void doCreate()}>
              {busy ? "创建中…" : "创建"}
            </button>
            <div className="spacer-10" />
            <div className="row" style={{ justifyContent: "center", gap: 18 }}>
              <button className="link-btn" onClick={() => setMode(vaults.length === 0 ? "empty" : "select")}>
                ← 返回解锁
              </button>
              <button className="link-btn" onClick={() => setMode("restore")}>
                从本地恢复
              </button>
            </div>
          </>
        )}

        {mode === "restore" && (
          <>
            <div className="field-caption">从本地恢复</div>
            <div className="muted small" style={{ marginBottom: 10 }}>
              选择手机端导出的备份文件（.spv），输入该库的主密码。
            </div>
            <button className="btn block" onClick={() => fileRef.current?.click()}>
              {restoreFile ? `已选择：${restoreFile.name}` : "选择保险库文件…"}
            </button>
            <input
              ref={fileRef}
              type="file"
              accept=".sicv,.spv"
              style={{ display: "none" }}
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) void pickRestore(f);
                e.target.value = "";
              }}
            />
            <div className="spacer-10" />
            <div className="form-row">
              <label className="field-caption">该库的主密码</label>
              <input
                className="input"
                type="password"
                value={restorePw}
                placeholder="输入该备份的主密码"
                onChange={(e) => setRestorePw(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void doRestore();
                }}
              />
            </div>
            <button className="btn primary block tall" disabled={busy} onClick={() => void doRestore()}>
              {busy ? "恢复中…" : "恢复"}
            </button>
            <div className="spacer-10" />
            <div className="row" style={{ justifyContent: "center" }}>
              <button className="link-btn" onClick={() => setMode(vaults.length === 0 ? "empty" : "select")}>
                ← 返回
              </button>
            </div>
          </>
        )}

        {mode === "cloud" && (
          <>
            <div className="field-caption">从云端恢复</div>
            {!cloudConfigured && (
              <div className="muted small" style={{ marginBottom: 10, lineHeight: 1.7 }}>
                还没有配置 WebDAV。请先解锁任意一个密码库，在「设置 → 同步与备份」里填写服务器地址与账号，
                再回到这里从云端恢复。
              </div>
            )}
            {cloudConfigured && (
              <>
                <div className="muted small" style={{ marginBottom: 10 }}>
                  选择云端已有的密码库，输入该库的主密码下载到本机。
                </div>
                {cloudList === null && busy && <div className="muted small">正在读取云端清单…</div>}
                {cloudList && cloudList.length === 0 && !busy && (
                  <div className="muted small" style={{ marginBottom: 10 }}>
                    云端没有任何密码库。
                  </div>
                )}
                {cloudList?.map((c) => (
                  <button
                    key={c.key}
                    className={`vault-row${cloudKey === c.key ? " active" : ""}`}
                    style={{ width: "100%", textAlign: "left", cursor: "pointer" }}
                    onClick={() => {
                      setCloudKey(c.key);
                      setError(null);
                    }}
                  >
                    <span className="vault-row-name">
                      <Icon name="cloud" size={15} /> {c.name}
                    </span>
                    <span className="muted small">
                      {cloudKey === c.key ? "已选择" : `${Math.max(1, Math.round(c.size / 1024))} KB`}
                    </span>
                  </button>
                ))}
                {cloudKey && (
                  <div className="form-row">
                    <label className="field-caption">该库的主密码</label>
                    <input
                      className="input"
                      type="password"
                      value={cloudPw}
                      placeholder="输入该密码库的主密码"
                      onChange={(e) => setCloudPw(e.target.value)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") void doCloudRestore();
                      }}
                    />
                  </div>
                )}
                <button
                  className="btn primary block tall"
                  disabled={busy || !cloudKey}
                  onClick={() => void doCloudRestore()}
                >
                  {busy ? "恢复中…" : "下载并恢复"}
                </button>
              </>
            )}
            <div className="spacer-10" />
            <div className="row" style={{ justifyContent: "center" }}>
              <button className="link-btn" onClick={() => setMode(vaults.length === 0 ? "empty" : "select")}>
                ← 返回
              </button>
            </div>
          </>
        )}

        {error && <div className="err-text">{error}</div>}
        </div>
      </div>
    </div>
  );
}
