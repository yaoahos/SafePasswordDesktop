import assert from "node:assert/strict";
import test from "node:test";

import { releaseNotes } from "../src/utils.ts";

/** 更新弹窗对 GitHub Release 正文的解析：标题要丢掉，8 条变更一条不少。 */
const body = `**2026/11/20 · 1.1.5**
- 变更：统一应用标识与云端文件命名
- 变更：本版应用标识与旧版不同，可与旧版同时安装；两版数据目录相互独立，旧版密码库需先导出再在本版导入
- 变更：请勿让新旧两版同时开启自动同步并连接同一 WebDAV 目录，避免互相同步同一批文件产生覆盖或冲突
- 优化：提升跨版本数据兼容性
- 优化：条目编辑页去掉字段区顶部的「字段」标题
- 优化：「＋添加字段」按钮移到字段列表下方，与手机端位置一致
- 优化：新建登录条目时网址字段排在账号上方
- 修复：同步被占用或异常时界面一直停留「正在同步…」的问题
`;

test("releaseNotes 过滤版本标题，保留全部 8 条变更", () => {
  const notes = releaseNotes(body);
  assert.equal(notes.length, 8);
  assert.equal(notes.some((line) => line.includes("2026/11/20")), false);
  assert.equal(notes.some((line) => line.startsWith("*")), false);
  assert.equal(
    notes[1],
    "变更：本版应用标识与旧版不同，可与旧版同时安装；两版数据目录相互独立，旧版密码库需先导出再在本版导入"
  );
  assert.equal(
    notes[7],
    "修复：同步被占用或异常时界面一直停留「正在同步…」的问题"
  );
});
