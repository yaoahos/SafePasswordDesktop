import { Dialog } from "../components/Dialog";
import { TEMPLATES } from "../utils";

export function TemplatePickerDialog({
  onPick,
  onClose,
}: {
  onPick: (templateId: string) => void;
  onClose: () => void;
}) {
  return (
    <Dialog title="新建条目" onClose={onClose}>
      <div className="muted small" style={{ marginBottom: 10 }}>
        选择条目类型
      </div>
      <div className="template-grid">
        {TEMPLATES.map((t) => (
          <button key={t.id} className="template-card" onClick={() => onPick(t.id)}>
            <span className="big">{t.emoji}</span>
            <span>
              <div className="tname">{t.name}</div>
              <div className="tdesc">
                {t.fields.length > 0 ? t.fields.map((f) => f.label).join(" · ") : "自由记录"}
              </div>
            </span>
          </button>
        ))}
      </div>
    </Dialog>
  );
}
