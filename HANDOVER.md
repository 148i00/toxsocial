# 交接文档（Session Handover）

> 写给下一位接手的 agent/开发者。项目：**ToxSocial** —— 基于 Tox 协议（c-toxcore）的去中心化类推特社交软件。
> 工作目录：`C:\Users\148i00\Documents\dsworkspace\toxproject`
> 语言：用户使用中文交流，请用中文回复。

## 0. 当前状态一句话

ToxSocial 已完成 M0–M4 和大部分 M5 功能；桌面端已能打包发布，最新版本 **v0.2.19**（代码中已包含尚未打 tag 的 v0.2.20 功能：好友 bio 展示 + 文件发送 UI + Relay 频道成员上报 + 评论回复嵌套 + 文件接收确认 + 开机自启）。最近重点做了：
1. 把频道页改成 **QQ 群聊式两栏布局**（左频道列表 + 右聊天窗口）。
2. 修复了用户反馈的 **操作卡顿/启动未响应**：根因是 c-toxcore FFI 被多线程并发调用，现已加全局串行锁。
3. 完成 **好友资料 bio 展示**、**文件发送 UI**、**Relay 频道成员上报**、**评论回复嵌套**（尚未发布）。

## 1. 环境事实（重要，勿重复踩坑）

| 项 | 值 |
|---|---|
| Rust | 1.97.1 stable (x86_64-pc-windows-msvc) |
| Node | v24.19.0 + npm |
| CMake | 4.4.2（`C:\Program Files\CMake\bin`，**不在 PATH，需显式加**） |
| LLVM | 22.1.8（bindgen 用，绑定已检入可不再需要） |
| vcpkg | `C:\Users\148i00\vcpkg`（libsodium 已装到 `installed\x64-windows`） |
| c-toxcore | submodule 在 `third_party/c-toxcore`（master=v0.2.23） |
| c-toxcore 静态库 | `build/c-toxcore/Release/toxcore_static.lib`（**注意是顶层 `build/`，不是 third_party 下**；构建产物不入 git） |
| 网络 | 走 clash-verge 代理（127.0.0.1:7897）；GitHub releases 大文件下载慢 |
| Relay | `https://toxsocial-relay.vcst.top`（Cloudflare Pages + D1，旧 Worker 已删除） |

**构建必需环境变量**（每次新 shell 都要设）：
```powershell
$env:SODIUM_LIB = "$env:USERPROFILE\vcpkg\installed\x64-windows\lib"
$env:PATH += ";$env:USERPROFILE\vcpkg\installed\x64-windows\bin;build\c-toxcore\vcpkg_installed\x64-windows\bin"
```

## 2. 代码结构

```
crates/tox-ffi/     手写 FFI 子集 + bindgen 输出（bindgen_output.rs，已检入）
crates/tox-core/    ToxSession 安全封装、事件循环线程、TOX_FFI_LOCK、bootstrap 节点表（bootstrap.rs）
crates/tox-social/  TSP/1 协议：envelope + FeedEngine（签名校验、持久化、Outbox/Dir 等）
crates/tox-store/   SQLite：kv/friends/posts/directory 等表
crates/tox-cli/     命令行客户端（init/show/add/send/post/timeline/run）
apps/desktop/       Tauri v2 应用（Rust 后端 + Vue3/Vite 前端 ui/）
server/             （已移至独立仓库 https://github.com/148i00/toxsocial-relay）
scripts/            bundle.ps1（打包）、upload-release.sh（GitHub Release 上传）
docs/               PLAN.md / ARCHITECTURE.md / PROTOCOL.md
```

## 3. 运行与构建

```powershell
# 前端构建（tauri-build 嵌入 dist，必须先跑）
cd apps/desktop/ui; npm run build; cd ../..

# 开发构建
cargo build -p toxsocial-desktop

# 启动（数据在 %APPDATA%\dev.toxsocial.desktop\：profile.tox + profile.db）
.\target\debug\toxsocial-desktop.exe

# 打包 Windows 安装包
powershell -ExecutionPolicy Bypass -File scripts\bundle.ps1
# 产物：
#   target\release\bundle\msi\ToxSocial_0.1.0_x64_en-US.msi
#   target\release\bundle\nsis\ToxSocial_0.1.0_x64-setup.exe
```

CLI 双实例联调：
```powershell
.\target\debug\tox-cli.exe init --save a.tox --name A
.\target\debug\tox-cli.exe run a.tox --db a.db
# 向 <save>.cmd 写命令：post <文本> / comment <post_id> <文本> / friends
```

## 4. 最近完成的重要工作

### 4.1 频道页 QQ 式布局（v0.2.17–v0.2.18）
- 左侧竖排“我的频道”列表，顶部有“＋ 创建/加入”按钮。
- 右侧聊天窗口：气泡消息、自己靠右、底部输入框、Enter 发送 / Shift+Enter 换行。
- 公共频道收进左侧“公共频道”折叠区。
- 频道管理（邀请、发布公共频道、成员）放在右侧“管理”折叠面板。
- 消息和日志做了数量上限（消息 300、日志 200），防止无限增长。

