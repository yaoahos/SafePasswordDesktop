/** 五边形标签图标（对齐移动端 Components.kt LabelGlyph 的路径） */
export function LabelGlyph({ color, size = 18 }: { color: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 1 1" style={{ flex: "none", display: "block" }}>
      <path d="M 0.06 0.50 L 0.30 0.10 L 0.94 0.10 L 0.94 0.90 L 0.30 0.90 Z" fill={color} />
    </svg>
  );
}
