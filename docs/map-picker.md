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

## 活动距离

首页活动列表、搜索结果和活动详情显示活动距用户的**直线距离**。不足 100 米显示“距你不足 100 米”，其余距离按百米取整，满一公里使用公里并保留一位小数。列表顶部说明直线距离，详情标注“距你直线约 …”；没有完整活动坐标时不显示距离。

### 服务配置

在高德控制台额外申请 **Web 服务** 类型 Key，开通[坐标转换服务](https://lbs.amap.com/api/webservice/guide/api/convert)，配置服务端出口 IP 白名单及调用额度。这个 Key 与选点页的 Web 端 JS API Key 不同，只注入 API 服务：

```bash
# deploy/standalone/.env；不要覆盖已有环境文件
LOOP_AMAP_WEB_SERVICE_KEY='实际的Web服务Key'
```

重启 API 服务后生效。Compose 已透传该变量；Kubernetes 在 `deploy/k8s/.env` 中填写同名变量（不加引号），更新 `loop-secrets` 并重启 API Deployment。未配置时坐标转换接口返回 503，客户端提示距离服务暂不可用。

客户端使用 `expo-location` 获取设备位置，通过 `POST /loop/maps/coordinates/convert` 将 WGS-84 转换为 GCJ-02，再使用 `geolib` 本地计算整页活动距离。请求 JSON 为 `{ "latitude": 39.9, "longitude": 116.4 }`，响应同名字段表示 GCJ-02；经纬度校验范围分别为 ±90 和 ±180。该公开接口供未登录浏览使用，公网入口需为此路径设置限流。坐标只在请求体传输，不进入 URL，不落库、不写入业务日志；请勿在入口开启该接口的请求／响应体记录。高德 Key 仅用于服务端向高德发出的请求，客户端不得配置为 `EXPO_PUBLIC_*`。

### 客户端行为与构建

- 首次点击“开启定位”申请前台权限；进入页面时只检查已有权限。拒绝后仍可浏览活动，可点击重试或进入系统设置开启权限。Web 拒绝后按页面提示修改网站权限。
- 页面之间共享一次定位及转换请求，内存缓存最多两分钟；过期即清除，有活动页面处于前台时重新获取，回到前台时重新检查权限与有效期。无后台定位或位置持久化。
- 首页下拉刷新、页面上的“刷新位置”立即重新定位。定位或转换失败后清除旧距离，显示重试入口。设备关闭定位服务会提示开启；定位误差超过 200 米或缺少精度信息时提示开启精确位置或改善信号，不展示误导性数字。
- Web 使用 HTTPS 或 localhost 安全上下文，并允许网站获取位置。Expo Web 定位显式设置 `maximumAge: 0`，避免浏览器返回旧坐标。原生位置获取和坐标转换均设有等待上限，超时后可重试。
- 新依赖 `expo-location` 使用项目 Expo SDK 54 对应的稳定版本；升级 Expo 时一起更新，不能单独安装其他 SDK 的定位模块。新增原生模块和 iOS 权限说明后，需按上文重新生成及构建 Android/iOS 原生包。仅重启 Metro 不生效。

本次只使用既有活动位置字段，不改 schema，不增加 migration，也不改变活动搜索排序。日志使用 `[user-location]` 和 `maps.convert.*`，记录权限状态、阶段、耗时及错误类型，不记录坐标或 Key。

### 手动联调

1. 未授权进入首页不弹权限框，点击开启后授权；检查首页、搜索、详情共享距离结果，切页不重复定位。
2. 使用不同活动坐标检查不足 100 米、百米、米／公里边界、远距离显示；缺少坐标的活动没有距离。
3. 拒绝权限、永久拒绝、仅授予粗略位置、关闭设备定位服务，检查提示与恢复入口；从系统设置返回后重新检查权限。
4. 刷新位置、首页下拉刷新、等待两分钟和切后台后返回，确认旧位置过期即不再显示距离；定位或网络超时后可重试，迟到的定位结果不覆盖新请求。
5. 使用有效 Web 服务 Key 检查实际转换结果；移除 Key 检查 503 提示，错误 Key／额度耗尽检查转换失败提示。确认浏览活动与导航仍可正常使用。
6. 分别使用 Android、iOS 与 HTTPS Web 验证定位；检查客户端、API 和入口日志均未包含精确坐标或 Key。
