# SafePassword core 库 Rust 移植规范（byte-level）

> 本文档面向 Tauri (Rust) 桌面端实现者，逐字节描述 Android 端 `core/` 模块（纯 Kotlin/JVM）的
> 加密格式、文件格式、JSON schema、WebDAV 协议行为与同步/合并算法，目标是 **字节兼容**：
> 桌面端与 Android 端可以互读 `.sicv` 保险库、`.sica` 附件、WebDAV 上的同一份数据和备份文件。
>
> 所有常量均直接摘自源码并标注 `文件名:行号`。凡源码行为存在歧义或历史不一致之处，
> 集中列在第 15 节「已知歧义」，实现前请务必阅读。
>
> 源文件（均位于 `E:\DSH\SafePassword\core\src\main\kotlin\com\\yaoahos\\safepassword\\core\\`，行号以 2026-11 时点版本为准）：
> `crypto/Crypto.kt`、`crypto/VaultContainer.kt`、`io/VaultCodec.kt`、`io/VaultXml.kt`、
> `io/AtomicFiles.kt`、`json/AppJson.kt`、`model/Model.kt`、`model/TemplateCatalog.kt`、
> `storage/VaultStore.kt`、`storage/BackupManager.kt`、`storage/AttachmentStore.kt`、
> `sync/WebDavClient.kt`、`sync/SyncEngine.kt`、`sync/VaultMerger.kt`、
> `security/SecurityAnalyzer.kt`、`security/PasswordGenerator.kt`、`security/PasswordStrength.kt`、
> `security/HibpClient.kt`；测试：`VaultContainerTest.kt`、`ToolsTest.kt`、`WebDavClientTest.kt`。
> 少量行为由 Android 调用方 `apps/android/.../AppViewModel.kt`、`apps/android/.../AppContainer.kt`
> 决定，也一并注明。

---

## 1. 加密原语（Crypto.kt）

### 1.1 PBKDF2

- 算法：`PBKDF2WithHmacSHA256`（Crypto.kt:26）。
- 迭代次数：`PBKDF2_ITERATIONS = 210_000`（Crypto.kt:14）。**注意：打开文件时用文件头里存储的
  iterations，而不是常量**（VaultContainer.kt:151），常量仅在 `create`/`changePassword` 时使用。
- 输出密钥长度：`KEY_BITS = 256` bit（Crypto.kt:15）。
- salt：由文件头携带，`create` 时 `SALT_BYTES = 16` 字节随机（VaultContainer.kt:89,96）；
  读取时允许 8–64 字节（VaultContainer.kt:155）。
- 口令类型：JVM 侧是 `CharArray` 经 `PBEKeySpec`（Crypto.kt:23-24）。
  **JCE 的 `PBKDF2WithHmacSHA256` 将口令按 UTF-8 字节编码**（无结尾 NUL，区别于 PKCS#12 的
  UTF-16LE+NUL）。Rust 侧应使用 `password.as_bytes()` 直接喂给 `pbkdf2` crate。
  （此编码行为无法在 core 源码内直接验证，属实现约定；请用第 16 节 T1 测试向量确认。）
- 派生结果直接作为 AES-256 密钥（`SecretKeySpec(encoded, "AES")`，Crypto.kt:28）——即 **KEK
  = PBKDF2-SHA256(password, salt, iterations) 的全部 32 字节**，不做额外 HKDF。
- JVM 侧在 `finally` 中调用 `spec.clearPassword()` 清零口令副本（Crypto.kt:30）。

### 1.2 AES-GCM

- 算法：`AES/GCM/NoPadding`（Crypto.kt:36,47）。
- IV：12 字节随机（`IV_BYTES = 12`，Crypto.kt:16），每次加密新生成（Crypto.kt:35）。
- Tag：128 bit（`TAG_BITS = 128`，Crypto.kt:17），附在密文尾部。
- 输出布局：`IV(12) || ciphertext || tag(16)`（Crypto.kt:40）。
- 解密要求输入 `blob.size >= 12 + 16 = 28`（Crypto.kt:44），否则抛 `IllegalArgumentException("密文长度无效")`。
- AAD（附加认证数据）按用途使用不同常量，见下表：

| 用途 | AAD 字节 | 出处 |
|---|---|---|
| DEK 包裹（key wrap） | ASCII `"SICV-WRAP"`（UTF-8 编码，9 字节） | Crypto.kt:54,57 |
| 保险库 payload | ASCII `"SICV-PAYLOAD"`（13 字节） | VaultContainer.kt:93,49,122 |
| 附件 | 附件 id 的 UTF-8 字节（UUID 字符串，36 字节） | AttachmentStore.kt:30,47 |

### 1.3 密钥包裹（DEK/KEK 两层结构）

- 主密码只用于派生 **KEK**；实际加密数据的是每次建库随机生成的 **DEK**（32 字节，VaultContainer.kt:98）。
- `wrapDek(kek, dek) = AES-GCM(kek, dek.encoded, aad="SICV-WRAP")` → 12 + 32 + 16 = **60 字节**
  （Crypto.kt:53-54）。
- `unwrapDek` 解出后要求长度恰好 32 字节，否则抛 `IllegalArgumentException("数据密钥长度无效")`（Crypto.kt:58）。
- 修改主密码 = 换 salt + 重新派生 KEK + 重新包裹 DEK；**payload 密文完全不变**
  （VaultContainer.kt:70-75），附件也不受影响（附件密钥是 `contentKey`，见 1.5/6）。

### 1.4 SecureRandom 与编码工具

- 随机源：`java.security.SecureRandom` 单例（Crypto.kt:19），用于 salt/DEK/IV/contentKey/密码生成
  （Crypto.kt:21；PasswordGenerator.kt:25）。Rust 用 `rand::rngs::OsRng`。
- Hex：`toHex = bytes.joinToString("") { "%02X".format(it) }`（Crypto.kt:64）——**大写、两位补零、
  无分隔符**。仅用于 HIBP（HibpClient.kt:33）。Rust：`format!("{:02X}", b)`。
- Base64：`java.util.Base64.getEncoder()/getDecoder()`（Crypto.kt:66-68）——**标准字母表、带 `=` padding、
  非 URL-safe**。仅用于 `Vault.contentKey`（VaultContainer.kt:39,82,101）。Rust：`base64` crate
  `BASE64_STANDARD`。
- SHA-1：`MessageDigest.getInstance("SHA-1")`（Crypto.kt:62）。**唯一用途是 HIBP k-anonymity
  查询**（HibpClient.kt:33），不用于任何文件格式。
- 清零实践：JVM 侧只对 `PBEKeySpec` 的口令副本 `clearPassword()`（Crypto.kt:30）；`ByteArray`
  从不显式清零（JVM 无法保证）。Rust 侧建议用 `zeroize::Zeroizing` 包住口令与 KEK 缓冲区，
  这是行为上的超集，不影响兼容性。

### 1.5 contentKey（附件内容密钥）

- `Vault.contentKey` 是一个 **base64(标准编码) 的 32 字节随机密钥**，明文存储在保险库 JSON 里
  （本身已被 DEK 加密保护）；为空时惰性生成（VaultContainer.kt:36-42, 80-84, 100-102, 131-133）。
- 它是附件文件（`.sica`）的加密密钥（AttachmentStore.kt:17,30）。
- 换主密码不换 contentKey（DEK 都没换）；`VaultStore.importEncrypted/importPlain` 生成新 DEK
  但 contentKey 原样保留（VaultStore.kt:127-131,137-141）——即导入的库与源库的附件密钥相同。
- 同步时 contentKey 作为「两个库是否同源」的判据（SyncEngine.kt:63-68,112-117）。

---

## 2. 保险库容器格式（VaultContainer.kt）

单文件扩展名 `.sicv`。字节布局（`DataOutputStream` 写入，**所有 int 均为大端 int32**）：

| 偏移 | 内容 | 大小 | 说明 |
|---|---|---|---|
| 0 | magic | 4 | ASCII `SICV`（0x53 0x49 0x43 0x56，VaultContainer.kt:87） |
| 4 | version | 4 | 大端 int32：`2` = payload 先 gzip 再加密（**当前写入版本**）；`1` = payload 直接是 JSON（旧格式，仅读） |
| 8 | iterations | 4 | 大端 int32，读取时校验 `10_000 <= n <= 10_000_000`（VaultContainer.kt:152） |
| 12 | salt_len | 4 | 大端 int32，校验 8 ≤ n ≤ 64（VaultContainer.kt:155） |
| 16 | salt | salt_len | 随机 salt（新建时 16 字节） |
| … | wrapped_dek_len | 4 | 大端 int32，校验 40 ≤ n ≤ 256（VaultContainer.kt:156） |
| … | wrapped_dek | wrapped_dek_len | `IV(12)||GCM(DEK)||tag`，AAD=`SICV-WRAP`，通常 60 字节 |
| … | payload_len | 4 | 大端 int32，校验 1 ≤ n ≤ 64 MiB（`MAX_PAYLOAD_BYTES = 64*1024*1024`，VaultContainer.kt:92,157） |
| … | payload | payload_len | `IV(12)||GCM(明文)||tag`，AAD=`SICV-PAYLOAD`；明文见下方版本说明 |

