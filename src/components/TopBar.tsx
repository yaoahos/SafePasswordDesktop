export type TopAction =
  | "GENERATOR"
  | "SECURITY"
  | "WEBDAV"
  | "THEME"
  | "IMPORT"
  | "SETTINGS"
  | "ABOUT"
  | "SYNC"
  | "LOCK";

/** 与 Compose Demo 的 TopAction 顺序、配色一致 */
export const TOP_ACTIONS: { key: TopAction; label: string; icon: string; color: string; tint: string }[] = [
  { key: "GENERATOR", label: "密码生成器", icon: "🎲", color: "#1677FF", tint: "#EAF2FF" },
  { key: "SECURITY", label: "安全检查", icon: "🛡", color: "#DC2626", tint: "#FDEAEA" },
  { key: "WEBDAV", label: "WebDAV 同步", icon: "☁", color: "#0891B2", tint: "#E6F6FA" },
  { key: "THEME", label: "主题", icon: "🎨", color: "#9333EA", tint: "#F5EBFE" },
  { key: "IMPORT", label: "导入密码", icon: "⤓", color: "#16A34A", tint: "#E8F7EE" },
  { key: "SETTINGS", label: "设置", icon: "⚙", color: "#4B5563", tint: "#EEF0F3" },
  { key: "ABOUT", label: "关于", icon: "ⓘ", color: "#0891B2", tint: "#E6F6FA" },
  { key: "SYNC", label: "同步", icon: "⟳", color: "#1677FF", tint: "#EAF2FF" },
  { key: "LOCK", label: "锁定", icon: "🔒", color: "#DC2626", tint: "#FDEAEA" },
];

export function TopBar({ onAction }: { onAction: (a: TopAction) => void }) {
  return (
    <div className="topbar">
      {TOP_ACTIONS.map((a) => (
        <button key={a.key} className="topbar-btn" onClick={() => onAction(a.key)} title={a.label}>
          <span
            className="topbar-icon"
            style={{ background: a.tint, color: a.color, fontSize: a.icon.length > 1 ? 12 : 15 }}
          >
            {a.icon}
          </span>
          <span>{a.label}</span>
        </button>
      ))}
    </div>
  );
}
