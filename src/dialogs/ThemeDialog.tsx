import { Dialog } from "../components/Dialog";
import type { ThemeMode } from "../types";
import type { AppCtx } from "../app-context";

const THEMES: { key: ThemeMode; label: string; icon: string }[] = [
  { key: "system", label: "跟随系统", icon: "🖥" },
  { key: "light", label: "浅色", icon: "☀️" },
  { key: "dark", label: "深色", icon: "🌙" },
];

export function ThemeDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  return (
    <Dialog title="主题" onClose={onClose}>
      <div className="choice-grid">
        {THEMES.map((t) => (
          <button
            key={t.key}
            className={`choice-card${ctx.settings.theme === t.key ? " active" : ""}`}
            onClick={() => void ctx.applySettings({ ...ctx.settings, theme: t.key })}
          >
            <span className="big">{t.icon}</span>
            <span>{t.label}</span>
          </button>
        ))}
      </div>
      <div className="spacer-10" />
      <div className="muted small">主题会立即生效并保存到本机设置。</div>
    </Dialog>
  );
}
