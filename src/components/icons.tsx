/** 统一线性图标（内联 SVG，随文字颜色走），替代 emoji 保证清晰与一致。 */
export type IconName =
  | "key"
  | "shield"
  | "palette"
  | "sliders"
  | "info"
  | "refresh"
  | "lock"
  | "minimize"
  | "maximize"
  | "close"
  | "search"
  | "plus"
  | "arrow-up"
  | "arrow-down"
  | "copy"
  | "external"
  | "eye"
  | "eye-off"
  | "edit"
  | "trash"
  | "dice"
  | "star"
  | "star-filled"
  | "folder"
  | "cloud"
  | "file-import"
  | "file-export"
  | "back"
  | "menu"
  | "tag";

const PATHS: Record<IconName, React.ReactNode> = {
  key: (
    <>
      <circle cx="8" cy="16" r="3.2" />
      <path d="M10.4 13.6 L19 5" />
      <path d="M15.5 5 H19 V8.5" />
    </>
  ),
  shield: (
    <>
      <path d="M12 3.2 L19.5 6.2 V12 C19.5 16.4 16.3 19.8 12 20.8 C7.7 19.8 4.5 16.4 4.5 12 V6.2 Z" />
      <path d="M9.2 12 L11.3 14.1 L15 10.3" />
    </>
  ),
  palette: (
    <>
      <path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.5 0 2.1-1 1.5-2-.6-1.1-.2-2.1 1.5-2.1h1.6a3.4 3.4 0 0 0 3.4-3.4A8.6 8.6 0 0 0 12 3.5Z" />
      <circle cx="8.6" cy="9.6" r="0.9" />
      <circle cx="12" cy="7.9" r="0.9" />
      <circle cx="15.4" cy="9.6" r="0.9" />
    </>
  ),
  sliders: (
    <>
      <path d="M4 7h16M4 12h16M4 17h16" />
      <circle cx="9" cy="7" r="1.7" fill="currentColor" stroke="none" />
      <circle cx="15" cy="12" r="1.7" fill="currentColor" stroke="none" />
      <circle cx="9" cy="17" r="1.7" fill="currentColor" stroke="none" />
    </>
  ),
  info: (
    <>
      <circle cx="12" cy="12" r="8.6" />
      <path d="M12 11v5.2" />
      <circle cx="12" cy="7.8" r="0.95" fill="currentColor" stroke="none" />
    </>
  ),
  refresh: (
    <>
      <path d="M19.6 12a7.6 7.6 0 1 1-2.3-5.4" />
      <path d="M19.6 4.6V8h-3.4" />
    </>
  ),
  lock: (
    <>
      <rect x="5.5" y="10.5" width="13" height="9" rx="2" />
      <path d="M8.5 10.5V8a3.5 3.5 0 0 1 7 0v2.5" />
      <circle cx="12" cy="15" r="1.1" fill="currentColor" stroke="none" />
    </>
  ),
  minimize: <path d="M5 12h14" />,
  maximize: <rect x="5.5" y="5.5" width="13" height="13" rx="1.5" />,
  close: <path d="M6 6l12 12M18 6L6 18" />,
  search: (
    <>
      <circle cx="11" cy="11" r="6.2" />
      <path d="M15.6 15.6 L20 20" />
    </>
  ),
  plus: <path d="M12 5v14M5 12h14" />,
  "arrow-up": <path d="M12 19V5M6 11l6-6 6 6" />,
  "arrow-down": <path d="M12 5v14M6 13l6 6 6-6" />,
  copy: (
    <>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M15 9V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h3" />
    </>
  ),
  external: (
    <>
      <path d="M14 4h6v6" />
      <path d="M20 4l-8.5 8.5" />
      <path d="M18.5 14v4.5a2 2 0 0 1-2 2H5.5a2 2 0 0 1-2-2V7.5a2 2 0 0 1 2-2H10" />
    </>
  ),
  eye: (
    <>
      <path d="M2.5 12s3.6-6 9.5-6 9.5 6 9.5 6-3.6 6-9.5 6-9.5-6-9.5-6Z" />
      <circle cx="12" cy="12" r="2.6" />
    </>
  ),
  "eye-off": (
    <>
      <path d="M3 4l18 16" />
      <path d="M9.6 6.3A9.6 9.6 0 0 1 12 6c5.9 0 9.5 6 9.5 6a17 17 0 0 1-3.2 3.7" />
      <path d="M6.3 8.2A16.9 16.9 0 0 0 2.5 12s3.6 6 9.5 6c1 0 1.9-.2 2.8-.5" />
    </>
  ),
  edit: (
    <>
      <path d="M4 20h4.2L20 8.2a2.1 2.1 0 0 0-3-3L5.2 17 Z" />
      <path d="M14.6 5.9l3.5 3.5" />
    </>
  ),
  trash: (
    <>
      <path d="M4.5 7h15" />
      <path d="M9.5 7V5.2a1.2 1.2 0 0 1 1.2-1.2h2.6a1.2 1.2 0 0 1 1.2 1.2V7" />
      <path d="M6.5 7l.9 12a2 2 0 0 0 2 1.9h5.2a2 2 0 0 0 2-1.9l.9-12" />
      <path d="M10.5 11v6M13.5 11v6" />
    </>
  ),
  dice: (
    <>
      <rect x="4.5" y="4.5" width="15" height="15" rx="3" />
      <circle cx="9" cy="9" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="15" cy="15" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="12" cy="12" r="1.15" fill="currentColor" stroke="none" />
    </>
  ),
  star: <path d="M12 3.6l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.8-5.2 2.8 1-5.8L3.5 9.8l5.9-.9 Z" />,
  "star-filled": (
    <path
      d="M12 3.6l2.6 5.3 5.9.9-4.3 4.1 1 5.8-5.2-2.8-5.2 2.8 1-5.8L3.5 9.8l5.9-.9 Z"
      fill="currentColor"
    />
  ),
  folder: (
    <path d="M3.5 7.5a2 2 0 0 1 2-2h3.4l2 2.4h7.6a2 2 0 0 1 2 2v7.6a2 2 0 0 1-2 2H5.5a2 2 0 0 1-2-2Z" />
  ),
  cloud: (
    <path d="M7.2 18.5h9.9a3.9 3.9 0 0 0 .4-7.8 5.6 5.6 0 0 0-10.8-1.3A3.6 3.6 0 0 0 7.2 18.5Z" />
  ),
  "file-import": (
    <>
      <path d="M13 3.5H7a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V9.5Z" />
      <path d="M13 3.5v6h6" />
      <path d="M12 12v5M9.7 14.7l2.3 2.3 2.3-2.3" />
    </>
  ),
  "file-export": (
    <>
      <path d="M13 3.5H7a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V9.5Z" />
      <path d="M13 3.5v6h6" />
      <path d="M12 17v-5M9.7 14.3l2.3-2.3 2.3 2.3" />
    </>
  ),
  back: <path d="M15 5l-7 7 7 7" />,
  menu: (
    <>
      <path d="M8.5 6.6h11M8.5 12h11M8.5 17.4h11" />
      <circle cx="4.7" cy="6.6" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="4.7" cy="12" r="1.15" fill="currentColor" stroke="none" />
      <circle cx="4.7" cy="17.4" r="1.15" fill="currentColor" stroke="none" />
    </>
  ),
  tag: (
    <>
      <path d="M4 4.8h8.6l7 7a2 2 0 0 1 0 2.8l-4.9 4.9a2 2 0 0 1-2.8 0l-7-7V4.8Z" />
      <circle cx="8.6" cy="8.6" r="1.5" />
    </>
  ),
};

export function Icon({
  name,
  size = 20,
  className,
  strokeWidth = 1.7,
}: {
  name: IconName;
  size?: number;
  className?: string;
  strokeWidth?: number;
}) {
  return (
    <svg
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}
