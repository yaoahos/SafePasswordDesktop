import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { webdavTestConnection } from "../api";
import type { AppSettings, WebDavConfig } from "../types";
import type { AppCtx } from "../app-context";

/** WebDAV 配置表单（顶栏“WebDAV 同步”弹窗与设置页共用） */
export function WebDavForm({ ctx, onSaved }: { ctx: AppCtx; onSaved?: () => void }) {
  const [cfg, setCfg] = useState<WebDavConfig>({ ...ctx.settings.webdav });
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; msg: string } | null>(null);

  const patch = (p: Partial<WebDavConfig>) => setCfg((c) => ({ ...c, ...p }));

  const save = async () => {
    const next: AppSettings = { ...ctx.settings, webdav: cfg };
    await ctx.applySettings(next);
    ctx.notify("WebDAV 配置已保存", "success");
    onSaved?.();
  };

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      await ctx.applySettings({ ...ctx.settings, webdav: cfg });
      await webdavTestConnection(cfg);
      setResult({ ok: true, msg: "连接成功" });
    } catch (e) {
      setResult({ ok: false, msg: String(e) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <>
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
      <label className="check-row">
        <input type="checkbox" checked={cfg.enabled} onChange={(e) => patch({ enabled: e.target.checked })} />
        启用 WebDAV 同步
      </label>
      <div className="spacer-10" />
      <div className="row">
        <button className="btn primary" disabled={testing} onClick={() => void save()}>
          保存配置
        </button>
        <button className="btn" disabled={testing} onClick={() => void test()}>
          {testing ? "测试中…" : "测试连接"}
        </button>
      </div>
      {result && <div className={result.ok ? "ok-text" : "err-text"}>{result.msg}</div>}
    </>
  );
}

export function WebDavDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  return (
    <Dialog title="WebDAV 同步" onClose={onClose}>
      <WebDavForm ctx={ctx} />
    </Dialog>
  );
}
