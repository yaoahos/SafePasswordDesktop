import { useState } from "react";
import type { VaultMeta } from "../types";
import { formatTime } from "../utils";

type Mode = "select" | "create";

export function AuthScreens({
  vaults,
  defaultVaultId,
  onUnlock,
  onCreate,
}: {
  vaults: VaultMeta[];
  /** 最近打开的密码库（对齐手机端 lastVaultId 预选） */
  defaultVaultId?: string | null;
  /** 成功返回 null，失败返回后端错误文案 */
  onUnlock: (id: string, password: string) => Promise<string | null>;
  onCreate: (name: string, password: string) => Promise<string | null>;
}) {
  const [mode, setMode] = useState<Mode>(vaults.length === 0 ? "create" : "select");
  const [selected, setSelected] = useState<string>(
    (defaultVaultId && vaults.some((v) => v.id === defaultVaultId) ? defaultVaultId : vaults[0]?.id) ?? "",
  );
  const [password, setPassword] = useState("");
  const [name, setName] = useState("");
  const [confirm, setConfirm] = useState("");
  const [master, setMaster] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

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
    if (master.length < 4) {
      setError("主密码至少 4 位");
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
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="auth-wrap">
      <div className="auth-card">
        <div className="auth-logo">
          <div className="badge">🔑</div>
          <div>
            <div className="auth-title">SafePassword</div>
            <div className="auth-sub">0.1.0-demo · 与手机端互通</div>
          </div>
        </div>
        <div className="spacer-16" />

        {mode === "select" ? (
          <>
            <div className="field-caption">选择要解锁的密码库</div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              {vaults.map((v) => (
                <button
                  key={v.id}
                  className={`vault-card${selected === v.id ? " active" : ""}`}
                  onClick={() => {
                    setSelected(v.id);
                    setError(null);
                  }}
                >
                  <span style={{ fontSize: 20 }}>🗄</span>
                  <span className="grow">
                    <div className="vname">{v.name}</div>
                    <div className="vmeta">上次打开 {formatTime(v.lastOpenedAt)}</div>
                  </span>
                  {selected === v.id && <span style={{ color: "var(--primary)" }}>●</span>}
                </button>
              ))}
            </div>
            <div className="spacer-10" />
            <div className="form-row">
              <label className="field-caption">主密码</label>
              <input
                className="input"
                type="password"
                autoFocus
                value={password}
                placeholder="输入主密码解锁"
                onChange={(e) => setPassword(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void doUnlock();
                }}
              />
            </div>
            <button className="btn primary block tall" disabled={busy || !selected} onClick={() => void doUnlock()}>
              {busy ? "解锁中…" : "解锁"}
            </button>
            <div className="spacer-10" />
            <div className="row">
              <button className="link-btn" onClick={() => setMode("create")}>
                ＋ 新建密码库
              </button>
            </div>
          </>
        ) : (
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
                placeholder="主密码（至少 4 位）"
                onChange={(e) => setMaster(e.target.value)}
              />
            </div>
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
            <button className="btn primary block tall" disabled={busy} onClick={() => void doCreate()}>
              {busy ? "创建中…" : "创建并进入"}
            </button>
            <div className="spacer-10" />
            {vaults.length > 0 && (
              <button className="link-btn" onClick={() => setMode("select")}>
                ← 返回解锁
              </button>
            )}
          </>
        )}

        {error && <div className="err-text">{error}</div>}
      </div>
    </div>
  );
}
