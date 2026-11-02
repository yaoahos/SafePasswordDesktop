import { useRef, useState } from "react";
import { Dialog } from "../components/Dialog";
import { importCsv, importSafeincloudXml, importVaultBytes } from "../api";
import { arrayBufferToBase64 } from "../utils";
import type { AppCtx } from "../app-context";

type Tab = "csv" | "xml" | "sicv";

const TABS: { key: Tab; label: string }[] = [
  { key: "csv", label: "CSV 文本" },
  { key: "xml", label: "SafeInCloud XML" },
  { key: "sicv", label: ".sicv 备份" },
];

const CSV_HINT = `title,username,password,url,notes
示例网站,user@example.com,123456,https://example.com,备注`;

export function ImportDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("csv");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sicvPw, setSicvPw] = useState("");
  const [sicvName, setSicvName] = useState("");
  const fileRef = useRef<HTMLInputElement>(null);

  const runText = async () => {
    setBusy(true);
    setError(null);
    try {
      const n = tab === "csv" ? await importCsv(text) : await importSafeincloudXml(text);
      ctx.notify(`成功导入 ${n} 条`, "success");
      setText("");
      await ctx.refreshFromBackend();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const runFile = async (file: File) => {
    setBusy(true);
    setError(null);
    try {
      const buf = await file.arrayBuffer();
      const name = sicvName.trim() || file.name.replace(/\.sicv$/i, "") || "导入的密码库";
      const meta = await importVaultBytes(arrayBufferToBase64(buf), sicvPw, name);
      ctx.notify(`已导入密码库「${meta.name}」并切换过去`, "success");
      await ctx.refreshFromBackend();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog title="导入密码" onClose={onClose}>
      <div className="tabs" style={{ marginBottom: 12 }}>
        {TABS.map((t) => (
          <button key={t.key} className={`tab${tab === t.key ? " active" : ""}`} onClick={() => setTab(t.key)}>
            {t.label}
          </button>
        ))}
      </div>

      {tab === "sicv" ? (
        <>
          <div className="muted small">选择 SafePassword 手机端导出的 .sicv 备份文件，导入后会切换到该密码库。</div>
          <div className="spacer-10" />
          <div className="form-row">
            <label className="field-caption">备份的主密码（未加密则留空）</label>
            <input
              className="input"
              type="password"
              value={sicvPw}
              onChange={(e) => setSicvPw(e.target.value)}
            />
          </div>
          <div className="form-row">
            <label className="field-caption">导入后的密码库名称（默认取文件名）</label>
            <input className="input" value={sicvName} placeholder="可留空" onChange={(e) => setSicvName(e.target.value)} />
          </div>
          <input
            ref={fileRef}
            type="file"
            accept=".sicv"
            style={{ display: "none" }}
            onChange={(e) => {
              const f = e.target.files?.[0];
              if (f) void runFile(f);
              e.target.value = "";
            }}
          />
          <button className="btn primary" disabled={busy} onClick={() => fileRef.current?.click()}>
            {busy ? "导入中…" : "选择 .sicv 文件…"}
          </button>
        </>
      ) : (
        <>
          <div className="muted small">
            {tab === "csv" ? "粘贴 CSV 文本，首行为表头：" : "粘贴 SafeInCloud 导出的 XML 文本。"}
          </div>
          {tab === "csv" && (
            <pre
              className="small muted"
              style={{ background: "var(--surface-2)", padding: "6px 8px", borderRadius: 8, overflowX: "auto" }}
            >
              {CSV_HINT}
            </pre>
          )}
          <div className="spacer-10" />
          <textarea
            className="input"
            rows={8}
            value={text}
            placeholder={tab === "csv" ? CSV_HINT : "<vault>…</vault>"}
            onChange={(e) => setText(e.target.value)}
          />
          <div className="spacer-10" />
          <button className="btn primary" disabled={busy || !text.trim()} onClick={() => void runText()}>
            {busy ? "导入中…" : "开始导入"}
          </button>
        </>
      )}
      {error && <div className="err-text">{error}</div>}
    </Dialog>
  );
}