**版本 1 / 2 的差别只在 payload 的明文部分**（头部布局完全一致）：

- `version = 1`：payload 明文 = `Vault` 的 JSON（UTF-8）。
- `version = 2`：payload 明文 = **gzip(JSON)**，即 `Vault` 的 JSON 先做一遍 gzip（RFC 1952），再交给 AES-GCM。
  gzip 是自描述格式，压缩级别不影响解码，但**两端都必须实现"先解压再解析 JSON"**。
  实测 10k 条目的库从 8.87 MB 降到约 0.55 MB（≈16×）。
- 读取端必须同时接受 1 和 2；写入端当前一律写 2（打开旧库后再次保存即自动升级）。
  桌面端与 Android 端**必须同时升级**：只升级一端会导致另一端读云端文件时报
  `不支持的数据库版本：2`。

- 打开流程（VaultContainer.kt:109-135）：读头 → 用 header 的 salt/iterations 派生 KEK →
  `unwrapDek` 失败 ⇒ `WrongPasswordException("主密码错误，或数据库文件已损坏")`（VaultContainer.kt:17）；
  payload GCM 解密失败同样 ⇒ `WrongPasswordException`（无法区分错密码与损坏，VaultContainer.kt:121-125）；
  头部读取失败 ⇒ `VaultFormatException("无法读取数据库文件：…")`；JSON 解析失败 ⇒
  `VaultFormatException("数据库内容无法解析：…")`（VaultContainer.kt:110-130）。
- 每个 `len` 字段读取时先校验范围，再校验 `size <= available()`（剩余可读字节数），最后才分配数组
  （`readChunk`，VaultContainer.kt:166-171）——防伪造长度撑爆内存。Rust 必须复刻同样的防御。
- payload 明文 = `Vault` 对象的 JSON（UTF-8，无 BOM），序列化配置见第 3 节；`version = 2` 时是这份 JSON 再 gzip 一遍。
- **每次 `toBytes()`/`saveTo()` 都用 DEK 重新加密 JSON，且 IV 全新随机**（VaultContainer.kt:44-64）：
  同一内容两次保存的文件字节不同（只有 magic/version/iterations/salt/wrapped_dek 相同）。
  互操作测试必须解密后比较，不能直接比文件字节。
- `saveTo` 走原子写（`AtomicFiles.writeAtomic`，VaultContainer.kt:66-68，见第 4 节）。
- Rust 映射：`u32::to_be_bytes`/`from_be_bytes`；`aes-gcm` crate，`AesGcm<Aes256, U12>`。

---

## 3. 明文保险库 JSON（Model.kt + AppJson.kt）

### 3.1 序列化配置（AppJson.kt:6-11）

```kotlin
Json {
    ignoreUnknownKeys = true   // 读取时忽略未知字段（向前兼容）
    encodeDefaults = true      // 默认值也全部写出（字段总是齐全）
    prettyPrint = false        // 紧凑单行
    isLenient = false
}
```

- **没有 `@SerialName`**：JSON 字段名 == Kotlin 属性名，全部为源码中的 camelCase
  （如 `templateId`、`labelIds`、`notesLabel`、`notesPosition`、`deviceId`、`contentKey`、
  `lastSyncedAt`）。Rust 侧**不要**加 `rename_all = "camelCase"`——直接用字段原名即可
  （已经是 camelCase；逐字段核对下表）。
- 枚举按**变体名**序列化（kotlinx 默认），即 `"ACTIVE"`、`"PASSWORD"` 等大写字符串。
- 可空字段为 `null` 时照常写出 `"deletedAt": null`（serde `Option<T>` 默认行为一致）。
- 时间戳：`Long` 毫秒（epoch millis，UTC），JSON number（非字符串）。
- 默认值生成器：`newId() = UUID.randomUUID().toString()`（小写带连字符，Model.kt:6）；
  `now() = System.currentTimeMillis()`（Model.kt:8）。**解码时缺字段才用默认值；编码总是全写**。

### 3.2 类型逐一列出

**`ItemStatus`**（Model.kt:11-15）：`"ACTIVE"` | `"ARCHIVED"` | `"TRASHED"`。

**`FieldType`**（Model.kt:18-27）：`"TEXT"` | `"PASSWORD"` | `"SECRET"` | `"URL"` | `"EMAIL"` |
`"PHONE"` | `"MULTILINE"`。`SECRET` 与 PASSWORD 一样加密显示，但**不参与**弱密码/重复/泄露检查
（Model.kt:21）。

**`Field`**（Model.kt:30-36）：

| 字段 | 类型 | 默认 |
|---|---|---|
| `id` | String | 新 UUID |
| `label` | String | 必填 |
| `value` | String | `""` |
| `type` | FieldType | `TEXT` |
| `sensitive` | Boolean | `type == PASSWORD \|\| type == SECRET` |

**`AttachmentRef`**（Model.kt:39-45）：`id`(新 UUID)、`name`(必填)、`mime`(默认 `"application/octet-stream"`)、
`size`(Long, 明文字节数, 默认 0)、`addedAt`(Long ms, 默认 now)。

**`Label`**（Model.kt:48-55）：`id`(新 UUID)、`name`(必填)、`color`(Int, ARGB, 默认
`0xFF0F766E.toInt()` = **-15763858**，JSON 里是负数)、`sortOrder`(Int, 默认 0, 越小越靠前)。

**`VaultItem`**（Model.kt:58-81）：

| 字段 | 类型 | 默认 |
|---|---|---|
| `id` | String | 新 UUID |
| `templateId` | String | `"login"` |
| `title` | String | `""` |
| `fields` | [Field] | `[]` |
| `notes` | String | `""` |
| `notesLabel` | String | `"备注"` |
| `notesType` | FieldType | `MULTILINE` |
| `notesPosition` | Int | `-1`（排在所有字段之后） |
| `url` | String | `""` |
| `labelIds` | [String] | `[]`（引用 `Label.id`，顺序有意义） |
| `favorite` | Boolean | `false` |
| `status` | ItemStatus | `ACTIVE` |
| `createdAt` | Long(ms) | now |
| `updatedAt` | Long(ms) | now |
| `deletedAt` | Long? | `null` |
| `attachments` | [AttachmentRef] | `[]` |
| `iconKey` | String | `""`（预设图标 id：letter/key/card/chat/work/custom；空=模板默认） |
| `iconData` | String | `""`（iconKey=="custom" 时为 base64 的方形 PNG，可能带 `data:image/png;base64,` 前缀） |

派生属性（不序列化）：`primaryPassword` = 第一个非空 PASSWORD 字段值（Model.kt:82-83）；
`primaryUsername` 按「账号/帐号/账户」→「用户名/登录名/登录账号」→「邮箱/电子邮件/Email/e-mail」→
模糊 contains → 「卡号/证件号」顺序匹配非空字段（Model.kt:85-103）。
`copyAsConflictCopy()`：新 id、title 未命名时补 `"未命名"` 后追加 `"（冲突副本）"`、favorite=false、
status=ACTIVE、createdAt/updatedAt=now、deletedAt=null、字段全部换新 id（Model.kt:110-121）。
`copyAsDuplicate()`：title 后缀 `" 副本"`，同样重造 id（Model.kt:123-134）。

**`Tombstone`**（Model.kt:143-147）：`id`(String 必填)、`deletedAt`(Long ms, 默认 now)。
永久删除条目后留墓碑防止云端复活；默认保留 90 天
`TOMBSTONE_TTL_MS = 90L*24*60*60*1000`（Model.kt:199），`purgeOldTombstones` 清理
`deletedAt < now - TTL` 的墓碑（Model.kt:189-191）。

**`Vault`**（Model.kt:150-163）：

| 字段 | 类型 | 默认 |
|---|---|---|
| `schema` | Int | `1` |
| `id` | String | 新 UUID（同时是本地文件名与附件目录名） |
| `name` | String | `"我的密码库"` |
| `items` | [VaultItem] | `[]` |
| `labels` | [Label] | `[]` |
| `createdAt` | Long(ms) | now |
| `updatedAt` | Long(ms) | now |
| `revision` | Long | `0`（`touch()` 时 +1，合并时取两侧最大 +1；不参与冲突判定） |
| `lastSyncedAt` | Long | `0`（同步判定的基准时间，见第 8 节） |
| `deviceId` | String | 新 UUID（下载云端库时保留本机的 deviceId） |
| `contentKey` | String | `""`（base64，见 1.5） |
| `tombstones` | [Tombstone] | `[]` |

`touch()`：`updatedAt = now(); revision += 1`（Model.kt:193-196）。

**`VaultMeta`**（Model.kt:204-211，明文、未加密，存于 index.json）：`id`(必填)、`name`、
`fileName`(必填)、`createdAt`(默认 now)、`lastOpenedAt`(默认 0)、`lastSyncedAt`(默认 0)。

