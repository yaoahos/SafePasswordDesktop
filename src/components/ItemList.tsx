import type { Label, SortMode, VaultItem } from "../types";
import { iconBg, itemIcon, itemMatches, labelColorCss } from "../utils";
import { Icon } from "./icons";

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
  sortMode,
  sortAsc,
  onChangeSort,
  syncText,
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
  sortMode: SortMode;
  sortAsc: boolean;
  onChangeSort: (m: SortMode) => void;
  /** 同步瞬时状态（对齐手机端 3 秒状态文本，显示在分类标题右侧） */
  syncText?: string | null;
}) {
  const visible = items.filter((i) => itemMatches(i, query, searchPasswords));

  return (
    <div className="item-list">
      <div className="item-list-top">
        <div className="list-head">
          <div className="list-title" title={sectionTitle}>
            {sectionTitle}
          </div>
          {syncText && <div className="list-sync">{syncText}</div>}
          <div className="list-ctls">
            <select
              className="input list-ctl list-ctl-sel"
              value={sortMode}
              title="排序方式"
              onChange={(e) => onChangeSort(e.target.value as SortMode)}
            >
              <option value="CREATED">最近创建</option>
              <option value="UPDATED">最近更新</option>
              <option value="TITLE">标题</option>
            </select>
            <button className="btn list-ctl" title="切换升 / 降序" onClick={() => onChangeSort(sortMode)}>
              <Icon name={sortAsc ? "arrow-up" : "arrow-down"} size={16} className="bold-ico" />
            </button>
            <button className="btn primary list-ctl" onClick={onNew}>
              <Icon name="plus" size={16} />
              新建
            </button>
          </div>
        </div>
        <div className="input-wrap">
          <span className="ico">
            <Icon name="search" size={16} />
          </span>
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
              <span
                className={`item-icon${icon.kind === "emoji" ? " emoji" : ""}`}
                style={iconBg(icon)}
              >
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
                  {item.favorite && (
                    <span className="star">
                      <Icon name="star-filled" size={13} />
                    </span>
                  )}
                </span>
                {labels.length > 0 && (
                  <div className="item-meta">
                    {labels.map((l) => (
                      <span key={l.id} className="chip-dot">
                        <span className="dot" style={{ background: labelColorCss(l.color) }} />
                        {l.name}
                      </span>
                    ))}
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
