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
4. loop-search: API 与 worker 共用的活动搜索索引 schema、查询和写入逻辑
5. loop-svc-model: 服务端共享模型、消息契约及数据库操作；`messaging` 模块定义消息 envelope、版本约定和强类型 payload，`outbox` 模块负责消息持久化
6. loop-infra: 数据库、Redis、RabbitMQ、Meilisearch、对象存储、邮件、可观测性等基础设施封装

后端服务按运行特征拆分，而不是按页面或业务名提前拆分。当前阶段普通业务优先沉淀在 `loop-api-svc` 的内部模块中，避免社区、活动、报名、用户关系等高耦合功能过早跨服务调用。只有长连接、异步任务、媒体处理、搜索索引、通知推送等运行模型明显不同的能力，才在需要时拆成独立进程或 worker。

## k8s部署

[deploy/k8s/](deploy/k8s/README.md) 提供当前开发阶段的 Kubernetes 配置，使用 Kustomize 管理，默认部署到 `loop` 命名空间。

- 业务服务：API、realtime、worker，均采用单副本部署。
- 基础设施：PostgreSQL、Redis、RabbitMQ、Meilisearch、MinIO，使用持久卷保存数据；邮件通过配置接入 SMTP 服务。
- 初始化任务：执行现有数据库 migration，创建媒体 bucket 并配置读取权限。
- 可选监控：OpenTelemetry Collector、Tempo、Loki、Alloy、Prometheus、Grafana。

部署顺序为：构建并推送镜像 → 准备 ConfigMap 和 Secret → 启动基础设施 → 完成初始化 Job → 启动业务服务。开发联调通过 `kubectl port-forward` 访问，后续可按集群环境接入域名、TLS 和入口路由。

镜像构建、凭证准备、部署命令和排查方法见 [Kubernetes 部署说明](deploy/k8s/README.md)。

## 本地开发

### Guix 构建环境

根目录的 `guix.scm` 定义后端 Rust 和前端 Expo/TypeScript 共用的构建环境，包含 Rust、Cargo、rustfmt、Clippy、Node/npm、C/C++ 工具链、CMake、pkg-config、OpenSSL、CA 证书和 PostgreSQL 客户端库。软件包使用当前 Guix channels 提供的版本，更新环境前可在宿主机执行 `guix pull`。

从仓库根目录进入容器：

```bash
guix shell -C -N -F -D -f guix.scm
```

`-C` 创建隔离容器，`-N` 允许下载 Cargo/npm 依赖，`-F` 提供 npm 原生二进制可能需要的标准 Linux 路径。`-D -f guix.scm` 显式加载该开发包的依赖，不要求预先授权自动加载。当前仓库目录会自动以可写方式共享进容器，构建产物会保留在宿主机。

如果 crates.io 或 npm 下载超时，需要先调整宿主机网络；使用代理时，在上述命令中添加 `-E '^(https?|all|no)_proxy$|^(HTTPS?|ALL|NO)_PROXY$'`，把已设置的代理变量传入容器。如果 Cargo 报告 Rust 版本低于依赖的最低要求，需要更新提供 Rust 工具链的 Guix channel 后重新进入环境。

容器中的 home 是临时目录。进入后先把依赖缓存设置到仓库内，再执行构建：

```bash
# 在仓库根目录设置；.cache/guix/ 已加入 .gitignore。
export CARGO_HOME="$PWD/.cache/guix/cargo"
export npm_config_cache="$PWD/.cache/guix/npm"

cd srv
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings

cd ../app
npm ci
npm run build:rich-editor
npx tsc --noEmit
npm run lint
# 如需构建 Web 静态产物：
npx expo export --platform web
```

如果希望交互式 `guix shell -C` 自动读取配置，可在宿主机的仓库根目录执行一次：

```bash
mkdir -p ~/.config/guix
pwd -P >> ~/.config/guix/shell-authorized-directories
guix shell -C -N -F
```

自动加载只适用于交互式 shell；通过 `-- COMMAND` 执行命令时仍需显式使用 `-D -f guix.scm`。这些参数的完整含义见 [Guix shell 官方说明](https://guix.gnu.org/manual/devel/en/guix.html#Invoking-guix-shell)。

该环境用于后端构建、前端静态检查与 Web 导出。Android/iOS 原生安装包仍需各自的 SDK 和平台工具。`make backend-up` 涉及 Podman Compose，应在配置好 Podman 的宿主机执行；数据库、Redis、RabbitMQ 等运行服务不由这个构建容器启动。

### 启动本地服务

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

## agents
* 暂时没有生成数据，migration归一，暂时不需要新增
