# Android Guix 环境

本目录为 Linux x86_64 提供 Android SDK、NDK、JDK 和模拟器的 Guix 包。包按组件构建，最后组合成标准 SDK 目录；实现参考 [nixpkgs androidenv](https://github.com/NixOS/nixpkgs/tree/master/pkgs/development/mobile/androidenv)。不需要 Android Studio，也没有额外的安装或启动脚本。

## 文件与版本

- `modules/loop/android-sources.scm`：固定下载地址、校验值和 Google SDK 组件元数据。
- `modules/loop/android.scm`：组件包、Linux 主机工具修正、组合 SDK。
- `android.scm`：本项目 Android 开发 manifest。
- `channels.scm`：验证环境对应的 Guix channel 提交。

| 组件 | 版本 |
| --- | --- |
| Command-line Tools | 23.0 |
| Platform Tools / ADB | 37.0.1 |
| Android Platform | API 36，revision 2 |
| Build Tools | 36.0.0；另含 Expo 库子项目使用的 AGP 默认版本 35.0.0 |
| NDK | 27.1.12297006 / r27b |
| Android CMake | 3.22.1 |
| Emulator | 37.1.11 |
| Google APIs 系统镜像 | API 36，x86_64，revision 7 |
| Temurin JDK | 17.0.20.1+1 |

独立工具采用配置时的稳定版本；Platform、Build Tools、NDK、CMake 按当前 Expo 54 / React Native 0.81.5 及 Reanimated / Worklets 的要求组合。Gradle 使用 Expo 生成的 wrapper，不额外安装系统 Gradle。

打包时的 ELF 修正工具固定为 patchelf 0.16.1，与 Nonguix 的选择一致：实际验证中 0.18.0 修正后的 Android CMake 会崩溃，0.16.1 可正常执行。该工具仅用于构建 Guix 包，不是应用运行依赖。

Command-line Tools 23 的上游 `sdkmanager` 默认转交给会自行下载程序的 Android CLI。这里使用同一版本归档内的 SDK Manager Java 入口和上游 Java 启动器，保证查询 SDK 时不额外下载未由 Guix 管理的工具。

SDK 和镜像来自 Google，使用受其 [SDK 条款](https://developer.android.com/studio/terms) 约束。归档及包元数据中保留原始许可证。Guix 修正 Linux 主机 ELF 的解释器和动态库路径，保留 NDK sysroot、Android 目标库及系统镜像原样。

## 构建与进入环境

以下命令都从仓库根目录执行。首次下载包含约 1.9 GB 的系统镜像，还需要 SDK、NDK、Java 和 Guix 依赖；请预留至少 15 GB 磁盘空间。

```bash
guix build -L guix/modules -e '(@ (loop android) android-sdk-with-emulator)'
guix shell -L guix/modules -m guix/android.scm
```

环境自动提供 `ANDROID_HOME`、`JAVA_HOME`，以及 `adb`、`emulator`、`sdkmanager`、`avdmanager`、Java、Node/npm 等命令。检查已安装组件：

```bash
java -version
adb version
sdkmanager --list_installed
emulator -accel-check
```

需要固定 Guix 依赖时，使用：

```bash
guix time-machine -C guix/channels.scm -- \
  shell -L guix/modules -m guix/android.scm
```

`ANDROID_HOME` 指向 `/gnu/store` 中的只读组合 SDK。不要在其中执行 `sdkmanager --install`、`--update` 或 `--licenses`。更新组件应修改 Scheme 包定义和校验值，再重新构建环境；缺少某个构建组件时也应显式补充包定义。

### 进入后找不到 adb、java 或 sdkmanager

先退出刚进入的环境，再显式启动不读取宿主启动配置的 Bash：

```bash
guix shell -L guix/modules -m guix/android.scm -- bash --noprofile --norc
command -v adb java sdkmanager
```

已在本机复现的原因是 Fish 的 `~/.config/fish/conf.d/uv-base.fish` 每次进入交互式 shell 都激活 Python base 环境。其 `activate.fish` 会先执行 `deactivate`，使用从父进程继承的 `_OLD_VIRTUAL_PATH` 覆盖 Guix 刚设置的 `PATH`，移除 SDK 命令所在目录。

若要继续使用默认 Fish，请在管理该文件的 Guix Home 配置源中，将自动激活条件改为以下内容，再执行 `guix home reconfigure <Home配置.scm>`。该文件若链接到 `/gnu/store`，应修改配置源。

```fish
if status is-interactive; and not set -q GUIX_ENVIRONMENT; and not set -q VIRTUAL_ENV; and test -f ~/.venvs/base/bin/activate.fish
    source ~/.venvs/base/bin/activate.fish
end
```

这样进入 Guix 环境或已有 Python 虚拟环境时不会重复激活 base。重新进入 Guix shell 后检查命令位置；也可使用 `guix shell --check -L guix/modules -m guix/android.scm` 检查启动配置是否覆盖环境变量。

## 创建并启动模拟器

在上述普通 Guix shell 内执行。模拟器在宿主桌面会话中运行，使用宿主的 KVM 和显示服务。

```bash
avdmanager create avd --name loop-api36 \
  --package 'system-images;android-36;google_apis;x86_64' \
  --device pixel_7
adb start-server
emulator -avd loop-api36
```

首次创建时按提示选择是否自定义硬件配置。AVD 数据和快照默认位于 `~/.android/avd/`；ADB 密钥也保存在用户目录，均不写入 store。需要其他位置可自行设置 `ANDROID_USER_HOME`、`ANDROID_AVD_HOME`。

若提示 KVM 不可用，在宿主机检查 `/dev/kvm`、BIOS/UEFI 的虚拟化选项和用户的 `kvm` 组权限。Guix System 的 `user-account` 声明应在现有 `supplementary-groups` 列表中加入 `"kvm"`，执行 `sudo guix system reconfigure <系统配置.scm>` 并重新登录，再检查 `emulator -accel-check`。Intel/AMD 主机分别需要内核的 `kvm_intel` / `kvm_amd` 模块。不要以 root 运行模拟器。

Wayland 桌面可通过 XWayland 显示模拟器；若 Qt 无法连接显示服务，在宿主终端检查 `DISPLAY` 和 XWayland。GPU 驱动问题可手动运行 `emulator -avd loop-api36 -gpu swiftshader`。这些是宿主环境设置，不在项目业务代码中添加兼容逻辑。

## 构建、安装与调试本项目

保持宿主模拟器运行，在另一个终端进入 FHS 构建容器。SDK 自身已由 Guix 打包；FHS 用于 Gradle/npm 下载的额外 Linux 预编译程序，如 Maven AAPT2 和 Hermes。

```bash
guix shell -C -N -F -L guix/modules -m guix/android.scm

# 在仓库根目录设置，缓存保留在宿主仓库中。
export GRADLE_USER_HOME="$PWD/.cache/guix/gradle"
export npm_config_cache="$PWD/.cache/guix/npm"

adb devices -l
cd app
npm ci
npm run build:rich-editor

EXPO_PUBLIC_API_BASE_URL=http://10.0.2.2:3000/loop \
EXPO_PUBLIC_REALTIME_URL=ws://10.0.2.2:3010/loop/realtime \
npm run android -- --device
```

`-N` 让容器共享宿主网络，ADB 使用先前在宿主启动的 server。Expo 首次运行会生成被 Git 忽略的 `app/android/`，构建 Debug APK、安装并启动 Metro。后续在 `app/` 执行 `npm start -- --localhost` 可继续 JS 调试；必要时运行 `adb reverse tcp:8081 tcp:8081`，多个设备连接时加 `adb -s <serial>`。

仅构建供上述 x86_64 模拟器使用的 APK（同样在 FHS 环境内）：

```bash
cd app
npm run build:rich-editor
npx expo prebuild --platform android --no-install
cd android
./gradlew :app:assembleDebug -PreactNativeArchitectures=x86_64 --console=plain --stacktrace
```

产物位于 `app/android/app/build/outputs/apk/debug/app-debug.apk`。Gradle daemon、依赖和日志使用上面的持久化缓存目录；模拟器日志直接输出到启动它的终端，设备日志用 `adb logcat` 查看。

模拟器中的 `10.0.2.2` 指向宿主机。API / WebSocket 可使用上例地址，SeaweedFS 的预签名上传与图片地址同样需要对模拟器可达；后端 `LOOP_S3_PRESIGN_ENDPOINT_URL`、`LOOP_S3_PUBLIC_BASE_URL` 建议配置为宿主局域网 IP，避免返回模拟器自己的 `127.0.0.1`。

Google、Maven、Gradle 或 npm 下载失败时，先修正宿主网络。Guix daemon 的下载网络和交互式 shell 的代理配置需要分别确认。构建容器可通过 `-E '^(https?|all|no)_proxy$|^(HTTPS?|ALL|NO)_PROXY$'` 保留已配置的代理；Java/Gradle 的代理需按其规则配置到用户的 `gradle.properties`，不会自动使用 curl 的代理变量。

## 更新包

Google 元数据来自 [SDK 仓库](https://dl.google.com/android/repository/repository2-3.xml) 和 [Google APIs 镜像仓库](https://dl.google.com/android/repository/sys-img/google_apis/sys-img2-3.xml)。更新时选择 stable channel，核对归档和校验值，并同步 `package.xml` 对应的 revision、SDK path、type-details 及依赖。构建阶段不调用 sdkmanager 联网下载。

升级 Expo / React Native 后，重新核对依赖中的 Android 版本配置，并同步组合包。更新 Guix 依赖时同步 `channels.scm`。验证应包含 `sdkmanager --list_installed`、NDK/CMake 执行、模拟器启动、项目实际 APK 构建和安装。
