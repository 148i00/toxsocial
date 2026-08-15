# ToxSocial — 去中心化社交软件（类推特）开发规划

> 基于 [TokTok/c-toxcore](https://github.com/TokTok/c-toxcore)（Tox 协议参考实现）构建的无服务器、端到端加密的社交平台。
> 本文件为总体规划，配套文档：[ARCHITECTURE.md](./ARCHITECTURE.md)（架构）、[PROTOCOL.md](./PROTOCOL.md)（社交协议）。

---

## 1. 项目愿景与定位

**一句话**：一个没有服务器的推特 —— 你的账号就是你的加密公钥，你"关注"的人就是你的 Tox 好友，你发的帖子端到端加密广播给所有好友，所有人的时间线在本地聚合。

| 维度 | 传统推特 | ToxSocial |
|---|---|---|
| 身份 | 邮箱/手机号（平台控制） | ToxID（公钥，用户完全拥有） |
| 服务器 | 中央服务器 | 无（P2P + DHT + TCP Relay） |
| 内容可见性 | 公开（平台可见） | 端到端加密，仅好友可见（频道可公开） |
| 封号/审查 | 平台可封 | 不可能（拉黑仅本地生效） |
| 离线消息 | 服务器缓存 | 无服务器 → 好友上线后回补（协议层解决） |

**产品功能（按优先级）**

| 功能 | 说明 | 版本 |
|---|---|---|
| 身份 | 生成 ToxID、昵称/头像/简介（profile 广播） | MVP |
| 连接 | 连接 bootstrap 节点、添加好友（好友请求）、接受/拒绝、在线状态 | MVP |
| 发帖 | 纯文本帖子（≤1000 字），广播给所有好友 | MVP |
| 时间线 | 聚合所有好友 + 自己的帖子，按时间倒序，分页加载 | MVP |
| 评论 | 广播评论（携带 post_id），本地聚合到帖子下 | MVP |
| 点赞/表情 | 广播 reaction（MVP 可做 👍，M4 扩展表情） | MVP |
| 关注管理 | 好友列表、删除好友、拉黑 | MVP |
| 二维码/Tox URI | 分享自己的 ToxID 邀请关注 | MVP |
| 同步回补 | 好友上线后自动拉取缺失的帖子/评论 | M4 |
| 附件 | 图片/文件，走 Tox 文件传输通道 | M4 |
| 频道 | 用 Tox 群聊（conference）做公开空间，邀请链接加入 | M4 |
| 多设备 | 同一账号多设备登录 + 历史同步（Merkle-Tox 可参考） | M5 |
| 音视频通话 | ToxAV（Windows 需 vpx/opus，复杂度高） | M5 可选 |
| 搜索/通知/多语言/打包 | 产品化打磨 | M5 |

---

## 2. 技术选型（已与用户确认）

| 层 | 选型 | 理由 |
|---|---|---|
| 传输核心 | **c-toxcore** v0.2.22+（C，GPLv3） | 活跃维护的 Tox 参考实现；DHT、Onion、TCP Relay、群聊、文件传输全内置 |
| FFI 绑定 | **bindgen** 生成 + 检入仓库 | 避免用户构建时依赖 libclang；官方 rs-toxcore-c 即此路线 |
| 安全封装 | 自研 `tox-core` crate | 管理 Tox 生命周期、事件循环、把 C 回调转成 Rust channel |
| 存储 | **SQLite**（`rusqlite`） | 消息历史、联系人、帖子、配置；单文件、零运维 |
| 社交协议 | `serde_json` 信封（前缀 `TSP/1 `，复用 NORMAL 消息） | 简单、可调试；好友消息上限 1372B 已确认 |
| 桌面端 | **Tauri v2** + Vue 3 + TypeScript + Pinia | 本机已有 Node 24；Rust 后端 + 轻量 Web 前端，打包小 |
| 开发工具 | `tox-cli`（clap）+ ratatui 可选 | 无 GUI 即可做协议联调/冒烟测试 |
| 异步运行时 | tokio | Tauri 命令 + 网络事件统一到异步模型 |

> 已排除：crates.io 上纯 Rust 的 `tox` crate（0.1.1，2020 年停更，不可用于生产）；
> toktok-stack monorepo（官方封装 + Merkle 历史同步，但依赖 Bazel，Windows 配置成本高，仅作 M5 参考）。

---

## 3. 总体架构

```
┌────────────────────────────────────────────────────────────┐
│ Tauri v2 App（Vue3 + TS 前端）                               │
│  登录/身份 · 时间线 · 发帖框 · 评论流 · 关注管理 · 频道 · 设置  │
├────────────────────────────────────────────────────────────┤
│ Rust 应用层（Tauri commands / 状态机 / 事件总线）             │
│  tox-social：社交协议编解码（envelope）· feed 引擎 · 同步回补   │
│  tox-store ：SQLite（帖子/评论/好友/资料/配置）                │
├────────────────────────────────────────────────────────────┤
│ tox-core（安全封装）                                          │
│  ToxSession 生命周期 · 事件循环线程（tox_iterate）            │
│  C 回调 → mpsc 事件 · friend/conference/file 安全 API        │
├────────────────────────────────────────────────────────────┤
│ tox-ffi（bindgen 绑定，检入仓库）                             │
├────────────────────────────────────────────────────────────┤
│ c-toxcore（C 静态库）+ libsodium                              │
└────────────────────────────────────────────────────────────┘
```

模块依赖图、线程模型、数据流、数据库 schema 详见 [ARCHITECTURE.md](./ARCHITECTURE.md)。

---

## 4. 核心设计决策

- **D1 关注 = 好友，内容仅广播给好友**：与推特的"公开广播"不同。隐私优先（端到端加密），
  公开性由"频道"（D5）补充。这也是 Tox 协议原生能力（friend message）。
- **D2 无服务器 → 离线回补（pull 模式）**：Tox 不缓存消息，好友离线时帖子会丢。
  方案：每个客户端持久化自己发过的全部帖子；好友上线后发 `sync_req`，对方回发缺失帖子
  （按时间增量），本地按 post_id 去重。详见 PROTOCOL.md。
- **D3 1372B 消息上限**：好友消息硬限制 1372B。帖子 ≤1000 字纯文本直发；
  长文/图片走"元数据广播 + Tox 文件传输"通道。
- **D4 消息类型**：c-toxcore 0.2.x 公开 API 只有 `TOX_MESSAGE_TYPE_NORMAL / ACTION`。
  社交协议以 `TSP/1 ` 前缀文本消息承载（JSON 信封），未来可平滑迁移到 custom packet。
- **D5 频道 = Tox 群聊（conference）**：public 群聊作为公开空间；加入频道 = 进入公共时间线；
  群内帖子同样进本地时间线（打频道标签）。解决"陌生人发现"问题。
- **D6 许可证**：c-toxcore 为 GPLv3。本项目整体按 **GPLv3** 开源（个人/开源使用无碍；
  若未来闭源分发需重新评估）。Tox 为实验性加密网络库（官方声明未经独立审计），记录在案。

---

## 5. 里程碑与阶段计划

单人兼职估算，共 6 个里程碑。每个里程碑有可验收的交付物。

| 里程碑 | 目标 | 关键交付物 | 验收标准 | 预估 |
|---|---|---|---|---|
| **M0** | 环境与构建管线 | cmake/LLVM/vcpkg 装好；c-toxcore 编译出静态库；bindgen 生成绑定；`tox-new` CLI 冒烟（创建实例、save/load） | `cargo build` 全绿；CLI 能生成 ToxID 并存盘 | ~3 天 |
| **M1** | 身份与连接 | `tox-core` 安全封装（事件循环、回调→channel）；bootstrap 连接；好友添加/接受；在线状态；profile 资料 | 两个 CLI 实例能互相加好友、看到对方上线 | ~5 天 |
| **M2** | 社交协议核心 | PROTOCOL v0.1 落地：envelope 编解码；发帖/评论/点赞 fan-out；SQLite 存储；`tox-cli` 双实例端到端 demo | 实例 A 发帖，实例 B 时间线出现该帖并可评论 | ~10 天 |
| **M3** | Tauri 桌面 MVP | 登录页（ToxID/二维码）、时间线、发帖框、评论流、关注管理、设置；事件总线打通 | 两个桌面端跨机互发帖子+评论，重启后历史保留 | ~10 天 |
| **M4** | 可靠性与社交扩展 | 离线回补协议；图片/文件附件；频道（conference 群聊 + 邀请链接）；表情 reactions | 好友离线期间发的帖子，上线后自动补齐；频道内多人群聊发帖 | ~10 天 |
| **M5** | 打磨与发布 | 搜索、通知、多语言、错误处理、打包（NSIS 安装包）、基础测试、README 文档 | 可安装分发；新手 10 分钟内完成注册→关注→发帖 | 持续 |

**关键路径**：M0 工具链是最大阻塞项（缺 cmake/clang/vcpkg）→ 优先解决。

---

## 6. 构建与环境（Windows）

本机现状：Rust 1.97.1 (MSVC) ✓、git ✓、Node 24 ✓；**缺 cmake / LLVM(clang) / vcpkg / ninja**。

```powershell
# 1. 工具链
winget install Kitware.CMake            # cmake
winget install LLVM.LLVM                # libclang（bindgen 需要；绑定检入后可不再需要）
git clone https://github.com/microsoft/vcpkg C:\dev\vcpkg
C:\dev\vcpkg\bootstrap-vcpkg.bat
C:\dev\vcpkg\vcpkg install libsodium:x64-windows

# 2. c-toxcore（含子模块 cmp 等）
git clone --recurse-submodules https://github.com/TokTok/c-toxcore third_party/c-toxcore
cmake -S third_party/c-toxcore -B build/c-toxcore -DCMAKE_BUILD_TYPE=Release `
      -DCMAKE_TOOLCHAIN_FILE=C:\dev\vcpkg\scripts\buildsystems\vcpkg.cmake
cmake --build build/c-toxcore --config Release

# 3. Rust 侧
cargo build   # build.rs: 定位静态库 + 调用 bindgen（或读取检入的 bindings.rs）
```

**bindgen 策略**：M0 用 bindgen 生成 `tox-ffi/src/bindings.rs` 并**检入仓库**，
`build.rs` 默认读取检入文件（用户机器无需 clang），CI 或维护者才重新生成。

---

## 7. 风险与对策

| 风险 | 影响 | 对策 |
|---|---|---|
| c-toxcore GPLv3 | 闭源分发受限 | 项目直接 GPLv3 开源（D6） |
| Tox 为实验性加密库（未审计） | 安全信任度 | 记录在案；MVP 面向学习/个人使用；后续可评估额外应用层签名 |
| 好友消息 1372B 上限 | 长文/图片发不了 | D3：附件走 Tox 文件传输 |
| 无服务器 → 离线丢失 | 好友错过帖子 | D2：pull 回补协议（M4） |
| bindgen/libclang 在 Windows 的配置坑 | M0 阻塞 | 绑定检入仓库；文档写清安装步骤 |
| Tox conference API 细节（privacy/moderation） | 频道功能延期 | M4 前研读 conference 文档与 toxxi/qTox 实现 |
| MSVC 构建 c-toxcore 的兼容问题 | M0 阻塞 | 官方支持 CMake+MSVC（qTox Windows 即此路线）；备选 vcpkg 的 toxcore port |
| 群聊中伪造消息（转发攻击） | 频道内容可信度 | M4 频道上线时给信封加 ed25519 签名（PROTOCOL.md §安全） |

---

## 8. 下一步行动（Phase 0 Checklist）

- [x] 安装 cmake / LLVM / vcpkg + libsodium（本机现状：全部缺失）
- [x] 初始化 cargo workspace（`tox-ffi` / `tox-core` / `tox-social` / `tox-store` / `tox-cli`）
- [x] 拉取 c-toxcore submodule 并编译出静态库
- [x] 生成并检入 bindgen 绑定（`bindgen_output.rs`，与手写子集交叉验证）
- [x] `tox-cli` 冒烟：`tox_new` → 打印 ToxID → save → load 验证
- [x] 双实例端到端联调：好友请求/接受 → UDP 加密连接 → 帖子/评论广播 → SQLite 持久化 → 时间线查询

## 9. 实际进度记录（M0-M4 已完成）

| 里程碑 | 状态 | 证据 |
|---|---|---|
| **M0** 环境与构建管线 | ✅ 完成 | cmake 4.4.2 + LLVM 22.1.8 + vcpkg(libsodium 1.0.22) + c-toxcore 0.2.23 静态库；`cargo check` 全绿；20 个单测通过；bindgen 0.72.1 生成 1491 行绑定 |
| **M1** 身份与连接 | ✅ 核心验证 | 双实例经公共 DHT bootstrap，好友请求/自动接受，`online (udp)` 加密连接，名字交换 |
| **M2** 社交协议核心 | ✅ 核心验证 | `TSP/1 ` 信封（post/comment/reaction/profile），发帖 fan-out 到在线好友，收端作者校验+SQLite 持久化+时间线聚合+评论线程 |
| **M3** Tauri 桌面 MVP | ✅ 核心验证 | Tauri v2 应用可启动；与 `tox-cli` Carol 完成好友请求/自动接受 → UDP 连接 → 发帖/评论 E2E；修复 3 处 Mutex 死锁 |
| **M4** 可靠性与社交扩展 | ✅ 核心验证 | 离线回补、长文分片、Markdown、Imgur 图片/视频外链、频道（conference）均已实现；CLI 双实例验证离线回补与频道 |

### 关键经验（踩坑记录）

1. **ToxID 校验和已从 CRC16 改为奇偶字节 XOR**（`data_checksum`，0.2.20+）：调试 ToxID 解析时勿用旧算法。
2. **`TOX_ERR_FRIEND_ADD` 枚举顺序变了**（0 = OK, 1 = NULL, 2 = TOO_LONG, 3 = NO_MESSAGE, 4 = OWN_KEY, 5 = ALREADY_SENT, 6 = BAD_CHECKSUM...）：错误码勿按旧文档硬编码。
3. **`tox_friend_get_connection_status` / `tox_friend_get_last_online` 在 0.2.23 增加了 `error` 参数**：手写 FFI 少参数会导致 C 代码写垃圾指针 → ACCESS_VIOLATION（0xC0000005）。教训：手写绑定必须与 bindgen 输出交叉核对。
4. **vcpkg 会为 c-toxcore 自动编译 opus/vpx**（`Dependencies.cmake` 无条件 find_package）：配置耗时 ~6 分钟属正常。
5. **c-toxcore 目标名是 `toxcore_static`**（VS 生成器），静态库位于 `build/c-toxcore/Release/toxcore_static.lib`；Windows 链接需 `pthreadVC3.lib`（vcpkg pthreads4w）。
6. **edition 2021 精确捕获**：闭包内 `tox.0` 只捕获字段（raw pointer）导致 Send 检查失败；先 `let tox = tox` 重绑定强制整体捕获。
7. **toxcore 非线程安全**（默认）：所有 FFI 调用集中在单线程（事件循环线程内），外部只经 channel 收事件 —— tox-core 已按此设计。
8. **`std::sync::Mutex` 不可重入**：持有 session/engine 锁时不要调用 `state.persist()` / `state.name_for()`；M3 E2E 发现 3 处死锁，修复方式为缩小锁作用域或从已持有的 `engine.store()` 直接读好友名。
