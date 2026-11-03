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
  onSaveWebdav,
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
  /** 在恢复页直接保存 WebDAV 配置（无需先有本地库） */
  onSaveWebdav?: (cfg: WebDavConfig) => Promise<void>;
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
  const [cloudCfg, setCloudCfg] = useState<WebDavConfig | null>(null);
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

  const DEFAULT_DAV: WebDavConfig = {
    baseUrl: "",
    username: "",
    password: "",
    remoteDir: "SafePassword",
    enabled: true,
  };

  const cfgReady = (c: WebDavConfig | null) =>
    !!c && /^https?:\/\//i.test((c.baseUrl || "").trim()) && !!c.username.trim();

  const loadCloud = async (cfg: WebDavConfig) => {
    setBusy(true);
    setError(null);
    setCloudList(null);
    try {
      setCloudList(await webdavListCloud(cfg));
    } catch (e) {
      setCloudList([]);
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const openCloud = async () => {
    setMode("cloud");
    setError(null);
    setCloudList(null);
    setCloudKey("");
    setCloudPw("");
    const cfg = { ...DEFAULT_DAV, ...(webdav ?? {}) };
    setCloudCfg(cfg);
    if (cfgReady(cfg)) await loadCloud(cfg);
  };

  const saveCloudCfg = async () => {
    if (!cloudCfg) return;
    const baseUrl = cloudCfg.baseUrl.trim();
    if (!/^https?:\/\//i.test(baseUrl)) {
      setError("服务器地址需以 http:// 或 https:// 开头");
      return;
    }
    if (!cloudCfg.username.trim()) {
      setError("请填写用户名");
      return;
    }
    const next: WebDavConfig = {
      ...cloudCfg,
      baseUrl,
      remoteDir: (cloudCfg.remoteDir || "").trim() || "SafePassword",
      enabled: true,
    };
    setBusy(true);
    setError(null);
    try {
      await onSaveWebdav?.(next);
      setCloudCfg(next);
      await loadCloud(next);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const doCloudRestore = async () => {
    if (!cloudCfg) return;
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
      const b64 = await webdavGetVault(cloudCfg, cloudKey);
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
            <div className="auth-section-title">
              <Icon name="cloud" size={18} /> 从云端恢复
            </div>
            <div className="auth-section-sub">
              只需要 WebDAV 地址与账号；服务器上只保存加密后的密码库，主密码不会上传。
            </div>

            {cloudCfg && (
              <div className="settings-group">
                <div className="settings-group-title">
                  <span className="settings-group-dot" /> WebDAV 服务器
                </div>
                <div className="form-row">
                  <label className="field-caption">服务器地址</label>
                  <input
                    className="input"
                    placeholder="https://dav.jianguoyun.com/dav/"
                    value={cloudCfg.baseUrl}
                    onChange={(e) => setCloudCfg({ ...cloudCfg, baseUrl: e.target.value })}
                  />
                </div>
                <div className="form-row">
                  <label className="field-caption">用户名</label>
                  <input
                    className="input"
                    placeholder="邮箱或账号"
                    value={cloudCfg.username}
                    onChange={(e) => setCloudCfg({ ...cloudCfg, username: e.target.value })}
                  />
                </div>
                <div className="form-row">
                  <label className="field-caption">密码 / 应用密码</label>
                  <input
                    className="input"
                    type="password"
                    placeholder="坚果云请用应用密码"
                    value={cloudCfg.password}
                    onChange={(e) => setCloudCfg({ ...cloudCfg, password: e.target.value })}
                  />
                </div>
                <div className="form-row">
                  <label className="field-caption">远程目录</label>
                  <input
                    className="input"
                    placeholder="SafePassword"
                    value={cloudCfg.remoteDir}
                    onChange={(e) => setCloudCfg({ ...cloudCfg, remoteDir: e.target.value })}
                  />
                </div>
                <button className="btn primary block tall" disabled={busy} onClick={() => void saveCloudCfg()}>
                  {busy ? "连接中…" : cfgReady(cloudCfg) ? "保存并读取云端清单" : "保存并连接"}
                </button>
              </div>
            )}

            {cloudList !== null && (
              <div className="settings-group">
                <div className="settings-group-title">
                  <span className="settings-group-dot" /> 云端密码库
                </div>
                {busy && <div className="cloud-empty">正在读取云端清单…</div>}
                {!busy && cloudList.length === 0 && (
                  <div className="cloud-empty">云端没有任何密码库。</div>
                )}
                {cloudList.length > 0 && (
                  <div className="cloud-list">
                    {cloudList.map((c) => (
                      <button
                        key={c.key}
                        className={`cloud-row${cloudKey === c.key ? " active" : ""}`}
                        onClick={() => {
                          setCloudKey(c.key);
                          setError(null);
                        }}
                      >
                        <span className="cloud-ico">
                          <Icon name="cloud" size={16} />
                        </span>
                        <span className="cloud-main">
                          <span className="cloud-name">{c.name}</span>
                          <span className="cloud-meta">更新于 {formatTime(c.updatedAt)}</span>
                        </span>
                        <span className="cloud-size">
                          {cloudKey === c.key
                            ? "已选择"
                            : `${Math.max(1, Math.round(c.size / 1024))} KB`}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
                {cloudKey && (
                  <div className="form-row" style={{ marginTop: 10 }}>
                    <label className="field-caption">该库的主密码</label>
                    <input
                      className="input"
                      type="password"
                      placeholder="输入该密码库的主密码"
                      value={cloudPw}
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
              </div>
            )}

            <div className="row" style={{ justifyContent: "center", marginTop: 12 }}>
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
