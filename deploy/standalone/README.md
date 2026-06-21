# 单机部署

`deploy/standalone` 保存本地或单机环境使用的 Compose 配置，覆盖数据库、缓存、对象存储和观测组件。

## 目录说明

```text
deploy/standalone/
  compose.yaml        # 单机 Compose 入口
  configs/            # Compose 挂载的 Prometheus、Loki、Tempo、Otel、Alloy、Grafana 配置
  examples/           # loop-event-svc 本地示例配置
  secrets/            # 本机 secret 文件目录，真实密钥禁止提交
```

## 常用命令

在仓库根目录执行：

```bash
cp deploy/standalone/examples/.env.example deploy/standalone/.env
make
```

`make` 等价于 `make backend-up`，会加载 `deploy/standalone/.env`，先通过 Podman Compose 启动 `compose.yaml` 中的 PostgreSQL、Redis、MinIO 和观测组件，再运行一次性 `migrate` 容器执行 `srv/migrations/` 下的 Diesel migration，最后用 `cargo run -p loop-event-svc` 前台启动后端业务服务。`compose.yaml` 使用 `${...}` 变量替换；如果手动执行 Compose，需要先进入 `deploy/standalone`，再加载 `.env`。

## 默认端口

| 组件 | 本机端口 | 说明 |
| --- | --- | --- |
| loop-event-svc | `3000` | 后端业务服务 |
| PostgreSQL | `5132` | 本地数据库 |
| Redis | `6319` | 本地缓存 |
| MinIO API | `9000` | S3 兼容对象存储 |
| MinIO Console | `9001` | MinIO 管理控制台 |
| OpenTelemetry Collector | `4317` | OTLP gRPC，供本机 cargo 服务上报 trace |
| Grafana | `3001` | 默认账号 `admin` / `admin` |

这些端口是 `.env.example` 的默认值；容器模式下后端宿主机端口使用 `LOOP_EVENT_SVC_PORT`，PostgreSQL 使用 `LOOP_PG_PORT`，Redis 和 MinIO 等 Compose 发布端口使用对应的 `LOOP_*_PORT` 变量。本地 `cargo run` 模式由 Makefile 固定监听 `0.0.0.0:3000`。Prometheus、Loki、Tempo 和 Alloy 不映射到宿主机，只在 Compose 网络内供 Grafana 和采集链路访问。

## 注意事项

- 后端业务服务只读取 `DATABASE_URL` 和 `REDIS_URL`。单机 `.env` 里的 `PGDATABASE`、`PGUSER`、`PGPASSWORD`、`LOOP_PG_PORT`、`LOOP_REDIS_PASSWORD` 和 `LOOP_REDIS_PORT` 是部署层输入，Makefile 会为本机业务服务拼接宿主机连接串，Compose 会为容器依赖拼接容器内连接串。
- `LOOP_HOST_CONFIG_FILE`、`LOOP_HOST_SECRETS_DIR` 和 `LOOP_HOST_LOG_DIR` 是宿主机路径输入，相对路径以 `deploy/standalone` 为基准，供 Compose 挂载到容器内固定路径。容器内的 `LOOP_CONFIG_FILE`、JWT key 文件路径、`LOOP_LOG_DIR` 和邮件模板 glob 已在业务 Dockerfile 中固定。
- PostgreSQL 18 官方镜像的默认 `PGDATA` 是 `/var/lib/postgresql/18/docker`，但镜像 `VOLUME` 是 `/var/lib/postgresql`，单机卷应挂载到这个父目录。旧版本曾常用 `/var/lib/postgresql/data`，切换后建议重建本地数据库卷。
- PostgreSQL 不再通过 `/docker-entrypoint-initdb.d` 加载初始化 SQL；schema 统一由 `migrate` 一次性容器执行 Diesel migration。首次构建 `migrate` 镜像需要下载并编译 `diesel_cli`，后续会复用本地镜像缓存。
- 从旧 init SQL 方式切换过来的本地数据库卷没有 Diesel migration 记录，可能已经存在业务表。首次使用新方式前建议删除旧 PostgreSQL 卷后重建，否则 `migrate` 可能因为表已存在而失败。
- Prometheus 抓取宿主机 cargo 服务的 `host.containers.internal:3000/metrics`；本地 `cargo run` 模式固定使用 `3000` 端口。
- Makefile 会给本机 cargo 服务设置 `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT=http://127.0.0.1:4317`，链路追踪会经 OpenTelemetry Collector 写入 Tempo。
- Alloy 会读取宿主机 `srv/log/loop-event-svc.log*` 并写入 Loki；Makefile 会在启动前创建 `srv/log`。
- 如果 schema 变更，开发环境可以删除旧 PostgreSQL 卷后重新启动，让 Diesel migration 在干净数据库上重新执行。
- 单机 Redis 默认启用密码认证，密码来自 `LOOP_REDIS_PASSWORD`，真实环境必须修改默认值。
- 因为单机版会拼接 PostgreSQL 和 Redis URL，用户名、密码、库名如果包含 `@`、`:`、`/`、`?`、`#`、`&` 等 URL 保留字符，需要先做 URL 编码；生产或集群部署建议直接通过 Secret 注入完整 `DATABASE_URL` 和 `REDIS_URL`。
- 邮件供应商、OSS bucket、region、endpoint、public base URL、key prefix 和 path-style 属于部署环境配置，单机环境通过 `.env` 注入，不写入业务 TOML。
- `secrets/` 只保留 `.gitkeep`，JWT 私钥、公钥等真实密钥文件需要手动放入该目录。
- MinIO 社区版预编译镜像已经停止继续发布新版本；当前单机开发固定到 legacy release。生产或 K8s 集群部署时应重新评估对象存储方案。
- `make` 运行本机业务服务进程，日志写入 `srv/log`；Compose 里的观测组件仍会随依赖启动。
