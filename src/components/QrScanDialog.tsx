/**
 * 动态验证码「识别二维码」对话框（对齐手机端扫码录入，桌面端按平台换成两种取图方式）：
 *   ① 选择本地图片文件（二维码截图）
 *   ② 截图识别：先最小化本窗口（不算切后台、不触发自动锁定），截取整个屏幕后
 *      全屏显示屏幕内容，由用户拖框选出二维码区域
 * 解码结果必须能解析为 TOTP 密钥（otpauth:// 或裸 Base32），否则提示换一张图。
 */
import { useEffect, useRef, useState } from "react";
import jsQR from "jsqr";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Dialog } from "./Dialog";
import { captureScreenPng, lockSuspend, pickAnyFile } from "../api";
import { parseTotp } from "../totp";

type Rect = { x: number; y: number; w: number; h: number };

const sleep = (ms: number) => new Promise((r) => window.setTimeout(r, ms));

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("无法读取图片"));
    img.src = src;
  });
}

/** 从图片的指定区域（显示坐标）解码二维码；区域为 null 时解整图 */
async function decodeQr(dataUrl: string, rect: Rect | null, displayWidth: number): Promise<string | null> {
  const img = await loadImage(dataUrl);
  const scale = rect && displayWidth > 0 ? img.naturalWidth / displayWidth : 1;
  const canvas = document.createElement("canvas");
  canvas.width = rect ? Math.max(1, Math.round(rect.w * scale)) : img.naturalWidth;
  canvas.height = rect ? Math.max(1, Math.round(rect.h * scale)) : img.naturalHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;
  if (rect) {
    ctx.drawImage(
      img,
      Math.round(rect.x * scale),
      Math.round(rect.y * scale),
      canvas.width,
      canvas.height,
      0,
      0,
      canvas.width,
      canvas.height,
    );
  } else {
    ctx.drawImage(img, 0, 0);
  }
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height);
  const found = jsQR(data.data, data.width, data.height, { inversionAttempts: "attemptBoth" });
  return found?.data ?? null;
}

/**
 * 自己发起的窗口动作（截屏取景前最小化、全屏切换）不算切后台：
 * 动作期间豁免自动锁定，结束后 0.8s 解除（吸收切换瞬间的失焦事件）。
 */
async function withLockSuspend(fn: () => Promise<void>): Promise<void> {
  lockSuspend.until = Date.now() + 120_000;
  try {
    await fn();
  } finally {
    window.setTimeout(() => {
      lockSuspend.until = 0;
    }, 800);
  }
}

