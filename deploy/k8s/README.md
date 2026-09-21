# Kubernetes 开发部署

这套清单使用 Kustomize，所有资源放在 `loop` 命名空间，适合当前开发阶段的单副本部署。

| 目录 | 内容 |
| --- | --- |
| `config/` | 业务 TOML、基础设施地址和普通环境变量 |
| `infra/` | PostgreSQL、Redis、RabbitMQ、Meilisearch、MinIO，均使用 StatefulSet 和 PVC |
| `bootstrap/` | 数据库初始化、媒体 bucket 初始化 Job |
| `apps/` | API、realtime、worker 的 Deployment，以及两个 HTTP Service |
| `observability/` | 可选的 OpenTelemetry Collector、Tempo、Loki、Alloy、Prometheus、Grafana |

部署前需要可访问的 Kubernetes 集群、`kubectl`、Kustomize、镜像构建工具及镜像仓库。集群需要默认 StorageClass，或在各 StatefulSet 的 `volumeClaimTemplates` 中显式填写 `storageClassName`。基础设施申请 23 GiB 存储，完整监控额外申请 16 GiB；实际使用量和内存需求随数据增长调整。

Guix 可用 `guix shell kubectl kustomize openssl` 临时提供命令行工具；若本地 channel 的 kubectl 版本较旧，需先更新 channel，并选用与集群版本兼容的客户端。

## 1. 构建镜像

以下命令在仓库根目录执行，把镜像仓库改为实际地址；使用 Podman 时可将 `docker` 替换为 `podman`。镜像需要与集群节点的 CPU 架构匹配。

```bash
export LOOP_REGISTRY=registry.example.com/loop
export LOOP_IMAGE_TAG=dev-001

docker build -f srv/loop-api-svc/Dockerfile -t "$LOOP_REGISTRY/loop-api-svc:$LOOP_IMAGE_TAG" .
docker build -f srv/loop-realtime-svc/Dockerfile -t "$LOOP_REGISTRY/loop-realtime-svc:$LOOP_IMAGE_TAG" .
docker build -f srv/loop-worker-svc/Dockerfile -t "$LOOP_REGISTRY/loop-worker-svc:$LOOP_IMAGE_TAG" .
docker build -f deploy/standalone/Dockerfile.migrate -t "$LOOP_REGISTRY/loop-migrate:$LOOP_IMAGE_TAG" .
docker build -f deploy/k8s/Dockerfile.minio --target server \
  -t "$LOOP_REGISTRY/loop-minio:RELEASE.2025-10-15T17-29-55Z" .
docker build -f deploy/k8s/Dockerfile.minio --target client \
  -t "$LOOP_REGISTRY/loop-minio-client:RELEASE.2025-08-13T08-35-41Z" .

for name in loop-api-svc loop-realtime-svc loop-worker-svc loop-migrate; do
  docker push "$LOOP_REGISTRY/$name:$LOOP_IMAGE_TAG" || exit 1
done
docker push "$LOOP_REGISTRY/loop-minio:RELEASE.2025-10-15T17-29-55Z"
docker push "$LOOP_REGISTRY/loop-minio-client:RELEASE.2025-08-13T08-35-41Z"
```

修改 `apps/`、`infra/`、`bootstrap/` 下 `kustomization.yaml` 的 `images`，使 `newName` 和 `newTag` 与推送的镜像一致。私有仓库还需要在 `loop` 命名空间创建拉取凭证，并为相应工作负载配置 `imagePullSecrets`。

