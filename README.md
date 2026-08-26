# LOOP

该项目是一款以“举办活动”为主的社交app, 提供一个平台让有共同爱好的人群聚集在一起参与活动，分享交流。

## 主要功能

1. 发布活动：用户可以创建并发布活动比如“web3技术交流会”，“烘焙展览会“
2. 浏览活动：用户可以在首页浏览活动
3. 搜索活动：用户可以根据tag,活动地点,活动时间,活动介绍相关等条件筛选活动
4. 参加活动：用户报名可以报名活动，如果活动发布方设置了签到或者门票等验证，参与者需要进行验证
5. 活动群聊：活动发起方和报名者可以加入该活动专用群聊
6. 社区板块：板块类似论坛，根据主题分类比如“科技”，“美食”，“摄影“，用户可以在板块发帖。主要用于孵化活动

## 项目结构

### 前端

app目录下，使用`react native` + typescript实现的客户端app

### 后端

后端都在srv目录下，具体目录：

1. loop-api-svc: 客户端主 HTTP API，承载账号、活动、媒体上传入口、报名、社区等普通请求/响应型业务
2. loop-realtime-svc: WebSocket 长连接服务，当前承载帖子讨论消息通知，后续复用到活动群聊、在线状态等实时能力
3. loop-worker-svc: 异步任务进程，当前负责 PostgreSQL outbox 投递、RabbitMQ 活动搜索消息消费和 Meilisearch 索引更新
4. loop-messaging: 跨业务模块共用的消息 envelope、版本约定和强类型 payload 契约
5. loop-search: API 与 worker 共用的活动搜索索引 schema、查询和写入逻辑
6. loop-svc-model: 服务器数据结构以及相关数据库操作
7. loop-infra: 数据库、Redis、RabbitMQ、Meilisearch、对象存储、邮件、可观测性等基础设施封装

后端服务按运行特征拆分，而不是按页面或业务名提前拆分。当前阶段普通业务优先沉淀在 `loop-api-svc` 的内部模块中，避免社区、活动、报名、用户关系等高耦合功能过早跨服务调用。只有长连接、异步任务、媒体处理、搜索索引、通知推送等运行模型明显不同的能力，才在需要时拆成独立进程或 worker。

## 社区与会话模型

社区以帖子为入口，但讨论交互使用类似 Telegram 的聊天线程，而不是嵌套评论：

1. 创建帖子时，在同一数据库事务中创建一个 `post_thread` 会话
2. 用户可以直接进入开放的帖子会话发言，不需要先申请加入群组
3. `conversations` 和 `conversation_messages` 是通用会话内核，帖子只通过 `kind`、`subject_id` 绑定业务上下文；未来活动群聊继续复用相同消息、已读、引用和订阅接口
4. 消息先写 PostgreSQL 并分配会话内单调递增的 `seq`，提交后再通过 Redis Pub/Sub 通知 WebSocket 服务
5. 实时事件只携带会话、消息和序号标识，客户端收到后从 HTTP API 按 `seq` 回补正文；定期对账负责覆盖断线和 Redis Pub/Sub 的 at-most-once 投递窗口

这种结构让 PostgreSQL 始终是消息事实来源，WebSocket 只是低延迟通知通道；帖子讨论与后续“聊天”模块共享同一套客户端会话组件。

## K8s 配置约定

后端不依赖配置中心，部署到 K8s 时使用 `ConfigMap` 管理普通配置，使用 `Secret` 管理敏感配置。启动时基础设施配置由 `loop-infra` 统一读取和初始化，业务服务只保留 JWT、TTL、权限等业务配置。服务会先读取 `LOOP_CONFIG_FILE` 指向的 TOML 文件，再用环境变量或 `*_FILE` secret 文件覆盖关键字段。`loop-api-svc` 当前编译启用了 `mail`、`storage` 和 `search` infra feature，因此邮件、S3、Meilisearch 的部署配置必须存在；worker 需要 PostgreSQL、RabbitMQ 和 Meilisearch 配置。敏感凭证必须通过 Secret 注入。

推荐挂载方式：

```text
ConfigMap -> /etc/loop/config/loop-api-svc.toml
Secret    -> /etc/loop/secrets/*
```

核心环境变量：

