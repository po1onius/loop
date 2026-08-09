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
2. loop-realtime-svc: 长连接运行时服务预留，后续承载 WebSocket、活动群聊、在线状态、消息投递等实时能力
3. loop-svc-model: 服务器数据结构以及相关数据库操作
4. loop-infra: 数据库、Redis、对象存储、邮件、可观测性等基础设施封装

后端服务按运行特征拆分，而不是按页面或业务名提前拆分。当前阶段普通业务优先沉淀在 `loop-api-svc` 的内部模块中，避免社区、活动、报名、用户关系等高耦合功能过早跨服务调用。只有长连接、异步任务、媒体处理、搜索索引、通知推送等运行模型明显不同的能力，才在需要时拆成独立进程或 worker。

## K8s 配置约定

后端不依赖配置中心，部署到 K8s 时使用 `ConfigMap` 管理普通配置，使用 `Secret` 管理敏感配置。启动时基础设施配置由 `loop-infra` 统一读取和初始化，业务服务只保留 JWT、TTL、权限等业务配置。服务会先读取 `LOOP_CONFIG_FILE` 指向的 TOML 文件，再用环境变量或 `*_FILE` secret 文件覆盖关键字段。`loop-api-svc` 当前编译启用了 `mail` 和 `storage` infra feature，因此邮件和 S3 的非敏感配置必须存在，敏感凭证必须通过 Secret 注入。

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
| `LOOP_PG_CONN` / `DATABASE_URL` | PostgreSQL 连接串 |
| `LOOP_REDIS_CONN` / `REDIS_URL` | Redis 连接串 |
| `LOOP_JWT_RSA_PRI_KEY_FILE` / `LOOP_JWT_RSA_PRIVATE_KEY_FILE` | JWT RSA 私钥文件 |
| `LOOP_JWT_RSA_PUB_KEY_FILE` / `LOOP_JWT_RSA_PUBLIC_KEY_FILE` | JWT RSA 公钥文件 |
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
  "event.read",
  "event.join",
  "event.create",
  "media.upload",
  "community.post.create",
]

organizer = [
  "event.read",
  "event.join",
  "event.create",
  "event.update_own",
  "media.upload",
]

admin = [
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

JWT 私钥、公钥、数据库密码、SMTP token、S3 secret access key 等敏感信息不要写入 ConfigMap，使用 K8s Secret 以环境变量或文件方式传入。

## 本地开发

仓库根目录提供 `Makefile` 封装常用命令：

```bash
make local-init
cp deploy/local/.env.example deploy/local/.env
make deps-up
make db-migrate
make dev-event
```

`make deps-up` 会启动 Postgres、Redis 和 MinIO，并创建本地媒体 bucket `loop-local`。MinIO API 地址为 `http://127.0.0.1:9000`，控制台地址为 `http://127.0.0.1:9001`，本地账号为 `loopadmin` / `loopadmin123`。

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

启动 Expo 前显式设置 API 地址：

```bash
EXPO_PUBLIC_API_BASE_URL=http://<电脑局域网IP>:3000/loop npx expo start
```

手机和电脑需要在同一局域网内，并确认防火墙允许手机访问电脑的 `3000` 和 `9000` 端口。

`make local-init` 会生成本地配置示例和 `deploy/local/secrets/` 目录。本地 MinIO 凭证在 `.env.example` 中直接使用字符串环境变量；需要自行放入 JWT RSA 私钥/公钥文件，并按需调整 `deploy/local/.env` 中的数据库、Redis、S3 endpoint、配置文件路径。

`.env` 使用 shell `source` 加载，包含 `&`、空格等特殊字符的值需要加引号，例如 PostgreSQL URL。

常用命令：

```bash
make db-status
make db-migrate
make fmt-check
make check
make clippy
make test-event
```

开发阶段数据库 schema 只保留一份当前初始化 migration，迁移文件位于 `srv/migrations/`。本地执行初始化前需要安装 Diesel CLI：

```bash
cargo install diesel_cli --no-default-features --features postgres
```

`make db-migrate` 会读取 `deploy/local/.env` 中的 `DATABASE_URL`，未设置时使用 `LOOP_PG_CONN` 并导出为 Diesel CLI 使用的 `DATABASE_URL`。当前仍处于开发阶段，数据库表结构、索引和约束以这份初始化 migration 为准；后续 schema 变更直接更新初始化脚本，并重建本地数据库，避免历史 migration 引入兼容分支和冗余逻辑。

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
  "event.read",
  "event.join",
  "event.create",
  "media.upload",
  "community.post.create",
]

organizer = [
  "event.read",
  "event.join",
  "event.create",
  "event.update_own",
  "media.upload",
]

admin = [
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