**`VaultIndex`**（Model.kt:214-216）：`vaults: [VaultMeta]`。

完整示例（encodeDefaults 后的形态，字段顺序即声明顺序）：

```json
{"schema":1,"id":"3f2b...-uuid","name":"我的密码库","items":[{"id":"...","templateId":"login","title":"GitHub","fields":[{"id":"...","label":"密码","value":"s3cret","type":"PASSWORD","sensitive":true}],"notes":"","notesLabel":"备注","notesType":"MULTILINE","notesPosition":-1,"url":"https://github.com","labelIds":[],"favorite":false,"status":"ACTIVE","createdAt":1730000000000,"updatedAt":1730000000000,"deletedAt":null,"attachments":[],"iconKey":"","iconData":""}],"labels":[{"id":"...","name":"工作","color":-15763858,"sortOrder":0}],"createdAt":1730000000000,"updatedAt":1730000000000,"revision":3,"lastSyncedAt":1730000100000,"deviceId":"...","contentKey":"BASE64-32-BYTES","tombstones":[]}
```

`VaultCodec.toJson/fromJson` 就是上述 JSON 的直接编解码（VaultCodec.kt:12-16）。

---

## 4. 文件布局（VaultStore.kt + AtomicFiles.kt）

### 4.1 目录结构

Android 端根目录 = `context.filesDir/data`（AppContainer.kt:11），桌面端可自选根目录但保持同样结构：

```
<root>/
  vaults/
    index.json                  # VaultIndex（明文 JSON）
    <vault-uuid>.sicv           # 每个保险库一个加密容器
  attachments/
    <vault-uuid>/               # 按 vault.id 分目录（VaultStore.kt:28）
      <attachment-uuid>.sica    # 加密附件（见第 6 节）
```

- `indexFile = vaults/index.json`（VaultStore.kt:19）。
- 库文件名 = `"${vault.id}.sicv"`（VaultStore.kt:65,147）；`list()` 扫描 `*.sicv`，把不在 index
  里的文件收编：`id = 文件名去掉 .sicv 后缀`、`name = "密码库"`、`createdAt = 文件 mtime`、
  `lastOpenedAt = 0`；同时剔除文件已不存在的 index 条目；结果按 `lastOpenedAt` **降序**（VaultStore.kt:31-59）。
  注意收编时的 id 可能与文件内 `Vault.id` 不一致（见第 15 节）。
- `open`/`save` 更新 index 中对应条目的 `name` 与 `lastOpenedAt`；`updateMeta` 按 id 整体替换（VaultStore.kt:81-113）。
- `delete`：删 `.sicv` + 递归删 `attachments/<vaultId>/` + 从 index 移除（VaultStore.kt:116-122）。
- `importEncrypted(bytes, password, newName)` / `importPlain(vault, password, newName)`：
  以**新 UUID** 建库（id 变化 ⇒ 文件名、附件目录都变），name 用 newName（空则沿用原名），
  `updatedAt = now`；条目/标签/墓碑/contentKey 原样带过来（VaultStore.kt:125-160）。
- 锁：所有公开方法 `@Synchronized`（JVM 对象监视器）——**只是进程内互斥，没有跨进程文件锁**。
  Rust 端可用进程内 Mutex 等价实现；不必也不应引入文件锁（Android 侧没有）。

### 4.2 原子写（AtomicFiles.kt:20-56）

`writeAtomic(path, bytes)`：

1. 确保父目录存在（`Files.createDirectories`）。
2. 写临时文件 `<目标名>.tmp`（CREATE + TRUNCATE_EXISTING）。
3. 最多 3 次尝试：先试 `ATOMIC_MOVE + REPLACE_EXISTING`，失败退回普通 `REPLACE_EXISTING` move；
   第 1、2 次失败后 `Thread.sleep(30 * (attempt+1))`（30ms、60ms）退避。
4. 全部失败后兜底：`copy(REPLACE_EXISTING)` + `deleteIfExists(tmp)`（不依赖 rename）。
5. 兜底也失败则抛出最后一次异常。

`index.json` 与 `.sicv` 均通过 `writeAtomic` 落盘（VaultStore.kt:174-180；VaultContainer.kt:66-68）。

### 4.3 同步结果类型（SyncEngine.kt:29-37）

`SyncResult`（Kotlin data class）字段——**调用方必须持久化 `container`**（下载/合并后是新实例，
SyncEngine.kt:30-31 注释；Android 调用方随后 `container.saveTo(vaultFile)` + `updateMeta`
（AppViewModel.kt:1069-1073））：

| 字段 | 类型 | 说明 |
|---|---|---|
| `container` | VaultContainer | 同步后的容器（可能是新实例） |
| `direction` | SyncDirection | `NONE` / `UPLOADED` / `DOWNLOADED` / `MERGED` / `CONFLICT`（SyncEngine.kt:10-16） |
| `conflicts` | Int | 冲突副本数（DETECT 时恒为 1） |
| `attachmentsUploaded` | Int | |
| `attachmentsDownloaded` | Int | |
| `message` | String | 面向用户的中文文案，见第 8 节逐条列出 |

---

## 5. 备份格式（BackupManager.kt）

- **不是 zip、不二次加密**：备份 = 直接复制 `.sicv` 文件（`Files.copy(REPLACE_EXISTING)`，
  BackupManager.kt:27）。`.sicv` 本身已加密。
- 目录：Android 端 `context.filesDir/backups`（AppContainer.kt:12）；`BackupManager(root)` 构造时建目录。
- 文件名：`"<safeName>-<yyyyMMdd-HHmmss>.sicv"`（BackupManager.kt:24-26）：
  - 时间戳：`SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US)` 本地时间。
  - safeName：vault 名中所有不匹配 `[^\p{L}\p{N}_-]` 的字符替换为 `_`（正则 `Regex("[^\\p{L}\\p{N}_-]")`），
    截断 40 字符，全空则 `"vault"`。即保留所有 Unicode 字母数字、`_`、`-`。
- `list()`：目录下 `*.sicv`，`createdAt = 文件 mtime`、`size = 文件大小`，按 createdAt **降序**；
  单个文件读取失败则跳过（BackupManager.kt:31-47）。
- `restore(backup, target)`：普通 `Files.copy(REPLACE_EXISTING)` **覆盖**目标（非原子，BackupManager.kt:49-52）。
- `prune(keep = 10)`：按 list 顺序保留前 `keep` 个，删除其余（失败忽略）。Android 在每次备份后
  调 `prune(10)`（AppViewModel.kt:1111-1112, 1575-1576）。
- **附件不进备份**：备份只含单个 `.sicv`。跨端恢复时附件需靠 WebDAV `attachments/` 目录补齐
  （第 8 节 syncAttachments）或另行拷贝。
- 恢复流程（Android）：`restore(info.path, vaultFile(meta))` 直接覆盖当前库文件（AppViewModel.kt:1138），
  下次打开即为备份内容；`lastSyncedAt` 随备份内容一起被带回旧值——恢复后第一次同步大概率
  走「本地已变」路径覆盖云端（见第 8 节）。

---

## 6. 附件（AttachmentStore.kt）

- 文件名：`<attachment-uuid>.sica`，位于 `attachments/<vault-uuid>/`（AttachmentStore.kt:23；目录由
  VaultStore.attachmentsDir 提供）。
- 字节布局：
  - magic 4 字节 ASCII `SICA`（0x53 0x49 0x43 0x41，AttachmentStore.kt:80）
  - version 1 字节，值 `1`（AttachmentStore.kt:81）
  - 其余：AES-GCM blob = `IV(12) || ciphertext || tag(16)`；密钥 = `Vault.contentKey`（32 字节），
    **AAD = 附件 id 的 UTF-8 字节**（AttachmentStore.kt:30,47）。
- `add(input, name, mime, maxBytes = 10 MiB)`：读入内存（超限抛
  `IllegalArgumentException("附件不能超过 N MB")`，N = maxBytes/1024/1024，AttachmentStore.kt:73），
  加密写盘，返回 `AttachmentRef(id, name, mime, size = 明文大小)`（AttachmentStore.kt:27-37）。
- `read(ref)`：校验 magic、version（不符抛 `IllegalStateException("附件格式无效")` /
  `("不支持的附件版本")`）后解密（AttachmentStore.kt:39-48）。
- `readRaw(id)/writeRaw(id, bytes)`：**明文透传**整文件字节（不加不解密），供同步时原样上传/下载
  （AttachmentStore.kt:50-55；SyncEngine.kt:231,234）——同步的附件字节必须与 Android 端逐字节一致。
- MIME 类型只存在 `AttachmentRef.mime`（JSON 里），**不写入 `.sica` 文件**。
- 写盘同样走 `AtomicFiles.writeAtomic`。

---

## 7. WebDAV 协议细节（WebDavClient.kt）

### 7.1 配置（WebDavConfig，@Serializable，WebDavClient.kt:28-57）

