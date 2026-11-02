# SafePasswordDesktop

SafePassword 的 Windows 桌面版，与 `E:\DSH\SafePassword` 项目文件夹平行、相互独立，不共享构建产物。

当前阶段：**界面布局 Demo**（纯演示数据，不读写真实密码库）。

## Demo 覆盖的布局要求

- 三栏布局：左侧边栏（全部 / 收藏 / 回收站 / 标签）｜ 中间密码库列表 ｜ 右侧条目预览
- 没有底部 tab
- 顶栏放置 8 个按钮：密码生成器、安全检查、WebDAV 同步、主题、导入密码、设置、同步、锁定
- 点击顶栏按钮弹出对应操作弹窗；弹窗**只能**用自己的关闭按钮关闭，点击弹窗外区域不会关闭

## 运行

```powershell
$env:JAVA_HOME = 'E:\AndroidBuild\jdkroot\jdk-17.0.2'
& 'E:\AndroidBuild\gradle-8.11.1\gradle-8.11.1\bin\gradle.bat' -p E:\DSH\SafePasswordDesktop run
```

## Demo 通过后的正式开发方向

- 接入密码库内核（可通过 Gradle 组合构建复用 SafePassword 的 `:core`，或独立引入）
- 真实的解锁 / 加密存储 / WebDAV 同步 / 导入导出 / 安全检查
- 打包 Windows 安装包（jpackage / install4j 等）
