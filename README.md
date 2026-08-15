# ToxSocial

> GitHub: https://github.com/148i00/toxsocial

基于 [c-toxcore](https://github.com/TokTok/c-toxcore)（Tox 协议）构建的**去中心化社交软件（类推特）**：
无服务器、端到端加密，身份即公钥（ToxID），关注即 Tox 好友，帖子加密广播给所有好友，时间线本地聚合。

## 文档

- [总体规划 docs/PLAN.md](./docs/PLAN.md)
- [架构设计 docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)
- [社交协议规范 docs/PROTOCOL.md](./docs/PROTOCOL.md)

## 技术栈

Rust (workspace) + Tauri v2 + Vue3 + SQLite，底层 c-toxcore（GPLv3）经 bindgen FFI 接入。

## 状态

**M0 ✅ / M1 ✅ / M2 ✅ / M3 ✅ / M4 ✅ / M5（多设备同步 + 搜索 + 通知 + 多语言 + 打包配置）✅**。

已完成：构建管线（CMake + vcpkg + c-toxcore 0.2.23 静态库 + bindgen）、`ToxSession` 安全封装、
`TSP/1` 社交协议（帖子/评论/点赞信封 + Feed 引擎 + SQLite 持久化）、`tox-cli` 双实例联调
（DHT bootstrap → 好友请求/接受 → UDP 加密连接 → 发帖 fan-out → 时间线聚合）、
Tauri v2 桌面端（Vue3 三栏 UI + Rust 命令/事件泵，已验证 App ↔ CLI 好友请求/发帖/评论）、
M4 离线回补（好友上线自动 `sync_req` / `sync_posts`，CLI 双实例已验证）、
帖子 Markdown 渲染（标题/粗体/斜体/代码/列表/引用/链接/图片）、
长文自动分片（`post_chunk`，>1000 字符自动拆分/重组）、
图片/视频外链上传（Imgur，设置页配置 Client ID 后发帖自动上传并插入 Markdown）、
频道（conference 群聊：创建/邀请/加入/发送，CLI + Desktop 面板）、
多设备同步基础版（设备互加好友 + 手动触发全量同步）、本地帖子全文搜索、应用内通知中心、中英文界面切换、Tauri 打包配置。

```powershell
# 构建（c-toxcore 静态库已就绪，无需重新编译）
$env:SODIUM_LIB = "$env:USERPROFILE\vcpkg\installed\x64-windows\lib"
$env:PATH += ";$env:USERPROFILE\vcpkg\installed\x64-windows\bin;build\c-toxcore\vcpkg_installed\x64-windows\bin"
cargo build -p tox-cli
# 两个终端分别启动实例，通过 <save>.cmd 命令文件发帖（post <text>）
.\target\debug\tox-cli.exe run alice.tox --db alice.db
```

# 构建 Windows 安装包（会自动复制 libsodium.dll / pthreadVC3.dll 到 target\release）
powershell -ExecutionPolicy Bypass -File scripts\bundle.ps1
# 产物：
#   target\release\bundle\msi\ToxSocial_0.1.0_x64_en-US.msi
#   target\release\bundle\nsis\ToxSocial_0.1.0_x64-setup.exe
```
