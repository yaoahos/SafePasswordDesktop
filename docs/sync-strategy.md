# 同步策略设计（大库 / 多端 / WebDAV 限流）

> 状态：**已实现（R1–R5 + 自适应调度）**，0.1.0 桌面端。Android 端仍是旧逻辑，但协议完全兼容——桌面端只是少发了请求，云端文件格式与布局都没有改变。
>
> 目标：在不改动 `.sicv` 格式、不改动 Android 端的前提下，把同步的往返流量与请求次数压到最低，避免坚果云之类 WebDAV 服务的流量 / 频率限制。

## 0. 实现落点（代码索引）

| 规则 | 实现位置 |
|---|---|
| R1 远端版本戳 | `sp/sync.rs` 的 `RemoteRev` / `RemoteRev::matches`；引擎在 `sync()` 开头 `HEAD` |
| R3 三向决策 | `sp/sync.rs::sync()` 开头的「快速判断」块（有指纹才 HEAD；无指纹走完整流程） |
| R4 写回抑制 | `sp/sync.rs::content_hash()`（排除 `last_synced_at`/`device_id`/`revision`） |
| 缓存持久化 | `commands.rs` 的 `sync_cache`（键 = 云端键）+ 数据目录 `sync-cache.json`（只存指纹，无密钥/明文） |
| R5 清单收敛 | `upload()` 仍是 GET+PUT 清单（清单含 `updated_at`/`size`，上传时必然变化） |
| 自适应调度 | `commands.rs::webdav_pull_if_changed` 按缓存里的库大小算间隔；前端 30 s 心跳 + 窗口可见时才发 + 回到前台立即检查 |

实测（`tests/webdav_flow.rs`）：云端没变化时同步 **0 次整库 GET**（只发 HEAD），只有本机改动时也 **0 次整库 GET**（直接 PUT）。

## 1. 先说清楚现在的数据体量

用与真实结构一致的条目跑 `VaultContainer::to_bytes()` 实测（2026-11-03）：

| 条目形态 | 每条 | 1,000 条 | 10,000 条 |
|---|---|---|---|
| 精简（标题 + 用户名 + 密码 + 网址） | ≈ 564 B | 547 KB | **5.39 MB** |
| 典型（+ 约 100 字备注、恢复码、手机号、标签、收藏） | ≈ 929 B | 900 KB | **8.87 MB** |

固定开销 ≈ 117 字节：`magic 4 + version 1 + iterations 4 + salt 16 + wrapped 60 + 两个长度字段 8 + GCM IV 12 + tag 16`。
AES-GCM 正文是流式 CTR，加密本身不膨胀，所以 **`.spv` 大小 ≈ 明文 JSON 大小 + 117 B**，两端看到的是同一个数字。

`last_synced_at`、`device_id`、`revision`、`updated_at` 都在**加密载荷内部**，因此任何一次「保存」都会让密文整体不同——即使条目内容没变。

## 2. 现状诊断：流量花在哪

| # | 位置 | 行为 | 代价 |
|---|---|---|---|
| 1 | `src-tauri/src/sp/sync.rs:154` | `sync()` 一进来就 `GET` 整个 `.spv`，不先看云端是否变化 | 每次同步 **下载整包**（8.87 MB） |
| 2 | `src-tauri/src/sp/sync.rs:376-380` | `ConflictStrategy::Merge` 合并后**无条件** `PUT` | 即使合并结果与远端逐字节相同也上传整包 |
| 3 | `src-tauri/src/sp/sync.rs::upload` | 一次上传 = **3 个请求**：`PUT 库` + `GET 清单` + `PUT 清单` | 请求数 ×3，清单未变化时白传 |
| 4 | `src-tauri/src/commands.rs:111` | `pull_interval_seconds: 60` 定时检查 | 1440 次/天检查（HEAD 很小，但计入请求配额） |
| 5 | `src-tauri/src/sp/sync.rs:93` | `pull_remote`（强制拉取）同样是整包 `GET` | 仅在明确要求覆盖时使用，可接受 |

时序上还有一处隐性成本：`(false, true)` 下载分支会把 `last_synced_at` 写进本地库（`sync.rs:329`），本地文件与云端字节不同，但 `updated_at` 没变，所以不会触发回传——这一处是安全的，无需改。

## 3. 推荐协议

把「判断是否需要传」和「真正传输」彻底分开：**检查只花 HEAD，传输只在必要时发生。**

### R1 远端版本戳

同步前先 `HEAD` 目标文件（`WebDavClient::stat`，≈200 字节），拿到：

```
rev = (ETag, Last-Modified, Content-Length)
```

