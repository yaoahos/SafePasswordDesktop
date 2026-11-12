/**
 * 条目图标归一化：与手机端 IconCodec.kt 完全同规则。
 *
 * 规则（对齐 core/.../icon/IconPlan.kt）：
 * - 宽高比在 3:4 ~ 4:3 之间：居中裁成正方形
 * - 超出这个范围：等比缩进正方形，四周留透明边（长条 logo 一裁就废）
 * - 目标边长 192；原图更小则不放大
 * - 编码「PNG / WebP 取小」：图形类 PNG 更小，照片类 WebP 小 6 倍
 * - 择优后仍超 28KB 再降到 128 重编
 *
 * 两端必须一致，否则同一张图在手机和桌面之间来回编辑会反复改字节。
 */

const TARGET = 192;
/** PNG 已经小于这个体积就不用试 WebP（logo 类通常 1KB 上下）。 */
const PNG_KEEP_BYTES = 16 * 1024;
/** 择优后仍超过这个体积，尺寸再降一档。 */
const FALLBACK_BYTES = 28 * 1024;
const FALLBACK_TARGET = 128;
const WEBP_QUALITY = 0.85;

export type IconPlan = {
  cropX: number;
  cropY: number;
  cropW: number;
  cropH: number;
  outW: number;
  outH: number;
  letterbox: boolean;
  size: number;
};

export function planIcon(width: number, height: number, target = TARGET): IconPlan {
  const ratio = width / height;
  if (ratio >= 3 / 4 && ratio <= 4 / 3) {
    const side = Math.min(width, height);
    const out = Math.min(side, target);
    return {
      cropX: Math.floor((width - side) / 2),
      cropY: Math.floor((height - side) / 2),
      cropW: side,
      cropH: side,
      outW: out,
      outH: out,
      letterbox: false,
      size: Math.max(out, 1),
    };
  }
  const longSide = Math.max(width, height);
  const scale = longSide > target ? target / longSide : 1;
  const outW = Math.max(1, Math.round(width * scale));
  const outH = Math.max(1, Math.round(height * scale));
  return {
    cropX: 0,
    cropY: 0,
    cropW: width,
    cropH: height,
    outW,
    outH,
    letterbox: true,
    size: Math.max(outW, outH),
  };
}

function draw(img: HTMLImageElement, plan: IconPlan): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = plan.size;
  canvas.height = plan.size;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("无法压缩图片");
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  if (plan.letterbox) {
    const dx = (plan.size - plan.outW) / 2;
    const dy = (plan.size - plan.outH) / 2;
    ctx.drawImage(img, plan.cropX, plan.cropY, plan.cropW, plan.cropH, dx, dy, plan.outW, plan.outH);
  } else {
    ctx.drawImage(img, plan.cropX, plan.cropY, plan.cropW, plan.cropH, 0, 0, plan.outW, plan.outH);
  }
  return canvas;
}

function toBlob(canvas: HTMLCanvasElement, type: string, quality?: number): Promise<Blob> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(
      (blob) => {
        if (blob) resolve(blob);
        else reject(new Error("无法压缩图片"));
      },
      type,
      quality
    );
  });
}

/**
 * PNG 与 WebP 各编一次取小者。
 *
 * 图形类图标 PNG 900B~1.1KB，WebP 反而要 1.8KB；照片类 PNG 88KB、WebP 只要 14KB。
 * 所以不能一刀切成 WebP，必须按实际字节数比。
 * 浏览器不支持 WebP 时 `toBlob` 会静默回退成 PNG，此时两者相同，取哪个都对。
 */
async function encodeSmallest(canvas: HTMLCanvasElement): Promise<Blob> {
  const png = await toBlob(canvas, "image/png");
  if (png.size <= PNG_KEEP_BYTES) return png;
  const webp = await toBlob(canvas, "image/webp", WEBP_QUALITY);
  return webp.size < png.size ? webp : png;
}

function blobToBase64(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result || "");
      resolve(url.slice(url.indexOf(",") + 1));
    };
    reader.onerror = () => reject(new Error("无法读取图片"));
    reader.readAsDataURL(blob);
  });
}

function loadImage(url: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("无法读取图片"));
    img.src = url;
  });
}

/** 读取用户选择的图片文件，返回归一化后的裸 base64（PNG 或 WebP）。失败时抛出带文案的 Error。 */
export async function normalizeIconFile(file: File): Promise<string> {
  const url = URL.createObjectURL(file);
  try {
    const img = await loadImage(url);
    const w = img.naturalWidth;
    const h = img.naturalHeight;
    if (!w || !h) throw new Error("图片尺寸无效");
    let blob = await encodeSmallest(draw(img, planIcon(w, h, TARGET)));
    if (blob.size > FALLBACK_BYTES) {
      blob = await encodeSmallest(draw(img, planIcon(w, h, FALLBACK_TARGET)));
    }
    return await blobToBase64(blob);
  } finally {
    URL.revokeObjectURL(url);
  }
}
