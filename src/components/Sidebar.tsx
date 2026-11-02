import type { Label, VaultMeta } from "../types";
import type { SectionFilter } from "../app-context";
import { LabelGlyph } from "./LabelGlyph";
import { Icon, type IconName } from "./icons";

const SECTIONS: { key: SectionFilter; label: string; icon: IconName }[] = [
  { key: "ALL", label: "全部", icon: "menu" },
  { key: "FAVORITES", label: "收藏", icon: "star" },
  { key: "TRASH", label: "回收站", icon: "trash" },
];

/**
 * 侧栏：只显示当前密码库（不允许在此切换，切换走「设置 → 密码库」）；
 * 底部不再放锁定按钮与软件名（对齐移动端侧栏，v1 项 9/14）。
 */
export function Sidebar({
  vault,
  itemCount,
  labels,
  section,
  labelFilter,
  onSection,
  onLabelFilter,
  onManageLabels,
}: {
  vault: VaultMeta;
  itemCount: number;
  labels: Label[];
  section: SectionFilter;
  labelFilter: string | null;
  onSection: (s: SectionFilter) => void;
  onLabelFilter: (id: string | null) => void;
  onManageLabels: () => void;
}) {
  return (
    <div className="sidebar">
      <div className="sidebar-head">
        <div className="title">{vault.name}</div>
        <div className="sub">{itemCount} 个条目 · 已加密</div>
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
          <span className="ico">
            <Icon name={s.icon} size={18} strokeWidth={1.9} />
          </span>
          <span>{s.label}</span>
        </button>
      ))}

      <button className="sidebar-row" onClick={onManageLabels}>
        <span className="ico">
          <Icon name="tag" size={18} strokeWidth={1.9} />
        </span>
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
          <LabelGlyph color={labelColor(l.color)} size={15} />
          <span className="grow" style={{ textAlign: "left" }}>
            {l.name}
          </span>
        </button>
      ))}

      <div className="sidebar-spacer" />
    </div>
  );
}

function labelColor(color: number): string {
  return `#${(color >>> 0).toString(16).padStart(8, "0").slice(-6)}`;
}
