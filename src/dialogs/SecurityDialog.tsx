import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { securityReport, securityReportFull } from "../api";
import type { SecurityReport, VaultItem } from "../types";
import { itemSubtitle } from "../utils";
import type { AppCtx } from "../app-context";

const SECTIONS: { key: keyof Omit<SecurityReport, "total">; title: string; color: string }[] = [
  { key: "breached", title: "已泄露（HIBP）", color: "#DC2626" },
  { key: "weak", title: "弱密码", color: "#F59E0B" },
  { key: "reused", title: "重复使用", color: "#9333EA" },
  { key: "old", title: "长期未更换", color: "#0891B2" },
];

export function SecurityDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [report, setReport] = useState<SecurityReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [full, setFull] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (withHibp: boolean) => {
    setBusy(true);
    setFull(withHibp);
    setError(null);
    try {
      setReport(withHibp ? await securityReportFull() : await securityReport());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const goto = (item: VaultItem) => {
    ctx.setSelectItemId(item.id);
    onClose();
  };

  return (
    <Dialog title="安全检查" onClose={onClose}>
      <div className="row">
        <div className="grow muted small">检查弱密码、重复使用、已泄露与长期未更换的条目</div>
        <button className="btn primary sm" disabled={busy} onClick={() => void run(false)}>
          {busy && !full ? "检查中…" : "开始检查"}
        </button>
        <button className="btn sm" disabled={busy} title="在线比对 Have I Been Pwned（需网络）" onClick={() => void run(true)}>
          {busy && full ? "检查中…" : "在线泄露检查"}
        </button>
      </div>
      {error && <div className="err-text">{error}</div>}

      {report && !busy && (
        <>
          <div className="spacer-10" />
          <div className="card" style={{ boxShadow: "none", border: "1px solid var(--border)" }}>
            <div className="row" style={{ fontSize: 14 }}>
              <span className="grow">共检查 {report.total} 个条目</span>
              {SECTIONS.map((s) => (
                <span key={s.key} style={{ color: s.color, fontWeight: 600 }}>
                  {report[s.key].length}
                </span>
              ))}
            </div>
          </div>
          {SECTIONS.map((s) => (
            <div key={s.key}>
              <div className="section-cap" style={{ color: s.color }}>
                {s.title}（{report[s.key].length}）
              </div>
              {report[s.key].length === 0 ? (
                <div className="muted small">太好了，没有发现问题</div>
              ) : (
                <div className="mini-list">
                  {report[s.key].map((item) => (
                    <div className="mini-row" key={item.id}>
                      <span className="grow">
                        <b>{item.title || "未命名"}</b>
                        <span className="muted"> · {itemSubtitle(item) || "—"}</span>
                      </span>
                      <button className="link-btn" onClick={() => goto(item)}>
                        查看
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </>
      )}
    </Dialog>
  );
}
