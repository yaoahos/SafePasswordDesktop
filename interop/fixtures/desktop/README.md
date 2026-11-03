# 桌面端生成的互操作样本

用于验证 Android 端能否读取桌面端写出的 **v2 容器**（payload = gzip 后 AES-GCM）。

| 文件 | 说明 |
|---|---|
| `known1.v2.spv` | 由 `interop/fixtures/android/known1.spv` 重新加密而来，1257 字节，头部 `version = 2` |

口令与期望明文复用 Android 侧样本：

- 口令：`../android/known1.password.txt`
- 期望 JSON：`../android/known1.vault.json`（条目数 5，库名 `测试密码库🚀`）

生成方式（桌面端仓库）：

```powershell
cargo run --example rewrite_fixture -- `
  interop\fixtures\android\known1.spv `
  interop\fixtures\android\known1.password.txt `
  interop\fixtures\desktop\known1.v2.spv
```

校验方式：

```powershell
cargo run --example read_fixture -- `
  interop\fixtures\desktop\known1.v2.spv `
  interop\fixtures\android\known1.password.txt `
  $env:TEMP\v2open.json
# $env:TEMP\v2open.json 应与 known1.vault.json 内容一致（桌面端已用 SHA-256 比对通过）
```