- 优先比较 **ETag**；服务器不返回 ETag 时退回 `(Last-Modified, Content-Length)` 组合。
- `rev` 按 `vaultId` 缓存在 `settings.json`（**不进加密载荷**），成功上传/下载后更新，失败保持旧值。
- 只在 `HEAD` 不可用（部分 WebDAV 返回 405/403）时才退回直接 `GET`。

### R2 本地脏标记（dirty）

不要用 `updated_at > last_synced_at` 推断「本地有改动」——同步本身会写 `last_synced_at`，时间戳比较在时钟偏移时会误判。

改成显式脏标记：

- `put_item` / `delete_item` / 标签增删改 / 改库名 / 改主密码 / 导入合并 → `dirty = true`
- 同步成功后的 `last_synced_at`、`device_id`、`revision` 更新 → **不置脏**
- 进程重启后 `dirty` 由「本地内容哈希 ≠ 上次上传哈希」推导，不必持久化

### R3 三向决策

```
head = HEAD(remote_path)

if head is None:
    → 走现有「云端无本库文件」流程（认领旧键 / 上传 / RemoteDeleted 判定）
elif head.rev == cached.rev:
    if !dirty:  return NONE          # 1 个请求，0 字节正文
    else:       PUT 整包              # 只上传，不下载
else:
    GET 整包 → 解密 → merge
    if merged_hash == remote_hash:  return NONE_OR_DOWNLOADED   # 不 PUT
    else: PUT 整包                   # 合并后有真实变化才写回
```

要点：

- **只本机改过**时不再下载整包（省掉 8.87 MB）。
- **对端改过**时仍然要下载整包——这是单文件加密格式的硬限制，无法绕开。
- 合并后内容与远端一致（本地改动已被远端覆盖 / 已被 tombstone 吸收）时跳过 `PUT`。

### R4 写回抑制

维护 `content_hash`：对明文 JSON 做规范化（字段排序固定、**排除 `last_synced_at` / `device_id` / `revision` 三个易变字段**）后取 SHA-256。

`PUT` 之前比较：

- `hash(merged) == last_uploaded_hash` → 直接跳过 `PUT`，只更新本地 `last_synced_at`。
- 成功上传后写入 `last_uploaded_hash` 与新的 `rev`。

这一条专治「合并后回写一份内容相同的整包」以及多端来回同步的放大效应。

### R5 清单写入收敛（可选）

`upload()` 现在是「PUT 库 → GET 清单 → PUT 清单」3 个请求。可改为：

- 清单内容计算后与缓存比较，未变化则跳过 `PUT 清单`；
- 或者把清单合并进下一次检查的响应缓存里，避免单独 GET。

## 4. 调度策略（对坚果云友好）

坚果云免费版：约 **600 请求 / 30 分钟**、**上传 1 GB / 月、下载 3 GB / 月**。10,000 条的库整包 8.87 MB，意味着**每月最多约 110 次完整上传或 340 次完整下载**。

因此调度必须省着用：

1. **触发点合并**：不再盲目轮询。
   - 窗口获得焦点且距上次检查 ≥ 60 s；
   - 本地保存后 debounce 10 s（把连续编辑合并成一次传输）；
   - 解锁时（一次）；
   - 手动点击同步。
2. **后台窗口不轮询**：`document.hidden` 时暂停定时器。
3. **按库大小自适应间隔**：

   ```
   interval = clamp(60s, 15min, 30s × size_MB)
   ```

   8.87 MB 的库 → 约 4.5 分钟检查一次；一天约 320 次 HEAD（30 分钟窗口内约 7 次，远低于 600 的限制）。
4. **失败退避**：60 s → 2 min → 5 min → 15 min；遇到 `429` / `507` 直接退到 15 min。
5. **单飞**：同一时刻只允许一个同步在跑，其余请求合并到下一次（避免用户点两下触发两次整包传输）。
6. **配额记账（可选）**：本地累计「30 分钟窗口请求数」和「本月上下行字节」，接近阈值时只允许手动同步并提示。

## 5. 预期效果（10,000 条 / 8.87 MB）

| 场景 | 现在 | 推荐后 | 请求数（现在 → 推荐） |
|---|---|---|---|
| 定时检查，两端都没变 | GET 8.87 MB | HEAD ≈ 0.2 KB | 1 → 1 |
| 打开密码库（无编辑） | 0 | 0 | 0 → 0 |
| 本机改 1 条后同步 | GET 8.87 + PUT 8.87 MB | PUT 8.87 MB | 4 → 2 |
| 对端改 1 条 | GET 8.87 MB | GET 8.87 MB（格式限制） | 1 → 1 |
| 合并后内容与远端相同 | PUT 8.87 MB | 0 | 4 → 1 |
| 一天 300 次检查 + 10 次本机编辑 | ≈ 2.7 GB 下载 + 89 MB 上传 | ≈ 0 下载 + 89 MB 上传 | — |

