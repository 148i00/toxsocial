# ToxSocial 安全审查报告

> 审查日期：v0.2.27 发布前 · 范围：crates（tox-ffi/tox-core/tox-social/tox-store）、apps/desktop（Rust 后端 + Vue 前端）、Relay 服务端（functions + worker.js）

## 1. 本次审查修复的漏洞

### 1.1 接收文件路径穿越（高危，已修复）
- **位置**：`apps/desktop/src/events.rs` `Event::FileReceived` 处理，`dir.join(&filename)` 直接写入。
- **风险**：恶意好友可发送文件名为 `..\..\..\Users\xxx\evil.exe` 的文件，写入 `%APPDATA%` 之外任意目录（Windows 下 `join` 拼接路径穿越）。
- **修复**：新增 `sanitize_filename()`——剥离一切路径分隔符只取 basename、过滤 Windows 非法字符（`<>:"|?*` 等）、限制长度 180、空名兜底为 `file`。

### 1.2 接收文件大小无限制（中危，已修复）
- **位置**：`Event::FileRecv` 处理，收到任何大小的文件都会缓冲进内存等待用户确认。
- **风险**：恶意好友发送数 GB 的"文件"导致接收端内存耗尽/卡死。
- **修复**：`file_size > 100MB` 直接自动拒绝（`reject_file`），不再弹窗。

## 2. 审查通过项（未发现问题）

| 面 | 结论 |
|---|---|
| **Markdown XSS** | `markdown.ts` 先 `escapeHtml` 再解析，链接仅允许 `http(s)/mailto` 白名单，`v-html` 安全 |
| **SQL 注入** | tox-store 全部查询使用 rusqlite 参数绑定；`search_posts` 的 LIKE 通配符已转义 |
| **Tox FFI 并发** | 所有 FFI 调用经 `TOX_FFI_LOCK` 串行化；内部布局读取（dht_node_count）只读指针且空值检查 |
| **Relay 防伪造** | 公开帖子 Ed25519 签名验证（edPk→X25519 映射防冒名）、±15s 时间校验、按 IP 写入限频、体积限制、删除接口需作者签名 |
| **附件存储** | 附件以 `media/attachments/<post_id>`（UUID）落盘，文件名不参与路径 |
| **帖子渲染** | 时间戳/文本经 Vue 文本插值（非 v-html） |
| **目录搜索放大** | `requestDirectorySearch` 深度限制为 2，防无限传播 |
| **更新检查** | 固定 HTTPS URL + 超时 10s，失败静默 |
| **好友消息** | Tox 消息 ≤1372B、TSP 信封 ≤1300B、签名/作者校验在 persist 前 |

## 3. 已知限制（可接受，记录在案）

- **profile.tox 明文存储**：Tox 私钥以明文存在 `%APPDATA%\dev.toxsocial.desktop\profile.tox`。建议后续用 toxencryptsave 加密（会影响多设备/离线恢复体验，暂缓）。
- **Tauri CSP 为 null**：前端为本地打包资源（无远程内容加载），风险低；后续可收紧。
- **帖子 ts 可被作者伪造**：签名是自签的，作者可声明 ±15s 窗口内的任意时间；Relay 已拒绝窗口外时间。
- **附件元信息不在签名内**：`Post.attachment` 字段未参与 `signing_string`，理论上好友可篡改自己转发的帖子的附件名（仅影响显示，不影响文件内容传输路径）。

## 4. 建议（后续迭代）
1. profile 加密（toxencryptsave）——涉及保存/恢复流程改造
2. 收紧 CSP + 附件 MIME 白名单（下载后按扩展名提示风险）
3. 发送文件大小在前端/后端统一限制提示（当前附件 20MB、接收上限 100MB）