| 环境变量 | 作用 |
| --- | --- |
| `LOOP_CONFIG_FILE` | 普通配置 TOML 文件路径 |
| `LOOP_HTTP_ADDR` | HTTP 监听地址，本地默认可用 `127.0.0.1:3000` |
| `LOOP_REALTIME_SVC_PORT` | 本地或单机部署的实时服务端口，默认 `3010` |
| `DATABASE_URL` | PostgreSQL 连接串 |
| `REDIS_URL` | Redis 连接串 |
| `MEILISEARCH_URL` | Meilisearch 服务地址 |
| `MEILISEARCH_API_KEY` / `MEILISEARCH_API_KEY_FILE` | 后端管理活动搜索索引使用的密钥 |
| `LOOP_EVENT_SEARCH_INDEX_UID` | API 与 worker 共用的版本化活动索引名，默认 `events_v1` |
| `RABBITMQ_URL` / `RABBITMQ_URL_FILE` | worker 使用的 AMQP/AMQPS 连接串 |
| `LOOP_JWT_RSA_PRI_KEY_FILE` | API 服务使用的 JWT RSA 私钥文件 |
| `LOOP_JWT_RSA_PUB_KEY_FILE` | API 与 realtime 服务使用的 JWT RSA 公钥文件 |
| `LOOP_ACCESS_TTL` | 覆盖 access token TTL |
| `LOOP_REFRESH_TTL` | 覆盖 refresh token TTL |
| `LOOP_PERM_VER` | 覆盖权限配置版本 |
| `LOOP_ROLE_PERM_JSON` | 用 JSON 覆盖 `role -> permissions` 映射 |
| `LOOP_EMAIL_FROM` / `LOOP_SMTP_SENDER` / `LOOP_SMTP_DOMAIN` | 邮件基础配置 |
| `LOOP_SMTP_TOKEN` / `LOOP_SMTP_TOKEN_FILE` | SMTP token，可直接传字符串或 secret 文件 |
| `LOOP_S3_BUCKET` / `LOOP_S3_REGION` | S3 bucket 和 region |
| `LOOP_S3_ENDPOINT_URL` | S3 兼容服务 endpoint，例如 MinIO/R2；AWS S3 可不填 |
| `LOOP_S3_PUBLIC_BASE_URL` | 可公开访问对象时使用的 CDN 或 bucket base URL |
| `LOOP_S3_ACCESS_KEY_ID` / `LOOP_S3_ACCESS_KEY_ID_FILE` | S3 access key id，可直接传字符串或 secret 文件 |
| `LOOP_S3_SECRET_ACCESS_KEY` / `LOOP_S3_SECRET_ACCESS_KEY_FILE` | S3 secret access key，可直接传字符串或 secret 文件 |
| `LOOP_S3_FORCE_PATH_STYLE` | 是否强制 path-style，MinIO 通常设为 `true` |
| `LOOP_S3_ALLOWED_MIME_TYPES` | 允许上传的 MIME 类型，逗号分隔 |

普通配置示例：

```toml
access_ttl = 900
refresh_ttl = 2592000

[perm]
perm_ver = 1

[perm.role_perm]
user = [
  "user.profile.read",
  "user.profile.update",
  "event.read",
  "event.join",
  "event.create",
  "media.upload",
  "community.read",
  "community.post.create",
  "community.message.create",
]

organizer = [
  "user.profile.read",
  "user.profile.update",
  "event.read",
  "event.join",
  "event.create",
  "event.update_own",
  "media.upload",
  "community.read",
  "community.post.create",
  "community.message.create",
]

admin = [
  "user.profile.read",
  "user.profile.update",
  "event.*",
  "community.*",
  "user.manage",
]

# SMTP token 必须通过 Secret 文件或环境变量注入。
[email]
from = "Loop <no-reply@example.com>"

[email.smtp]
sender = "smtp-user"
domain = "smtp.example.com"

# S3 access key 和 secret access key 必须通过 Secret 文件或环境变量注入。
[storage]
bucket = "loop-media"
region = "us-east-1"
endpoint_url = "http://127.0.0.1:9000"
public_base_url = "http://127.0.0.1:9000/loop-media"
key_prefix = "media"
force_path_style = true
presign_expires_secs = 900
max_upload_bytes = 10485760
allowed_mime_types = ["image/jpeg", "image/png", "image/webp", "image/gif"]
```

JWT 私钥、公钥、数据库密码、SMTP token、S3 secret access key、Meilisearch API key、RabbitMQ URL 等敏感信息不要写入 ConfigMap，使用 K8s Secret 以环境变量或文件方式传入。

## 活动搜索索引

Meilisearch 是 PostgreSQL 中已发布活动的派生读模型，不是活动事实来源。活动创建或草稿发布时，API 在同一个 PostgreSQL 事务中写入活动和 `async_outbox`；提交后立即返回，不等待 RabbitMQ 或 Meilisearch。worker 的 outbox relay 使用 `FOR UPDATE SKIP LOCKED` 领取记录，以 persistent message、mandatory routing 和 publisher confirm 投递到 RabbitMQ，确认成功后才标记 outbox 已发布。relay 在确认后、数据库提交前退出时可能产生重复消息，这是至少一次投递的正常语义。

