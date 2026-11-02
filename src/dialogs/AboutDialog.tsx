import { Dialog } from "../components/Dialog";

export function AboutDialog({ onClose }: { onClose: () => void }) {
  const rows = [
    { k: "版本", v: "0.1.0" },
    { k: "技术栈", v: "Tauri 2 + React" },
    { k: "功能范围", v: "与手机端互通" },
  ];
  return (
    <Dialog title="关于 SafePassword" onClose={onClose}>
      <div className="auth-logo" style={{ marginBottom: 14 }}>
        <div className="badge">🔑</div>
        <div>
          <div className="auth-title" style={{ fontSize: 17 }}>
            SafePassword
          </div>
          <div className="auth-sub">Windows 桌面版</div>
        </div>
      </div>
      <div className="card" style={{ boxShadow: "none", border: "1px solid var(--border)" }}>
        {rows.map((r) => (
          <div className="field-row" key={r.k}>
            <div className="field-label">{r.k}</div>
            <div className="field-value">{r.v}</div>
          </div>
        ))}
      </div>
    </Dialog>
  );
}