| 字段 | 默认 | 说明 |
|---|---|---|
| `baseUrl` | `""` | 以 `http://` 或 `https://` 开头才算已配置（`isConfigured`） |
| `username` | `""` | |
| `password` | `""` | |
| `remoteDir` | `"SafePassword"` | 服务器上的集合目录名 |
| `enabled` | `false` | |

- `normalizedBaseUrl()`：trim 后去尾部 `/` 再补一个 `/`（如 `https://dav.example.com/`）（:36）。
- `normalizedDir()`：trim 后去掉首尾 `/`，空则回落 `"SafePassword"`（:38）。
- **`remotePath(vaultKey) = "<remoteDir>/SafePassword.spv"` —— 参数被忽略！**（:40）
  即一个账号下所有库共用同一个远端文件名 `SafePassword.spv`。桌面端必须跟随此行为才能与
  Android 互通（详见第 15 节歧义 A）。
- `attachmentsDir(vaultKey) = "<remoteDir>/attachments/<vaultKey>"`（:42）。
- `vaultKey(name)`（:48-53）：name trim → 小写 → 所有 `[^a-z0-9\u4e00-\u9fa5]+`（非 ASCII 字母数字、
  非 CJK）替换为 `-` → trim `-` → 截断 64 字符 → 空则 `"vault"`。**保留中文**（\u4e00-\u9fa5）。
  SyncEngine 用 `vaultKey(vaultName.ifBlank{vaultId})`（SyncEngine.kt:48）。

### 7.2 HTTP 客户端行为

- 认证：HTTP **Basic**（ktor `Auth`+`basic` 插件，:82-90）。凭据**抢占式**只发给配置的 host
  （`Url(baseUrl).host`，大小写不敏感比较，:77,87-89）——历史 bug：曾无条件抢占发送，3xx 跳转到
  第三方主机会泄露凭据（:73-76 注释）。跟随重定向开启（:81）。注意：401 challenge 触发的补发
  不区分主机（ktor 插件行为），Rust 端可选择加固，但不影响正常服务器互通。
- 超时（:92-96）：request 60_000ms、connect 20_000ms、socket 60_000ms。
- `expectSuccess = false`：状态码自行处理（:80）。
- User-Agent（仅 GET，:123）：`SafePassword-Android/1.4`。桌面端可改 UA，但服务器白名单场景需注意。
- URL 构造（:234-238）：`URLBuilder(baseUrl)` + 逐段 `appendPathSegments(segment)`——每段按 UTF-8
  百分号编码（vaultKey 可能含中文）。空段被丢弃。

### 7.3 各方法语义

**`test()`**（:99-115）：对 baseUrl 发 `PROPFIND`，`Depth: 0`，`Content-Type: application/xml`，
请求体（:241-242）：

```xml
<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:resourcetype/></D:prop></D:propfind>
```

2xx=成功；401/403=「认证失败…」；404=「服务器地址或远程目录不存在」；其他=「连接失败（HTTP n）：…」（body 前 120 字符）。
**响应 XML 从不解析——客户端根本没有目录列举功能**；PROPFIND 仅作连通性测试。

**`get(path)`**（:117-161）：最多 2 次尝试。`Accept: */*`。返回值/异常：

| 状态 | 行为 |
|---|---|
| 404 | 返回 `null`（视为文件不存在） |
| 409 | 坚果云等对未创建的库返回 409；做一次 HEAD `exists` 探测：HEAD 非 2xx → 视为不存在返回 `null`；HEAD 2xx → 抛 `WebDavException("下载失败（HTTP 409，路径 …）…同名目录…", 409)`（:128-139）。测试 `WebDavClientTest.kt:24-71` 覆盖这两种情况 |
| 2xx 且 body 非空 | 返回字节 |
| 2xx 且 body 为空 | 抛 `WebDavException("下载失败：服务器返回了空文件（path）")`（:142-144） |
| 5xx 且是第 1 次尝试 | `delay(600ms)` 后重试（:147） |
| 5xx 第 2 次 | 落入 else 抛「下载失败（HTTP …）」 |
| 401/403 | 抛「WebDAV 认证失败…」（:148-149） |
| 其他 | 抛「下载失败（HTTP n，重定向到 Location，路径 …）：body 前 120 字」（:150-157） |

（:160 的兜底「服务器暂时不可用」在当前控制流下不可达，属防御性代码。）

**`exists(path)`**（:163-166）：HEAD，2xx ⇒ true（其余一律 false，不区分 401 等）。

**`stat(path)`**（:172-184）：HEAD；非 2xx ⇒ `null`；否则 `RemoteStat{ lastModified: Last-Modified
解析为 epoch ms（RFC_1123_DATE_TIME，失败尝试 Instant.parse，再失败 0）, size: Content-Length
（解析失败 -1）, etag: ETag 头原样（无则 null） }`（:62-66）。`lastModified = 0` 表示服务器没给，
调用方视为「无法判断是否变化」。**目前只有 Android UI 层用它做 HEAD 级变化检测**
（AppViewModel.kt:1018-1031），SyncEngine 不用 etag。

**`put(path, bytes)`**（:186-199）：先 `ensureParent` 再 PUT，`Content-Type: application/octet-stream`；
非 2xx 抛「上传失败（HTTP n）：body 前 120」。

**`ensureParent(path)`**（:208-215）：把路径去掉最后一段，逐级 `MKCOL`（`a`、`a/b`、…）。

**`mkcol(path)`**（:217-228）：2xx=成功；**405=成功（已存在）**；401/403=「没有创建目录的权限」；
其他抛错。

**`delete(path)`**（:201-206）：DELETE；2xx 或 **404 视为成功**；其他抛错。

**不存在 MOVE/COPY**：客户端从不移动远端文件。

---

## 8. 同步与合并（SyncEngine.kt + VaultMerger.kt）

### 8.1 `SyncEngine` 构造

`SyncEngine(webDav, config, attachmentStore, vaultId, vaultName)`；`remoteKey = config.vaultKey(
vaultName.ifBlank { vaultId })`（SyncEngine.kt:48）。远端主文件路径恒为
`<remoteDir>/SafePassword.spv`（见 7.1）。

### 8.2 `sync(local, password, conflictStrategy = MERGE)` 逐步流程（SyncEngine.kt:84-218）

1. `GET remotePath`。
2. **远端不存在（null）**——首次同步/云端被删：`ensureContentKey()` → `lastSyncedAt = now()` →
   `touch()`（updatedAt=now、revision+=1）→ `PUT local.toBytes()` → `syncAttachments` →
   `UPLOADED`，message `"已上传到 WebDAV"`（:91-104）。
3. 远端存在：用**同一个主密码** `VaultContainer.open(remoteBytes, password)`；任何异常 ⇒
   `SyncException("无法用当前主密码解密远端数据库，请确认两端使用同一主密码")`（:106-110）。
4. **同源校验**：两侧 contentKey 均非空且不相等 ⇒
   `SyncException("远端数据库与本地数据库不是同一来源（内容密钥不一致），已停止同步")`（:112-117）。
5. 变更判定（**纯时间戳，无 etag**，:119-121）：
   - `last = local.vault.lastSyncedAt`
   - `localChanged = local.vault.updatedAt > last`
   - `remoteChanged = remote.vault.updatedAt > last`
6. 四种情况：
   - **都未变**：仍执行 `syncAttachments`；`NONE`，message `"已是最新状态"`（:124-133）。
   - **只有远端变**：`downloaded = remote.vault.copy(lastSyncedAt = now(), deviceId = 本机 deviceId)`
     → `local.withVault(downloaded)`（**保留本机 DEK/salt/wrappedDek，仅替换 vault JSON**，
     VaultContainer.kt:77-78）→ `DOWNLOADED`，message `"已下载远端更新"`（:135-149）。
   - **只有本地变**：`lastSyncedAt = now()` → `PUT local.toBytes()` → `UPLOADED`，
     message `"已上传本地更新"`（:151-162）。
   - **两边都变**——按 `conflictStrategy`（SyncEngine.kt:18-27）：
     - `DETECT`：不上传不下传，`CONFLICT`、`conflicts = 1`，message
       `"检测到本地与云端都有新修改，请选择处理方式"`（:165-172）。
     - `KEEP_LOCAL`：`lastSyncedAt = now()` → PUT → `UPLOADED`，message `"已保留本地版本并覆盖云端"`（:173-184）。
     - `KEEP_REMOTE`：同「只有远端变」下载路径，`DOWNLOADED`，message `"已使用云端版本覆盖本地"`（:185-199）。
     - `MERGE`：`VaultMerger.merge(local.vault, remote.vault, last)` → `local.withVault(merged)`
       → `PUT updated.toBytes()` → `MERGED`，`conflicts = merge.conflicts`，message
       `"已合并，产生 N 个冲突副本"` 或 `"已合并本地与远端更新"`（:200-216）。
7. 调用方必须持久化返回的 `container`（4.3 节）。

### 8.3 `pullRemote(local, password)`（强制拉取，:54-82）

