import { useEffect, useMemo, useState } from "react";
import { Dialog } from "../components/Dialog";
import { securityReport, securityReportFull } from "../api";
import type { SecurityReport, VaultItem } from "../types";
import { itemSubtitle } from "../utils";
import type { AppCtx } from "../app-context";

/** 对齐移动端 SecurityScreen：已泄露/重复使用/弱密码/长期未更新 */
const SECTIONS: { key: "breached" | "weak" | "reused" | "old"; title: string; hint: string; color: string }[] = [
  { key: "breached", title: "已泄露密码", hint: "在公开泄露库中出现过，请立即更换", color: "#FF3B30" },
  { key: "weak", title: "弱密码", hint: "强度不足，建议使用生成器生成", color: "#FF9500" },
  { key: "reused", title: "重复使用的密码", hint: "多个条目共用同一个密码", color: "#FFCC00" },
  { key: "old", title: "长期未更新", hint: "超过 180 天没有修改", color: "#AF52DE" },
];

/** 半圆仪表：180°→0° 顺时针，按各段数量占比上色 */
function Gauge({ report }: { report: SecurityReport }) {
  const R = 78;
  const CX = 110;
  const CY = 104;
  const stroke = 14;
  const polar = (deg: number): [number, number] => {
    const rad = (deg * Math.PI) / 180;
    return [CX + R * Math.cos(rad), CY - R * Math.sin(rad)];
  };
  const arc = (from: number, to: number): string => {
    const [x1, y1] = polar(180 - 180 * from);
    const [x2, y2] = polar(180 - 180 * to);
    return `M ${x1.toFixed(2)} ${y1.toFixed(2)} A ${R} ${R} 0 0 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
  };
  const total = report.total || 1;
  let cursor = 0;
  const segs = SECTIONS.map((s) => {
    const frac = report[s.key].length / total;
    const from = cursor;
    cursor += frac;
    return { color: s.color, from, to: cursor, count: report[s.key].length };
  });

  return (
    <div className="sec-gauge">
      <svg viewBox="0 0 220 112" width="100%" height="100%">
        <path d={arc(0, 1)} fill="none" stroke="var(--border)" strokeWidth={stroke} strokeLinecap="round" />
        {segs.map(
          (s, i) =>
            s.count > 0 && (
              <path
                key={i}
                d={arc(s.from, s.to)}
                fill="none"
                stroke={s.color}
                strokeWidth={stroke}
                strokeLinecap="butt"
              />
            ),
        )}
      </svg>
      <div className="sec-gauge-center">
        <div className="sec-gauge-total">{report.total}</div>
        <div className="muted small">个密码</div>
      </div>
    </div>
  );
}

export function SecurityDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [report, setReport] = useState<SecurityReport | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const run = async (withHibp: boolean) => {
    setBusy(true);
    setError(null);
    try {
      setReport(withHibp ? await securityReportFull() : await securityReport());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  // 打开即先跑本地检查（秒出结果），在线泄露检查由按钮触发
  useEffect(() => {
    void run(false);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const goto = (item: VaultItem) => {
    ctx.setSelectItemId(item.id);
    onClose();
  };

  const issueCount = useMemo(() => {
    if (!report) return 0;
    return report.breached.length + report.weak.length + report.reused.length + report.old.length;
  }, [report]);

  const zero = report && issueCount === 0;

  return (
    <Dialog title="安全中心" onClose={onClose} width={560}>
      {report && <Gauge report={report} />}

      <button className="btn primary block tall" disabled={busy} onClick={() => void run(true)}>
        {busy ? "检查中…" : "安全检查"}
      </button>
      {error && <div className="err-text">{error}</div>}

      {report && (
        <>
          <div className="sec-stat-grid">
            {SECTIONS.map((s) => (
              <div className="sec-stat" key={s.key}>
                <div className="sec-stat-count" style={{ color: s.color }}>
                  {report[s.key].length}
                </div>
                <div className="row" style={{ gap: 6 }}>
                  <span className="sec-dot" style={{ background: s.color }} />
                  <span className="muted small">{s.title}</span>
                </div>
              </div>
            ))}
          </div>

          {zero && (
            <div className="sec-ok-card">
              <div style={{ fontSize: 22 }}>✓</div>
              <div className="sec-ok-title">没有发现明显问题</div>
              <div className="muted small">继续为每个账号使用独立的长密码。</div>
            </div>
          )}

          {!zero &&
            SECTIONS.filter((s) => report[s.key].length > 0).map((s) => (
              <div className="sec-issue" key={s.key}>
                <div className="sec-issue-head">
                  <span className="sec-dot" style={{ background: s.color }} />
                  <span className="grow" style={{ fontWeight: 700 }}>
                    {s.title}（{report[s.key].length}）
                  </span>
                </div>
                <div className="setting-sub">{s.hint}</div>
                {report[s.key].slice(0, 20).map((item) => (
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
                {report[s.key].length > 20 && (
                  <div className="muted small">仅显示前 20 条，共 {report[s.key].length} 条</div>
                )}
              </div>
            ))}

          <div className="sec-foot muted small">
            在线泄露检查使用 HIBP k-anonymity：只发送 SHA-1 前缀 5 位，密码本身不会离开本机。
          </div>
        </>
      )}
    </Dialog>
  );
}
