import { getCurrentWindow } from "@tauri-apps/api/window";

import { Icon } from "./icons";

const win = getCurrentWindow();

/** 自绘窗口按钮（decorations:false 时唯一的最小化/最大化/关闭入口） */
export function WindowControls() {
  return (
    <div className="win-ctrls" data-no-drag="true">
      <button className="win-btn" title="最小化" onClick={() => void win.minimize()}>
        <Icon name="minimize" size={15} />
      </button>
      <button className="win-btn" title="最大化 / 还原" onClick={() => void win.toggleMaximize()}>
        <Icon name="maximize" size={13} />
      </button>
      <button className="win-btn win-close" title="关闭" onClick={() => void win.close()}>
        <Icon name="close" size={15} />
      </button>
    </div>
  );
}
