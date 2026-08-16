# ToxSocial 可选目录 / Relay（Cloudflare Worker）

如果你没有自己的服务器，可以用 **Cloudflare Workers 免费版** 部署一个轻量目录和 Relay。

## 功能
- `POST /api/directory`：注册公开资料
- `GET /api/directory?q=...`：搜索用户
- `POST /api/outbox`：发布公开帖子
- `GET /api/outbox?pubkey=...&since=...`：拉取公开帖子
- `POST /api/channels`：发布公共频道
- `GET /api/channels`：获取公共频道列表
- `POST /api/channels/members/report`：上报当前在线频道成员（用于加入时找任意成员拉人）

## 部署步骤
1. 注册 Cloudflare 账号（免费）
2. 安装 Wrangler：
   ```bash
   npm install -g wrangler
   ```
3. 登录：
   ```bash
   wrangler login
   ```
4. 在 `server/` 目录创建 KV namespace：
   ```bash
   wrangler kv:namespace create DIRECTORY
   wrangler kv:namespace create OUTBOX
   ```
5. 把返回的 id 填到 `wrangler.toml`（参考下面）
6. 部署：
   ```bash
   wrangler deploy
   ```

## wrangler.toml 示例
```toml
name = "toxsocial-relay"
main = "worker.js"
compatibility_date = "2025-01-01"

[[kv_namespaces]]
binding = "DIRECTORY"
id = "你的DIRECTORY_KV_ID"

[[kv_namespaces]]
binding = "OUTBOX"
id = "你的OUTBOX_KV_ID"
```

## 说明
- 这只是**可选增强**
- 没有它，ToxSocial 也能通过好友网络进行 P2P 目录查找和公开内容分发
- 有它之后，搜索和发现会更快、更稳定

## 当前部署状态（2026-08-16）
- Worker 名称：`toxsocial-relay`
- Workers.dev 地址：`https://toxsocial-relay.339148983.workers.dev`
- 已绑定路由：
  - `vcst.top/*`
  - `www.vcst.top/*`

### 让 vcst.top 生效
如果访问 `https://vcst.top` 仍失败，请在 Cloudflare Dashboard 为 `vcst.top` 添加 DNS 记录：

1. 进入域名 `vcst.top` → DNS → Records
2. 添加：
   - Type: `CNAME`
   - Name: `@`
   - Target: `toxsocial-relay.339148983.workers.dev`
   - Proxy: `Proxied`（橙色云）
3. 再添加一条：
   - Type: `CNAME`
   - Name: `www`
   - Target: `toxsocial-relay.339148983.workers.dev`
   - Proxy: `Proxied`

保存后等待几分钟即可通过 `https://vcst.top` 访问。
