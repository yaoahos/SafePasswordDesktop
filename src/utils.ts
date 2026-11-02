import type { Field, FieldType, VaultItem } from "./types";

// ---------------------------------------------------------------- 基础工具

export function newId(): string {
  const c: Crypto | undefined = typeof crypto !== "undefined" ? crypto : undefined;
  if (c && typeof c.randomUUID === "function") return c.randomUUID();
  return "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx".replace(/[xy]/g, (ch) => {
    const r = (Math.random() * 16) | 0;
    const v = ch === "x" ? r : (r & 0x3) | 0x8;
    return v.toString(16);
  });
}

/** ARGB int32 → #RRGGBB（取后 6 位） */
export function labelColorCss(color: number): string {
  const hex = (color >>> 0).toString(16).padStart(8, "FF");
  return `#${hex.slice(-6)}`;
}

export function formatTime(ms: number): string {
  if (!ms) return "—";
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

/** 对齐手机端 ClipboardHelper.copySensitive：复制敏感内容后 30 秒自动清空（仅当剪贴板仍是该内容时） */
let clearTimer: ReturnType<typeof setTimeout> | null = null;
let clearToken = 0;

function scheduleClear(text: string): void {
  const token = ++clearToken;
  if (clearTimer) clearTimeout(clearTimer);
  clearTimer = setTimeout(() => {
    if (token !== clearToken) return; // 期间又复制了新内容，交由新定时器处理
    clearTimer = null;
    void (async () => {
      try {
        const cur = await navigator.clipboard.readText();
        if (cur !== text) return; // 用户已在别处复制了别的内容，不动它
      } catch {
        // WebView2 可能拒绝读剪贴板：本应用未再复制过则尽力清空
      }
      try {
        await navigator.clipboard.writeText("");
      } catch {
        /* 清空失败忽略 */
      }
    })();
  }, 30_000);
}

export async function copyText(text: string): Promise<void> {
  if (!text) return;
  let ok = false;
  try {
    await navigator.clipboard.writeText(text);
    ok = true;
  } catch {
    // 回退到 execCommand
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    try {
      ok = document.execCommand("copy");
    } finally {
      ta.remove();
    }
  }
  if (ok) scheduleClear(text);
}

/** 浏览器侧下载：无插件，用 Blob + <a download> */
export function downloadText(filename: string, text: string, mime = "text/plain;charset=utf-8"): void {
  const blob = new Blob([text], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 3000);
}

export function arrayBufferToBase64(buf: ArrayBuffer): string {
  const bytes = new Uint8Array(buf);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** 下载二进制文件：b64 为标准 Base64（后端 export/attachment_read 返回） */
export function downloadBase64(filename: string, b64: string, mime = "application/octet-stream"): void {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const blob = new Blob([bytes], { type: mime });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

// ---------------------------------------------------------------- 模板

export interface TemplateFieldDef {
  label: string;
  type: FieldType;
  sensitive?: boolean;
}

export interface TemplateDef {
  id: string;
  name: string;
  emoji: string;
  fields: TemplateFieldDef[];
}

export const TEMPLATES: TemplateDef[] = [
  {
    id: "login",
    name: "登录账号",
    emoji: "🔑",
    fields: [
      { label: "账号", type: "TEXT" },
      { label: "密码", type: "PASSWORD" },
      { label: "网址", type: "URL" },
    ],
  },
  {
    id: "bank_card",
    name: "银行卡",
    emoji: "💳",
    fields: [
      { label: "卡号", type: "TEXT", sensitive: true },
      { label: "持卡人", type: "TEXT" },
      { label: "有效期", type: "TEXT" },
      { label: "CVV", type: "PASSWORD" },
      { label: "银行", type: "TEXT" },
    ],
  },
  {
    id: "email",
    name: "邮箱",
    emoji: "📧",
    fields: [
      { label: "邮箱", type: "EMAIL" },
      { label: "密码", type: "PASSWORD" },
      { label: "服务器", type: "TEXT" },
    ],
  },
  {
    id: "wifi",
    name: "无线网络",
    emoji: "📶",
    fields: [
      { label: "网络名称", type: "TEXT" },
      { label: "密码", type: "PASSWORD" },
      { label: "加密方式", type: "TEXT" },
    ],
  },
  {
    id: "identity",
    name: "身份证件",
    emoji: "🪪",
    fields: [
      { label: "姓名", type: "TEXT" },
      { label: "证件号", type: "TEXT", sensitive: true },
      { label: "有效期", type: "TEXT" },
      { label: "签发机关", type: "TEXT" },
    ],
  },
  { id: "note", name: "笔记", emoji: "📝", fields: [] },
  { id: "custom", name: "自定义", emoji: "📎", fields: [] },
];

export function templateById(id: string): TemplateDef {
  return TEMPLATES.find((t) => t.id === id) ?? TEMPLATES[0];
}

export function templateEmoji(id: string): string {
  return templateById(id).emoji;
}

export function makeField(label: string, type: FieldType, sensitive = false): Field {
  return { id: newId(), label, value: "", type, sensitive };
}

export function newItemFromTemplate(templateId: string): VaultItem {
  const tpl = templateById(templateId);
  const now = Date.now();
  return {
    id: newId(),
    templateId: tpl.id,
    title: "",
    fields: tpl.fields.map((f) => makeField(f.label, f.type, f.sensitive ?? false)),
    notes: "",
    notesLabel: "备注",
    notesType: "MULTILINE",
    notesPosition: -1,
    url: "",
    labelIds: [],
    favorite: false,
    status: "ACTIVE",
    createdAt: now,
    updatedAt: now,
    deletedAt: null,
    attachments: [],
    iconKey: "",
    iconData: "",
  };
}

// ---------------------------------------------------------------- 条目展示

/** 副标题：账号类字段 → url → 备注首行（对齐 Rust subtitle()） */
export function itemSubtitle(item: VaultItem): string {
  const nonEmpty = item.fields.filter((f) => f.value.trim() !== "");
  const exact = (labels: string[]): string | undefined =>
    nonEmpty.find((f) => labels.some((l) => f.label.toLowerCase() === l.toLowerCase()))?.value;
  const sub =
    exact(["账号", "帐号", "账户", "用户名", "登录名", "登录账号", "邮箱", "电子邮件", "email", "e-mail"]) ??
    nonEmpty.find((f) =>
      /账号|帐号|账户|用户名|登录名|邮箱|邮件|email|登录|卡号|证件号/i.test(f.label),
    )?.value;
  if (sub) return sub;
  if (item.url.trim()) return item.url;
  return item.notes.split(/\r?\n/)[0] ?? "";
}

export type ItemIcon =
  | { kind: "img"; src: string }
  | { kind: "char"; char: string }
  | { kind: "emoji"; emoji: string };

export function itemIcon(item: VaultItem): ItemIcon {
  if (item.iconKey === "custom" && item.iconData) {
    const src = item.iconData.startsWith("data:")
      ? item.iconData
      : `data:image/png;base64,${item.iconData}`;
    return { kind: "img", src };
  }
  if (item.iconKey && item.iconKey !== "custom") {
    return { kind: "char", char: item.iconKey.charAt(0).toUpperCase() };
  }
  return { kind: "emoji", emoji: templateEmoji(item.templateId) };
}

/** 掩码：与 Demo 一致的固定 12 个圆点 */
export const MASK = "••••••••••••";

export function isMaskedField(type: FieldType, sensitive: boolean): boolean {
  return sensitive || type === "PASSWORD" || type === "SECRET";
}

/** 搜索：title / 字段值 / 备注，大小写不敏感 */
export function itemMatches(item: VaultItem, query: string, searchPasswords: boolean): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  if (item.title.toLowerCase().includes(q)) return true;
  if (item.notes.toLowerCase().includes(q)) return true;
  if (item.url.toLowerCase().includes(q)) return true;
  return item.fields.some((f) => {
    if (!searchPasswords && isMaskedField(f.type, f.sensitive)) return false;
    return (
      f.value.toLowerCase().includes(q) || f.label.toLowerCase().includes(q)
    );
  });
}

/** 复制副本（对齐 Rust copy_as_duplicate） */
export function duplicateItem(item: VaultItem): VaultItem {
  const now = Date.now();
  return {
    ...item,
    id: newId(),
    title: `${item.title.trim() === "" ? "未命名" : item.title} 副本`,
    favorite: false,
    status: "ACTIVE",
    deletedAt: null,
    createdAt: now,
    updatedAt: now,
    fields: item.fields.map((f) => ({ ...f, id: newId() })),
  };
}