GET 远端（null ⇒ `SyncException("云端没有找到密码库文件，无法拉取")`）→ 解密（同上错误）→
contentKey 校验（message 尾部为「已停止拉取」）→ `remote.vault.copy(lastSyncedAt=now(),
deviceId=本机)` → `local.withVault` → syncAttachments → `DOWNLOADED`，message `"已用云端数据覆盖本地"`。
**无视时间戳**，用于 UI 的「拉取云端数据」按钮。

### 8.4 附件同步（syncAttachments，:220-240）

- 收集所有条目 attachments 按 id 去重（`distinctBy { it.id }`）；无附件直接 `(0, 0)`。
- 远端目录 `<remoteDir>/attachments/<remoteKey>`，每个附件远端路径 `$dir/<id>.sica`。
- 逐个处理（**无并发、无删除传播、无版本冲突处理**）：
  - 本地缺失 → GET（null 则跳过）→ `writeRaw` 原样落盘 → `downloaded++`。
  - 本地存在但远端 HEAD 非 2xx → `readRaw` 原样 PUT → `uploaded++`。
- 返回 `(uploaded, downloaded)` 计入 SyncResult。

### 8.5 合并算法（VaultMerger.kt:19-88，纯函数 `merge(local, remote, lastSyncedAt)`）

结果类型 `VaultMerger.Result(vault: Vault, conflicts: Int)`（:17；Kotlin `internal`，Rust 直接公开即可）。

1. **墓碑并集**：`local.tombstones + remote.tombstones`，同 id 保留 `deletedAt` 更大者（:24-30）。
2. 遍历 `localMap.keys + remoteMap.keys`（先本地条目顺序，后仅远端条目，:35）：
   - 有墓碑时（:40-49）：
     - `localGone = localItem == null || tombstone.deletedAt >= localItem.updatedAt`
     - `remoteGone = remoteItem == null || tombstone.deletedAt >= remoteItem.updatedAt`
     - 两者都 gone ⇒ **跳过（条目保持删除，不复活）**。
     - 否则（删除后另一端又编辑过）⇒ 撤销墓碑（从墓碑表移除），条目复活。
   - 仅一侧有条目 ⇒ 取该侧（:52-53）。
   - 两侧都有（:54-70）：
     - `localNew = localItem.updatedAt > lastSyncedAt`；`remoteNew` 同理。
     - **冲突条件**：`localNew && remoteNew && updatedAt 不等 && !sameContent(a, b)`：
       胜者 = `updatedAt` 更大者（相等时取 local），败者追加 `copyAsConflictCopy()`（新 id、
       title 加 `"（冲突副本）"`），`conflicts++`。
     - 否则取 `updatedAt` 更大者（相等取 local）。
   - `sameContent`（:90-97）：`title、fields（含 Field.id 的整体相等）、notes、url、labelIds（顺序敏感）、
     status、attachments（列表相等）` 七项全等——**注意 favorite/notesLabel/notesType/notesPosition/
     iconKey 等不在比较之列**。
3. **标签合并**：先放全部 local.labels，再对 remote.labels 按 id `putIfAbsent`（**同 id 本地优先**）（:74-76）。
4. 结果 = `local.copy(items=merged, labels=…, tombstones=…, updatedAt=now(), lastSyncedAt=now(),
   revision = max(local.revision, remote.revision) + 1)`，随后 `purgeOldTombstones()`
   （删除 `deletedAt < now - 90 天` 的墓碑）（:78-87）。
5. **name/schema/id/contentKey 等取 local**（`local.copy` 未覆盖这些字段）。

### 8.6 时钟偏移

**没有任何时钟偏移处理**：变更判定与合并全部依赖两台设备的墙钟 `updatedAt`/`lastSyncedAt`
直接比较。某台设备时钟快会导致其修改总是「更新」，时钟慢则可能被误判为「未变」而漏传。
Rust 端如需改进（如向量时钟）会偏离 Android 行为——建议先保持一致，另行记录。

Android UI 层的廉价预检（AppViewModel.kt:1008-1035）：定时/回前台时对远端 `stat()`（HEAD），
`lastModified <= 0` ⇒ 保守认为有变化；首次检查 ⇒ `stat.lastModified > vault.lastSyncedAt`；
否则与本地缓存的 `lastRemoteMtime` 比较；同步后重新 stat 更新缓存。这只是省流量优化，
真正判定仍在 `sync()` 内。

---

## 9. 安全分析（SecurityAnalyzer.kt）

输入：整个 `Vault` + 调用方提供的 `breachedCounts: Map<条目id, 泄露次数>`（默认空 map）。只分析
`status == ACTIVE` 的条目（:19）。Android 端取前 50 个不同密码逐一查 HIBP 后填充该 map
（AppViewModel.kt:1356-1367）。

规则（`OLD_THRESHOLD_MILLIS = 180 天`，:16）：

| 规则 | 判据 | 出处 |
|---|---|---|
| weak（弱密码） | `primaryPassword` 存在且 `PasswordStrength.evaluate(pw).score <= 1` | :23-25 |
| reused（重复密码） | 同一密码字符串在 ACTIVE 条目中出现次数 `> 1`（精确字符串匹配，区分大小写） | :27-29 |
| breached（已泄露） | `breachedCounts[item.id] > 0` | :31 |
| old（陈旧） | `now - item.updatedAt > 180 天` | :32-33 |

- `primaryPassword` = 第一个 PASSWORD 类型且值非空的字段（Model.kt:82-83）；SECRET 字段不参与。
- 输出 `SecurityReport{ total: Int, weak/reused/breached/old: List<VaultItem> }`（:7-13）；
  weak/reused 各自 `distinctBy { id }`（:37-38）。
- `passwordIndex(vault): Map<密码, List<条目id>>`——ACTIVE 条目的 primaryPassword → id 列表
  （:44-51），供 HIBP 批量查询去重。

---

## 10. 密码生成（PasswordGenerator.kt）与强度（PasswordStrength.kt）

### 10.1 字符集常量（PasswordGenerator.kt:16-23）

| 常量 | 值 | 说明 |
|---|---|---|
| `LOWER` | `abcdefghijkmnopqrstuvwxyz` | 25 字符，**无 `l`** |
| `LOWER_FULL` | `abcdefghijklmnopqrstuvwxyz` | 26 |
| `UPPER` | `ABCDEFGHJKLMNPQRSTUVWXYZ` | 25，**无 `I`、`O`** |
| `UPPER_FULL` | `ABCDEFGHIJKLMNOPQRSTUVWXYZ` | 26 |
| `DIGITS` | `23456789` | 8，**无 `0`、`1`** |
| `DIGITS_FULL` | `0123456789` | 10 |
| `SYMBOLS` | `!@#%^&*()-_=+[]{};:,.?` | 21 |
| `AMBIGUOUS` | `O0Il1\|`'"` | 9 个易混字符 |

- `excludeAmbiguous=true`（默认）⇒ 用无歧义集；false ⇒ 用 full 集，且 **symbols 变为
  `SYMBOLS + AMBIGUOUS`**（:28-31）。

### 10.2 `GeneratorOptions`（:5-13）

默认：`length = 20`、`lowercase/uppercase/digits/symbols = true`、`excludeAmbiguous = true`、
`requireEach = true`。

### 10.3 `generate(options)` 算法（:27-60）

1. 依选项收集激活字符集（未激活任何集 ⇒ 兜底只用 lower）。
2. `length = coerceIn(4, 128)`；`requireEach && length < 激活集数` ⇒ `length = 激活集数`。
3. `requireEach` ⇒ 先从每个激活集各取 1 字符（`random.nextInt(集长度)`）。
4. 从并集 `all` 随机填充到 length。
5. Fisher–Yates 逆序洗牌（`random.nextInt(i+1)`）。
6. PRNG = `java.security.SecureRandom`（:25）。Rust 用 `OsRng`；**无法也不需要与 Android 位级一致**
   （随机输出只需统计性质一致）。

### 10.4 `passphrase(wordCount=4, separator="-", capitalize=true, includeNumber=true)`（:62-78）

- `wordCount = coerceIn(3, 8)`。
- 词表：120 个英文单词（:80-98，全小写，见下）。
- 复制词表 → `Collections.shuffle(pool, SecureRandom)` → 取前 count 个 → capitalize 时首字母大写 →
  `includeNumber` 时追加一个 `random.nextInt(10, 100)`（即 **10–99**）作为最后一个「词」→
  `joinToString(separator)`。示例：`Apple-Bridge-Cloud-42`。

词表（120 词，按源码顺序）：

```
apple arrow atlas autumn banjo beach berry birch blade bloom breeze bridge brook
cabin candle cedar chalk cherry cloud clover coast comet coral crane creek crown
dawn delta dove dune eagle earth ember fable fern field flame flint forest frost
garden glade glass globe gold grain grape grove harbor hazel heron hill honey
island ivory jade jasmine juniper kelp lake lantern leaf lemon lily linen lotus
lunar maple marble meadow melon mint moon moss mountain nest night north oak ocean
olive onyx opal orange orchid otter pearl pebble pine plum pond poppy prairie
quartz rain raven reed ridge river robin rose sail sand scarlet sea shell silver
sky snow solar sparrow spring spruce star stone storm stream summer sun swan
thistle tide timber topaz trail tulip valley violet wave willow wind winter wolf
wood wren zephyr
```

