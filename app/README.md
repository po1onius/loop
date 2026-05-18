# Loop App

React Native + Expo 客户端。

## 本地启动

先在仓库根目录启动后端依赖和事件服务：

```bash
make deps-up
make db-migrate
make dev-event
```

再启动 Expo：

```bash
npx expo start
```

## Android 真机 + Expo Go

真机不能访问电脑上的 `127.0.0.1` 或 Android 模拟器专用的 `10.0.2.2`。使用 Expo Go 真机调试时，先在 `deploy/local/.env` 中手动把本地服务地址配置成电脑局域网 IP：

```bash
LOOP_HTTP_ADDR='0.0.0.0:3000'
LOOP_S3_ENDPOINT_URL='http://<电脑局域网IP>:9000'
LOOP_S3_PUBLIC_BASE_URL='http://<电脑局域网IP>:9000/loop-local'
```

然后启动后端：

```bash
make dev-event
```

启动 Expo 前显式设置客户端 API 地址：

```bash
EXPO_PUBLIC_API_BASE_URL=http://<电脑局域网IP>:3000/loop npx expo start
```

手机和电脑需要在同一局域网内，并确认防火墙允许手机访问电脑的 `3000` 和 `9000` 端口。
