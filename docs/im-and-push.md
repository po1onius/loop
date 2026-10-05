# 活动群聊与系统通知

## 当前行为

- 活动正式发布时在同一事务中创建唯一 `event_group` 会话；保存草稿不建群。
- 发布者自动入群。无需审核的报名、被拒后重新报名成功、审核通过都会在报名事务中初始化已读位置并写入会话变更事件。
- 群成员资格由活动发布者和 `event_participations.status = joined` 决定。待审核者、被拒绝者、其他用户不可读写或订阅。
- 新成员可以查看历史消息，入群前的消息不计未读。不支持独立建群、邀请、单独退群、好友和私聊。活动自然结束后继续保留群聊；退出活动／移除参与者尚未提供接口。
- 文字、图片、引用回复、历史分页、已读、免打扰复用通用会话模块。活动详情可进入群聊，群内可查看成员及原活动。
- 每个登录客户端维持一条 WebSocket，同时接收个人通知和页面会话订阅。前台恢复、重连、会话订阅成功后补拉；会话页面每 30 秒补拉，未配置实时地址时每 4 秒补拉，聊天列表每 30 秒同步。
- 消息使用会话内递增序号；客户端重试复用消息 UUID，服务端串行处理同一会话内的并发重试。会话状态和消息正文以 HTTP API 返回为准。

## 消息链路

API 在业务事务内写 `async_outbox`，relay 投递 `conversation.changed.v1` CloudEvent。RabbitMQ 两个独立 durable 队列：

- `loop.conversation.realtime.v1`：发布 Redis 个人频道和会话频道，由 realtime 转发在线通知。
- `loop.conversation.push.v1`：生成 `push_deliveries`；FCM sender 检查设备绑定、当前群资格、免打扰和已读后发送。

每个队列有独立 `.dead` 死信队列。无效事件拒绝到死信；基础设施处理失败重新入队并退出 worker，让进程管理器重启。FCM 限流／临时失败最多尝试 5 次，至少从 60 秒起退避并遵守 `Retry-After`；永久失败与重试耗尽记录 `push.delivery.failed`。`UNREGISTERED` 会停用设备。

FCM 项目未配置时系统发送器关闭，在线 IM 仍可运行。推送任务保留，重新开启时超过 24 小时的消息跳过；任务记录 30 天后清理。FCM 接受请求不代表设备已展示，更不代表已读。进程在外部请求成功后、记录投递状态前退出时可能再次发送；通知使用消息 ID 作为 Android tag／APNs collapse ID，消息正文仍按 ID 去重。

## 本地升级

本次修改现有初始化 schema，增加 `push_devices` 和 `push_deliveries`，不新增生产数据兼容 migration，也没有外键。按照仓库开发约定，**已有开发数据库需要由开发者手动重建后再初始化**；仅重复运行 migration 不会补齐表。不要对需要保留的数据执行重建。

已有 API TOML 的 `user`、`organizer` 权限列表加入 `conversation.read`、`conversation.message.create`，移除旧 `community.message.create`。配置示例已更新，本地文件不会自动覆盖。

API、realtime、worker 和客户端需一起更新；WebSocket 协议由单会话认证变为用户认证：

```json
{"type":"authenticate","access_token":"<access token>"}
{"type":"subscribe","conversation_id":"<uuid>"}
{"type":"unsubscribe","conversation_id":"<uuid>"}
```

服务端回复 `ready`、`subscribed`，推送 `conversation.message_created`（序号以字符串发送）或 `conversation.changed`。一个连接最多订阅 32 个会话；订阅和转发前都校验访问资格，JWT 到期后关闭连接。

## Firebase 服务端配置

1. 创建 Firebase 项目并启用 Firebase Cloud Messaging HTTP v1 API。
2. 给发送身份授予发送消息所需的 IAM 权限，例如 Firebase Cloud Messaging API Admin。服务端采用 Google 官方 Rust 认证库的 ADC，可使用部署身份或服务账号文件。
3. 设置 `LOOP_FCM_PROJECT_ID`；服务账号文件通过 `GOOGLE_APPLICATION_CREDENTIALS` 指向。不要把服务账号私钥放入客户端配置或提交到仓库。

