import { useEffect, useRef, type ReactNode } from "react";

/** 弹窗外壳：不能点外部关闭、不能 Esc 关闭、右上角 X、底部“关闭”按钮（设置除外）。 */
export function Dialog({
  title,
  onClose,
  showCloseButton = true,
  children,
  width,
}: {
  title: string;
  onClose: () => void;
  /** 设置弹窗传 false：无底部关闭按钮 */
  showCloseButton?: boolean;
  children: ReactNode;
  width?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);

  // 捕获阶段吞掉 Esc —— 弹窗只能通过右上角 X / 底部关闭按钮自己关闭
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        e.preventDefault();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, []);

  // 把键盘焦点保持在弹窗内（点击外部后 Tab 不会跑到背景页面）
  useEffect(() => {
    ref.current?.focus();
  }, []);

  return (
    <div className="dialog-overlay">
      <div
        className="dialog-panel"
        style={width ? { width } : undefined}
        tabIndex={-1}
        ref={ref}
      >
        <div className="dialog-header">
          <div className="dialog-title">{title}</div>
          <button className="dialog-x" onClick={onClose} title="关闭" aria-label="关闭">
            ✕
          </button>
        </div>
        <div className="dialog-body">{children}</div>
        {showCloseButton && (
          <div className="dialog-close">
            <button className="btn block tall" onClick={onClose}>
              关闭
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