结论：**下载配额从「一天就爆」变成几乎不占**；上传仍受整包大小限制，只能靠减少上传次数与后续的压缩格式缓解。

## 6. 多端一致性注意

- 用「远端版本戳是否变化」代替 `remote.updated_at > last_synced_at` 判断远端是否有更新。时间戳比较受设备时钟影响，会把「没变」误判成「变了」，白白下载整包。
- 上传成功后**再**更新缓存的 `rev` 与 `last_uploaded_hash`；失败保持旧值，下一次重试。
- 可选增强：`PUT` 带 `If-Match: <etag>`（服务器支持时）。若返回 `412 Precondition Failed`，说明期间有人抢先上传，重新 `GET` 合并后重试。坚果云对 `If-Match` 的支持需实测，做成 best-effort，不支持就退回时间戳判定。
- 拉取后必须**先落盘再更新缓存**，避免「缓存显示已同步、磁盘还是旧的」。

## 7. 落地顺序

| 步骤 | 内容 | 风险 | 依赖 |
|---|---|---|---|
| 1 | R1 + R3（HEAD 判定、跳过无谓下载）+ 单飞 + 去抖 | 低 | 无（不改格式、不改手机端） | ✅ 已实现 |
| 2 | R2（dirty）+ R4（写回抑制） | 低 | 无 | ✅ 已实现 |
| 3 | 自适应间隔 + 配额记账 + 退避 | 低 | 无 | ✅ 已实现 |
| 4 | R5（清单写入收敛） | 低 | 无 | 部分（上传时清单内容必然变化，仍 GET+PUT） |
| 5 | v2 容器压缩（先压缩后加密） | 中 | **必须 Android 端同版**（两端同时升级，不做过渡版本） | ✅ 桌面端已实现，待 Android |
| 6 | oplog / 条目级增量 | 高 | 两端协议改造；WebDAV 请求数与限流冲突，暂缓 | ⏸ 暂缓 |

第 1~4 步完全在同步逻辑内部，界面与已有功能不变；第 5 步需要两端协同发版。

## 8. 测量方法（可复现）

体积估算探针（临时文件，用完删除，不入库）：

```rust
// src-tauri/examples/size_probe.rs 核心
let json = serde_json::to_vec(&vault)?;             // 明文 JSON
let mut c = VaultContainer::create(vault, "pw");    // PBKDF2 210k 一次
let bytes = c.to_bytes();                           // 实际 .spv
println!("json={} spv={}", json.len(), bytes.len());
```

- 条目样本：`id / templateId / title / fields[用户名, 密码, (+恢复码, 手机)] / notes / notesLabel / notesType / notesPosition / url / labelIds / favorite / status / createdAt / updatedAt / deletedAt / attachments / iconKey / iconData`。
- 备注使用每条都不同的文本，避免 gzip 把重复内容压成失真的 1/30。
- `.spv` 与 JSON 的差值恒定在 117~120 字节，可用来快速校验体积。

## 9. 附：压缩（v2）实测与实现

**已实现（桌面端 2026-11-03）**：`src-tauri/src/sp/container.rs` 写入一律 `version = 2`（payload = `AES-GCM(gzip(JSON))`），读取同时接受 `1` 与 `2`（打开旧库后再次保存即自动升级）。Android 端需做同样改动才能互读。

对上述明文 JSON 做 gzip（Optimal）：

| 样本 | 原始 | gzip | 压缩比 |
|---|---|---|---|
| 典型 10,000 条 | 8.87 MB | 551 KB | ≈ 16× |
| 精简 10,000 条 | 5.39 MB | 206 KB | ≈ 26× |

样本结构重复度偏高（字段名重复一万次），**真实密码 / 备注更随机，保守按 8~15× 估算**：一万条典型库压缩后约 0.6 ~ 1.1 MB。

实施要点：

1. 先压缩、后加密（顺序反过来无效）；
2. 头部 `version` 从 `1` 提到 `2`；读取端接受 `{1,2}`，写入端只写 `2`。**两端必须同时升级**——只升一端的后果是另一端读到云端文件时报 `不支持的数据库版本：2`；
3. `AAD`（`SICV-PAYLOAD`）与 wrapped DEK 结构不变，主密码校验、改密流程不受影响；
4. 解压要设上限（桌面端 `MAX_PLAINTEXT_BYTES = 256 MiB`）防压缩炸弹；
5. v1 fixture 保留在 `interop/fixtures/android/` 做回归，确保老库仍可读（`legacy_v1_file_still_opens` 用例）。