export function QrScanDialog({
  onDecoded,
  onClose,
}: {
  /** 解码出的动态验证码密钥原文（otpauth:// 或 Base32） */
  onDecoded: (text: string) => void;
  onClose: () => void;
}) {
  /** 截图/图片的 data URL；null = 还没取到图 */
  const [imgUrl, setImgUrl] = useState<string | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [rect, setRect] = useState<Rect | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);
  const dragRef = useRef<{ x0: number; y0: number } | null>(null);
  /** 是否已把窗口切到全屏选区（关闭时要还原） */
  const fullscreenRef = useRef(false);

  // 卸载兜底：还原全屏、解除锁定豁免
  useEffect(
    () => () => {
      lockSuspend.until = 0;
      if (fullscreenRef.current) {
        void getCurrentWindow()
          .setFullscreen(false)
          .catch(() => undefined);
      }
    },
    [],
  );

  const pickLocal = async () => {
    setErr(null);
    try {
      const f = await pickAnyFile(true);
      if (!f) return;
      const mime = f.name.toLowerCase().endsWith(".png") ? "image/png" : "image/jpeg";
      setImgUrl(`data:${mime};base64,${f.base64}`);
      setRect(null);
      await withLockSuspend(async () => {
        try {
          await getCurrentWindow().setFullscreen(true);
          fullscreenRef.current = true;
        } catch {
          /* 全屏失败时仍可窗口内大图选取 */
        }
      });
    } catch (e) {
      setErr(String(e));
    }
  };

  /** 截图识别：最小化本窗口 → 截取整个屏幕 → 全屏显示供框选 */
  const shotScreen = async () => {
    setErr(null);
    setBusy(true);
    try {
      await withLockSuspend(async () => {
        const win = getCurrentWindow();
        await win.minimize();
        await sleep(700); // 等最小化动画结束，避免把自己截进画面
        const b64 = await captureScreenPng();
        await win.unminimize();
        await win.show();
        await win.setFocus().catch(() => undefined);
        try {
          await win.setFullscreen(true);
          fullscreenRef.current = true;
        } catch {
          /* 全屏失败时仍可窗口内大图选取 */
        }
        setImgUrl(`data:image/png;base64,${b64}`);
        setRect(null);
      });
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  const decode = async (region: Rect | null) => {
    if (!imgUrl) return;
    setBusy(true);
    setErr(null);
    try {
      const text = await decodeQr(imgUrl, region, imgRef.current?.clientWidth ?? 0);
      if (!text) {
        setErr("所选区域没有识别出二维码，可重新框选或换一张图");
        return;
      }
      if (!parseTotp(text)) {
        setErr("这不是动态验证码密钥，换一个二维码试试");
        return;
      }
      if (fullscreenRef.current) {
        fullscreenRef.current = false;
        await getCurrentWindow()
          .setFullscreen(false)
          .catch(() => undefined);
      }
      onDecoded(text);
    } catch (e) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  };

  // 拖框选区（以 wrap 的显示坐标计）
  const onPointerDown = (e: React.PointerEvent) => {
    if (!imgUrl) return;
    const el = wrapRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    dragRef.current = { x0: e.clientX - r.left, y0: e.clientY - r.top };
    setRect(null);
    el.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const start = dragRef.current;
    const el = wrapRef.current;
    if (!start || !el) return;
    const r = el.getBoundingClientRect();
    const x = e.clientX - r.left;
    const y = e.clientY - r.top;
    setRect({
      x: Math.min(start.x0, x),
      y: Math.min(start.y0, y),
      w: Math.abs(x - start.x0),
      h: Math.abs(y - start.y0),
    });
  };
  const onPointerUp = () => {
    dragRef.current = null;
  };

  const cancel = () => {
    if (fullscreenRef.current) {
      fullscreenRef.current = false;
      void getCurrentWindow()
        .setFullscreen(false)
        .catch(() => undefined);
    }
    onClose();
  };

  // 有图 = 全屏选区模式；没图 = 菜单对话框
  if (imgUrl) {
    return (
      <div className="qr-pick-overlay">
        <div className="qr-pick-tip">拖框选出二维码区域，不框选则识别整图</div>
        <div
          ref={wrapRef}
          className="qr-shot-wrap qr-pick-wrap"
          style={{ cursor: "crosshair" }}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
        >
          <img ref={imgRef} src={imgUrl} alt="待识别图片" draggable={false} />
          {rect && rect.w > 2 && rect.h > 2 && (
            <div
              className="qr-shot-rect"
              style={{ left: rect.x, top: rect.y, width: rect.w, height: rect.h }}
            />
          )}
        </div>
        {err && <div className="err-text">{err}</div>}
        <div className="qr-pick-bar">
          <button
            className="btn primary"
            disabled={busy}
            onClick={() => void decode(rect && rect.w > 4 && rect.h > 4 ? rect : null)}
          >
            {busy ? "识别中…" : rect && rect.w > 4 && rect.h > 4 ? "识别所选区域" : "识别整图"}
          </button>
          <button
            className="btn"
            onClick={() => {
              setRect(null);
              setErr(null);
            }}
          >
            清除选区
          </button>
          <button className="btn" disabled={busy} onClick={() => void shotScreen()}>
            {busy ? "处理中…" : "重新截图"}
          </button>
          <button className="btn" disabled={busy} onClick={() => void pickLocal()}>
            换一张图
          </button>
          <button className="btn" onClick={cancel}>
            取消
          </button>
        </div>
      </div>
    );
  }

  return (
    <Dialog title="识别二维码" onClose={onClose} width={560}>
      <div className="muted small" style={{ lineHeight: 1.7 }}>
        「截图识别」会先最小化本窗口，再截取整个屏幕，随后全屏显示屏幕内容，由你拖框选出二维码区域；该过程不会触发自动锁定。也可以直接选择本地的二维码图片。
      </div>
      <div className="row" style={{ gap: 8, marginTop: 12 }}>
        <button className="btn" disabled={busy} onClick={() => void pickLocal()}>
          选择图片文件…
        </button>
        <button className="btn primary" disabled={busy} onClick={() => void shotScreen()}>
          {busy ? "处理中…" : "截图识别"}
        </button>
      </div>
      {err && (
        <div className="err-text" style={{ marginTop: 10 }}>
          {err}
        </div>
      )}
    </Dialog>
  );
}
