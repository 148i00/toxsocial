# 交接文档（Session Handover）

> 写给下一位接手的 agent/开发者。项目：**ToxSocial** —— 基于 Tox 协议（c-toxcore）的去中心化类推特社交软件。
> 工作目录：`C:\Users\148i00\Documents\dsworkspace\toxproject`

## 0. 目标（持久目标，来自上一 session 的 goal-47ab64a6）

> 基于 Tox 协议（c-toxcore + Rust FFI）开发一个去中心化的类推特社交软件：先完成规划文档，再按阶段实现（身份/连接 → 帖子广播与时间线 → 评论 → Tauri 桌面端 MVP）。技术栈已选定：Tauri v2 前端 + Rust 后端 + 自建 c-toxcore（CMake）+ bindgen 绑定。

**当前进度**：M0 ✅ / M1 ✅ / M2 ✅ / M3 ✅ / **M4 ✅**（离线回补、长文、Markdown、Imgur、频道全部完成）／ **M5 进行中：多设备同步（设备即好友）已实现基础版**（设置页可添加设备、手动触发全量同步）。

## 1. 环境事实（重要，勿重复踩坑）

| 项 | 值 |
|---|---|
| Rust | 1.97.1 stable (x86_64-pc-windows-msvc) |
| Node | v24.19.0 + npm |
| CMake | 4.4.2（`C:\Program Files\CMake\bin`，**不在 PATH，需显式加**） |
| LLVM | 22.1.8（bindgen 用，绑定已检入可不再需要） |
| vcpkg | `C:\Users\148i00\vcpkg`（libsodium 1.0.22 已装到 `installed\x64-windows`） |
| c-toxcore | submodule 在 `third_party/c-toxcore`（master=v0.2.23，含 cmp 子模块） |
| c-toxcore 静态库 | `build/c-toxcore/Release/toxcore_static.lib`（**注意目录是顶层 `build/`，不是 third_party 下**；构建产物不入 git） |
| 网络 | 走 clash-verge 代理（127.0.0.1:7897）；GitHub releases 大文件下载慢，vcpkg 卡过 PowerShell 7.6.3 下载（已缓存） |

**构建必需的环境变量**（每次新 shell 都要设）：
```powershell
$env:SODIUM_LIB = "$env:USERPROFILE\vcpkg\installed\x64-windows\lib"      # 链接 libsodium
$env:PATH += ";$env:USERPROFILE\vcpkg\installed\x64-windows\bin;build\c-toxcore\vcpkg_installed\x64-windows\bin"  # 运行期 DLL（pthreadVC3.dll 等）
```

## 2. 代码结构

```
crates/tox-ffi/     手写 FFI 子集（ABI-safe 访问器）+ bindgen 输出（bindgen_output.rs，检入，仅验证用）
crates/tox-core/    ToxSession 安全封装：事件循环线程、回调→channel、save/load、bootstrap 节点表（bootstrap.rs）
crates/tox-social/  TSP/1 协议：envelope（post/comment/reaction/profile/sync_req/sync_posts）+ FeedEngine（作者校验、持久化）
crates/tox-store/   SQLite：kv/friends/posts 表，幂等 upsert，timeline/thread 查询
crates/tox-cli/     命令行客户端（init/show/add/send/post/timeline/run），run 支持 <save>.cmd 命令文件驱动
apps/desktop/       Tauri v2 应用（Rust 后端 + Vue3/Vite 前端 ui/）
docs/               PLAN.md（含里程碑、踩坑记录）/ ARCHITECTURE.md / PROTOCOL.md
```

## 3. 运行方法

```powershell
# 构建前端（tauri-build 嵌入 dist，必须先跑）
cd apps/desktop/ui; npm run build; cd ../..

# 构建应用（设好上面两个环境变量）
cargo build -p toxsocial-desktop

# 启动（数据在 %APPDATA%\dev.toxsocial.desktop\：profile.tox + profile.db）
.\target\debug\toxsocial-desktop.exe
```

CLI 双实例联调（M1/M2 验证方法，全部跑通过）：
```powershell
.\target\debug\tox-cli.exe init --save a.tox --name A
.\target\debug\tox-cli.exe run a.tox --db a.db     # 两个终端分别跑
# 等双方 bootstrap 后：向 <save>.cmd 写命令：post <文本> / comment <post_id> <文本> / friends
```

## 4. M3/M4 当前状态与下一步

**M3 已完成**：
- 桌面端 Rust：`state.rs`（AppState：ToxSession+FeedEngine+数据目录）、`events.rs`（事件泵：tox 事件→SQLite→前端 emit，含 10s 心跳）、`commands.rs`（10 个 Tauri 命令 + DTO）
- 前端：`ui/src/`（App.vue 三栏布局：导航/时间线/右侧；组件：PostComposer、PostCard、ThreadView、FriendsPanel、SettingsPanel；ToxID 二维码、实时事件刷新）
- 图标已生成（apps/desktop/icons/，含 icon.ico —— **tauri-build 在 Windows 强制要求，缺失会构建失败**）
- 编译：`cargo check/build -p toxsocial-desktop` 全绿；前端 vite build 成功
- 验证过的运行时行为：启动 → 自动创建/加载身份 → 打印 ToxID → bootstrap 到 4 个 DHT 节点 → 心跳日志每 10s（friends/online 计数）。**应用退出码 0 是用户手动关窗口，非 bug。**
- M3 收尾：App 自动接受好友请求、UDP 连接、发帖/评论 E2E 已跑通；修复 3 处 Mutex 死锁（见 §5 新踩坑）

