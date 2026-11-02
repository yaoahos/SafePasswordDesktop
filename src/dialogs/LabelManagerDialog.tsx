import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { LabelGlyph } from "../components/LabelGlyph";
import { updateVault } from "../api";
import type { Label } from "../types";
import { labelColorCss, newId } from "../utils";
import type { AppCtx } from "../app-context";

/** 对齐移动端 LabelManagerScreen.labelPalette（20 色） */
const LABEL_PALETTE = [
  "#1E88E5",
  "#3949AB",
  "#5E35B1",
  "#8E24AA",
  "#D81B60",
  "#E53935",
  "#F4511E",
  "#FB8C00",
  "#FFB300",
  "#FDD835",
  "#7CB342",
  "#43A047",
  "#00897B",
  "#00ACC1",
  "#26A69A",
  "#66BB6A",
  "#EF5350",
  "#EC407A",
  "#546E7A",
  "#6D4C41",
];

/** hex → 有符号 ARGB int32（serde 需要 i32，直接传无符号数会溢出失败） */
function hexToInt32(hex: string): number {
  const clean = hex.replace("#", "").trim();
  const rgb = clean.length >= 6 ? clean.slice(-6) : clean.padStart(6, "0");
  return (0xff000000 | Number.parseInt(rgb, 16)) | 0;
}

/** int32 颜色 → #RRGGBB */
function int32ToHex(color: number): string {
  return `#${(color >>> 0).toString(16).padStart(8, "0").slice(-6).toUpperCase()}`;
}

function normalizeHex(input: string): string | null {
  const clean = input.replace("#", "").trim();
  if (!/^[0-9a-fA-F]{6}$/.test(clean)) return null;
  return `#${clean.toUpperCase()}`;
}

