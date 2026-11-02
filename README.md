# SafePasswordDesktop

Windows 桌面端（Tauri 2 + React + TypeScript），与 Android 端 `E:\DSH\SafePassword` 平行独立。
功能对齐 Android 1.4.1，保险库格式与手机端**字节兼容**（可互通 WebDAV 同步与备份）。

## 当前状态（v0.1.0-demo）

- **功能**：密码库管理（建库 / 条目 / 标签 / 收藏 / 归档 / 回收站）、条目编辑（字段增删改与上下移、备注内联与位置调整、生成密码回填、图标更换、保存并新建、标题空校验）、复制敏感内容 30 秒自动清空、URL 一键打开（opener）、密码生成器、强度与安全检查（HIBP）、CSV / SafeInCloud XML / JSON 导入导出、导出 `.sicv`（可拷回手机）、本地备份（自动备份间隔可调 + 保留 10 份）、WebDAV 同步（手动 / 自动 / 拉取 + 四种冲突策略）、附件（≤10 MB 加密存储）、设置（主题 / 自动锁定 / 同步参数等 5 组）。
- **字节兼容**：`interop/` 用 Android core 生成夹具做双向逐字节比对（T1 通过）；格式规格见 `docs/core-spec.md`。
- **测试**：86 个（81 单元 + 3 命令层 E2E + 1 WebDAV 双设备同步 E2E〔内嵌 mock 服务器，走真实 HTTP〕+ 1 mock 冒烟），另有 1 个实网 HIBP 测试默认跳过（手动 `cargo test -- --ignored`）。**全绿是底线**。
- **安全**：`settings.json` 敏感字段 Windows DPAPI 加密落盘（`dpapi:` 前缀，见 `sp/dpapi.rs`）；主密码仅存内存；CSP 收紧；保险库 AES-GCM + PBKDF2 210k 与手机端同参。
- **安装包**：NSIS ≈2.0 MiB、MSI ≈2.8 MiB（WebView2 走系统，不随包分发）。

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
