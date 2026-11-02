import { useEffect, useRef, useState } from "react";
import type { AttachmentRef, FieldType, Label, Vault, VaultItem } from "../types";
import { attachmentAdd, attachmentRead, attachmentRemove } from "../api";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Dialog } from "./Dialog";
import {
  MASK,
  arrayBufferToBase64,
  copyText,
  formatSize,
  formatTime,
  isMaskedField,
  itemIcon,
  itemSubtitle,
  labelColorCss,
  newId,
} from "../utils";

const FIELD_TYPES: FieldType[] = ["TEXT", "PASSWORD", "SECRET", "URL", "EMAIL", "PHONE", "MULTILINE"];

/** 对齐手机端 ItemEditScreen.kt:901-908 typeLabel */
function typeLabel(t: FieldType): string {
  switch (t) {
    case "TEXT":
      return "文本";
    case "PASSWORD":
      return "密码";
    case "SECRET":
      return "秘密";
    case "URL":
      return "网址";
    case "EMAIL":
      return "邮箱";
    case "PHONE":
      return "电话";
    case "MULTILINE":
      return "多行文本";
  }
}

export function ItemDetail({
  item,
  labels,
  onSave,
  onVault,
  onToggleFavorite,
  onDuplicate,
  onDelete,
  onRestore,
  onPurge,
  notify,
}: {
  item: VaultItem;
  labels: Label[];
  /** 保存编辑（putItem） */
  onSave: (next: VaultItem) => Promise<void>;
  /** 附件操作后同步父级 vault（后端已落库并返回完整 Vault） */
  onVault: (v: Vault) => void;
  onToggleFavorite: () => Promise<void>;
  onDuplicate: () => Promise<void>;
  /** 移回收站（status=TRASHED） */
  onDelete: () => Promise<void>;
  onRestore: () => Promise<void>;
  /** 彻底删除（后端 purge） */
  onPurge: () => Promise<void>;
  notify: (msg: string, kind?: "info" | "success" | "error") => void;
}) {
  const [draft, setDraft] = useState<VaultItem>(item);
  const [shown, setShown] = useState<Record<string, boolean>>({});
  const [dirty, setDirty] = useState(false);
  /** 字段添加/编辑弹窗（对齐手机端 FieldDialog） */
  const [fieldDlg, setFieldDlg] = useState<null | { mode: "add" } | { mode: "edit"; id: string }>(null);
  const [fLabel, setFLabel] = useState("");
  const [fType, setFType] = useState<FieldType>("TEXT");

  useEffect(() => {
    setDraft(item);
    setShown({});
    setDirty(false);
  }, [item]);

  const patch = (p: Partial<VaultItem>) => {
    setDraft((d) => ({ ...d, ...p }));
    setDirty(true);
  };

  const patchField = (fieldId: string, p: Partial<VaultItem["fields"][number]>) => {
    setDraft((d) => ({
      ...d,
      fields: d.fields.map((f) => (f.id === fieldId ? { ...f, ...p } : f)),
    }));
    setDirty(true);
  };

  const save = async () => {
    try {
      await onSave({ ...draft, updatedAt: Date.now() });
      setDirty(false);
      notify("已保存", "success");
    } catch (e) {
      notify(String(e), "error");
    }
  };

  const fileRef = useRef<HTMLInputElement>(null);

  const b64ToBytes = (b64: string) => {
    const bin = atob(b64);
    const out = new Uint8Array(new ArrayBuffer(bin.length));
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  };

  /** 附件操作后端已落库：同步父级与草稿（后端返回的 Vault 是新状态） */
  const syncFromVault = (v: Vault) => {
    onVault(v);
    const it = v.items.find((i) => i.id === item.id);
    if (it) setDraft(it);
    setDirty(false);
  };

  const addAttach = async (f: File) => {
    try {
      // 先把未保存的编辑落库，避免附件操作把草稿改动顶掉
      if (dirty) await save();
      const buf = await f.arrayBuffer();
      const data = arrayBufferToBase64(buf);
      const v = await attachmentAdd(item.id, f.name, f.type || "application/octet-stream", data);
      syncFromVault(v);
      notify("已添加附件", "success");
    } catch (e) {
      notify(String(e), "error");
    }
  };

  const downloadAttach = async (a: AttachmentRef) => {
    try {
      const bytes = b64ToBytes(await attachmentRead(a.id));
      const blob = new Blob([bytes], { type: a.mime || "application/octet-stream" });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = a.name;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 10_000);
    } catch (e) {
      notify(String(e), "error");
    }
  };

  const removeAttach = async (id: string) => {
    try {
      if (dirty) await save();
      const v = await attachmentRemove(item.id, id);
      syncFromVault(v);
      notify("已移除附件", "success");
    } catch (e) {
      notify(String(e), "error");
    }
  };

  const icon = itemIcon(draft);
  const sub = itemSubtitle(draft);
  const trashed = draft.status === "TRASHED";
  /** 对齐手机端 ItemViewScreen.kt:105 的四级 url fallback */
  const resolvedUrl = (() => {
    if (draft.url.trim()) return draft.url.trim();
    const urlField = draft.fields.find((f) => f.type === "URL");
    if (urlField && urlField.value.trim()) return urlField.value.trim();
    const byLabel = draft.fields.find((f) =>
      ["网址", "地址", "URL", "Website", "网站"].includes(f.label),
    );
    if (byLabel && byLabel.value.trim()) return byLabel.value.trim();
    const byScheme = draft.fields.find((f) => /^https?:\/\//i.test(f.value.trim()));
    return byScheme ? byScheme.value.trim() : "";
  })();
  /** 对齐手机端 withScheme（ItemViewScreen.kt:627）：trim、无 "://" 补 http://；协议不在 opener:default 白名单则由插件拒绝 */
  const openExternal = () => {
    if (!resolvedUrl) {
      notify("没有可打开的网址", "error");
      return;
    }
    const target = resolvedUrl.includes("://") ? resolvedUrl : `http://${resolvedUrl}`;
    void openUrl(target).catch(() => notify("无法打开网址", "error"));
  };

  return (
    <div className="detail">
      <div className="detail-header">
        <span className={`detail-icon${icon.kind === "emoji" ? " emoji" : ""}`}>
          {icon.kind === "img" ? <img src={icon.src} alt="" /> : icon.kind === "char" ? icon.char : icon.emoji}
        </span>
        <div className="grow">
          <input
            className="input"
            style={{ fontWeight: 700, fontSize: 20, height: 34, border: "none", background: "transparent", padding: 0 }}
            value={draft.title}
            placeholder="未命名"
            disabled={trashed}
            onChange={(e) => patch({ title: e.target.value })}
          />
          <div className="detail-sub">
            {sub ? `${sub} · ` : ""}
            更新于 {formatTime(draft.updatedAt)}
            {draft.status === "ARCHIVED" ? " · 已归档" : trashed ? " · 回收站" : ""}
          </div>
        </div>
        <button className="icon-btn" title={draft.favorite ? "取消收藏" : "收藏"} onClick={() => void onToggleFavorite()}>
          <span className="star" style={{ fontSize: 17 }}>
            {draft.favorite ? "★" : "☆"}
          </span>
        </button>
      </div>

      <div className="card">
        <div className="row" style={{ alignItems: "center", gap: 8 }}>
          <div className="grow">字段</div>
          {!trashed && (
            <button
              className="btn sm"
              onClick={() => {
                setFLabel("");
                setFType("TEXT");
                setFieldDlg({ mode: "add" });
              }}
            >
              ＋添加字段
            </button>
          )}
        </div>
        {draft.fields.length === 0 && <div className="muted small">无字段，可在下方备注中记录</div>}
        {draft.fields.map((f) => {
          const masked = isMaskedField(f.type, f.sensitive) && !shown[f.id];
          return (
            <div className="field-row" key={f.id}>
              <div className="field-label" title={f.label}>
                {f.label}
              </div>
              {trashed ? (
                <div className="field-value">{masked ? MASK : f.value || "—"}</div>
              ) : f.type === "PASSWORD" || f.type === "SECRET" ? (
                <input
                  className="input field-value"
                  type={masked ? "password" : "text"}
                  value={f.value}
                  onChange={(e) => patchField(f.id, { value: e.target.value })}
                />
              ) : (
                <input
                  className="input field-value"
                  value={f.value}
                  placeholder="—"
                  onChange={(e) => patchField(f.id, { value: e.target.value })}
                />
              )}
              {isMaskedField(f.type, f.sensitive) && (
                <button
                  className="icon-btn"
                  title={shown[f.id] ? "隐藏" : "显示"}
                  onClick={() => setShown((s) => ({ ...s, [f.id]: !s[f.id] }))}
                >
                  {shown[f.id] ? "🙈" : "👁"}
                </button>
              )}
              <button
                className="icon-btn"
                title="复制"
                onClick={() => {
                  void copyText(f.value).then(
                    () => notify(`已复制“${f.label}”`, "success"),
                    () => notify("复制失败", "error"),
                  );
                }}
              >
                ⧉
              </button>
              {!trashed && (
                <>
                  <button
                    className="icon-btn"
                    title="编辑字段"
                    onClick={() => {
                      setFLabel(f.label);
                      setFType(f.type);
                      setFieldDlg({ mode: "edit", id: f.id });
                    }}
                  >
                    ✎
                  </button>
                  <button
                    className="icon-btn"
                    title="删除字段"
                    onClick={() => patch({ fields: draft.fields.filter((x) => x.id !== f.id) })}
                  >
                    🗑
                  </button>
                </>
              )}
            </div>
          );
        })}
        {resolvedUrl && (
          <div className="field-row">
            <div className="field-label">网址</div>
            <div className="field-value" title={resolvedUrl}>
              {resolvedUrl}
            </div>
            {!trashed && (
              <button className="icon-btn" title="打开链接" onClick={openExternal}>
                ↗
              </button>
            )}
            <button
              className="icon-btn"
              title="复制"
              onClick={() => {
                void copyText(resolvedUrl).then(
                  () => notify("已复制“网址”", "success"),
                  () => notify("复制失败", "error"),
                );
              }}
            >
              ⧉
            </button>
          </div>
        )}
      </div>

      <div className="card">
        <div className="card-title">{draft.notesLabel || "备注"}</div>
        {trashed ? (
          <div className="notes-text">{draft.notes || "—"}</div>
        ) : (
          <textarea
            className="input"
            rows={4}
            value={draft.notes}
            placeholder="备注…"
            onChange={(e) => patch({ notes: e.target.value })}
          />
        )}
      </div>

      <div className="card">
        <div className="card-title">标签</div>
        <div className="row" style={{ flexWrap: "wrap", gap: 8 }}>
          {labels.length === 0 && <span className="muted small">暂无标签，可在“标签管理”中创建</span>}
          {labels.map((l) => {
            const on = draft.labelIds.includes(l.id);
            const color = labelColorCss(l.color);
            return (
              <button
                key={l.id}
                disabled={trashed}
                className="tag-chip"
                style={{
                  cursor: trashed ? "default" : "pointer",
                  border: `1px solid ${on ? color : "var(--border)"}`,
                  background: on ? `${color}22` : "var(--surface)",
                  color: on ? color : "var(--text-2)",
                }}
                onClick={() => {
                  if (trashed) return;
                  patch({
                    labelIds: on ? draft.labelIds.filter((id) => id !== l.id) : [...draft.labelIds, l.id],
                  });
                }}
              >
                <span className="dot" style={{ background: color }} />
                {l.name}
              </button>
            );
          })}
        </div>
      </div>

      <div className="card">
        <div className="card-title">附件</div>
        {draft.attachments.length === 0 && (
          <div className="muted small" style={{ padding: "4px 0" }}>
            暂无附件
          </div>
        )}
        {draft.attachments.map((a) => (
          <div className="attach-row" key={a.id}>
            <span>📎</span>
            <span className="grow">{a.name}</span>
            <span className="muted small">{formatSize(a.size)}</span>
            <button className="btn sm" onClick={() => void downloadAttach(a)}>
              下载
            </button>
            {!trashed && (
              <button className="btn sm" onClick={() => void removeAttach(a.id)}>
                移除
              </button>
            )}
          </div>
        ))}
        {!trashed && (
          <div className="attach-row">
            <button className="btn sm" onClick={() => fileRef.current?.click()}>
              添加附件…
            </button>
            <input
              ref={fileRef}
              type="file"
              style={{ display: "none" }}
              onChange={(e) => {
                const f = e.target.files?.[0];
                if (f) void addAttach(f);
                e.target.value = "";
              }}
            />
            <span className="muted small">单个 ≤ 10 MB，加密存储</span>
          </div>
        )}
      </div>

      <div className="detail-actions">
        {!trashed && (
          <>
            <button className="btn primary" disabled={!dirty} onClick={() => void save()}>
              保存
            </button>
            <button className="btn" onClick={() => void onDuplicate()}>
              复制副本
            </button>
            <button className="btn" onClick={() => onDelete().catch((e) => notify(String(e), "error"))}>
              删除
            </button>
          </>
        )}
        {trashed && (
          <>
            <button className="btn primary" onClick={() => onRestore().catch((e) => notify(String(e), "error"))}>
              恢复
            </button>
            <button className="btn danger" onClick={() => onPurge().catch((e) => notify(String(e), "error"))}>
              彻底删除
            </button>
          </>
        )}
      </div>
      {fieldDlg && (
        <Dialog
          title={fieldDlg.mode === "add" ? "添加字段" : "编辑字段"}
          onClose={() => setFieldDlg(null)}
          showCloseButton={false}
          width={360}
        >
          <div className="muted small">字段名</div>
          <input
            className="input"
            autoFocus
            value={fLabel}
            placeholder="自定义字段"
            onChange={(e) => setFLabel(e.target.value)}
          />
          <div className="muted small" style={{ marginTop: 10 }}>
            类型
          </div>
          <select className="input" value={fType} onChange={(e) => setFType(e.target.value as FieldType)}>
            {FIELD_TYPES.map((t) => (
              <option key={t} value={t}>
                {typeLabel(t)}
              </option>
            ))}
          </select>
          <div className="row" style={{ justifyContent: "flex-end", gap: 8, marginTop: 14 }}>
            <button className="btn" onClick={() => setFieldDlg(null)}>
              取消
            </button>
            <button
              className="btn primary"
              onClick={() => {
                const label = fLabel.trim() || "自定义字段";
                const sensitive = fType === "PASSWORD" || fType === "SECRET";
                if (fieldDlg.mode === "add") {
                  patch({
                    fields: [...draft.fields, { id: newId(), label, value: "", type: fType, sensitive }],
                  });
                } else {
                  patchField(fieldDlg.id, { label, type: fType, sensitive });
                }
                setFieldDlg(null);
              }}
            >
              {fieldDlg.mode === "add" ? "添加" : "保存"}
            </button>
          </div>
        </Dialog>
      )}
      <div className="spacer-16" />
    </div>
  );
}