### 10.5 强度评估（PasswordStrength.kt）

输入 UTF-8/UTF-16 字符串，输出 `StrengthResult{ score: Int, label: String, entropyBits: Double, crackTime: String }`。

1. 空白 ⇒ `(0, "很弱", 0.0, "瞬间")`（:24-26）。
2. `lowercase()` 后命中内置 42 个常见弱口令集合 ⇒ 同上（:27-29）。集合：
   `123456, 123456789, qwerty, password, 111111, 12345678, abc123, 1234567, password1, 12345,
   1234567890, iloveyou, 000000, 123123, admin, letmein, welcome, monkey, dragon, sunshine,
   princess, football, baseball, master, shadow, superman, qazwsx, 666666, 888888, woaini,
   5201314, a123456, 123qwe, zxcvbnm, asdfgh, qwertyuiop, 1qaz2wsx, passw0rd, p@ssw0rd, root,
   guest, test`（:14-21）。
3. 字符池 `pool`：出现小写 +26、大写 +26、数字 +10、**任一非字母数字字符** +33；`pool==0 ⇒ 1`
   （:31-36）。
4. `entropy = length * log2(pool)`，再依次乘（累乘，:38-43）：
   - `length < 6` ⇒ ×0.6
   - `length >= 8 && 全数字` ⇒ ×0.55
   - 存在 ≥4 连续相同字符（`hasRun`，:61-68）⇒ ×0.75
   - 存在 ≥4 长度的连续升/降序（相邻码点差 ±1，`hasSequence`，:70-81）⇒ ×0.85
5. 分数阈值（:44-50）：`<28 → 0`、`<36 → 1`、`<60 → 2`、`<80 → 3`、否则 `4`；
   标签：`很弱/弱/一般/强/很强`（:51-57）。
6. 破解时间（:83-95）：`guesses = 2^(entropy-1)`，`seconds = guesses / 1e10`（100 亿次/秒）：
   `<1 → "瞬间"`、`<60 → "N 秒"`、`<3600 → "N 分钟"`、`<86400 → "N 小时"`、`<31_536_000 → "N 天"`、
   `<31_536_000*1000 → "N 年"`、否则 `"超过 1000 年"`（数值均 `toLong()` 截断）。

注：`hasRun`/`hasSequence`/池判定都基于 Kotlin `Char`（UTF-16 码元）。非 ASCII 口令在 Rust 的
`char`（Unicode 标量）语义下结果可能有细微差异；如需完全一致请按 UTF-16 码元处理。

---

## 11. HIBP（HibpClient.kt）

超时：request 30s / connect 15s / socket 30s（:21-25）。`expectSuccess = false`。**core 内无缓存、
无限速**（调用方负责节流——Android 一次最多查 50 个不同密码）。

### 11.1 `passwordCount(password): Int`（k-anonymity，:31-50）

1. 空白 ⇒ 0。
2. `hash = UPPERCASE hex(SHA-1(password UTF-8))`（Crypto.toHex/sha1，:33）。
3. `prefix = hash[0..5)`（5 字符）、`suffix = hash[5..)`。
4. `GET https://api.pwnedpasswords.com/range/<prefix>`，请求头 `Add-Padding: true`（:36-38）。
   **口令本体不离开设备。**
5. 非 200 ⇒ `HibpException("泄露库查询失败（HTTP n）")`。
6. 逐行解析 `SUFFIX:COUNT`（split `:`，2 段，suffix 大小写不敏感比较），命中返回 COUNT，
   未命中返回 0（:42-49）。
7. **无 API Key**（range 接口免费）。

### 11.2 `emailBreaches(email, apiKey): List<String>`（:55-77）

- 空 email ⇒ `[]`；空 apiKey ⇒ `HibpException("邮箱泄露检查需要填写 HIBP API Key")`。
- `GET https://haveibeenpwned.com/api/v3/breachedaccount/<URLEncoder.encode(email,"UTF-8")>?truncateResponse=true`
  （注意 `URLEncoder` 是 form 编码：空格 → `+`）。
- 请求头：`hibp-api-key: <apiKey>`、`user-agent: SafePassword-Android`。
- 响应：200 → JSON 数组，收集每个对象的 `"Name"` 字段（解析失败抛
  `HibpException("无法解析 HIBP 响应")`）；404 → `[]`（无泄露）；401 → `"HIBP API Key 无效"`；
  429 → `"HIBP 请求过于频繁，请稍后再试"`；其他 → `"邮箱查询失败（HTTP n）"`。

---

## 12. XML 导入/导出（VaultXml.kt）

与移动端/桌面端互通的**明文** XML（不含任何加密；导出的是已解锁库的内容，属敏感文件）。

### 12.1 结构

根元素 `<database>`（匹配大小写不敏感，:43-45）：

```xml
<?xml version='1.0' encoding='UTF-8' standalone='no'?>
<database>
  <label id="1" name="工作"/>
  <card id="c1" title="GitHub" time_stamp="1730000000" template="false" star="true" symbol="key">
    <field name="用户名" type="login">alice</field>
    <field name="密码" type="password">s3cret</field>
    <field name="网址" type="website">https://github.com</field>
    <label_id>1</label_id>
    <notes>个人账号</notes>
    <custom_icon>iVBORw0KGgo=</custom_icon>
  </card>
</database>
```

### 12.2 导出（export，:62-138）

- 输出：UTF-8、带 XML 声明、`INDENT=yes`（:82-88）。
- labels 按列表顺序重编号为 `"1".."n"`（`mapIndexed { index+1 }`，:68），写成
  `<label id="n" name="原名"/>`。
- 只导出 `status == ACTIVE` 的条目（:76-78）。每个条目一个 `<card>`：
  - 属性：`id = item.id`；`title`（空则 `"未命名"`）；`time_stamp = item.updatedAt / 1000`
    （**秒**，:100）；`template = "false"` 恒定（:101）；`star = "true"` 仅当 favorite（:102）；
    `symbol = iconKey` 仅当无自定义图标且 `iconKey` 非空且 != `"custom"`（:134-136）。
  - `<field name=.. type=..>value</field>`：label 为空写 `"自定义字段"`（:113）；label 为「备注」
    （忽略大小写）的字段跳过（:112）。type 由 `exportType` 启发式判定（:148-175）：label 含
    「密码」→`password`、含「卡号」→`card_number`、含「持卡人」→`card_holder`、含「有效期」→`expiry`、
    含 `cvv`→`cvv`、含「银行」→`bank`、含「网络名称」或 `wifi`→`network_name`、含「证件号」→
    `identity_number`、含「用户名/账号/帐号/登录」→`login`、含「邮箱」或类型 EMAIL→`email`、
    含「秘密」→`secret`、含「电话」或类型 PHONE→`phone`、含「网址/url/website」或类型 URL→
    `website`；否则按字段类型 PASSWORD→`password`、SECRET→`secret`、EMAIL→`email`、PHONE→
    `phone`、URL→`website`、其余→`text`。
  - 当 `item.url` 非空且没有任何 URL 类字段时，补一个
    `<field name="website" type="url">item.url</field>`（:104-117）。URL 类字段判定：type==URL 或
    label 为「网址/地址」（忽略大小写）或 label 含 `website`/`url`（忽略大小写）。
  - `<label_id>` 元素（每个去重后的 labelId 对应重编号后的 id）（:118-122）。
  - `<notes>`（非空时）（:123-127）。
  - `<custom_icon>`（非空 iconData 时）：去掉 `base64,` 前缀并删除所有空白字符（:128-133）。

### 12.3 导入（parse，:40-56, 177-258）

返回 `List<XmlCard{ item: VaultItem, labelNames: List<String> }>`（:24-27）——**标签以名字
返回，不带数字 id**，由调用方决定匹配/新建逻辑。

- XML 安全：禁用 DTD（`disallow-doctype-decl`）、外部通用/参数实体、`ACCESS_EXTERNAL_DTD/SCHEMA`
  （均为 runCatching 容错，:309-327）；解析前去掉 BOM（:38, 329）；namespace 不感知（:311）；
  标签名匹配大小写不敏感（:333-343）。
- `<label id name>` 建 `id → name` 映射（空 id/name 丢弃，:47-51）。
- `<card>`：`template` 或 `deleted` 属性为 `"true"`/`"1"`（忽略大小写，:356）⇒ 丢弃（模板/已删卡不导
  入，:178）。
