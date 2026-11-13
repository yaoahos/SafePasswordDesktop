import { useState } from "react";
import { Dialog } from "../components/Dialog";
import { checkUpdate, updateDownloadAndInstall } from "../api";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { UpdateInfo } from "../types";
import type { AppCtx } from "../app-context";
import { APP_VERSION } from "../version";
import { CHANGELOG, MAX_CHANGELOG_VERSIONS } from "../changelog";

const PROJECT_URL = "https://github.com/yaoahos/SafePasswordDesktop";
const RELEASES_URL = "https://github.com/yaoahos/SafePasswordDesktop/releases";

const LINKS: { name: string; url: string }[] = [
  { name: "Tauri", url: "https://tauri.app" },
  { name: "React", url: "https://react.dev" },
  { name: "Vite", url: "https://vitejs.dev" },
  { name: "RustCrypto", url: "https://github.com/RustCrypto/AEADs" },
  { name: "quick-xml", url: "https://github.com/tafia/quick-xml" },
  { name: "reqwest", url: "https://github.com/seanmonstar/reqwest" },
];

export function AboutDialog({ ctx, onClose }: { ctx: AppCtx; onClose: () => void }) {
  const [checking, setChecking] = useState(false);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [status, setStatus] = useState<string>("尚未检查更新");
  const [installing, setInstalling] = useState(false);
  /** 应用内更新日志（最多 5 个版本，对齐手机端更新日志页） */
  const [showLog, setShowLog] = useState(false);

  const open = (url: string) => {
    void openUrl(url).catch(() => ctx.notify("链接打开失败，请稍后重试", "error"));
  };

  const check = async () => {
    setChecking(true);
    setStatus("正在检查更新…");
    try {
      const r = await checkUpdate(APP_VERSION);
      setInfo(r);
      setStatus(r.hasUpdate ? `发现新版本 ${r.latest}` : `已是最新版本（${r.current}）`);
    } catch (e) {
      setInfo(null);
      setStatus(String(e));
    } finally {
      setChecking(false);
    }
  };

  const install = async () => {
    if (!info?.hasUpdate) return;
    if (!info.assetUrl) {
      open(info.url || RELEASES_URL);
      ctx.notify("这个版本没有安装包，已打开下载页", "success");
      return;
    }
    setInstalling(true);
    try {
      await updateDownloadAndInstall(info.assetUrl);
      ctx.notify("安装包已启动，请按向导完成安装", "success");
    } catch {
      ctx.notify("下载安装包失败，请检查网络后重试", "error");
    } finally {
      setInstalling(false);
    }
  };

  return (
    <Dialog title="关于 SafePassword" onClose={onClose} width={560}>
      <div className="about-head">
        <img className="about-logo" src="/icon.png" alt="" />
        <div>
          <button className="about-name" onClick={() => open(PROJECT_URL)} title={PROJECT_URL}>
            SafePassword
          </button>
          <div className="muted small">版本 {APP_VERSION} · Windows 桌面版</div>
        </div>
      </div>

      <div className="about-card">
        <div className="about-title">功能范围</div>
        <div className="about-text">
          Windows 桌面密码管理器：多密码库、自定义字段、条目图标、标签、回收站、密码生成器、强度分析、HIBP
          泄露检查、WebDAV 同步与备份、主题换色，保险库与手机端字节兼容。
        </div>
        <div className="about-text muted">不包含浏览器扩展与自动填充、Passkey、共享协作。</div>
      </div>

      <div className="about-card">
        <div className="about-title">技术栈</div>
        <div className="about-text">Tauri 2 · Rust · React 19 · TypeScript</div>
        <div className="about-text">AES-256-GCM · PBKDF2-HMAC-SHA256（210,000 次）</div>
      </div>

      <div className="about-card">
        <div className="about-title">开源致谢</div>
        <div className="about-links">
          {LINKS.map((l) => (
            <button key={l.name} className="about-link" onClick={() => open(l.url)}>
              {l.name}
            </button>
          ))}
        </div>
      </div>

      <div className="about-card">
        <div className="about-title">安全说明</div>
        <div className="about-text">
          主密码不落盘；保险库使用 AES-256-GCM 加密；设置中的 WebDAV 密码与 HIBP 密钥经 Windows
          DPAPI 加密后写入本机；界面启用严格 CSP。
        </div>
      </div>

      <div className="about-card">
        <div className="about-title">更新</div>
        <div className="row" style={{ gap: 10 }}>
          <button className="btn sm" disabled={checking || installing} onClick={() => void check()}>
            {checking ? "正在检查更新…" : "检查更新"}
          </button>
          {info?.hasUpdate && (
            <button className="btn primary sm" disabled={installing} onClick={() => void install()}>
              {installing ? "下载中…" : info.assetUrl ? "下载并安装" : "打开下载页"}
            </button>
          )}
          <button className="btn sm" onClick={() => setShowLog((v) => !v)}>
            {showLog ? "收起更新日志" : "更新日志"}
          </button>
          <button className="btn sm" onClick={() => open(RELEASES_URL)}>
            GitHub Releases
          </button>
        </div>
        <div className="setting-sub" style={{ marginTop: 8 }}>
          {status}
        </div>
        <label className="check-row" style={{ marginTop: 10 }}>
          <input
            type="checkbox"
            checked={ctx.settings.checkUpdatesOnLaunch}
            onChange={(e) =>
              void ctx.applySettings({ ...ctx.settings, checkUpdatesOnLaunch: e.target.checked })
            }
          />
          启动时自动检查更新
        </label>
        <div className="setting-sub" style={{ marginTop: 4 }}>
          开启后每次启动静默查询一次，发现新版本只提示，不会自动下载安装。
        </div>
        {showLog && (
          <div style={{ marginTop: 12 }}>
            {CHANGELOG.slice(0, MAX_CHANGELOG_VERSIONS).map((e, i) => (
              <div key={e.version}>
                {i > 0 && <div className="hr" style={{ margin: "10px 0" }} />}
                <div style={{ fontWeight: 500, fontSize: 13 }}>
                  {e.date} · {e.version}
                </div>
                <ul style={{ margin: "6px 0 0", paddingLeft: 18 }}>
                  {e.items.map((line, j) => (
                    <li key={j} className="muted small" style={{ lineHeight: 1.7 }}>
                      {line}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        )}
      </div>

      <div className="about-foot muted small">
        SafePassword 是独立实现，与任何第三方产品无关联。
      </div>
    </Dialog>
  );
}
