import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { webdavPull, webdavSync } from "../api";
import type { SyncStrategy, SyncSummary } from "../types";
import type { AppCtx } from "../app-context";

const STRATEGIES: { key: SyncStrategy; label: string }[] = [
  { key: "DETECT", label: "自动检测" },
  { key: "KEEP_LOCAL", label: "保留本机" },
  { key: "KEEP_REMOTE", label: "保留远程" },
  { key: "MERGE", label: "合并" },
];

export function SyncDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [strategy, setStrategy] = useState<SyncStrategy>("DETECT");
  const [busy, setBusy] = useState<"sync" | "pull" | null>(null);
  const [summary, setSummary] = useState<SyncSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  const cfg = ctx.settings.webdav;

  const run = async (kind: "sync" | "pull") => {
    setBusy(kind);
    setError(null);
    setSummary(null);
    try {
      const s = kind === "sync" ? await webdavSync(cfg, strategy) : await webdavPull(cfg);
      setSummary(s);
      await ctx.refreshFromBackend();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Dialog title="同步" onClose={onClose}>
      {!cfg.enabled && (
        <div className="err-text" style={{ marginTop: 0 }}>
          尚未启用 WebDAV 同步，请先在“WebDAV 同步”或“设置 → 同步与备份”中配置。
        </div>
      )}
      <div className="form-row">
        <label className="field-caption">冲突策略</label>
        <div className="seg">
          {STRATEGIES.map((s) => (
            <button key={s.key} className={strategy === s.key ? "active" : ""} onClick={() => setStrategy(s.key)}>
              {s.label}
            </button>
          ))}
        </div>
      </div>
      <div className="spacer-10" />
      <div className="row" style={{ justifyContent: "flex-end" }}>
        <button className="btn primary" disabled={busy !== null} onClick={() => void run("sync")}>
          {busy === "sync" ? "同步中…" : "立即同步"}
        </button>
        <button className="btn" disabled={busy !== null} onClick={() => void run("pull")}>
          {busy === "pull" ? "拉取中…" : "从远程拉取"}
        </button>
      </div>
      {error && <div className="err-text">{error}</div>}

      {summary && (
        <div className="card" style={{ boxShadow: "none", border: "1px solid var(--border)", marginTop: 14 }}>
          <div className="row" style={{ fontSize: 14 }}>
            <span className="grow">结果</span>
            <b>{directionText(summary.direction)}</b>
          </div>
          <div className="hr" />
          <div className="small muted" style={{ lineHeight: 1.9 }}>
            冲突：{summary.conflicts}
            <br />
            上传附件：{summary.attachmentsUploaded} · 下载附件：{summary.attachmentsDownloaded}
            {summary.message && (
              <>
                <br />
                {summary.message}
              </>
            )}
          </div>
        </div>
      )}
    </Dialog>
  );
}

function directionText(d: SyncSummary["direction"]): string {
  switch (d) {
    case "NONE":
      return "无变化";
    case "UPLOADED":
      return "已上传";
    case "DOWNLOADED":
      return "已下载";
    case "MERGED":
      return "已合并";
    case "CONFLICT":
      return "存在冲突";
  }
}
