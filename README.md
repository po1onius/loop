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

1. loop-event-svc: 服务器核心业务“活动”相关实现
2. loop-im-svc: 服务器活动对应群聊功能实现
3. loop-community-svc: 服务器社区板块功能实现
4. loop-svc-model: 服务器数据结构以及相关数据库操作

## K8s 配置约定

后端不依赖配置中心，部署到 K8s 时使用 `ConfigMap` 管理普通配置，使用 `Secret` 管理敏感配置。`loop-event-svc` 启动时会先读取 `LOOP_CONFIG_FILE` 指向的 TOML 文件，再用环境变量或 `*_FILE` secret 文件覆盖关键字段。

推荐挂载方式：

```text
ConfigMap -> /etc/loop/config/loop-event-svc.toml
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
| `LOOP_SMTP_TOKEN_FILE` | SMTP token secret 文件 |

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
  "community.post.create",
]

organizer = [
  "event.read",
  "event.join",
  "event.create",
  "event.update_own",
]

admin = [
  "event.*",
  "community.*",
  "user.manage",
]

[email]
from = "Loop <no-reply@example.com>"

[email.smtp]
sender = "smtp-user"
domain = "smtp.example.com"
```

JWT 私钥、公钥、数据库密码、SMTP token 等敏感信息不要写入 ConfigMap，使用 K8s Secret 以环境变量或文件方式传入。

## 本地开发

仓库根目录提供 `Makefile` 封装常用命令：

```bash
make local-init
cp deploy/local/.env.example deploy/local/.env
make deps-up
make db-migrate
make dev-event
```

`make local-init` 会生成本地配置示例和 `deploy/local/secrets/` 目录。需要自行放入 JWT RSA 私钥/公钥文件，并按需调整 `deploy/local/.env` 中的数据库、Redis、配置文件路径。

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

数据库 schema 使用 Diesel migrations 管理，迁移文件位于 `srv/migrations/`。本地执行迁移前需要安装 Diesel CLI：

```bash
cargo install diesel_cli --no-default-features --features postgres
```

`make db-migrate` 会读取 `deploy/local/.env` 中的 `DATABASE_URL`，未设置时使用 `LOOP_PG_CONN` 并导出为 Diesel CLI 使用的 `DATABASE_URL`。数据库表结构、索引和约束只通过 migrations 管理；后续 schema 变更请新增 migration，不要直接改历史 migration。

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
  "community.post.create",
]

organizer = [
  "event.read",
  "event.join",
  "event.create",
  "event.update_own",
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
