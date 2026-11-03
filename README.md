# SafePasswordDesktop

Windows 桌面端（Tauri 2 + React + TypeScript），与 Android 端 `E:\DSH\SafePassword` 平行独立。
功能对齐 Android 1.4.1，保险库格式与手机端**字节兼容**（可互通 WebDAV 同步与备份）。

## 当前状态（v0.1.0-demo）

- **功能**：密码库管理（建库 / 条目 / 标签 / 收藏 / 归档 / 回收站 / 改名与删除同步云端）、条目编辑（字段增删改与上下移、备注内联与位置调整、生成密码回填、图标更换、保存并新建、标题空校验）、列表行内收藏即点即生效、复制敏感内容 30 秒自动清空、URL 一键打开（opener）、密码生成器、强度与安全检查（HIBP）、CSV / XML / JSON 导入导出、导出加密备份 `.spv`（系统保存对话框，可拷回手机）、本地备份（默认安装目录 `backup`，自动备份间隔可调 + 保留 10 份）、WebDAV 多密码库同步（解锁即同步 / 自动 / 定时拉取 / 手动；云端删除与改名会传播到其它端）、设置（主题 / 自动锁定 / 同步参数等分组）。
- **云端布局**：对齐手机端多库布局——`<remoteDir>/SafePassword-<key>.spv` + `SafePassword-index.json` 清单（key 由库名派生），并兼容旧版单文件路径。
- **字节兼容**：`interop/` 用 Android core 生成夹具做双向逐字节比对（T1 通过）；格式规格见 `docs/core-spec.md`。
- **测试**：85 个（80 单元 + 3 命令层 E2E + 1 WebDAV 双设备同步 E2E〔内嵌 mock 服务器，走真实 HTTP〕+ 1 mock 冒烟），另有 1 个实网 HIBP 测试默认跳过（手动 `cargo test -- --ignored`）。**全绿是底线**。
- **安全**：`settings.json` 敏感字段 Windows DPAPI 加密落盘（`dpapi:` 前缀，见 `sp/dpapi.rs`）；主密码仅存内存；CSP 收紧；保险库 AES-GCM + PBKDF2 210k 与手机端同参。
- **安装包**：NSIS ≈2.15 MiB、MSI ≈3.0 MiB（WebView2 走系统，不随包分发）。

## 构建

```powershell
cd E:\DSH\SafePasswordDesktop
npm install
npm run tauri dev      # 开发（vite dev server）
npm run tauri build    # 打包（NSIS + MSI；NSIS 工具首次下载需要代理时设 HTTP(S)_PROXY=http://127.0.0.1:7890）
```

`src-tauri` 下 `cargo test` 是回归底线；详细协作约定、数据目录、测试目标 0xC0000139 清单坑见 `AGENTS.md`。

## 约定

- 不在 `SafePassword` 仓库混入桌面端内容；未经用户同意不推送任何远端。
