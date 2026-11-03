import { useState } from "react";
import type { Field } from "../types";
import { Dialog } from "../components/Dialog";
import { Icon } from "../components/icons";
import { copyText, formatTime } from "../utils";

/** 密码/秘密字段的历史值：查看、复制、删除。 */
export function PasswordHistoryDialog({
  field,
  onClose,
  onDelete,
  notify,
}: {
  field: Field;
  onClose: () => void;
  /** 删除第 index 条（后端立即落盘） */
  onDelete: (index: number) => Promise<void>;
  notify: (msg: string, kind?: "info" | "success" | "error") => void;
}) {
  const [shown, setShown] = useState<Record<number, boolean>>({});
  const [busy, setBusy] = useState(false);
  const entries = field.history ?? [];

  const remove = async (index: number) => {
    if (busy) return;
    setBusy(true);
    try {
      await onDelete(index);
      setShown({});
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog title="历史记录" onClose={onClose} width={460}>
      <div className="muted small" style={{ marginBottom: 10 }}>
        {field.label || "密码"}
      </div>
      {entries.length === 0 ? (
        <div className="hint-text">还没有历史记录。改一次这个字段就会把旧值记下来。</div>
      ) : (
        <div className="history-list">
          {entries.map((entry, index) => {
            const open = shown[index] === true;
            return (
              <div className="history-row" key={`${entry.changedAt}-${index}`}>
                <div className="history-main">
                  <div className="history-value" title={open ? entry.value : undefined}>
                    {open ? entry.value : "•".repeat(Math.min(Math.max(entry.value.length, 8), 16))}
                  </div>
                  <div className="history-time">{formatTime(entry.changedAt)}</div>
                </div>
                <button
                  className="icon-btn"
                  title={open ? "隐藏" : "显示"}
                  onClick={() => setShown((s) => ({ ...s, [index]: !open }))}
                >
                  <Icon name={open ? "eye-off" : "eye"} size={15} />
                </button>
                <button
                  className="icon-btn"
                  title="复制"
                  onClick={() => {
                    void copyText(entry.value).then(
                      () => notify("已复制历史密码", "success"),
                      () => notify("复制失败", "error"),
                    );
                  }}
                >
                  <Icon name="copy" size={15} />
                </button>
                <button
                  className="icon-btn"
                  title="删除这一条"
                  disabled={busy}
                  onClick={() => {
                    void remove(index);
                  }}
                >
                  <Icon name="trash" size={15} />
                </button>
              </div>
            );
          })}
        </div>
      )}
    </Dialog>
  );
}
