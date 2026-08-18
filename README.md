# ToxSocial

> GitHub: https://github.com/148i00/toxsocial
> Relay Server: https://github.com/148i00/toxsocial-relay

**中文**：基于 [c-toxcore](https://github.com/TokTok/c-toxcore)（Tox 协议）构建的去中心化社交软件（类推特）。无服务器、端到端加密，身份即公钥（ToxID），关注即 Tox 好友，帖子加密广播给所有好友，时间线本地聚合。

**English**: A decentralized social network (Twitter-like) built on the [Tox protocol](https://github.com/TokTok/c-toxcore) via c-toxcore. It is serverless and end-to-end encrypted. Your identity is your public key (ToxID), following means adding Tox friends, posts are encrypted and broadcast to all friends, and your timeline is aggregated locally.

---

## 功能 / Features

- 身份即 ToxID，本地保存私钥 / Identity = ToxID, private key stored locally
- 好友请求、在线状态、关注管理 / Friend requests, online status, following management
- 帖子、评论、反应，端到端加密广播 / Posts, comments, reactions with E2E encrypted broadcast
- 离线回补（好友上线自动同步）/ Offline backfill (auto sync when friends come online)
- 长文自动分片 / Long-post chunking
- Markdown 渲染 / Markdown rendering
- 图片/视频外链上传（Imgur）/ Image/video upload via Imgur
- 频道群聊（创建/邀请/加入/发送）/ Channels (create/invite/join/send)
- 公共频道目录与 Relay 成员上报 / Public channel directory and Relay member reporting
- 评论回复嵌套 / Nested comment replies
- 多设备同步基础版 / Basic multi-device sync
- 本地全文搜索 / Local full-text search
- 应用内通知中心 / In-app notification center
- 中英文界面 / Chinese & English UI
- 头像上传与展示 / Avatar upload & display
- 后台运行/系统托盘 / Background running & system tray
- 开机自启 / Launch at startup
- 支持多个 Relay 服务器 / Multiple Relay server support

## 技术栈 / Tech Stack

Rust workspace + Tauri v2 + Vue 3 + SQLite, powered by c-toxcore (GPLv3) through bindgen FFI.

## 构建 / Build

### 环境要求 / Requirements

- Rust (stable, MSVC toolchain on Windows)
- Node.js + npm
- c-toxcore static library (see `docs/PLAN.md`)
- libsodium and pthread (Windows)

### 前端构建 / Frontend build

```bash
cd apps/desktop/ui
npm install
npm run build
cd ../..
```

### 桌面端开发构建 / Desktop development build

```bash
cargo build -p toxsocial-desktop
```

### 打包安装包 / Package installers (Windows)

```bash
powershell -ExecutionPolicy Bypass -File scripts/bundle.ps1
```

Output:

```text
target/release/bundle/msi/ToxSocial_0.1.0_x64_en-US.msi
target/release/bundle/nsis/ToxSocial_0.1.0_x64-setup.exe
```

### CLI 双实例联调 / CLI two-instance testing

```bash
cargo build -p tox-cli
./target/debug/tox-cli.exe init --save a.tox --name A
./target/debug/tox-cli.exe run a.tox --db a.db
```

## 可选 Relay 服务器 / Optional Relay

- Core features are fully **P2P / serverless**.
- Default public Relay: `https://toxsocial-relay.vcst.top`
- The client supports multiple Relay URLs and merges directories, public channels, and public posts.
- Self-hosted Relay source: https://github.com/148i00/toxsocial-relay

## 文档 / Docs

- [总体规划 / Plan](docs/PLAN.md)
- [架构设计 / Architecture](docs/ARCHITECTURE.md)
- [社交协议规范 / Protocol](docs/PROTOCOL.md)

## 发布 / Releases

GitHub Releases are published at:

```text
https://github.com/148i00/toxsocial/releases
```