- `<field>`（:185-209）：`type` 小写化；name/value 均 trim；全空跳过。`type=="notes"` 的字段值并入
  noteParts（不作为字段）。第一个 URL 类字段的值提升为 `item.url`。每个字段 `sensitive =
  (type==PASSWORD || type==SECRET)`。
  - `safeType`（:260-279）：`password|cvv|pin → PASSWORD`；`secret → SECRET`；`url|website → URL`；
    `email|e-mail → EMAIL`；`phone|tel → PHONE`；`multiline|notes|note → MULTILINE`；
    `login|username|user → EMAIL`（若 name 含 `email`/「邮箱」）否则 `TEXT`；其他：name 为
    「网址/地址」或含 `url/website`（忽略大小写）⇒ `URL`，否则 `TEXT`。
  - `safeLabel`（:281-307）：`login/username/user` → 含 email/邮箱 ⇒ 「邮箱」；通用名
    （`login/username/user/account/账号/帐号/登录名/登录账号` 或空）⇒ 「用户名」；否则原名。
    `password` → 通用名（`password/pass/pwd/密码`）⇒ 「密码」；`secret` → 「秘密」；
    `url/website` → 「网址」；`email/e-mail` → 「邮箱」；`phone/tel` → 「电话」；
    `card_number → 卡号`、`card_holder → 持卡人`、`expiry|expiration → 有效期`、`cvv → CVV`、
    `bank → 银行`、`network_name|ssid → 网络名称`、`identity_number → 证件号`、
    `notes|note → 备注`；其他 → 原名（空则 value 空时 `""`，否则 type 下划线转空格，仍空 ⇒
    「自定义字段」）。
- `<notes>` 子元素置于 noteParts 最前，最终 `item.notes = noteParts.joinToString("\n\n")`（:211-212, 244）。
- `<label_id>` → 依次映射为标签名（查不到 ⇒ `"标签 <id>"`），去重（:214-217）。
- `time_stamp`：数字且在 `1..9_999_999_999` ⇒ ×1000（当作**秒**）；否则原样（当作毫秒）；否则
  `now()`（:219-221）。
- 模板推断（:223-229）：有「卡号」字段 ⇒ `bank_card`；有「网络名称」⇒ `wifi`；有「证件号」⇒
  `identity`；无字段但有备注 ⇒ `note`；否则 `login`。
- 图标：`<custom_icon>` 优先、其次 `<image>`；剥掉 `data:...;base64,` 前缀与空白（:231-238）；
  `iconKey = image 非空 ? "custom" : symbol 属性`（:250-254）。
- `favorite = star 属性为 true/1`（:247）；createdAt = updatedAt = 解析出的 timestamp（:248-249）。
- **`<file>` 附件节点有意不解析、不保留**（类注释 :34-35）。导入的卡不带附件。

往返行为（`ToolsTest.kt:53-123`）：label 名、favorite、备注（元素与 notes 字段拼接）、自定义图标
base64、`login` 字段名的邮箱识别等都有测试断言，可直接作为 Rust 侧单测蓝本。

### 12.4 附：CSV 导入/导出（VaultCodec.kt，不同格式）

- 导出（:18-31）：表头固定 `标题,用户名,密码,网址,备注,标签`（CRLF 行尾）；跳过 TRASHED
  （ARCHIVED 会导出）；用户名 = 第一个 label ∈ {用户名, 邮箱, 账号} 的字段值；密码 =
  `primaryPassword`；标签 = 标签名 `|` 连接。
- CSV 转义（:81-98）：单元格以 `= + - @ Tab CR` 开头 ⇒ 前置 `'` 防公式注入；含 `, " \n \r` 之一 ⇒
  双引号包裹、内部 `"` 写成 `""`。
- 导入（:33-79）：自解析（支持 `""` 转义、忽略孤立 `\r`、丢弃全空行）；表头归一化
  （小写、去空格去下划线；`title/name/标题/名称 → title`，`username/user/login/账号/用户名/邮箱 →
  username`，`password/pass/密码 → password`，`url/website/网址/链接 → url`，`notes/note/备注/注释
  → notes`）；title/username/password 三列都不存在 ⇒ 返回空；title 空 ⇒ 用 username，再空 ⇒
  `"导入的账号"`；生成 LOGIN 模板条目（用户名 TEXT、密码 PASSWORD+sensitive），createdAt/updatedAt=now。

---

## 13. 模板目录（TemplateCatalog.kt）

模板 id 为字符串常量（:17-23）：`login`、`bank_card`、`email`、`wifi`、`identity`、`note`、`custom`。

| id | 名称 | 描述 | 字段（label : type，※=sensitive 强制 true） |
|---|---|---|---|
| `login` | 登录 | 网站、应用账号 | 账号:TEXT、密码:PASSWORD、网址:URL |
| `bank_card` | 银行卡 | 银行卡、信用卡 | 卡号:TEXT※、持卡人:TEXT、有效期:TEXT、CVV:PASSWORD、银行:TEXT |
| `email` | 邮箱 | 电子邮箱账号 | 邮箱:EMAIL、密码:PASSWORD、服务器:TEXT |
| `wifi` | Wi-Fi | 无线网络密码 | 网络名称:TEXT、密码:PASSWORD、加密方式:TEXT |
| `identity` | 证件 | 身份证、护照、驾照 | 姓名:TEXT、证件号:TEXT※、有效期:TEXT、签发机关:TEXT |
| `note` | 安全笔记 | PIN、许可证、机密文本 | （无字段） |
| `custom` | 自定义 | 自由添加字段 | （无字段） |

- `FieldDef.sensitive` 默认 = `type ∈ {PASSWORD, SECRET}`（:6）。
- `find(id)`：未知 id 回落到列表最后一项 `custom`（:73）。
- `createItem(templateId, title)`：按模板字段创建条目（:77-86）。

---

## 14. Rust 移植注意事项

1. **Base64**：标准字母表 + padding（`contentKey`），**不要**用 URL-safe 变体。HIBP 用大写 hex。
2. **PBKDF2**：`pbkdf2` + `hmac` + `sha2` crate，`pbkdf2::pbkdf2_hmac::<Sha256>(password_bytes,
   salt, iterations, &mut okm)`，输出 32 字节。**iterations 必须取文件头存储值**，不是常量
   210000。口令按 UTF-8 字节（见 1.1 的说明，用 T1 向量验证）。
3. **AES-GCM**：`aes-gcm` crate（`Aes256Gcm`，96-bit nonce）。输出布局 `nonce(12) || ct || tag(16)`。
   AAD 必须逐字节一致：`b"SICV-WRAP"`、`b"SICV-PAYLOAD"`、附件 id 的 UTF-8。
4. **大端 int32**：`DataInputStream.readInt()` 是大端。用 `u32::from_be_bytes` / `to_be_bytes`
   （长度字段用 `u32`，读回后做范围校验）。
5. **JSON**：`serde_json` + `serde`。字段名逐个按第 3 节抄写（已是 camelCase，勿再 rename_all）；
   `#[serde(deny_unknown_fields)]` **不要**加（Kotlin 是 ignoreUnknownKeys=true）；Option 字段
   默认就写出 null，与 kotlinx 一致；为缺字段提供 `#[serde(default = ...)]`；枚举用
   `#[derive(Deserialize)]` + 变体名即 wire 值（或显式 `serialize`/`deserialize` 为大写字符串）。
   颜色 `Label.color` 必须是**有符号 i32**（默认 -15763858）。
6. **UTF-16 差异**：Kotlin `Char` 是 UTF-16 码元。JSON 字符串、XML 文本两边都是 UTF-8，无差异；
   但 `PasswordStrength.hasRun/hasSequence`、CSV/label 的 `contains` 等按 Char 迭代的逻辑，
   对 BMP 外字符（emoji 等）Rust 按 `char` 迭代会有细微差异——如需逐位一致请按 UTF-16 码元处理。
7. **UUID**：`uuid` crate v4，`to_string()` 输出小写带连字符，与 `UUID.randomUUID().toString()` 一致。
8. **时间**：全程 epoch millis（i64）；仅 XML 的 `time_stamp` 是秒（导出时 `/1000`，
   导入时启发式 ×1000）。
9. **密码生成**：`rand::rngs::OsRng` + `Rng::gen_range`。注意 Kotlin `random.nextInt(10, 100)`
   是 [10, 100) 即 10–99；`nextInt(n)` 是 [0, n)。Fisher–Yates 从尾部向前。
10. **原子写**：Windows 下 `std::fs::rename` 目标已存在会失败——需 `ReplaceFileW`（或先 remove
    再 rename）并实现同样的重试退避（3 次、30ms/60ms）+ copy 兜底（第 4.2 节）。`.tmp` 后缀一致。
11. **WebDAV**：`reqwest` + 自定义 method（PROPFIND/MKCOL）；PROPFIND body 与头（Depth: 0、
    application/xml）逐字节一致；GET 需处理「2xx 空 body = 错误」与「409 → HEAD 探测」；MKCOL
    405 视为成功；DELETE 404 视为成功；路径段按 UTF-8 百分号编码（中文 vaultKey）。
