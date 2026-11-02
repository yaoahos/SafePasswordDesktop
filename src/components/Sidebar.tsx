import type { Label, VaultMeta } from "../types";
import type { SectionFilter } from "../app-context";

const SECTIONS: { key: SectionFilter; label: string; icon: string }[] = [
  { key: "ALL", label: "全部", icon: "☰" },
  { key: "FAVORITES", label: "收藏", icon: "★" },
  { key: "TRASH", label: "回收站", icon: "🗑" },
];

export function Sidebar({
  vault,
  labels,
  section,
  labelFilter,
  onSection,
  onLabelFilter,
  onSwitchVault,
  onManageLabels,
  onLock,
}: {
  vault: VaultMeta;
  labels: Label[];
  section: SectionFilter;
  labelFilter: string | null;
  onSection: (s: SectionFilter) => void;
  onLabelFilter: (id: string | null) => void;
  onSwitchVault: () => void;
  onManageLabels: () => void;
  onLock: () => void;
}) {
  return (
    <div className="sidebar">
      <div className="sidebar-head" style={{ cursor: "pointer" }} onClick={onSwitchVault} title="切换密码库">
        <div className="row">
          <div className="grow">
            <div className="title">{vault.name}</div>
            <div className="sub">我的密码库 · 点击切换</div>
          </div>
          <span className="muted">▾</span>
        </div>
      </div>

      {SECTIONS.map((s) => (
        <button
          key={s.key}
          className={`sidebar-row${section === s.key && labelFilter === null ? " active" : ""}`}
          onClick={() => {
            onLabelFilter(null);
            onSection(s.key);
          }}
        >
          <span className="ico">{s.icon}</span>
          <span>{s.label}</span>
        </button>
      ))}

      <button className="sidebar-row" onClick={onManageLabels}>
        <span className="ico">🏷</span>
        <span>标签管理</span>
      </button>

      <div className="sidebar-divider" />
      <div className="sidebar-caption">标签</div>

      {labels.length === 0 && (
        <div className="sidebar-caption" style={{ color: "var(--text-faint)" }}>
          暂无标签
        </div>
      )}
      {labels.map((l) => (
        <button
          key={l.id}
          className={`label-row${labelFilter === l.id ? " active" : ""}`}
          onClick={() => onLabelFilter(labelFilter === l.id ? null : l.id)}
        >
          <span className="dot" style={{ background: labelColor(l.color) }} />
          <span className="grow" style={{ textAlign: "left" }}>
            {l.name}
          </span>
        </button>
      ))}

      <div className="sidebar-spacer" />

      <button className="sidebar-row" onClick={onLock}>
        <span className="ico">🔒</span>
        <span>锁定</span>
      </button>
      <div className="sidebar-foot">SafePassword 0.1.0-demo</div>
    </div>
  );
}

function labelColor(color: number): string {
  return `#${(color >>> 0).toString(16).padStart(8, "0").slice(-6)}`;
}
