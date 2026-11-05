import { useState, type CSSProperties } from "react";
import { Icon } from "./icons";

interface Props {
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  autoFocus?: boolean;
  maxLength?: number;
  /** 额外加到 input 上的类名（例如 with-eye 之外的自定义样式） */
  className?: string;
  style?: CSSProperties;
  onEnter?: () => void;
  /** 图标尺寸，紧凑输入框用 15，常规用 16~17 */
  iconSize?: number;
}

/**
 * 带「显示 / 隐藏」眼睛的密码输入框。
 * 全应用统一用它，避免有些密码框能看、有些不能看。
 */
export function PasswordInput({
  value,
  onChange,
  placeholder,
  autoFocus,
  maxLength,
  className = "",
  style,
  onEnter,
  iconSize = 17,
}: Props) {
  const [shown, setShown] = useState(false);
  return (
    <div className="input-wrap">
      <input
        className={`input with-eye ${className}`.trim()}
        type={shown ? "text" : "password"}
        value={value}
        placeholder={placeholder}
        autoFocus={autoFocus}
        maxLength={maxLength}
        style={style}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && onEnter) onEnter();
        }}
      />
      <button
        type="button"
        className="icon-btn in-frame"
        title={shown ? "隐藏" : "显示"}
        onClick={() => setShown((v) => !v)}
      >
        <Icon name={shown ? "eye-off" : "eye"} size={iconSize} />
      </button>
    </div>
  );
}