RabbitMQ 使用 durable topic exchange `loop.events.v1` 和 durable queue `loop.search.events.v1`。消息采用 CloudEvents 1.0 structured JSON envelope，由 `loop-messaging` 统一维护 `specversion`、`id`、`type`、`source`、`subject`、`time`、`datacontenttype`、`correlationid`、`causationid`、`traceparent` 和强类型 `data`。AMQP properties 同步写入 message id、type、correlation id、timestamp 与 `application/cloudevents+json` content type，consumer 会校验 properties 与消息体一致。业务消息类型为 `com.loop.event.search.refresh.v1`，routing key 为 `event.search.refresh.v1`，其 `data` 只携带活动 ID。

consumer 手动 ACK，并在每次消费时从 PostgreSQL 读取活动最新状态后执行 Meilisearch upsert 或 delete；所以重复消息和并发乱序不会让旧快照覆盖新活动。无效 envelope、未知版本或 transport metadata 不一致的消息会进入 `loop.search.events.dead.v1` 死信队列，处理基础设施错误时消息重新入队且 worker 退出，由编排器重启。已发布 outbox 记录保留 7 天后由 worker 清理。

API 启动时只确保索引及查询 settings 存在；worker 启动时额外按活动主键游标把 PostgreSQL 中全部已发布活动幂等 upsert 到索引。启动同步不会清空共享索引，避免多个 worker 滚动启动时制造搜索空窗。索引 schema 发生破坏性变化时通过 `LOOP_EVENT_SEARCH_INDEX_UID` 切换到 `events_v2` 这类新索引名。

搜索接口为 `GET /loop/event/search`，支持 `q` 全文关键词、英文逗号分隔的 `tags`、精确 `location`、RFC 3339 格式的 `start_from` / `start_to`，以及 `limit` / `offset`。全文字段包含标题、完整活动正文、摘要、地点和标签；响应同时返回当前结果集的标签与地点聚合，供 App 构建筛选项。公开索引只写入 `published` 活动，不包含草稿。

## 登录令牌机制

服务端采用短期 access token 与长期 refresh token 组合。默认 access token 有效期为
`900` 秒（15 分钟），refresh token 有效期为 `2592000` 秒（30 天），可分别通过
`LOOP_ACCESS_TTL` 和 `LOOP_REFRESH_TTL` 覆盖。每次成功刷新都会同时签发新的 access
token 和 refresh token，并从签发时重新计算 refresh token 的 30 天有效期，因此当前
登录会话采用滚动过期策略。

refresh token 只以 SHA-256 哈希形式写入数据库，明文仅返回客户端；每个 refresh token
只能消费一次。同一次登录轮换出的 token 属于同一个 token family，服务端检测到旧
refresh token 被重复使用时会撤销该 family 中仍活跃的 token，以限制令牌泄露影响。

移动客户端仅把 refresh token 写入系统安全存储，access token 只保存在内存中；Web
端不把 refresh token 写入 `localStorage`。客户端会在 access token 到期前主动刷新，
多个并发请求共享一次刷新操作；接口返回 `401` 时最多刷新并重试一次。只有 refresh
token 明确失效或被服务端拒绝时才清理会话并跳转登录页，临时断网和服务端 `5xx` 会
保留会话并稍后重试。

用户主动注销时，客户端会立即清除内存和系统安全存储中的登录令牌，并调用服务端撤销
当前 token family。注销接口保持幂等且不暴露 refresh token 是否存在；其他设备上的
独立登录会话不会受到影响。“记住密码”属于单独的本机登录辅助设置，不随注销清除。

## 本地开发

仓库根目录的 `Makefile` 会启动依赖、执行初始化 migration，并同时运行 API 与 realtime 服务。首次使用先准备单机环境文件和 JWT 密钥：

```bash
cp deploy/standalone/examples/.env.example deploy/standalone/.env
mkdir -p deploy/standalone/secrets srv/log
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
  -out deploy/standalone/secrets/jwt_private.pem
openssl rsa -pubout \
  -in deploy/standalone/secrets/jwt_private.pem \
  -out deploy/standalone/secrets/jwt_public.pem
make backend-up
```

