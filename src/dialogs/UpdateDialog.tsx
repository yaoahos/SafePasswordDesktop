import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Dialog } from "../components/Dialog";
import { updateDownloadAndInstall } from "../api";
import type { UpdateInfo } from "../types";
import { releaseNotes } from "../utils";

/**
 * 启动时发现新版本：列出本次更新内容，并提供「稍后 / 立即更新」两个选择。
 * 用户选择立即更新后原地切换成「正在下载」，失败也留在弹窗里提示，不静默吞掉。
 */
export function UpdateDialog({
  info,
  onClose,
  onFinished,
}: {
  info: UpdateInfo;
  onClose: () => void;
  onFinished: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const bullets = releaseNotes(info.notes);
  const sizeMb = info.assetSize > 0 ? (info.assetSize / 1024 / 1024).toFixed(1) : "";

  const install = async () => {
    if (busy) return;
    setError("");
    if (!info.assetUrl) {
      // 没有可下载的安装包（比如只有源码包）：退回浏览器下载页
      try {
        await openUrl(info.url || "https://github.com/yaoahos/SafePasswordDesktop/releases");
      } catch {
        setError("链接打开失败，请稍后重试");
        return;
      }
      onClose();
      return;
    }
    setBusy(true);
    try {
      await updateDownloadAndInstall(info.assetUrl);
      onFinished();
    } catch {
      setError("下载安装包失败，请检查网络后重试");
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog title={`发现新版本 ${info.latest}`} onClose={onClose} width={460}>
      <div className="muted small" style={{ marginBottom: 10 }}>
        当前版本 {info.current}
      </div>
      <div className="update-notes">
        {bullets.length === 0 ? (
          <div className="update-note-line">本次更新主要是问题修复与稳定性优化。</div>
        ) : (
          bullets.map((line, i) => (
            <div className="update-note-line" key={`${i}-${line.slice(0, 12)}`}>
              · {line}
            </div>
          ))
        )}
      </div>
      <div className="update-hint">
        {sizeMb ? `安装包 ${sizeMb} MB · ` : ""}
        更新会关闭应用，已解锁的密码库会重新上锁。
      </div>
      {error && <div className="update-error">{error}</div>}
      <div className="row" style={{ justifyContent: "flex-end", marginTop: 14 }}>
        <button className="btn" disabled={busy} onClick={onClose}>
          稍后
        </button>
        <button className="btn primary" disabled={busy} onClick={() => void install()}>
          {busy ? "正在下载…" : "立即更新"}
        </button>
      </div>
    </Dialog>
  );
}
