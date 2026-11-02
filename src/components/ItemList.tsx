import type { Label, VaultItem } from "../types";
import { itemIcon, itemMatches, itemSubtitle, labelColorCss } from "../utils";

export function ItemList({
  items,
  allLabels,
  sectionTitle,
  query,
  onQuery,
  searchPasswords,
  selectedId,
  onSelect,
  onNew,
}: {
  items: VaultItem[];
  allLabels: Label[];
  sectionTitle: string;
  query: string;
  onQuery: (q: string) => void;
  searchPasswords: boolean;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNew: () => void;
}) {
  const visible = items.filter((i) => itemMatches(i, query, searchPasswords));

  return (
    <div className="item-list">
      <div className="item-list-top">
        <div className="row">
          <div className="grow" style={{ fontWeight: 600 }}>
            {sectionTitle}
          </div>
          <button className="btn primary sm" onClick={onNew}>
            ＋ 新建
          </button>
        </div>
        <div className="input-wrap">
          <span className="ico">🔍</span>
          <input
            className="input with-ico"
            placeholder="搜索标题、账号、网址"
            value={query}
            onChange={(e) => onQuery(e.target.value)}
          />
        </div>
      </div>

      <div className="item-scroll">
        {visible.length === 0 && (
          <div className="empty-hint">
            {items.length === 0 ? "这里还没有条目，点击右上角“新建”添加" : "没有匹配的条目"}
          </div>
        )}
        {visible.map((item) => {
          const icon = itemIcon(item);
          const labels = item.labelIds
            .map((id) => allLabels.find((l) => l.id === id))
            .filter((l): l is Label => Boolean(l));
          return (
            <button
              key={item.id}
              className={`item-card${selectedId === item.id ? " selected" : ""}`}
              onClick={() => onSelect(item.id)}
            >
              <span className={`item-icon${icon.kind === "emoji" ? " emoji" : ""}`}>
                {icon.kind === "img" ? (
                  <img src={icon.src} alt="" />
                ) : icon.kind === "char" ? (
                  icon.char
                ) : (
                  icon.emoji
                )}
              </span>
              <span className="grow" style={{ minWidth: 0 }}>
                <span className="item-title-row">
                  <span className="item-title">{item.title || "未命名"}</span>
                  {item.favorite && <span className="star">★</span>}
                </span>
                <div className="item-sub">{itemSubtitle(item)}</div>
                {(labels.length > 0 || item.status === "ARCHIVED") && (
                  <div className="item-meta">
                    {labels.map((l) => (
                      <span key={l.id} className="chip" style={{ background: `${labelColorCss(l.color)}22` }}>
                        <span className="dot" style={{ background: labelColorCss(l.color), display: "inline-block", marginRight: 4 }} />
                        {l.name}
                      </span>
                    ))}
                    {item.status === "ARCHIVED" && <span className="chip archived">已归档</span>}
                  </div>
                )}
              </span>
            </button>
          );
        })}
      </div>
    </div>
  );
}
