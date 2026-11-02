# SafePasswordDesktop

Windows 桌面端（Tauri 2 + React + TypeScript），与 Android 端 `E:\DSH\SafePassword` 平行独立。

## 当前状态

- 技术栈已从 Compose Desktop 切换为 **Tauri 2**（用户 2026-11-01 决定），旧 CMP 布局 Demo 归档在 `legacy/cmp-demo/`（UI 规格仍以其 AGENTS.md 为准）。
- Rust 核心与 Android `core/` 字节兼容是硬性验收标准，格式规格见 `docs/core-spec.md`。
- 工具链：Rust 1.99.1（MSVC）、Node 26、VS Build Tools 17.14 + Windows SDK 10.0.26100。

## 构建

```powershell
cd E:\DSH\SafePasswordDesktop
npm install
npm run tauri dev      # 开发
npm run tauri build    # 打包
```

## 约定

- 不在 `SafePassword` 仓库混入桌面端内容；未经用户同意不推送任何远端。