### 4.2 卡顿/未响应根因修复（v0.2.19，重要）
- **根因**：c-toxcore 不是线程安全的，但原来的代码里：
  - `tox-iterate` 后台线程持续调用 `tox_iterate()`
  - 事件泵线程、bootstrap 线程、Tauri 命令线程也直接调 Tox FFI
  - 多线程同时操作同一个 `Tox*`，导致数据竞争、随机卡死、未响应。
- **修复**：
  1. 在 `crates/tox-core/src/session.rs` 增加全局 `static TOX_FFI_LOCK: Mutex<()>`。
  2. 所有 `ToxSession` 的 FFI 方法进入时都先拿 `TOX_FFI_LOCK`。
  3. `tox-iterate` 线程在 `tox_iteration_interval` 和 `tox_iterate` 周围也拿同一把锁。
  4. 事件泵从 `recv_timeout(250ms)` 持有 session 锁改为 `try_recv()` + 50ms sleep，避免长时间阻塞 UI 命令。
  5. bootstrap 从“一次性锁住 session 循环 22 个节点”改为“每个节点单独拿锁/放锁”。

### 4.3 好友资料 bio + 文件发送 UI（未发布）
- 好友资料页现在显示 bio：`friends` 表新增 `bio` 字段并自动迁移；收到 TSP Profile 或 Tox 状态消息时更新并持久化。
- 关注列表新增“文件”按钮，可直接选择文件发送给好友（后端新增 `send_file_to_friend_by_toxid`，并兼容 `data:` URL 前缀）。
- 收到文件时后端已保存到 `media/`，前端通知中心提示保存路径。

### 4.4 Relay 频道成员上报 + 评论回复嵌套（未发布）
- Relay 公共频道新增 `members` 字段和 `POST /api/channels/members/report`：客户端定期上报“我在哪些公共频道”，Relay 5 分钟 TTL 过滤在线成员。
- 客户端加入公共频道时不再只找 host，而是按 host → co-host → 任意在线成员依次尝试 `join_channel` 申请。
- 评论回复支持嵌套：`thread_for` 改为递归 CTE，评论可以回复到某条评论（`reply_to` 指向评论 id），前端按缩进展示层级。
- 修复删除频道后重启又出现：创建/删除/加入频道后都会立即 `state.persist()` 保存 Tox 会话。

### 4.5 文件确认 + 公共频道加入修复 + 开机自启（未发布）
- 接收好友文件不再自动接受，改为弹窗手动“接受/拒绝”。
- 修复“已经是好友的人无法通过 `join_channel` 加入公共频道”：好友发来的普通 `join_channel <id>` 消息现在也会触发自动邀请。
- 好友上线时除了 `sync_req`，还会主动把自己的帖子推送过去，避免好友看不到首页/个人主页帖子。
- 设置页新增“开机自启”开关（tauri-plugin-autostart）。
- 只有频道创建者或 host 才能发布为公共频道，被拉入的普通成员不能再发布。
- 频道消息改为全局内存缓冲：即使不在频道页，收到消息也会保留，切回频道页仍能看到（重启前有效）。
- 公共频道“发布”按钮按权限显示：只有创建者/host 可见；已加入的频道会清除“已申请”状态。
- 头像增加目录（Relay Directory）兜底：好友还没广播头像时也能从搜索结果显示。
- Relay 不可用时在左下角显示警告。
- 英文语言已全面接入 i18n：所有用户可见文案（导航、通知、频道、设置、文件、评论、公共频道日志等）均已支持中/英切换。
- Relay 已重新部署到 Cloudflare Pages，`members` 字段已生效。
- 支持自定义 Relay 服务器：设置页可填写任意 Relay 地址，目录/公开帖子/公共频道/成员上报都会切换到该地址。
- 支持同时使用多个 Relay：设置页每行一个地址，客户端会同时读写所有 Relay，并自动合并目录/公共频道/公开帖子。
- 服务器源码已单独发布：https://github.com/148i00/toxsocial-relay
- Relay 已加基础防滥用：请求体大小限制、字段格式校验、按 IP 写入限频。

### 4.6 发布历史
- v0.2.13：频道创建命名、String slice 修复
- v0.2.14：自己公共频道直接进入、我的频道列表、评论 Shift+Enter、公共页自动请求
- v0.2.15：公共频道防重复加入、普通频道删除、公共频道复制邀请、频道成员列表
- v0.2.16：频道独立发内容区、消息带频道标签
- v0.2.17：频道聊天区 QQ 气泡化（只改频道区域）
- v0.2.18：QQ 式两栏布局 + 消息/日志上限
- v0.2.19：修复 c-toxcore 多线程并发导致的卡死/未响应

## 5. 关键技术事实与踩坑（务必先读 docs/PLAN.md §9）

