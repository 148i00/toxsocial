## "社区"功能实施计划（Reddit 式，与群组分离）

### 设计概要
- **社区 = 按主题聚合的帖子流**（`community_id`），与"群组"（实时聊天）完全分离
- 每个社区绑定一个 Tox conference（成员准入 + 实时分发）+ 在 Relay 注册元数据（名称/简介/历史聚合）
- 帖子信封新增可选 `c`（community_id）字段；发布到社区的帖子**仍发给所有在线好友**（好友未加入也能在时间线看到，帖子本身公开），社区页只是按社区聚合的视图
- 复用现有全部机制：评论/赞踩/附件/签名/三路分发/幂等去重——社区帖子就是带 `channel_id` 的普通 Post（posts 表 `channel_id` 列当初为此而设）

### 实施步骤

**1. 协议层（crates/tox-social）**
- `Envelope::Post` 加可选字段 `#[serde(rename = "c", default, skip_serializing_if)] community: Option<String>`（仿 `att` 模式）
- `publish_public_post` / `publish_long_public_post` 加 `community: Option<&str>` 参数，写入 Post.community 与 PostRow.channel_id

**2. 本地存储（crates/tox-store）**
- posts 表 `channel_id` 已存在，无需迁移；`persist` 的 Post 分支 `channel_id: None` → 改为 `p.community.clone()`

**3. 发布链（apps/desktop/src/commands.rs + relay.rs）**
- `publish_post` 命令加 `community: Option<String>` 参数 → 传入 feed 层 → 信封带 `c` → 本地 persist channel_id
- Relay 上传 body 加 `"community"`；**Relay 端**（独立仓库 functions + worker.js）：posts 表 `ALTER TABLE ADD COLUMN community TEXT`（ensure 迁移）、INSERT 带列、`GET /api/outbox?community=` 过滤、社区 id 格式校验（64hex）
- 客户端 `fetch_relay_public_posts` 透传 community 参数（拉取指定社区）

**4. 后端命令（commands.rs）**
- `create_community(name, desc)`：conference_new → conference_get_id 得 channel_id → kv `my_communities` 记录（channel_id + conference_number + name）→ 调 `register_public_channel` 注册到 Relay（复用现有命令逻辑）
- `join_community(channel_id)`：复用 joinPublic 的联系人拉入流程（host→co-host→成员顺序）
- `my_communities()` / `discover_communities()`：本地 kv + `list_public_channels`（已有）
- 社区 conference 实时分发：社区收到 TSP Post 信封（带 `c`）→ persist（channel_id=community）——复用上一计划已设计好的 ConferenceMessage TSP 解析分支（`engine.handle_incoming`，author==peer_key 校验）

**5. 前端（Vue）**
- 左侧导航加 **"社区"** 按钮（view = "communities"）
- 新组件 `CommunitiesPanel.vue`（两栏，仿群组页）：
  - 左栏：我的社区列表（kv）+ "创建社区"（名称/简介）+ "发现社区"折叠区（Relay 公共列表 + 加入按钮，复用 joinPublic 联系人拉入流程）
  - 右栏：选中社区的**帖子流**——顶部 PostComposer（提交时带 community）+ PostCard 列表（本地 `channel_id` 过滤 + Relay `?community=` 拉取合并）+ 点击帖子进现有详情页评论
- `PostComposer` 加可选 prop `community`（社区页传入时显示"发布到 {社区名}"）
- App.vue：view 类型加 `"communities"`、路由分支、FriendPanel 等不动
- mock（tauri-mock.ts）同步模拟社区命令与数据

**6. i18n + HANDOVER**
- 社区相关中英文文案；HANDOVER 记录"社区 vs 群组"定位与历史说明

### 兼容与限制（文档说明）
- 旧版客户端收到的社区帖子：无 `c` 字段 → 当普通公开帖处理（时间线可见），不受影响
- Relay 未迁移前列过滤不可用（帖子仍上传，community 列缺失时忽略）——Relay 独立仓库同步部署后生效
- 社区 conference 无历史：新加入成员的历史靠 Relay 聚合（社区页从 Relay 拉 `?community=`）

### 测试
- cargo build + tox-store 现有测试（PostRow 构造补字段）
- 前端 build + mock 数据加社区场景
- GUI 测试：创建社区 → 发帖到社区 → 社区流显示 → 详情评论（web-gui-tester 流程）