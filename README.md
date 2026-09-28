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
- 基础设施：PostgreSQL、Redis、RabbitMQ、Meilisearch、SeaweedFS，使用持久卷保存数据；邮件通过配置接入 SMTP 服务。
- 初始化任务：执行现有数据库 migration，创建媒体 bucket 并配置读取权限。
- 可选监控：OpenTelemetry Collector、Tempo、Loki、Alloy、Prometheus、Grafana。

部署顺序为：构建并推送镜像 → 准备 ConfigMap 和 Secret → 启动基础设施 → 完成初始化 Job → 启动业务服务。开发联调通过 `kubectl port-forward` 访问，后续可按集群环境接入域名、TLS 和入口路由。

镜像构建、凭证准备、部署命令和排查方法见 [Kubernetes 部署说明](deploy/k8s/README.md)。

## 本地开发

### Guix 统一开发环境

[guix/env.scm](guix/env.scm) 统一提供 Android SDK/NDK/JDK、模拟器、Rust/Cargo/rustfmt/Clippy、Node/npm、C/C++ 工具链、CMake、pkg-config、OpenSSL、PostgreSQL 客户端库。从仓库根目录进入：

```bash
guix shell -L guix/modules -m guix/env.scm
```

可以在多个终端使用同一命令，分别运行模拟器、`make backend-run` 和前端。普通 Guix shell 保留宿主网络、桌面、KVM 及用户缓存。Podman/Compose 由宿主机安装，容器先在宿主机通过 `make infra-up` 单独启动和初始化；Guix manifest 不包含容器工具。Android 模拟器访问宿主 API 使用 `10.0.2.2`。完整的后端构建、前端启动、Android 安装及地址配置见 [统一开发环境说明](guix/README.md)。iOS 构建仍需要 macOS/Xcode。

```bash
cd srv
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings

cd ../app
npm ci
npm run build:rich-editor
npx tsc --noEmit
npm run lint
```

Gradle/npm 下载的 Linux 预编译程序可能要求 `/lib64/ld-linux-x86-64.so.2` 等标准路径；在 Guix System 上需要 FHS 时，使用同一份 manifest 进入构建容器：

```bash
guix shell -C -N -F -L guix/modules -m guix/env.scm
```

容器内编译与普通环境使用相同软件清单，但宿主上的模拟器和 Podman 应继续在普通 shell 中运行。Android NDK 编译需按 [文档](guix/README.md) 清除宿主 GCC 的头文件与链接搜索路径。容器的缓存持久化和 ADB 连接方式也在该文档中说明。

软件包取自当前 Guix channel；`guix/channels.scm` 保存可复现的 channel 提交。若 Rust 版本不满足 Cargo.lock 中依赖的要求，在宿主机更新提供工具链的 channel 后重新进入环境，不降低项目依赖版本。网络下载失败时先检查宿主网络和代理配置。

### 启动本地服务

仓库根目录的 `Makefile` 分别提供容器初始化和业务进程启动入口。首次使用先在宿主机准备单机环境文件和 JWT 密钥：

```bash
cp deploy/standalone/examples/.env.example deploy/standalone/.env
mkdir -p deploy/standalone/secrets srv/log
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
  -out deploy/standalone/secrets/jwt_private.pem
openssl rsa -pubout \
  -in deploy/standalone/secrets/jwt_private.pem \
  -out deploy/standalone/secrets/jwt_public.pem
make infra-up

# 容器初始化成功后进入开发环境，再启动业务进程。
guix shell -L guix/modules -m guix/env.scm
make backend-run
```

`make infra-up` 在宿主机启动 Postgres、Redis、RabbitMQ、Meilisearch 和 SeaweedFS 等容器，并完成 bucket 初始化和数据库 migration。`make backend-run` 在开发环境中运行 API、realtime 与 worker，不调用容器工具。`make backend-up` 保留为按顺序执行这两个步骤的便捷入口，需要当前环境同时具备宿主容器工具和 Cargo。RabbitMQ AMQP 地址为 `127.0.0.1:5672`，管理界面为 `http://127.0.0.1:15672`，示例账号为 `loop` / `looprabbit123`。Meilisearch 地址为 `http://127.0.0.1:7700`。SeaweedFS S3 地址为 `http://127.0.0.1:9000`（映射容器的 `8333` 端口），示例访问密钥为 `loopadmin` / `loopadmin123`。只发布 S3 端口，关闭 Admin UI、WebDAV 和数据湖接口。

如果使用 Android 真机上的 Expo Go 调试客户端，手机不能访问电脑上的 `127.0.0.1`。需要手动把本地服务地址配置成电脑局域网 IP，例如 `192.168.1.23`。

后端监听地址需要允许局域网访问：

```bash
LOOP_HTTP_ADDR='0.0.0.0:3000'
```

SeaweedFS 预签名上传地址也必须使用手机可访问的地址，否则插入图片会在直传对象存储时失败：

```bash
LOOP_S3_PRESIGN_ENDPOINT_URL='http://<电脑局域网IP>:9000'
LOOP_S3_PUBLIC_BASE_URL='http://<电脑局域网IP>:9000/loop-local'
```

