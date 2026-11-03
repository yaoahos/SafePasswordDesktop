# Android 端改造方案：容器 v2（gzip）+ 版本重置 1.0.0

> 面向 `E:\DSH\SafePassword` 仓库的会话使用。桌面端已完成同样的改动（提交 `8621862`），两端协议必须一致。
> 本文所有行号以 2026-11-03 时点的 Android 源码为准，改动前请先 `git status` 确认工作区干净。

---

## 1. 为什么改

桌面端保险库容器已升级到 `version = 2`：**payload 先 gzip 再 AES-GCM**。实测 10k 条目的库从 8.87 MB 降到约 0.55 MB（≈16×），这是应对坚果云免费额度（每月上传 1 GB / 下载 3 GB）的关键一步。

Android 端必须同步升级，否则两端对同一份云端文件互相读不了。

**不做过渡版本**：两端直接切成新基线，Android 应用版本重置为 `1.0.0`（`versionCode = 1`）。

---

## 2. 格式规范（与桌面端一致）

头部布局**完全不变**，只有 `version` 字段的取值和 payload 的明文内容变化：

| 偏移 | 内容 | 说明 |
|---|---|---|
| 0 | magic | ASCII `SICV` |
| 4 | version | 大端 int32：`2` = gzip 后加密（写入值）；`1` = 明文 JSON 直接加密（只读兼容） |
| 8 | iterations | 不变 |
| 12 | salt_len + salt | 不变 |
| … | wrapped_dek_len + wrapped_dek | 不变（AAD `SICV-WRAP`） |
| … | payload_len + payload | `AES-GCM(明文)`，AAD `SICV-PAYLOAD`；明文见下 |

- `version = 1`：payload 明文 = `Vault` 的 JSON（UTF-8）。
- `version = 2`：payload 明文 = **gzip(JSON)**（RFC 1952，标准 gzip 流；压缩级别不影响解码）。

规则：

- **读**：接受 `{1, 2}`；`2` 先 gunzip 再解析 JSON。
- **写**：一律写 `2`（打开旧库后再保存即自动升级）。
- gunzip 必须设解压上限（桌面端 256 MiB），防压缩炸弹。

---

## 3. 代码改动（`core/src/main/kotlin/com/yaoahos/safepassword/core/crypto/VaultContainer.kt`）

### 3.1 常量（当前 `:88` 附近）

```kotlin
private const val VERSION = 1
```

改为：

```kotlin
/** 当前写出的版本：payload 先 gzip 再加密。 */
private const val WRITE_VERSION = 2
/** 仍可读取的旧版本：payload 是明文 JSON。 */
private const val PLAIN_VERSION = 1
/** 解压后的明文上限，防压缩炸弹。 */
private const val MAX_PLAINTEXT_BYTES = 256 * 1024 * 1024
```

### 3.2 `toBytes()`（当前 `:44-62`）

现状：

```kotlin
val payload = Crypto.encrypt(
    dek,
    AppJson.instance.encodeToString(Vault.serializer(), vault).toByteArray(Charsets.UTF_8),
    PAYLOAD_AAD
)
...
data.writeInt(VERSION)
```

改为：

```kotlin
val json = AppJson.instance.encodeToString(Vault.serializer(), vault).toByteArray(Charsets.UTF_8)
val payload = Crypto.encrypt(dek, gzip(json), PAYLOAD_AAD)
...
data.writeInt(WRITE_VERSION)
```

### 3.3 `open()`（当前 `:109-130`）

现状（关键一行在 `:122-127`）：

```kotlin
val plain = Crypto.decrypt(dek, header.payload, PAYLOAD_AAD)
...
AppJson.instance.decodeFromString(Vault.serializer(), plain.toString(Charsets.UTF_8))
```

改为：

```kotlin
val sealed = Crypto.decrypt(dek, header.payload, PAYLOAD_AAD)
val plain = if (header.version == PLAIN_VERSION) sealed else gunzip(sealed)
...
AppJson.instance.decodeFromString(Vault.serializer(), plain.toString(Charsets.UTF_8))
```

> 注意：`header.version` 在解密前就已读入，不需要先尝试解密判断；错密码仍然只在 `Crypto.decrypt` / `unwrapDek` 处表现为 `WrongPasswordException`。

### 3.4 `Header` 与 `readHeader()`（当前 `:141-158`）

- `Header` 数据类增加字段 `val version: Int`（放在 `iterations` 前）。
- `:149-150` 的校验改为：

```kotlin
val version = data.readInt()
if (version != PLAIN_VERSION && version != WRITE_VERSION) {
    throw VaultFormatException("不支持的数据库版本：$version")
}
```

- `return Header(version, iterations, salt, wrapped, payload)`。
- 其余 `readChunk` 的范围校验、`available()` 检查保持原样。

### 3.5 新增两个工具函数（放同文件 private）

```kotlin
private fun gzip(bytes: ByteArray): ByteArray {
    val out = ByteArrayOutputStream(bytes.size / 4 + 64)
    GZIPOutputStream(out).use { it.write(bytes) }
    return out.toByteArray()
}

private fun gunzip(bytes: ByteArray): ByteArray {
    val out = ByteArrayOutputStream(bytes.size * 4)
    GZIPInputStream(bytes.inputStream()).use { gz ->
        val buf = ByteArray(64 * 1024)
        var total = 0
        while (true) {
            val n = gz.read(buf)
            if (n < 0) break
            total += n
            if (total > MAX_PLAINTEXT_BYTES) throw VaultFormatException("数据库内容解压后过大")
            out.write(buf, 0, n)
        }
    }
    return out.toByteArray()
}
```

