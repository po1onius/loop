# K8s 集群部署

该目录目前只保留集群部署结构，具体 manifests、Helm values 和环境 overlay 后续再补充。

建议边界：

- `base/`：业务服务的通用 Kustomize base，例如 Namespace、Deployment、Service、ConfigMap、Secret 示例。
- `overlays/dev/`：开发或测试集群差异配置。
- `overlays/prod/`：生产集群差异配置。
- `helm-values/`：PostgreSQL、Redis、对象存储、Prometheus、Grafana、Loki、Tempo、OpenTelemetry Collector、Alloy 等成熟组件的 Helm values。

集群部署不要直接复用 `deploy/standalone` 的 Compose 配置。两者在服务发现、日志采集、Secret 注入、存储和网络暴露方式上差异较大，应单独维护。