初始化镜像包含当前 `srv/migrations/`，修改 migration 后需重新构建。MinIO 社区版已归档，最新发布要求自行构建镜像，因此提供了固定 release 的 [Dockerfile.minio](Dockerfile.minio)；后续可评估替换为其他 S3 兼容存储。[MinIO 发布说明](https://github.com/minio/minio/releases/tag/RELEASE.2025-10-15T17-29-55Z)

## 2. 准备配置和 Secret

```bash
cp deploy/k8s/examples/.env.example deploy/k8s/.env
mkdir -p deploy/k8s/secrets
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
  -out deploy/k8s/secrets/jwt_private.pem
openssl rsa -pubout \
  -in deploy/k8s/secrets/jwt_private.pem \
  -out deploy/k8s/secrets/jwt_public.pem
```

编辑 `.env`，替换所有 `replace-*` 值。连接串内的密码必须与对应服务密码一致，特殊字符需要 URL 编码；这里的文件由 `kubectl --from-env-file` 读取，值不要加 shell 引号，也不要写变量引用。本地 `.env` 和 JWT 文件已被 Git 忽略，已有密钥无需重新生成。

编辑 [config/runtime.env](config/runtime.env)：

- 填写真实的 SMTP 发件人、用户名和域名，在 `.env` 中填写对应 token；当前邮件实现通过 SMTP relay 发送，部署清单不自建邮件服务器。
- `LOOP_S3_ENDPOINT_URL` 是后端访问 MinIO 的集群内地址；`LOOP_S3_PRESIGN_ENDPOINT_URL` 和 `LOOP_S3_PUBLIC_BASE_URL` 是客户端访问地址。本机联调可使用默认的 `127.0.0.1:9000` 和端口转发；真机调试改为电脑局域网 IP。
- `LOOP_ENV=local` 会启用现有 API 的开发 CORS，方便 Expo Web 联调。
- 业务权限和有效期配置位于 [config/loop-api-svc.toml](config/loop-api-svc.toml)。

```bash
kubectl apply -f deploy/k8s/namespace.yaml
kubectl -n loop create secret generic loop-secrets \
  --from-env-file=deploy/k8s/.env --dry-run=client -o yaml | kubectl apply -f -
kubectl -n loop create secret generic loop-jwt \
  --from-file=jwt_private.pem=deploy/k8s/secrets/jwt_private.pem \
  --from-file=jwt_public.pem=deploy/k8s/secrets/jwt_public.pem \
  --dry-run=client -o yaml | kubectl apply -f -
kubectl apply -k deploy/k8s/config
```

各容器按需读取 Secret 字段，realtime 仅挂载 JWT 公钥，API 同时挂载公私钥。

## 3. 启动依赖并初始化

```bash
kubectl apply -k deploy/k8s/infra
for name in db cache rabbitmq search minio; do
  kubectl -n loop rollout status "statefulset/$name" --timeout=300s || exit 1
done

kubectl apply -k deploy/k8s/bootstrap
kubectl -n loop wait --for=condition=complete job/loop-migrate job/loop-minio-init --timeout=600s
kubectl -n loop logs job/loop-migrate
kubectl -n loop logs job/loop-minio-init
```

两个 Job 成功后再启动业务服务。数据库 Job 只执行现有的初始化 migration；MinIO Job 创建 `loop-dev` bucket，并按当前媒体访问方式开放该 bucket 的匿名读取，写入仍需凭证或预签名 URL。

Kubernetes 不提供 Compose 的 `depends_on`，因此这里显式分阶段部署。初始化失败时先查看 Job 日志并修正配置，再删除对应的失败 Job、重新应用 `bootstrap/`。Job 的 Pod 模板不可原地修改；需要重新运行时同样先删除对应 Job。完成的 Job 会保留，便于查看日志。

当前初始化 migration 的 SQL 发生变化后，重新运行已成功的 Job 不会自动改写数据库表结构。开发阶段仍按根 README 的约定，手动重建可丢弃的开发数据库后再初始化。

## 4. 启动应用并访问

```bash
kubectl apply -k deploy/k8s/apps
for name in loop-api-svc loop-realtime-svc loop-worker-svc; do
  kubectl -n loop rollout status "deployment/$name" --timeout=300s || exit 1
done
```

API 和 realtime 使用现有 `/metrics` 路由做启动、就绪和存活探针，这只能检查 HTTP 进程能否响应。worker 没有 HTTP 监听和健康接口，当前依赖进程失败后由 Kubernetes 重启；Deployment 就绪不代表消费进度正常，应结合 `worker.start` 日志和队列状态确认。

分别在三个终端中保持端口转发运行：

```bash
kubectl -n loop port-forward svc/loop-api-svc 3000:3000
kubectl -n loop port-forward svc/loop-realtime-svc 3010:3010
kubectl -n loop port-forward svc/minio 9000:9000
```

客户端 API 地址为 `http://127.0.0.1:3000/loop`，实时地址为 `ws://127.0.0.1:3010/loop/realtime`。真机联调时，给端口转发增加 `--address=0.0.0.0`，客户端和 S3 外部地址均使用电脑局域网 IP，并放行对应防火墙端口。

所有 Service 默认仅供集群内访问。接入域名时，根据集群现有入口配置 API、WebSocket 和 S3 路由及 TLS，保持 S3 请求的 Host 和路径不被改写，否则预签名校验会失败。

## 5. 按需开启监控

```bash
kubectl apply -k deploy/k8s/observability
for name in tempo loki prometheus grafana; do
  kubectl -n loop rollout status "statefulset/$name" --timeout=300s || exit 1
done
for name in otel-collector alloy; do
  kubectl -n loop rollout status "deployment/$name" --timeout=300s || exit 1
done
```

取消 `config/runtime.env` 中 `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` 的注释，再应用配置并重启应用：

```bash
kubectl apply -k deploy/k8s/config
kubectl -n loop rollout restart deployment/loop-api-svc deployment/loop-realtime-svc deployment/loop-worker-svc
kubectl -n loop port-forward svc/grafana 3001:3000
```

Grafana 地址为 `http://127.0.0.1:3001`，初始账号密码来自 `.env`。已预配置 Prometheus、Tempo、Loki 数据源；Prometheus 采集 API 和 realtime 的指标，Alloy 通过 Kubernetes API 采集三个业务服务的标准输出，调用链经 Collector 写入 Tempo。Alloy 仅拥有当前命名空间的 Pod 发现和日志读取权限。

Tempo 使用 3.x 的单进程本地存储配置，保留 24 小时调用链；Loki 和 Prometheus 保留 7 天数据。监控存储均使用 PVC，Alloy 自身临时状态使用 `emptyDir`。

## 更新与排查

应用更新使用新的镜像标签，修改 `apps/kustomization.yaml` 后再次 `kubectl apply -k deploy/k8s/apps`。普通配置使用固定名称 ConfigMap，修改后需要重新应用 `config/` 并重启相关应用；Secret 环境变量和 JWT 变更也需要重启。监控配置带内容哈希，重新应用 `observability/` 会触发对应工作负载更新。

```bash
kubectl -n loop get pods,svc,pvc,jobs
kubectl -n loop get events --sort-by=.metadata.creationTimestamp
kubectl -n loop logs deployment/loop-api-svc --tail=100
kubectl -n loop logs deployment/loop-realtime-svc --tail=100
kubectl -n loop logs deployment/loop-worker-svc --tail=100
```

PVC 长期 Pending 时检查默认 StorageClass 和存储供应器；ImagePullBackOff 时检查镜像地址、标签及拉取凭证；CrashLoopBackOff 时查看 `kubectl logs --previous`。数据库、RabbitMQ 等组件在已有数据卷上的账号不会随初始化环境变量自动重建，需要在组件内显式更新凭证。

三个业务服务目前同时写标准输出和本地日志，清单为本地日志挂载 1 GiB `emptyDir`；长时间运行需关注容量，后续可统一为标准输出日志。扩容时还需要补充按 Pod 发现的指标采集、worker 消费进度监控，以及 HTTP/WebSocket 的优雅退出。

渲染检查不需要连接集群：

```bash
for part in config infra bootstrap apps observability; do
  kustomize build "deploy/k8s/$part" > "/tmp/loop-$part.yaml" || exit 1
done
```

基础设施版本固定为编写时核对的稳定发布，后续升级时显式更新标签并查看对应发布说明：

| 组件 | 版本与发布说明 |
| --- | --- |
| PostgreSQL | [18.6](https://www.postgresql.org/docs/release/) |
| Redis | [8.10.2](https://github.com/redis/redis/releases/tag/8.10.2) |
| RabbitMQ | [4.3.6](https://www.rabbitmq.com/docs/download) |
| Meilisearch | [1.54.0](https://github.com/meilisearch/meilisearch/releases/tag/v1.54.0) |
| MinIO / mc | [2025-10-15](https://github.com/minio/minio/releases/tag/RELEASE.2025-10-15T17-29-55Z) / [2025-08-13](https://github.com/minio/mc/releases/tag/RELEASE.2025-08-13T08-35-41Z) |
| OpenTelemetry Collector | [0.161.0](https://github.com/open-telemetry/opentelemetry-collector-releases/releases/tag/v0.161.0) |
| Tempo / Loki | [3.0.3](https://github.com/grafana/tempo/releases/tag/v3.0.3) / [3.7.8](https://github.com/grafana/loki/releases/tag/v3.7.8) |
| Alloy / Prometheus | [1.19.2](https://github.com/grafana/alloy/releases/tag/v1.19.2) / [3.14.0](https://github.com/prometheus/prometheus/releases/tag/v3.14.0) |
| Grafana | [13.2.2](https://grafana.com/grafana/download?edition=oss) |
