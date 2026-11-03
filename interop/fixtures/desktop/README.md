# 桌面端生成的互操作样本

用于验证 Android 端能否读取桌面端写出的容器：magic `SPV`（3 字节）+ `version = 2`（payload = gzip 后 AES-GCM）。

| 文件 | 说明 |
|---|---|
| `known1.v2.spv` | 由 `interop/fixtures/android/known1.vault.json` 重新加密而来，1256 字节，头部 `version = 2` |

口令与期望明文复用 Android 侧样本：

- 口令：`../android/known1.password.txt`
- 期望 JSON：`../android/known1.vault.json`（条目数 5，库名 `测试密码库🚀`）

生成方式（桌面端仓库，从明文 JSON 重新打包）：

```powershell
cargo run --example make_fixture -- `
  interop\fixtures\android\known1.vault.json `
  interop\fixtures\android\known1.password.txt `
  interop\fixtures\desktop\known1.v2.spv
```

格式变更（magic `SICV` → `SPV`、删除 v1 兼容）后，`interop/fixtures/android/*.spv`
同样是按新格式由 `make_fixture` 由各自的 `.vault.json` 重新生成的；`.vault.json`
与 `.password.txt` 未变，仍是跨端比对的基准。

校验方式：

```powershell
cargo run --example read_fixture -- `
  interop\fixtures\desktop\known1.v2.spv `
  interop\fixtures\android\known1.password.txt `
  $env:TEMP\v2open.json
# $env:TEMP\v2open.json 应与 known1.vault.json 内容一致（桌面端已用 SHA-256 比对通过）
```