**M4 离线回补已完成**：
1. ✅ `tox-store` 新增 `latest_ts_for_author` / `posts_by_author_since`
2. ✅ `tox-social::FeedEngine` 新增 `latest_ts_for_author` / `self_posts_since` / `handle_sync_posts`
3. ✅ 好友上线时自动发送 `sync_req`（CLI + Desktop 事件泵）
4. ✅ 收到 `sync_req` 自动回 `sync_posts`（按 1300B 分块）
5. ✅ 收到 `sync_posts` 自动校验并持久化，CLI/Desktop 均会打印/emit
6. ✅ CLI 双实例 E2E：Alice 离线期间发帖，Bob 重连后自动收到缺失帖子
7. ✅ Desktop 取关功能可用：新增 `remove_friend_by_toxid` 命令并接入 FriendsPanel
8. ✅ ThreadView 反应展示增强：按 emoji 分组计数显示
9. ✅ TimelineItem 新增 `reactions` 汇总，PostCard 直接展示各 emoji 计数
10. ✅ 帖子支持 Markdown 渲染（标题/粗体/斜体/代码/列表/引用/链接/图片，已做 HTML 转义防 XSS）
11. ✅ 长文自动分片（`post_chunk`）：>1000 字符的帖子拆成多条 TSP 消息，接收端自动重组为完整帖子
12. ✅ 图片/视频外链上传（Imgur）：发帖时可选择本地图片/视频，自动上传到 Imgur 并插入 Markdown 链接；设置页可配置 Imgur Client ID
13. ✅ 频道（conference 群聊）：CLI 可创建/邀请/加入/发消息，Desktop 新增频道面板（创建/邀请/发送/消息日志）
14. ✅ M5 多设备同步（基础版）：设备各自独立身份、互加好友；设置页“多设备同步”可添加设备 ToxID 并“立即同步”触发 `sync_req` 全量/增量拉取
15. ✅ M5 搜索：本地帖子全文搜索（`search_posts`），首页搜索框实时显示结果
16. ✅ M5 通知中心：侧边栏铃铛 + 未读计数，帖子/评论/反应/好友/频道事件实时通知
17. ✅ M5 多语言：中文/English 切换（设置页），主要界面文案已 i18n
18. ✅ M5 打包配置：`tauri.conf.json` 已启用 bundle（`active: true`，targets=all），可生成安装包
19. ✅ UI 优化：深色渐变背景、阴影、圆角、滚动条、按钮动效等
20. ✅ 头像功能：设置页上传头像（Imgur 外链）或直接填写图片 URL；资料广播给好友；侧边栏/帖子/评论/好友列表显示头像。发帖框支持“图片 URL”直接插入 Markdown 图片
21. ✅ 后台运行：关闭窗口时隐藏到系统托盘，保持 Tox 在线；托盘菜单可“显示主窗口/退出”
22. ✅ 公共频道目录（UI 雏形）：频道页展示公共频道列表，可向 host 发送加入申请；实际自动邀请需 host 端实现
23. ✅ 抗封禁邀请机制：任何频道成员都可生成“ToxID + 频道ID + join_channel 附言”邀请链接；收到 `join_channel <频道ID>` 好友请求会自动接受并邀请对方进频道
24. ✅ 好友目录递归查找：新增 `dir_req/dir_resp` 协议，搜索时先查本地目录，再向好友请求；好友没有会自动转发给好友的好友（TTL 深度控制），结果写入本地目录
25. ✅ 内容分发架构（无服务器版）：帖子支持“公开”标记；公开帖子进入本地公共 Outbox，好友可通过 `outbox_req/outbox_resp` 递归拉取公开内容；新增“公共”时间线页
26. ✅ 可选 Cloudflare Worker 目录/Relay：`server/worker.js` + 部署文档，免费版可部署，无需自购服务器
27. ✅ 客户端已接入 Relay：默认 `https://toxsocial-relay.vcst.top`，公开帖子自动发布到 Relay，找人会同时搜索本地/好友/Relay，公共页可拉取 Relay 内容
28. ✅ UI 优化：找人结果排除已有好友；频道邀请支持 ToxID 直接邀请；频道邀请信息一键复制/一键粘贴加入；侧边栏新增“＋ 添加好友”弹窗
29. ✅ 公共频道 host：用户可把当前频道“发布为公共频道”到 Relay，公共频道列表从 Relay 拉取并展示 host；点击加入会自动向 host 发送 `join_channel` 申请
30. ✅ 频道创建命名：频道面板新增频道名称输入框，创建/加入时可直接命名；修复频道 ID 的 `String().slice` 兼容问题
31. ✅ v0.2.13 发布：Windows MSI/NSIS 安装包已构建并上传 GitHub Release
32. ✅ 自己创建的公共频道可直接进入（修复 `ownToxid` Ref 未 `.value` 导致误判为好友添加自己）
33. ✅ 频道页新增“我的频道”列表：自动加载本地所有会议，创建/加入后自动显示并切换
34. ✅ 评论框改为多行文本域：Enter 发送、Shift+Enter 换行
35. ✅ 打开“公共”页面自动请求好友网络 + Relay 公共内容
36. ✅ v0.2.14 发布：Windows MSI/NSIS 安装包已构建并上传 GitHub Release
37. ✅ 公共频道加入按钮防重复：加入中/已申请/已进入自动禁用，避免重复发送申请和重复日志
38. ✅ “我的频道”列表新增删除按钮（新增 `conference_delete` Rust 命令）
39. ✅ 公共频道列表新增“复制邀请”按钮（ToxID + 频道ID + join_channel 附言）
40. ✅ 新增“频道成员”列表，进入频道后显示成员及公钥
41. ✅ v0.2.15 发布：Windows MSI/NSIS 安装包已构建并上传 GitHub Release
42. ✅ 频道页新增独立的“向当前频道发内容”输入区，显示当前频道名称；Enter 发送、Shift+Enter 换行
43. ✅ 频道消息增加频道标签，多频道同时在线时不会混淆消息来源
44. ✅ v0.2.16 发布：Windows MSI/NSIS 安装包已构建并上传 GitHub Release
45. ✅ 频道聊天区改为 QQ 群聊式气泡窗口，只改频道聊天区域，保留公共频道/邀请/成员等原有位置
46. ✅ v0.2.17 发布：Windows MSI/NSIS 安装包已构建并上传 GitHub Release