export function LabelManagerDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const vault = ctx.vault;
  const [view, setView] = useState<"list" | "edit">("list");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [color, setColor] = useState(LABEL_PALETTE[0]);
  const [sortOrder, setSortOrder] = useState("0");
  const [hexInput, setHexInput] = useState("");
  const [showPalette, setShowPalette] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  if (!vault) return null;

  const sorted = [...vault.labels].sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name, "zh-Hans-CN"));
  const countOf = (id: string) => vault.items.filter((i) => i.labelIds.includes(id)).length;

  const persist = async (labels: Label[], okMsg: string) => {
    setBusy(true);
    try {
      await updateVault({ ...vault, labels, updatedAt: Date.now() });
      await ctx.refreshFromBackend();
      ctx.notify(okMsg, "success");
      setError(null);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const openNew = () => {
    setEditingId(null);
    setName("");
    setColor(LABEL_PALETTE[0]);
    setSortOrder(String(vault.labels.length));
    setHexInput("");
    setShowPalette(false);
    setError(null);
    setView("edit");
  };

  const openEdit = (l: Label) => {
    setEditingId(l.id);
    setName(l.name);
    setColor(int32ToHex(l.color));
    setSortOrder(String(l.sortOrder));
    setHexInput("");
    setShowPalette(false);
    setError(null);
    setView("edit");
  };

  const applyHex = () => {
    const hex = normalizeHex(hexInput);
    if (!hex) {
      setError("请输入 6 位十六进制颜色码，如 #2563EB");
      return;
    }
    setColor(hex);
    setError(null);
  };

  const save = async () => {
    const trimmed = name.trim();
    if (!trimmed) {
      setError("请输入标签名称");
      return;
    }
    const dup = vault.labels.some((l) => l.id !== editingId && l.name === trimmed);
    if (dup) {
      setError("已存在同名标签");
      return;
    }
    const order = Number(sortOrder);
    const nextOrder = Number.isFinite(order) ? Math.trunc(order) : vault.labels.length;
    const labels = editingId
      ? vault.labels.map((l) => (l.id === editingId ? { ...l, name: trimmed, color: hexToInt32(color), sortOrder: nextOrder } : l))
      : [...vault.labels, { id: newId(), name: trimmed, color: hexToInt32(color), sortOrder: nextOrder }];
    const ok = await persist(labels, editingId ? "标签已保存" : "标签已创建");
    if (ok) setView("list");
  };

  const remove = async () => {
    if (!editingId) return;
    const ok = await persist(
      vault.labels.filter((l) => l.id !== editingId),
      "标签已删除",
    );
    if (ok) {
      ctx.notify("注意：条目上仍保留该标签引用，请在条目中移除", "info");
      setView("list");
    }
  };

  return (
    <Dialog title="标签管理" onClose={onClose} width={520}>
      {view === "list" && (
        <>
          <button className="btn primary block" onClick={openNew}>
            ＋ 新建标签
          </button>
          <div className="spacer-10" />
          {sorted.length === 0 && <div className="muted small">还没有标签</div>}
          <div className="label-list">
            {sorted.map((l) => (
              <button key={l.id} className="label-list-row" onClick={() => openEdit(l)}>
                <LabelGlyph color={labelColorCss(l.color)} size={22} />
                <span className="grow" style={{ textAlign: "left" }}>
                  <span className="label-name">{l.name}</span>
                  <span className="muted small"> · {countOf(l.id)} 个条目</span>
                </span>
                <span className="label-hex muted small">{int32ToHex(l.color)}</span>
                <span className="muted">✎</span>
              </button>
            ))}
          </div>
          <div className="setting-sub" style={{ marginTop: 8 }}>
            点击标签行编辑名称、颜色与排序；数字越小越靠前。
          </div>
        </>
      )}

      {view === "edit" && (
        <>
          <div className="section-cap">{editingId ? "编辑标签" : "新建标签"}</div>
          <div className="form-row">
            <label className="field-caption">标签名称</label>
            <input
              className="input"
              autoFocus
              value={name}
              onChange={(e) => setName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void save();
              }}
            />
          </div>

          <div className="form-row">
            <label className="field-caption">颜色</label>
            <div className="row">
              <LabelGlyph color={color} size={28} />
              <span className="mono">{color}</span>
              <button className="btn sm" onClick={() => setShowPalette((v) => !v)}>
                {showPalette ? "收起调色板" : "选择颜色"}
              </button>
            </div>
          </div>

          {showPalette && (
            <div className="palette-box">
              <div className="palette-grid">
                {LABEL_PALETTE.map((c) => (
                  <button
                    key={c}
                    className={`palette-dot${color.toUpperCase() === c ? " active" : ""}`}
                    style={{ background: c }}
                    title={c}
                    onClick={() => {
                      setColor(c);
                      setError(null);
                    }}
                  />
                ))}
              </div>
              <div className="row" style={{ marginTop: 10 }}>
                <input
                  className="input grow"
                  placeholder="自定义颜色 #2563EB"
                  value={hexInput}
                  onChange={(e) => setHexInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") applyHex();
                  }}
                />
                <button className="btn sm" onClick={applyHex}>
                  应用
                </button>
              </div>
            </div>
          )}

          <div className="form-row">
            <label className="field-caption">排序</label>
            <input
              className="input"
              style={{ width: 90, textAlign: "center" }}
              type="number"
              value={sortOrder}
              onChange={(e) => setSortOrder(e.target.value)}
            />
          </div>
          <div className="setting-sub">数字越小越靠前。</div>

          {error && <div className="err-text">{error}</div>}

          <div className="row" style={{ marginTop: 14, justifyContent: "flex-end" }}>
            {editingId && (
              <button className="btn sm danger" disabled={busy} onClick={() => void remove()} style={{ marginRight: "auto" }}>
                删除标签
              </button>
            )}
            <button className="btn" onClick={() => setView("list")}>
              取消
            </button>
            <button className="btn primary" disabled={busy} onClick={() => void save()}>
              {busy ? "保存中…" : "保存"}
            </button>
          </div>
        </>
      )}
    </Dialog>
  );
}