需要 `import java.io.ByteArrayOutputStream`、`java.util.zip.GZIPInputStream`、`java.util.zip.GZIPOutputStream`。

可选：对 gzip 明文做 `Arrays.fill(json, 0)` 擦除（与桌面端一致的安全习惯），但不要引入额外依赖。

---

## 4. 测试

### 4.1 `core/src/test/kotlin/com/yaoahos/safepassword/core/VaultContainerTest.kt`

现有用例若断言 `version == 1` 需要改成 `2`。新增：

1. `create 后 open 往返`：条目数、内容一致（现有用例改断言版本即可）。
2. `压缩确实生效`：构造 300 条相似条目，断言 `toBytes().size * 2 < JSON 字节数`。
3. `v1 旧库仍可读`：用仓库里的 `interop/fixtures/android/known1.sicv`（v1）打开，条目数 = 5。
4. `不支持的版本`：把头部 version 改成 `3`，断言抛 `VaultFormatException` 且消息为 `无法读取数据库文件：不支持的数据库版本：3`。

### 4.2 跨端互操作（关键）

桌面端已生成 v2 样本：`E:\DSH\SafePasswordDesktop\interop\fixtures\desktop\known1.v2.spv`（1257 字节，version=2）
- 口令文件：`E:\DSH\SafePasswordDesktop\interop\fixtures\android\known1.password.txt`
- 期望 JSON（与 v1 样本解析结果完全相同）：`E:\DSH\SafePasswordDesktop\interop\fixtures\android\known1.vault.json`

Android 侧验证步骤：

1. 把 `known1.v2.spv` 复制到 `core/src/test/resources/`（或 `interop/fixtures/`，按现有约定）。
2. 用口令打开它，序列化结果与 `known1.vault.json` 逐字节比对（桌面端用 SHA-256 比过，`items=5`、库名 `测试密码库🚀`）。
3. 反向：Android 用 v2 保存一份，交回桌面端 `cargo run --example read_fixture -- <file> <password-file> <out.json>` 验证能读、内容一致。

### 4.3 回归

- 现有 `VaultContainerTest`、`VaultMergerTest`、`WebDavClientTest` 全绿。
- 老库（v1 `.sicv`）打开后**再保存一次**，文件版本变成 2 且内容不变。

---

## 5. 版本重置为 1.0.0

`apps/android/build.gradle.kts:25-26`：

```kotlin
versionCode = 10
versionName = "1.5.2"
```

改为：

```kotlin
versionCode = 1
versionName = "1.0.0"
```

同步需要一起改的文案（按 AGENTS.md 的约定，三处一致）：

- 应用内"关于/更新日志"里的版本号与条目；
- `README.md` 顶部的版本/下载说明；
- GitHub Release 标题与正文（正文只写 `**2026/11/xx · 1.0.0**` + `- 新增/优化/修复/变更/安全 …` 条目，不要写安装包名、大小、SHA-256、`>` 提示）。

> 版本号重置会让应用商店/更新检查认为是"降级"，本项目是自用分发，直接覆盖安装即可；若桌面端的自更新仓库也维护 Android 资产，记得把旧的 `1.5.x` 资产下掉或标注废弃。

---

## 6. 发布顺序与风险

**必须两端一起换。** 云端 `.spv` 是共享文件：

- 桌面端写 v2 后，**未升级的 APK** 同步时会报 `无法读取数据库文件：不支持的数据库版本：2`；
- 反之，APK 写 v2 后，未升级的桌面端同样报错。

建议顺序：

1. 先在 Android 上改完、跑通全部测试、装到手机上自测（此时云端若已是 v2，正好验证读取；若还是 v1，先存一次让它升级）。
2. 再用桌面端新版登录同一账号，两边互相同步一轮，确认库名、条目、更新时间都对。
3. 都确认无误后，再决定是否上传 Release。

**回滚**：如果必须回到 v1 时代，注意 v1 客户端无法读取 v2 文件；此时要用桌面端"导出"出 v2 文件，在桌面端打开后……桌面端也只会写 v2。真要回滚只能人工构造 v1 容器（不推荐）。所以发版前务必两端都测通。

---

## 7. 验收清单

- [ ] `VaultContainer.kt` 读写都走 v2，读仍兼容 v1
- [ ] `MAX_PLAINTEXT_BYTES` 上限生效（构造超限输入抛 `VaultFormatException`）
- [ ] 单测：v2 往返、压缩比、v1 兼容、版本 3 拒绝
- [ ] 互操作：Android 打开 `known1.v2.spv` 得到与 `known1.vault.json` 相同的内容
- [ ] 反向：桌面端能打开 Android 写出的 v2 文件
- [ ] `versionCode = 1`、`versionName = "1.0.0"`，关于页/README/Release 三处一致
- [ ] 云端同步全链路：桌面改 → 手机拉 → 手机改 → 桌面拉，均成功且体积明显变小