启动 Expo 前显式设置 API 和 WebSocket 地址：

```bash
EXPO_PUBLIC_API_BASE_URL=http://<电脑局域网IP>:3000/loop \
EXPO_PUBLIC_REALTIME_URL=ws://<电脑局域网IP>:3010/loop/realtime \
npx expo start
```

手机和电脑需要在同一局域网内，并确认防火墙允许手机访问电脑的 `3000`、`3010` 和 `9000` 端口。未设置 `EXPO_PUBLIC_REALTIME_URL` 时客户端会退回短轮询，功能仍可使用，但跨设备消息会有数秒延迟。

`make backend-run` 会同时启动 API、realtime 和 worker；任一进程退出时会结束另外两个进程并返回对应退出码，避免本地开发时异步消费静默停止。

单机示例中的 RabbitMQ 与 SeaweedFS 凭证直接使用字符串环境变量；需要按实际环境调整 `deploy/standalone/.env` 中的数据库、Redis、RabbitMQ、S3 endpoint、配置文件路径和密码。

单机默认通过 `COMPOSE_FILE='compose.yaml:compose.seaweedfs.yaml'` 加载本地对象存储。`weed mini` 使用 `loop-seaweedfs-data` 卷持久化数据，`s3-init` 使用 AWS CLI 创建 bucket（已存在则复用），然后配置匿名 `GetObject` 和浏览器 GET/HEAD/PUT CORS；匿名上传、删除和列举不开放。`make infra-up` 等待初始化完成后退出，成功后再执行 `make backend-run`。CORS 当前允许所有来源用于开发，部署到公网前修改 [cors.json](deploy/s3/cors.json) 中的 `AllowedOrigins` 为实际前端域名。

已存在的 `deploy/standalone/.env` 不要覆盖，补充或调整下面的配置；现有 bucket 和访问密钥可以继续使用：

```bash
COMPOSE_FILE='compose.yaml:compose.seaweedfs.yaml'
LOOP_S3_API_PORT='9000'
LOOP_S3_INTERNAL_ENDPOINT_URL='http://seaweedfs:8333'
LOOP_S3_ENDPOINT_URL='http://127.0.0.1:9000'
LOOP_S3_PRESIGN_ENDPOINT_URL='http://127.0.0.1:9000'
LOOP_S3_PUBLIC_BASE_URL='http://127.0.0.1:9000/loop-local'
```

`LOOP_S3_ENDPOINT_URL` 用于宿主机运行的后端，`LOOP_S3_INTERNAL_ENDPOINT_URL` 用于 API 容器，`LOOP_S3_PRESIGN_ENDPOINT_URL` 用于客户端直传。真机调试时将预签名地址和 `LOOP_S3_PUBLIC_BASE_URL` 改成电脑局域网 IP。旧的 `LOOP_MINIO_API_PORT`、`LOOP_MINIO_CONSOLE_PORT` 配置可以删除。若旧 MinIO 容器仍占用 9000 端口，先手动停止它；旧 MinIO 数据卷不会被读取或删除，也不能直接挂载给 SeaweedFS，有文件需要保留时应另行通过 S3 复制。

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

开发阶段数据库 schema 只保留一份当前初始化 migration，迁移文件位于 `srv/migrations/`。`make infra-up` 使用迁移容器执行，无需在 Guix 开发环境安装 Diesel CLI。当前仍处于开发阶段，数据库表结构、索引和约束以这份初始化 migration 为准；初始化 migration 发生变更后需要手动重建本地开发数据库，再重新执行 `make infra-up`。

### 切换到 AWS S3

业务服务保持使用标准 AWS S3 SDK。目标 bucket、权限和 CORS 在云端预先配置好后，单机环境只需调整 `.env` 并重启：

```bash
COMPOSE_FILE='compose.yaml'
LOOP_S3_BUCKET='your-bucket'
LOOP_S3_REGION='ap-southeast-1'
LOOP_S3_INTERNAL_ENDPOINT_URL=''
LOOP_S3_ENDPOINT_URL=''
LOOP_S3_PRESIGN_ENDPOINT_URL=''
LOOP_S3_FORCE_PATH_STYLE='false'
LOOP_S3_ACCESS_KEY_ID='your-access-key'
LOOP_S3_SECRET_ACCESS_KEY='your-secret-key'
# 使用临时凭证时还需设置 LOOP_S3_SESSION_TOKEN。
LOOP_S3_PUBLIC_BASE_URL='https://media.example.com'
```

此配置不启动 SeaweedFS，也不运行本地 `s3-init`；已运行的本地 SeaweedFS 可以手动停止。清空自定义 endpoint 后由 SDK 根据 region 选择 AWS S3 地址。访问凭证当前通过环境变量注入，不支持自动使用实例角色凭证链。已有文件需复制到目标 bucket 并保留对象 key；媒体完整公开 URL 会写入数据库，建议从一开始就使用稳定的媒体域名，切换时保持 URL 路径不变。变更存储不会自动改写历史链接。

## agents
* 暂时没有产生数据，不需要新增migration