1. **ToxID 校验和 = 奇偶字节 XOR**（非 CRC16，0.2.20+ 改了）；38 字节 = 32 pubkey + 4 nospam + 2 checksum。
2. **`tox_friend_get_connection_status` / `tox_friend_get_last_online` 有 error 参数**（0.2.23），手写 FFI 缺参 → 写垃圾指针 → 0xC0000005 崩溃。
3. `TOX_ERR_FRIEND_ADD` 枚举顺序已变（0=OK,1=NULL,2=TOO_LONG,3=NO_MESSAGE,4=OWN_KEY,**5=ALREADY_SENT**,6=BAD_CHECKSUM...）。
4. **toxcore 非线程安全，FFI 调用必须走 `TOX_FFI_LOCK`**：
   - 新加任何 Tox FFI 调用，必须放在 `crates/tox-core/src/session.rs` 的方法里，并在方法开头 `let _guard = TOX_FFI_LOCK.lock().unwrap();`。
   - 不要在 `session.rs` 之外直接调用 `tox_*`。
   - `tox-iterate` 线程和命令/事件线程共用同一把锁。
5. **锁顺序**：`state.session` → `TOX_FFI_LOCK`；不要反过来，也不要在持有 `TOX_FFI_LOCK` 时再调用另一个会拿 `TOX_FFI_LOCK` 的公开方法（`Mutex` 不可重入）。
   - 曾出现 `friend_list()` 内部调用 `friend_count()` 导致重复加锁，已改为直接调用 FFI。
6. **`std::sync::Mutex` 不可重入**：持有 session/engine 锁时不要调用 `state.persist()`（会再锁 session）或 `state.name_for()`（会再锁 engine）。
7. 好友消息上限 1372B；`TSP/1 ` 前缀 + JSON 信封；信封 ≤1300B。
8. vcpkg 装 libsodium 时卡过 PowerShell 7.6.3 下载（已缓存到 `vcpkg\downloads\`）。
9. c-toxcore CMake 会无条件 find_package(opus/vpx) → vcpkg 自动编译它们，configure 慢（~6 分钟）属正常；目标名 `toxcore_static`。
10. npm 的 allow-scripts 会拦 esbuild postinstall：`node node_modules/esbuild/install.js` 手动补。
11. `tauri icon` 命令必须从含 tauri.conf.json 的目录跑：`& .\ui\node_modules\.bin\tauri.cmd icon <png>`。
12. Vue `<script setup>` 里 ref 在普通函数中必须用 `.value`，模板中才会自动解包（曾因 `String(ownToxid)` 拿到 `[object Object]` 导致自己频道误判）。

## 6. 当前已知问题 / 下一步

### 需要用户验证
- v0.2.19 的卡顿/未响应修复是否生效。如果还卡，需要继续排查：
  - 是否特定操作卡（如频道切换、发消息、公共页刷新）。
  - 是否启动时卡（可能 `AppState::load` 里 `sync_friends` 同步执行较慢，可考虑移到后台）。
  - 是否 Relay 慢导致公共频道 15s 轮询卡 UI（可降低频率或改手动刷新）。

### 待办功能
- 实际 DHT 节点数（Tox API 不直接暴露，当前显示配置 bootstrap 数）。

### 可选的进一步优化
- 用 `<KeepAlive>` 缓存 ChannelsPanel，避免每次切换页面都重新加载频道/公共列表。
- 公共频道轮询从 15s 降低到 30s，或只在展开公共频道时刷新。
- 频道成员很多时做懒加载/限制显示。
- 消息列表继续增长时可做虚拟滚动。

## 7. 发布流程

```bash
# 1. 前端构建 + 打包
cd apps/desktop/ui && npm run build && cd ../..
powershell -ExecutionPolicy Bypass -File scripts/bundle.ps1

# 2. 打 tag 并推送
git tag -a vX.Y.Z -m "ToxSocial vX.Y.Z"
git push origin vX.Y.Z

# 3. 上传 GitHub Release（读取 ~/.toxsocial_gh_token 或 GITHUB_TOKEN）
bash scripts/upload-release.sh vX.Y.Z "ToxSocial vX.Y.Z" "更新说明" \
  target/release/bundle/msi/ToxSocial_0.1.0_x64_en-US.msi \
  target/release/bundle/nsis/ToxSocial_0.1.0_x64-setup.exe
```

## 8. 约定与备忘

- 提交规范：git 已用 `toxsocial-dev` 身份提交；保持小步提交、中文 commit message 也行。
- `.gitignore` 已忽略：target/、build/、ui/node_modules、ui/dist、test-*.tox/db/cmd。
- 许可证：c-toxcore GPLv3，本项目整体 GPL-3.0-or-later。
- 用户明确要求：**只改频道相关时不要顺手改其他页面**；如果改动范围超出频道，先和用户确认。
- 用户偏好：中文交流；喜欢直接给可安装的 Release。
