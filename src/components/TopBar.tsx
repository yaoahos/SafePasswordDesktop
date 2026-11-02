import { Icon, type IconName } from "./icons";
import { WindowControls } from "./WindowControls";

export type TopAction =
  | "GENERATOR"
  | "SECURITY"
  | "THEME"
  | "SETTINGS"
  | "ABOUT"
  | "SYNC"
  | "LOCK";

export const APP_VERSION = "0.1.0";

/** 顶部动作：线性图标、放大加深；WebDAV 与导入入口已移除（改由设置页承载） */
export const TOP_ACTIONS: { key: TopAction; label: string; icon: IconName; color: string }[] = [
  { key: "GENERATOR", label: "密码生成器", icon: "key", color: "#1677FF" },
  { key: "SECURITY", label: "安全中心", icon: "shield", color: "#DC2626" },
  { key: "THEME", label: "主题", icon: "palette", color: "#9333EA" },
  { key: "SETTINGS", label: "设置", icon: "sliders", color: "#374151" },
  { key: "ABOUT", label: "关于", icon: "info", color: "#0891B2" },
  { key: "SYNC", label: "同步", icon: "refresh", color: "#1677FF" },
  { key: "LOCK", label: "锁定", icon: "lock", color: "#C2185B" },
];

/** 应用标题栏（标准桌面软件样式）：左侧图标 + 应用名 + 版本，右侧窗口按钮 */
export function AppTitleBar() {
  return (
    <div className="titlebar" data-tauri-drag-region="true">
      <div className="titlebar-brand" data-tauri-drag-region="true">
        <img className="titlebar-logo" src="/icon.png" alt="" draggable={false} />
        <span className="titlebar-title">SafePassword</span>
        <span className="titlebar-version">v{APP_VERSION}</span>
      </div>
      <div className="titlebar-grow" data-tauri-drag-region="true" />
      <WindowControls />
    </div>
  );
}

/** 顶栏 = 标题栏（上）+ 工具栏（下，动作按钮靠左） */
export function TopBar({ onAction }: { onAction: (a: TopAction) => void }) {
  return (
    <div className="topbar">
      <AppTitleBar />
      <div className="toolbar">
        {TOP_ACTIONS.map((a) => (
          <button key={a.key} className="topbar-btn" onClick={() => onAction(a.key)} title={a.label}>
            <span className="topbar-icon" style={{ color: a.color }}>
              <Icon name={a.icon} size={19} />
            </span>
            <span>{a.label}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