`make backend-up` 会启动 Postgres、Redis、RabbitMQ、Meilisearch 和 MinIO，执行初始化 migration，并同时运行 API、realtime 与 worker。RabbitMQ AMQP 地址为 `127.0.0.1:5672`，管理界面为 `http://127.0.0.1:15672`，示例账号为 `loop` / `looprabbit123`。Meilisearch 地址为 `http://127.0.0.1:7700`。MinIO API 地址为 `http://127.0.0.1:9000`，控制台地址为 `http://127.0.0.1:9001`，示例账号为 `loopadmin` / `loopadmin123`。

如果使用 Android 真机上的 Expo Go 调试客户端，手机不能访问电脑上的 `127.0.0.1`。需要手动把本地服务地址配置成电脑局域网 IP，例如 `192.168.1.23`。

后端监听地址需要允许局域网访问：

```bash
LOOP_HTTP_ADDR='0.0.0.0:3000'
```

MinIO 预签名上传地址也必须使用手机可访问的地址，否则插入图片会在直传对象存储时失败：

```bash
LOOP_S3_ENDPOINT_URL='http://<电脑局域网IP>:9000'
LOOP_S3_PUBLIC_BASE_URL='http://<电脑局域网IP>:9000/loop-local'
```

启动 Expo 前显式设置 API 和 WebSocket 地址：

```bash
EXPO_PUBLIC_API_BASE_URL=http://<电脑局域网IP>:3000/loop \
EXPO_PUBLIC_REALTIME_URL=ws://<电脑局域网IP>:3010/loop/realtime \
npx expo start
```

手机和电脑需要在同一局域网内，并确认防火墙允许手机访问电脑的 `3000`、`3010` 和 `9000` 端口。未设置 `EXPO_PUBLIC_REALTIME_URL` 时客户端会退回短轮询，功能仍可使用，但跨设备消息会有数秒延迟。

`make backend-up` 会同时启动 API、realtime 和 worker；任一进程退出时会结束另外两个进程并返回对应退出码，避免本地开发时异步消费静默停止。

单机示例中的 RabbitMQ 与 MinIO 凭证直接使用字符串环境变量；需要按实际环境调整 `deploy/standalone/.env` 中的数据库、Redis、RabbitMQ、S3 endpoint、配置文件路径和密码。

`.env` 使用 shell `source` 加载，包含 `&`、空格等特殊字符的值需要加引号，例如 PostgreSQL URL。

静态检查命令：

```bash
cd srv
cargo fmt --all --check
cargo check --workspace
cargo clippy --workspace --all-targets -- -D warnings

cd ../app
npx tsc --noEmit
npm run lint
```

开发阶段数据库 schema 只保留一份当前初始化 migration，迁移文件位于 `srv/migrations/`。本地执行初始化前需要安装 Diesel CLI：

```bash
cargo install diesel_cli --no-default-features --features postgres
```

`make backend-up` 会根据 `deploy/standalone/.env` 组装 `DATABASE_URL` 并执行 migration。当前仍处于开发阶段，数据库表结构、索引和约束以这份初始化 migration 为准；初始化 migration 发生变更后需要手动重建本地开发数据库，再重新执行 `make backend-up`，避免历史 migration 引入兼容分支和冗余逻辑。

## 后端权限模型

访问控制使用稳定的权限点，而不是直接用 URL 字符串作为权限：

1. JWT 只保存 `user_id`、`role`、`perm_ver` 等身份声明
2. 服务端配置保存 `role -> permissions` 映射
3. 中间件按当前配置实时判断 role 是否拥有目标权限
4. 配置缺失、role 不存在、权限不匹配时默认拒绝
5. `perm_ver` 提升后，旧 token 会被视为过期权限并要求重新登录或刷新

权限配置示例：

```toml
[perm]
perm_ver = 1

[perm.role_perm]
user = [
  "user.profile.read",
  "user.profile.update",
  "event.read",
  "event.join",
  "event.create",
  "media.upload",
  "community.read",
  "community.post.create",
  "community.message.create",
]

organizer = [
  "user.profile.read",
  "user.profile.update",
  "event.read",
  "event.join",
  "event.create",
  "event.update_own",
  "media.upload",
  "community.read",
  "community.post.create",
  "community.message.create",
]

admin = [
  "user.profile.read",
  "user.profile.update",
  "event.*",
  "community.*",
  "user.manage",
]
```

公开接口在代码中声明，当前包括登录、刷新 token、注册、验证码发送。新增受保护接口时，需要在后端路由权限表中绑定稳定权限点，例如 `event.create`，再通过配置决定哪些 role 拥有该权限。

### 公用

loop-dto：使用rust定义的前后端之间请求，响应的数据结构，后端直接使用同时会编译到typescript供前端使用

## agents
* 暂时没有生成数据，migration归一，暂时不需要新增
