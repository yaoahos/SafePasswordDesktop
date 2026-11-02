import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { updateVault } from "../api";
import type { Label } from "../types";
import { labelColorCss, newId } from "../utils";
import type { AppCtx } from "../app-context";

const PRESET_COLORS = [
  "#2563EB",
  "#0891B2",
  "#16A34A",
  "#CA8A04",
  "#EA580C",
  "#DC2626",
  "#9333EA",
  "#64748B",
];

/** hex → 有符号 ARGB int32（serde 需要 i32，直接传无符号数会溢出失败） */
function hexToInt32(hex: string): number {
  return Number.parseInt(hex.replace("#", ""), 16) | 0;
}

export function LabelManagerDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const vault = ctx.vault;
  const [name, setName] = useState("");
  const [color, setColor] = useState(PRESET_COLORS[0]);
  const [error, setError] = useState<string | null>(null);

  if (!vault) return null;

  const persist = async (labels: Label[], okMsg: string) => {
    try {
      await updateVault({ ...vault, labels, updatedAt: Date.now() });
      await ctx.refreshFromBackend();
      ctx.notify(okMsg, "success");
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const add = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("请输入标签名称");
      return;
    }
    if (vault.labels.some((l) => l.name === trimmed)) {
      setError("已存在同名标签");
      return;
    }
    const label: Label = {
      id: newId(),
      name: trimmed,
      color: hexToInt32(color),
      sortOrder: vault.labels.length,
    };
    await persist([...vault.labels, label], "标签已创建");
    setName("");
  };

  const rename = async (label: Label, nextName: string) => {
    const trimmed = nextName.trim();
    if (!trimmed || trimmed === label.name) return;
    await persist(
      vault.labels.map((l) => (l.id === label.id ? { ...l, name: trimmed } : l)),
      "标签已重命名",
    );
  };

  const remove = async (label: Label) => {
    await persist(
      vault.labels.filter((l) => l.id !== label.id),
      "标签已删除",
    );
    ctx.notify("注意：条目上仍保留该标签引用，请在条目中移除", "info");
  };

  return (
    <Dialog title="标签管理" onClose={onClose}>
      <div className="card" style={{ boxShadow: "none", border: "1px solid var(--border)" }}>
        {vault.labels.length === 0 && <div className="muted small">还没有标签</div>}
        {vault.labels.map((l) => (
          <div className="mini-row" key={l.id}>
            <span className="dot" style={{ background: labelColorCss(l.color) }} />
            <input
              className="input grow"
              style={{ height: 30 }}
              defaultValue={l.name}
              onBlur={(e) => void rename(l, e.target.value)}
            />
            <button className="link-btn" style={{ color: "var(--danger)" }} onClick={() => void remove(l)}>
              删除
            </button>
          </div>
        ))}
      </div>

      <div className="spacer-10" />
      <div className="section-cap">新建标签</div>
      <div className="row">
        <input
          className="input grow"
          placeholder="标签名称"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void add();
          }}
        />
        <button className="btn primary" onClick={() => void add()}>
          添加
        </button>
      </div>
      <div className="spacer-10" />
      <div className="row" style={{ gap: 10 }}>
        {PRESET_COLORS.map((c) => (
          <button
            key={c}
            onClick={() => setColor(c)}
            title={c}
            style={{
              width: 26,
              height: 26,
              borderRadius: "50%",
              border: color === c ? "3px solid var(--text)" : "2px solid transparent",
              background: c,
              cursor: "pointer",
            }}
          />
        ))}
      </div>
      {error && <div className="err-text">{error}</div>}
    </Dialog>
  );
}
