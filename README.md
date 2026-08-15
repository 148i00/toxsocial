# ToxSocial

基于 [c-toxcore](https://github.com/TokTok/c-toxcore)（Tox 协议）构建的**去中心化社交软件（类推特）**：
无服务器、端到端加密，身份即公钥（ToxID），关注即 Tox 好友，帖子加密广播给所有好友，时间线本地聚合。

## 文档

- [总体规划 docs/PLAN.md](./docs/PLAN.md)
- [架构设计 docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md)
- [社交协议规范 docs/PROTOCOL.md](./docs/PROTOCOL.md)

## 技术栈

Rust (workspace) + Tauri v2 + Vue3 + SQLite，底层 c-toxcore（GPLv3）经 bindgen FFI 接入。

## 状态

规划阶段（M0：环境与构建管线尚未开始）。