**下一步（M5 剩余）**：
- 评论回复嵌套（当前只预填 @用户名）
- 用户资料页显示 bio
- 文件发送 UI（后端已支持）
- 实际 DHT 节点数（Tox API 不直接暴露，当前显示配置 bootstrap 数）

## 5. 关键技术事实与踩坑（务必先读 docs/PLAN.md §9）

1. **ToxID 校验和 = 奇偶字节 XOR**（非 CRC16，0.2.20+ 改了）；38 字节 = 32 pubkey + 4 nospam + 2 checksum
2. **`tox_friend_get_connection_status` / `tox_friend_get_last_online` 有 error 参数**（0.2.23），手写 FFI 缺参 → 写垃圾指针 → 0xC0000005 崩溃（已修，教训：手写 FFI 必须与 bindgen_output.rs 交叉核对）
3. `TOX_ERR_FRIEND_ADD` 枚举顺序已变（0=OK,1=NULL,2=TOO_LONG,3=NO_MESSAGE,4=OWN_KEY,**5=ALREADY_SENT**,6=BAD_CHECKSUM...）
4. toxcore 非线程安全：所有 FFI 调用集中在单线程（事件循环线程内）——tox-core 的架构已按此设计，**新代码不要在其他线程直接调 tox FFI**（经 Mutex<ToxSession> 也要注意锁顺序：session → engine）
5. 好友消息上限 1372B；`TSP/1 ` 前缀 + JSON 信封；信封 ≤1300B（中文 300 字内安全）
6. vcpkg 装 libsodium 时卡过 PowerShell 7.6.3 下载（已缓存到 `vcpkg\downloads\`）
7. c-toxcore CMake 会无条件 find_package(opus/vpx) → vcpkg 自动编译它们，configure 慢（~6 分钟）属正常；目标名 `toxcore_static`
8. npm 的 allow-scripts 会拦 esbuild postinstall：`node node_modules/esbuild/install.js` 手动补
9. `tauri icon` 命令必须从含 tauri.conf.json 的目录跑：`& .\ui\node_modules\.bin\tauri.cmd icon <png>`
10. edition 2021 闭包精确捕获：raw pointer 字段需先 `let x = x;` 重绑定再进闭包（Send 检查）
11. **`std::sync::Mutex` 不可重入**：持有 session/engine 锁时不要调用 `state.persist()`（会再锁 session）或 `state.name_for()`（会再锁 engine）。M3 E2E 中因此出现过 3 处死锁：好友请求自动接受后不持久化、收到帖子后不打印/不 emit、前端拉时间线时卡死。修复方式是先缩小锁作用域，或在已持 engine 锁时直接从 `engine.store()` 读好友名。

## 6. 约定与备忘

- 提交规范：git 已用 `toxsocial-dev` 身份提交；M3 主体与死锁修复已提交，本 session 的 M4 离线回补实现（`tox-store`、`tox-social`、`tox-cli`、`apps/desktop/src/events.rs`）和文档更新待提交
- `.gitignore` 已忽略：target/、build/、ui/node_modules、ui/dist、test-*.tox/db/cmd
- 许可证：c-toxcore GPLv3，本项目整体 GPL-3.0-or-later
- 语言：用户用中文交流，回复用中文
- 用户刚说要换 "anchored standard (experimental)" 预设开新 session —— 本交接文档就是为那个 session 准备的
