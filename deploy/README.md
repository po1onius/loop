# 部署目录

`deploy` 按部署形态拆分：

```text
deploy/
  standalone/  # 单机 Podman Compose 部署，当前可直接使用
  k8s/         # K8s 集群部署结构预留，后续再补充 manifests / Helm values
```

单机部署入口见 `deploy/standalone/README.md`。

K8s 集群部署不要直接复用单机 Compose 配置，后续应按集群服务发现、Secret、存储和观测采集方式单独维护。
