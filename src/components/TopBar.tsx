import { Icon, type IconName } from "./icons";
import { WindowControls } from "./WindowControls";
import { APP_VERSION } from "../version";

export type TopAction =
  | "GENERATOR"
  | "SECURITY"
  | "THEME"
  | "SETTINGS"
  | "ABOUT"
  | "SYNC"
  | "LOCK";

/** 图标色调（语义 token，随主题/暗色/强调色联动，定义见 styles.css 的 --tone-*） */
type Tone = "primary" | "danger" | "violet" | "muted" | "cyan" | "crimson";

/** 顶部动作：线性图标、放大加深；WebDAV 与导入入口已移除（改由设置页承载） */
export const TOP_ACTIONS: { key: TopAction; label: string; icon: IconName; tone: Tone }[] = [
  { key: "GENERATOR", label: "密码生成器", icon: "dice", tone: "primary" },
  { key: "SECURITY", label: "安全中心", icon: "shield", tone: "danger" },
  { key: "THEME", label: "主题", icon: "palette", tone: "violet" },
  { key: "SETTINGS", label: "设置", icon: "sliders", tone: "muted" },
  { key: "ABOUT", label: "关于", icon: "info", tone: "cyan" },
  { key: "SYNC", label: "同步", icon: "refresh", tone: "primary" },
  { key: "LOCK", label: "锁定", icon: "lock", tone: "crimson" },
];

/** 应用标题栏（标准桌面软件样式）：左侧图标 + 应用名 + 版本，右侧窗口按钮 */
export function AppTitleBar() {
  return (
    <div className="titlebar" data-tauri-drag-region="true">
      <div className="titlebar-brand" data-tauri-drag-region="true">
        <img className="titlebar-logo" src="/icon.png" alt="" draggable={false} />
        <span className="titlebar-title">MySafePass</span>
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
            <span className={`topbar-icon tone-${a.tone}`}>
              <Icon name={a.icon} size={19} />
            </span>
            <span>{a.label}</span>
          </button>
        ))}
      </div>
    </div>
  );
}