12. **XML**：`quick-xml` 或 `roxmltree`；必须禁 DTD/外部实体（防 XXE）；标签匹配大小写不敏感；
    导出带 XML 声明与缩进（缩进格式不影响互通，导入方 trim）。
13. **错误类型**：为 `WrongPassword`、`VaultFormat`、`Sync`、`Hibp`、`WebDav(status)` 建立对应
    错误枚举，文案与 Kotlin 一致（UI 显示直接复用）。
14. **清零**：`zeroize` 包裹口令/KEK 缓冲区（超集行为，无兼容影响）。
15. **不要引入的行为**：文件锁、etag 冲突判定、时钟偏移补偿、附件增量校验（md5 等）、CSV/JSON
    的 pretty print——Android 端都没有，加了会造成行为分叉。

---

## 15. 已知歧义 / 需要确认的点

- **A. 远端文件名固定为 `SafePassword.spv`**：`WebDavConfig.remotePath()` 完全忽略 `vaultKey`
  参数（WebDavClient.kt:40），一个 WebDAV 账号下一个远端文件。但 `WebDavClientTest.kt:34` 使用
  `<dir>/<key>.sicv` 这类旧式路径，说明**旧版本**曾用 `<dir>/<key>.sicv`。桌面端应
  跟随**当前** Android 行为（`SafePassword.spv`）才能互通；如桌面端生成别的文件名，两端永远互相
  看不见。`vaultKey` 仅剩附件目录在用（SyncEngine.kt:223）。
- **B. PBKDF2 口令编码**：JCE `PBKDF2WithHmacSHA256` 按 UTF-8 编码口令（无 NUL 终止）是 JDK 实现
  事实，但 core 源码无法直接证明。**必须用 Android 真机生成的 `.sicv` 做 T1 验证**；若口令含
  非 ASCII（中文密码），编码差异会立即暴露。
- **C. 冲突判定无 etag**：SyncEngine 完全用 `updatedAt`/`lastSyncedAt` 时间戳比较；
  `RemoteStat.etag` 被解析但从未消费；`stat()` 只在 Android UI 层做 HEAD 级预检
  （AppViewModel.kt:1018-1031）。任务书里假设的「etag 冲突检测」不存在。
- **D. 无时钟偏移处理**（8.6 节）。
- **E. index.json 收编逻辑**：`list()` 把未知 `.sicv` 的 id 取为文件名后缀，可能与文件内
  `Vault.id` 不一致（VaultStore.kt:46），进而附件目录（按 index 的 id 建）与库内 id 脱节。
  桌面端若自行创建文件，建议文件名严格使用 `Vault.id`。
- **F. 合并比较忽略的字段**：`sameContent` 不比较 `favorite`、`notesLabel`、`notesType`、
  `notesPosition`、`iconKey`、`iconData`（VaultMerger.kt:90-97）——只改这些字段的两侧副本合并时
  取 `updatedAt` 更大者，不产生冲突副本。
- **G. 备份不含附件**（第 5 节）；恢复后附件靠 WebDAV 或手工补齐。
- **H. `get()` 的 5xx 重试**：第二次 5xx 落入通用错误分支抛「下载失败（HTTP n）…body…」，
  :160 的「服务器暂时不可用」文案实际不可达。
- **I. 导入（importEncrypted/Plain）保留 contentKey 但换 DEK**（VaultStore.kt:127-147）：
  导入库与源库附件互通（同 contentKey），但附件目录换成了新 vault id，文件需另行拷贝/同步。
- **J. XML 导出 type 判定是 label 子串启发式**（12.2 节），中文 label 命中规则与原版
  其它客户端的真实 type 集合未必完全一致；往返本实现是自洽的（ToolsTest 有断言）。
- **K. `stat()` 的 `size` 在无 Content-Length 时为 -1，`lastModified` 无头时为 0**——调用方
  必须容忍。
- **L. 合并结果的 `name`/`schema`/`id`/`contentKey` 取本地侧**（VaultMerger.kt:78 用
  `local.copy`）；改名只在一端发生时，另一端下次合并不会带过来。

---

## 16. 互操作测试清单（interop test cases）

用 Android 真机（或 JVM 跑 core）产生样本文件，Rust 端读取/产生，双向验证：

- **T1 打开 Android 生成的 `.sicv`**：含非 ASCII 主密码（如 `密码-测试-123`）与 ASCII 主密码各一例；
  校验 PBKDF2 口令编码假设（歧义 B）。解析后逐字段比对 `Vault` JSON。
- **T2 Rust 生成 `.sicv`，Android 打开**：确认 magic/version/iterations/salt/wrapped 长度与 JSON
  schema 被接受；条目内容一致。
- **T3 头部校验**：坏 magic、version=2、iterations=9999/10000001、salt_len=7/65、
  wrapped_len=39/257、payload_len 超过实际文件——Rust 与 Kotlin 抛同类错误且不 panic/OOM。
- **T4 错误密码** ⇒ 与 `WrongPasswordException` 等价错误；截断 payload ⇒ 同样报「主密码错误或损坏」。
- **T5 changePassword 往返**：Android 改密码 → Rust 打开；Rust 改密码 → Android 打开；并验证
  payload 密文段字节不变（仅 salt/wrapped_dek 变）。
- **T6 contentKey 稳定性**：改密码/同步/合并前后 contentKey 不变；换主密码后 Android 与 Rust
  都能解同一 `.sica` 附件（T8）。
- **T7 附件**：Android 添加附件 → Rust `read()` 明文一致；Rust 写 `.sica`（SICA magic + v1 +
  AAD=id）→ Android 读取一致；`readRaw/writeRaw` 透传字节逐位一致（同步路径依赖）。
- **T8 JSON 兼容**：Android 库含全部字段（含 null deletedAt、tombstones、attachments、中文 label、
  color 负值、iconData data-URI 前缀）→ Rust 解析；Rust 序列化后 Android 打开无损；再验证
  **忽略未知字段**（Rust 输出里塞一个 `"_future": 1`，Android 仍能打开）。
- **T9 首次同步**：Rust 端 `sync()` 对空远端 ⇒ PUT `SafePassword.spv`；Android 端配置同一 WebDAV
  目录后能下载并合并（direction=DOWNLOADED）。确认远端路径两边一致（歧义 A）。
- **T10 双向同步矩阵**：仅本地变 / 仅远端变 / 都变（四种 ConflictStrategy）/ 都未变——用
  真实 WebDAV（坚果云或自建 Nextcloud）跑一遍，比对 `SyncResult` 的 direction/conflicts/message。
- **T11 合并规则**：本地新增+远端新增；同 id 双端编辑（时间戳差值有/无、内容同/异）；一侧删除
  （墓碑）另一侧未变/已编辑；墓碑 90 天过期；标签同 id 冲突取本地；`revision = max+1`。
  （可直接把 VaultMerger 的输入构造为 JSON 用例，双端各跑一遍比对输出 Vault JSON。）
- **T12 WebDAV 边角**：GET 404→null；GET 409+HEAD 404→null；GET 409+HEAD 200→报错；GET 2xx 空
  body→报错；MKCOL 405 视为成功；DELETE 404 成功；PUT 前自动建多级目录；中文 vaultKey 的
  百分号编码；Basic 认证与 401 报错文案（可参照 WebDavClient.kt:24-71 三个测试）。
- **T13 备份/恢复**：Android 备份 → Rust 复制还原 → Android 打开；文件名模式
  `<safeName>-<yyyyMMdd-HHmmss>.sicv` 与 prune(keep=10) 行为一致（中文库名、特殊字符库名）。
- **T14 XML 往返**：Android 导出 XML → Rust parse → 逐字段比对（labelNames、favorite、
  notes 拼接、iconData、time_stamp 秒）；Rust export → Android parse（可用 ToolsTest.kt:53-123
  的样例 XML 与断言作为固定用例）。另加：`deleted="1"`/`template="true"` 卡跳过、秒/毫秒
  time_stamp 各一例。
- **T15 CSV 往返与公式注入**：含 `,`、`"`、换行、以 `=`/`+`/`-`/`@` 开头的值，比对
  ToolsTest.kt:30-50 行为（`'` 前缀、CRLF、标签 `|` 连接）。
- **T16 密码强度/生成**：对固定口令列表（含 42 个常见弱口令、`aaaa…`×5、`1234…`×6、中文口令）
  比对 score/label/entropy（entropy 允许 1e-9 浮点差）；生成器验证长度钳制 [4,128]、
  requireEach、excludeAmbiguous 字符集、passphrase 词表/格式。
- **T17 HIBP**：`passwordCount("password")` > 0（线上）；未知随机口令 = 0；非 200 报错；
  离线比对解析器（构造 `SUFFIX:count` 文本灌入解析函数）。
- **T18 空库/极端值**：空 items/labels/tombstones 的库往返；64MiB 上限；附件 10MiB 上限报错；
  超长标题/备注（含 emoji）JSON 与 XML 往返。

---

*文档生成时间：2026-11-02。源码如有更新，请以 `core/` 模块对应文件行号为准复核本文常量。*
