# SafePasswordDesktop — 项目交接（供新会话读取）

> 本项目所有工作在**新会话**中进行；主 SafePassword（Android）项目在另一个会话。
> 两个项目文件夹平行、相互独立：`E:\DSH\SafePassword`（Android）与 `E:\DSH\SafePasswordDesktop`（本项目）。

## 一句话现状

Windows 桌面布局 Demo 已完成两轮用户反馈修改，`compileKotlin` 通过、窗口可正常运行；
等用户对 Demo 满意后进入正式开发（接入 :core 密码库内核、真实加密/同步、打包安装包）。

## 技术栈与构建

- Compose Desktop（`org.jetbrains.compose` 1.7.3）+ Kotlin 2.1.0 + `kotlin.plugin.compose` 2.1.0
- `kotlin { jvmToolchain(17) }`；mainClass = `com.safeincloud.desktop.MainKt`
- 源码单文件：`src/main/kotlin/com/safeincloud/desktop/Main.kt`（约 940 行）
- 资源：`src/main/resources/app-icon.png`（192×192，从 SafePassword `mipmap-xxxhdpi/ic_launcher.png` 拷来，蓝底云+钥匙）
- **构建命令**（pwsh 单次调用，JAVA_HOME 不跨调用持久）：

```powershell
cd E:\DSH\SafePasswordDesktop; $env:JAVA_HOME='E:\AndroidBuild\jdkroot\jdk-17.0.2'; & 'E:\AndroidBuild\gradle-8.11.1\gradle-8.11.1\bin\gradle.bat' <task> --console=plain --no-daemon
```

- 运行 demo：`<task>` 用 `:run`（后台跑，窗口标题 "SafePassword"）；停旧版先 `Get-Process java | Where MainWindowTitle -like '*SafePassword*' | Stop-Process -Force`。

## 用户需求（按时间顺序，全部已实现）

**m07908（原始需求）**："新建一个SafePasswordDesktop的项目，主要在Windows系统使用，界面参考SafePassword项目的平板UI，注意这两个项目的文件夹是平行的，不要混到一块去了；去除底部的tab，把工具页的六个常用工具按钮与同步、锁定两个按钮调整到顶部，点击顶部按钮时弹窗操作，但注意弹窗只能自己关闭，不能因为其它区域的点击而自动关闭；整个界面分为三栏，左侧是侧边栏，中间是密码库，右侧是条目预览；最终开发前先生成一个demo让我预览测试一下效果"

**m08002（第二轮六点）**："项目图标也用SafePassword一样的；去除页面左上角的SafePassword标题，顶部按钮靠左侧，按钮图标颜色使用移动端一样的彩色图标；设置里的各项内容也与移动端保持一致，但改成弹窗中顶部tab切换分类；三栏的宽度再调整一下，尽量保持美观，适配不同的屏幕宽度；侧边栏的标签管理与锁定还是要保留；修改demo后再给我看看"

**m08146（第三轮五点）**："页面上SafePassword Desktop简化为SafePassword，没有必要专门强调桌面端；侧边栏标签管理调整到回收站下方；设置中，去除底部的关闭按钮，安全中去除指纹解锁、允许截屏，去除其他的tab；把关于作为一个顶部按钮放到设置后面；所有弹窗的右上角关闭按钮从文字形式变成X的图标形式"

## 当前界面结构（Demo 最终形态）

- **窗口**：title = "SafePassword"，icon = painterResource("app-icon.png")，1280×820 默认
- **顶栏**（58dp，按钮靠左、彩色图标 24dp 圆角底 + 15dp 图标 + 12.5sp 文字，按 `TopAction.entries` 顺序 9 个）：
  密码生成器(蓝) / 安全检查(红) / WebDAV 同步(青) / 主题(紫) / 导入密码(绿) / 设置(灰) / **关于(青, Info 图标)** / 同步(蓝) / 锁定(红)
  —— `enum class TopAction` + `toolIconTints` 映射在 Main.kt 顶部
- **三栏**（`DemoApp` 用 `BoxWithConstraints`）：Sidebar 固定 230dp ｜ Divider ｜ ItemList 宽 `(maxWidth*0.34f).coerceIn(300.dp, 440.dp)` ｜ Divider ｜ ItemPreview `weight(1f)`
- **侧边栏**：我的密码库头 + 全部/收藏/回收站（`Section.entries`）→ **标签管理** → Divider → "标签" 标签列表（demoLabels 四色圆点）→ Spacer(weight) → 锁定 → 底部 "SafePassword 0.1.0-demo"
- **弹窗** `ActionDialog`：`DialogProperties(dismissOnClickOutside=false, dismissOnBackPress=false)` —— 只能自己关（硬性需求）；右上角 X 图标（`Icons.Outlined.Close` 20dp 灰 0xFF9AA0A6）；底部大关闭按钮**除设置弹窗外**都保留
- **设置弹窗** `SettingsContent(notify)`：顶部 `ScrollableTabRow` 五个 Tab（安全/同步与备份/密码库/显示与搜索/导入与导出，无"其他"）；每行用 `DemoSettingRow(title, subtitle, icon?, trailing?, onClick?)`；安全组只剩 自动锁定 + 修改主密码；无底部关闭按钮
- **关于弹窗** `AboutContent()`：版本/技术栈/功能范围三行
- **演示数据**：`demoItems`（坚果云/GitHub/招商银行…）与 `demoLabels`（工作/社交/银行/开发），纯展示不读写真实库

## 注意事项

- 本项目**未 git init**（未纳入版本控制）；如需提交先问用户（沿用 SafePassword 的约定：未经同意不推送，本项目连 init 都没做）
- Hindsight 记忆工具在本会话 401（API key 未配置），别依赖
- 弹窗内容函数：Generator/Security/WebDav/Theme/Import/Settings/Sync/Lock/About（`when(action)` 分发在 ActionDialog 内）
- 已知小无用变量：`SettingsContent` 里 `searchPasswords` 之外的状态都已清；若见 `Icons.Outlined.Archive/Backup/Folder/Info/Label/Upload` 等 import 均为设置行图标所用
- 用户偏好：改完直接编译重启 demo 给他看，**不要装机调试、不要推送**；反馈通常是一次多点、原文引用