宿主 `make backend-run` 的 `.env` 示例：

```bash
LOOP_FCM_PROJECT_ID='your-firebase-project'
GOOGLE_APPLICATION_CREDENTIALS='/absolute/path/service-account.json'
```

Compose：文件放在 `${LOOP_HOST_SECRETS_DIR}/firebase/service-account.json`，worker 只挂载这个 Firebase 子目录。先手动创建目录并保证容器用户可读文件。只设置项目 ID 不提供有效凭证会使 worker 启动失败，应修正环境配置。

Kubernetes：

```bash
kubectl -n loop create secret generic loop-firebase \
  --from-file=service-account.json=/absolute/path/service-account.json \
  --dry-run=client -o yaml | kubectl apply -f -
```

在 `deploy/k8s/config/runtime.env` 设置项目 ID，重新应用 config/apps 并重启 worker。worker 还需要 `loop-secrets` 的 `REDIS_URL`，清单已接入。FCM 未启用时 Firebase Secret 可不存在。

## Android／iOS 客户端配置

客户端使用 React Native Firebase Messaging，直接取得 FCM token；`expo-notifications` 只负责 Android 通知频道配置，不经过 Expo Push Service。需要原生开发包或正式包，Expo Go 不支持。

1. 在 Firebase 注册 Android 应用，包名与 `app.json` 的 `android.package` 一致，默认 `com.anonymous.loop`。下载 `google-services.json` 放到 `app/`。
2. iOS 注册对应 bundle ID，默认同为 `com.anonymous.loop`。下载 `GoogleService-Info.plist` 放到 `app/`，在 Firebase 配置 APNs 密钥，并为 Apple 应用启用 Push Notifications。
3. 可用环境变量 `LOOP_ANDROID_PACKAGE`、`LOOP_IOS_BUNDLE_ID`、`LOOP_GOOGLE_SERVICES_ANDROID`、`LOOP_GOOGLE_SERVICES_IOS` 指定应用标识和配置文件路径。生产 iOS 构建设置 `LOOP_APNS_ENVIRONMENT=production` 并使用匹配的签名配置。
4. 设置 API 和实时地址，重新生成并构建原生项目：

```bash
cd app
npm ci
export EXPO_PUBLIC_API_BASE_URL=http://10.0.2.2:3000/loop
export EXPO_PUBLIC_REALTIME_URL=ws://10.0.2.2:3010/loop/realtime
npx expo prebuild --platform android --no-install
npm run android -- --device
```

Guix System 的 FHS、NDK 环境要求继续按 `guix/README.md` 操作。已有不包含 Firebase 模块的开发包需要重新安装。iOS 需要 macOS/Xcode 和有效签名。

Android 13+ 会申请通知权限；Android 需要可用的 Google 服务和 FCM 网络连接。无 GMS 设备不在当前 FCM 覆盖范围。iOS 通过 APNs 投递。

前台收到通知时，当前会话仅更新消息，其他会话展示应用内横幅；后台由系统展示通知。点击通知会验证当前账号与会话权限后进入群聊。登录注册设备、token 轮换上报，退出登录解除注册并请求删除本机 FCM token；没有网络时撤销无法即时确认，日志会说明失败，已交给系统的通知也无法撤回。

## 联调检查

用发布者、普通用户、待审核用户分别检查：发布建群、直接报名入群、审核通过入群、拒绝后重新申请、非成员 HTTP／WebSocket 访问拒绝、重复发送不重复入库、免打扰与已读抑制推送、多设备列表更新、后台通知、冷启动点击通知、重连补消息、退出登录和切换账号。

FCM 真机检查需要有效项目、原生配置、凭证和通知权限；静态检查不能证明实际投递成功。运行日志可按 conversation ID、message ID、registration ID 关联；不记录完整设备 token、私钥或消息正文。

参考：[FCM HTTP v1](https://firebase.google.com/docs/cloud-messaging/send/v1-api)、[React Native Firebase Messaging](https://rnfirebase.io/messaging/usage)、[FCM 错误码](https://firebase.google.com/docs/cloud-messaging/error-codes)。
