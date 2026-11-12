/**
 * RFC 6238 TOTP（浏览器端，Web Crypto 实现）。
 * 与手机端 core 的 Totp.kt 同口径：支持 otpauth:// 链接与裸 Base32 密钥。
 */

export type TotpParams = {
  secret: Uint8Array;
  period: number;
  digits: number;
  algorithm: "SHA1" | "SHA256" | "SHA512";
  issuer: string;
  label: string;
};

/** RFC 4648 Base32 解码，容忍空格/连字符/小写/缺 padding。 */
export function base32Decode(input: string): Uint8Array | null {
  let cleaned = "";
  for (const c of input) {
    if (/\s/.test(c) || c === "-" || c === "=") continue;
    cleaned += c.toUpperCase();
  }
  if (!cleaned) return null;
  const out: number[] = [];
  let buffer = 0;
  let bits = 0;
  for (const c of cleaned) {
    let value: number;
    if (c >= "A" && c <= "Z") value = c.charCodeAt(0) - 65;
    else if (c >= "2" && c <= "7") value = c.charCodeAt(0) - 50 + 26;
    else return null;
    buffer = (buffer << 5) | value;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out.push((buffer >> bits) & 0xff);
    }
  }
  if (bits > 0 && (buffer & ((1 << bits) - 1)) !== 0) return null;
  return Uint8Array.from(out);
}

function normalizeAlgorithm(raw: string): TotpParams["algorithm"] | null {
  const name = raw.toUpperCase().replace(/[-_]/g, "");
  if (name === "SHA1" || name === "SHA") return "SHA1";
  if (name === "SHA256") return "SHA256";
  if (name === "SHA512") return "SHA512";
  return null;
}

/** 解析 otpauth:// URI 或裸 Base32 密钥；无法识别返回 null。 */
export function parseTotp(input: string): TotpParams | null {
  const text = input.trim();
  if (!text) return null;
  if (/^otpauth:\/\//i.test(text)) {
    let url: URL;
    try {
      url = new URL(text);
    } catch {
      return null;
    }
    if (url.protocol !== "otpauth:") return null;
    if (url.hostname.toLowerCase() !== "totp") return null;
    const label = decodeURIComponent(url.pathname.replace(/^\//, ""));
    const secret = base32Decode(url.searchParams.get("secret") ?? "");
    if (!secret || secret.length === 0) return null;
    const digits = Number.parseInt(url.searchParams.get("digits") ?? "6", 10) || 6;
    const period = Number.parseInt(url.searchParams.get("period") ?? "30", 10) || 30;
    const algorithm = normalizeAlgorithm(url.searchParams.get("algorithm") ?? "SHA1");
    if (!algorithm || period <= 0) return null;
    const issuer = url.searchParams.get("issuer") || label.split(":")[0].trim();
    return { secret, period, digits, algorithm, issuer, label };
  }
  const secret = base32Decode(text);
  if (!secret || secret.length === 0) return null;
  return { secret, period: 30, digits: 6, algorithm: "SHA1", issuer: "", label: "" };
}

function counterBytes(counter: number): BufferSource {
  const bytes = new Uint8Array(8);
  let value = counter;
  for (let i = 7; i >= 0; i--) {
    bytes[i] = value & 0xff;
    value = Math.floor(value / 256);
  }
  return bytes;
}

/** 生成指定周期序号的 TOTP 码；失败返回 null。 */
export async function generateTotp(params: TotpParams, counter: number): Promise<string | null> {
  try {
    const subtle = globalThis.crypto?.subtle;
    if (!subtle) return null;
    const hash = params.algorithm === "SHA1" ? "SHA-1" : params.algorithm === "SHA256" ? "SHA-256" : "SHA-512";
    // 复制成 ArrayBuffer 支撑的视图，兼容不同 TS 版本的 BufferSource 约束。
    const keyBytes: BufferSource = new Uint8Array(params.secret);
    const key = await subtle.importKey("raw", keyBytes, { name: "HMAC", hash }, false, ["sign"]);
    const mac = new Uint8Array(await subtle.sign("HMAC", key, counterBytes(counter)));
    const offset = mac[mac.length - 1] & 0x0f;
    const binary =
      ((mac[offset] & 0x7f) << 24) | (mac[offset + 1] << 16) | (mac[offset + 2] << 8) | mac[offset + 3];
    const code = binary % 10 ** params.digits;
    return String(code).padStart(params.digits, "0");
  } catch {
    return null;
  }
}

/** 距当前周期结束还剩几秒（1..period）。 */
export function totpRemainingSeconds(nowMs: number, period: number): number {
  if (period <= 0) return 0;
  return period - (Math.floor(nowMs / 1000) % period);
}
