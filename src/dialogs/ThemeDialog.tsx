import { useState } from "react";
import { Dialog } from "../components/Dialog";
import type { ThemeMode } from "../types";
import type { AppCtx } from "../app-context";

const THEMES: { key: ThemeMode; label: string }[] = [
  { key: "system", label: "跟随系统" },
  { key: "light", label: "浅色" },
  { key: "dark", label: "深色" },
];

/** 对齐移动端 ThemeScreen.themePresets（20 色） */
const THEME_PRESETS = [
  "#1677FF",
  "#FF9500",
  "#EF4444",
  "#E11D48",
  "#DB2777",
  "#A855F7",
  "#6366F1",
  "#2563EB",
  "#0891B2",
  "#0D9488",
  "#16A34A",
  "#65A30D",
  "#CA8A04",
  "#EA580C",
  "#475569",
  "#0F172A",
  "#334155",
  "#7C2D12",
  "#166534",
  "#581C87",
];

export function ThemeDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [hex, setHex] = useState("");
  const [error, setError] = useState<string | null>(null);
  const accent = (ctx.settings.accent || "#1677FF").toUpperCase();

  const applyAccent = (c: string) => {
    setError(null);
    void ctx.applySettings({ ...ctx.settings, accent: c.toUpperCase() });
  };

  const applyHex = () => {
    const clean = hex.replace("#", "").trim();
    if (!/^[0-9a-fA-F]{6}$/.test(clean)) {
      setError("请输入 6 位十六进制颜色码，如 #2563EB");
      return;
    }
    applyAccent(`#${clean.toUpperCase()}`);
  };

  return (
    <Dialog title="主题" onClose={onClose} width={460}>
      <div className="section-cap">模式</div>
      <div className="seg">
        {THEMES.map((t) => (
          <button
            key={t.key}
            className={ctx.settings.theme === t.key ? "active" : ""}
            onClick={() => void ctx.applySettings({ ...ctx.settings, theme: t.key })}
          >
            {t.label}
          </button>
        ))}
      </div>

      <div className="section-cap" style={{ marginTop: 14 }}>
        强调色
      </div>
      <div className="theme-grid">
        {THEME_PRESETS.map((c) => (
          <button
            key={c}
            className={`theme-swatch${accent === c ? " active" : ""}`}
            style={{ background: c }}
            title={c}
            onClick={() => applyAccent(c)}
          />
        ))}
      </div>

      <div className="row" style={{ marginTop: 10 }}>
        <input
          className="input grow"
          placeholder="自定义颜色 #2563EB"
          value={hex}
          onChange={(e) => setHex(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") applyHex();
          }}
        />
        <button className="btn sm" onClick={applyHex}>
          应用
        </button>
      </div>
      <div className="setting-sub">当前：{accent}</div>
      {error && <div className="err-text">{error}</div>}
    </Dialog>
  );
}
