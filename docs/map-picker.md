# 活动地图选点

地图页面由 API 服务提供：`GET /loop/maps/picker`，脚本位于同目录的 `picker.js`。原生客户端通过 WebView 打开，Web 通过 iframe 打开。搜索和坐标转换使用高德 JS API 2.0；不需要额外部署网页服务，也不需要新增原生地图 SDK。

## 配置高德

在高德开放平台申请 **Web 端（JS API）** Key 和配套安全密钥，不要使用 Android、iOS 或 Web 服务类型的 Key。为页面实际使用的域名配置白名单，并按项目用途开通相应服务和调用额度。配置方式参见 [官方安全密钥文档](https://lbs.amap.com/api/javascript-api-v2/guide/abc/jscode)。

在 `deploy/standalone/.env` 中补充以下配置（不要覆盖已有文件）：

```bash
LOOP_AMAP_JS_KEY='实际的JS API Key'
LOOP_AMAP_SECURITY_CODE='配套的安全密钥'
```

`make backend-run` 读取此文件；Compose 已向 API 容器传入两个变量。修改后重启 API。两者同时为空时，其他功能正常运行，地图页面返回 503；只配置一个时 API 启动失败并明确提示缺少配套配置。JS Key 会出现在网页中，安全密钥不会发给客户端，也不要放入 `EXPO_PUBLIC_*`。

Kubernetes 在 `deploy/k8s/.env` 中填写同名变量（不加 shell 引号），按 [部署说明](../deploy/k8s/README.md) 更新 `loop-secrets`，再重启 API Deployment。对应 Secret 字段仅注入 API 容器。

客户端继续使用 `EXPO_PUBLIC_API_BASE_URL`，例如：

```bash
EXPO_PUBLIC_API_BASE_URL=https://api.example.com/loop npx expo start
```

地图、脚本及 `/_AMapService` 代理必须处于相同 API 域名和路径前缀下。公网入口将 `/loop/maps/` 转发到 API，保留路径和查询参数。Web 页面跨域嵌入时，入口不要为选点页设置禁止嵌入的 `X-Frame-Options: DENY/SAMEORIGIN`；如设置 CSP `frame-ancestors`，加入实际 Web 前端域名。

代理为高德 JS SDK 的请求追加 `jscode`，只允许选点所需的地点查询、逆地理编码、坐标转换和城市定位接口，校验 Key，不接受任意上游 URL，不转发客户端 Cookie/Authorization，也不跟随上游跳转。它是供 SDK 使用的公开接口，不使用活动 API 的 Bearer token；公网部署时在入口为 `/loop/maps/_AMapService/` 设置限流，并配置高德调用额度。页面地址片段承载回显地点，不进入 HTTP 请求；地图业务日志记录阶段、结果数量、耗时和错误类别，不记录密钥或用户坐标。入口及通用 HTTP trace 的查询参数日志需按实际部署设置脱敏。

## 定位权限与开发环境

只有点击“定位到我这里”才调用浏览器定位。取消或拒绝权限不影响搜索与拖动选点。浏览器定位结果先经高德 `convertFrom(..., "gps")` 转换成 GCJ-02，再展示并保存；不在业务代码中自行实现坐标转换。

WebView/浏览器定位需要安全上下文。真机通过 `http://局域网IP` 访问时，不保证能定位；请为 API 配置设备信任的 HTTPS 域名和证书。不要通过关闭 TLS 校验或放宽 WebView 安全限制解决环境问题。Web 前端也使用 HTTPS；如配置 `Permissions-Policy`，允许地图 iframe 的来源使用 geolocation。搜索、拖动选点本身不需要定位权限。

`app/app.config.js` 已声明 Android 粗略／精确位置权限以及 iOS `NSLocationWhenInUseUsageDescription`，不申请后台定位。Android 点击定位时同时申请粗略与精确权限，符合 Android 12+ 的要求；当前 WebView 定位需要精确权限，仅允许粗略位置时提示继续搜索或拖动选点。现有安装包需要重新构建：

```bash
cd app
npx expo prebuild --platform android --no-install
npm run android
```

Guix System 按 [开发环境说明](../guix/README.md) 在构建前清除宿主 NDK 头文件相关环境变量；iOS 在 macOS 上重新生成／构建原生工程。仅重启 Metro 不会更新安装包权限。

## 数据与发布

活动新增 `location_latitude`、`location_longitude`、`location_provider`、`location_coordinate_system`、`location_poi_id`、`location_note`，保留名称和地址。位置可以整体为空；选定位置时名称、地址、经纬度、`amap` 提供商和 `GCJ-02` 坐标系必须完整。服务端校验坐标范围，数据库 CHECK 保证位置字段成组存在，不使用外键。

草稿新增、更新、发布、活动详情和搜索响应都包含位置字段；清除地点同时清除坐标、POI 和补充说明，取消选点保留原值。换成不同坐标时清除旧楼层说明，避免带入其他场地。暂未增加“附近活动”或距离排序。

遵循 README 的开发约定，只修改 `srv/migrations/20260515000000_init_schema/up.sql`，没有新增历史数据迁移。**已有开发数据库需要由开发者确认数据可丢弃后手动重建，再执行 `make infra-up`**；仅重启 API 或重跑已执行的 migration 不会增加字段。同步更新 API、worker、realtime 和客户端，避免共用模型字段不一致。Meilisearch 的已有活动文档需要由 worker 启动重建同步。

## 联调检查

配置 Key 后打开 `/loop/maps/picker`，应能加载地图与搜索结果。直接浏览器打开用于检查页面；确认结果回填需在 App 中进行。

- 搜索不同城市的地点、选择附近地点、拖动到没有 POI 的位置，检查名称和地址与坐标一致。
- 授予／拒绝定位权限，检查仅主动定位请求权限，拒绝后仍能搜索。
- 打开已有位置再取消，原值不变；确认新位置、保存并重新打开草稿，位置正确回显。
- 清除地点后保存草稿，检查详情／接口中的所有位置字段为空。
- 发布后查看详细地址、补充说明和导航目标；检查搜索结果同样携带位置字段。
- 快速输入关键词、拖动地图，确认旧查询不会覆盖新选择；网络错误时展示重试提示。

调试时查看客户端 `[map-picker]` 与服务端 `maps.*` 日志。Key/域名错误、额度耗尽、TLS 或网络错误需要调整实际配置；不通过业务兜底伪造地点。首次上线仍需真实高德凭证及 Android/iOS 设备联调。
