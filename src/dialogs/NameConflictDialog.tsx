import { useState } from "react";
import { Dialog } from "../components/Dialog";

/** 同步时云端已有同名密码库：提示并让用户给本地这个库改名 */
export function NameConflictDialog({
  currentName,
  message,
  busy,
  onCancel,
  onRename,
}: {
  currentName: string;
  message: string;
  busy: boolean;
  onCancel: () => void;
  onRename: (name: string) => void;
}) {
  const [name, setName] = useState(suggestName(currentName));
  const trimmed = name.trim();
  const invalid =
    !trimmed || trimmed.toLowerCase() === currentName.trim().toLowerCase();

  return (
    <Dialog title="云端已有同名密码库" onClose={onCancel} width={440}>
      <div className="muted small" style={{ lineHeight: 1.8, marginBottom: 12 }}>
        {message || `云端已存在名为「${currentName}」的密码库。`}
        <br />
        云端文件名按库内容生成，同名会是两个互不相干的库，请给本机这个库换一个名字再同步。
      </div>
      <label className="field-caption" htmlFor="name-conflict-input">
        本机密码库的新名字
      </label>
      <input
        id="name-conflict-input"
        className="input"
        value={name}
        autoFocus
        maxLength={40}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && !invalid && !busy) onRename(trimmed);
        }}
      />
      {trimmed && invalid && (
        <div className="hint-text" style={{ marginTop: 6 }}>
          新名字不能和现在的名字相同
        </div>
      )}
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 16 }}>
        <button className="btn" onClick={onCancel} disabled={busy}>
          取消
        </button>
        <button
          className="btn primary"
          disabled={invalid || busy}
          onClick={() => onRename(trimmed)}
        >
          {busy ? "改名中…" : "改名并同步"}
        </button>
      </div>
    </Dialog>
  );
}

/** 建议名字：`测试库` → `测试库 2`，已带序号就往后加 */
function suggestName(name: string): string {
  const base = name.trim() || "密码库";
  const m = base.match(/^(.*?)\s*(\d+)$/);
  if (m && m[1]) return `${m[1]} ${Number(m[2]) + 1}`;
  return `${base} 2`;
}
