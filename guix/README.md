# Guix 统一开发环境

本目录为 Linux x86_64 提供统一的 Android、Rust 和 Node 开发环境。Android SDK、NDK、JDK 和模拟器按组件构建，最后组合成标准 SDK 目录；实现参考 [nixpkgs androidenv](https://github.com/NixOS/nixpkgs/tree/master/pkgs/development/mobile/androidenv)。不需要 Android Studio，也没有额外的安装或启动脚本。

## 文件与版本

- `modules/loop/android-sources.scm`：固定下载地址、校验值和 Google SDK 组件元数据。
- `modules/loop/android.scm`：组件包、Linux 主机工具修正、组合 SDK。
- `env.scm`：Android、Rust、Node 及编译依赖共用的开发 manifest。
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
guix shell -L guix/modules -m guix/env.scm
```

环境自动提供 `ANDROID_HOME`、`JAVA_HOME`，以及 `adb`、`emulator`、`sdkmanager`、`avdmanager`、Java、Rust/Cargo/rustfmt/Clippy、Node/npm、GCC、CMake、pkg-config、OpenSSL、PostgreSQL 客户端库。Rust 的 Cargo 和工具分别来自 `rust:cargo`、`rust:tools` 输出。普通 shell 中可构建后端、启动前端和模拟器；多个终端使用同一个 manifest。检查已安装组件：

```bash
rustc --version
cargo --version
cargo fmt --version
cargo clippy --version
node --version
npm --version
pkg-config --modversion libpq openssl
java -version
adb version
sdkmanager --list_installed
emulator -accel-check
```

需要固定 Guix 依赖时，使用：

```bash
guix time-machine -C guix/channels.scm -- \
  shell -L guix/modules -m guix/env.scm
```

`ANDROID_HOME` 指向 `/gnu/store` 中的只读组合 SDK。不要在其中执行 `sdkmanager --install`、`--update` 或 `--licenses`。更新组件应修改 Scheme 包定义和校验值，再重新构建环境；缺少某个构建组件时也应显式补充包定义。

### 进入后找不到 adb、java 或 sdkmanager

先退出刚进入的环境，再显式启动不读取宿主启动配置的 Bash：

```bash
guix shell -L guix/modules -m guix/env.scm -- bash --noprofile --norc
command -v adb java sdkmanager
```

已在本机复现的原因是 Fish 的 `~/.config/fish/conf.d/uv-base.fish` 每次进入交互式 shell 都激活 Python base 环境。其 `activate.fish` 会先执行 `deactivate`，使用从父进程继承的 `_OLD_VIRTUAL_PATH` 覆盖 Guix 刚设置的 `PATH`，移除 SDK 命令所在目录。

若要继续使用默认 Fish，请在管理该文件的 Guix Home 配置源中，将自动激活条件改为以下内容，再执行 `guix home reconfigure <Home配置.scm>`。该文件若链接到 `/gnu/store`，应修改配置源。

```fish
if status is-interactive; and not set -q GUIX_ENVIRONMENT; and not set -q VIRTUAL_ENV; and test -f ~/.venvs/base/bin/activate.fish
    source ~/.venvs/base/bin/activate.fish
end
```

这样进入 Guix 环境或已有 Python 虚拟环境时不会重复激活 base。重新进入 Guix shell 后检查命令位置；也可使用 `guix shell --check -L guix/modules -m guix/env.scm` 检查启动配置是否覆盖环境变量。

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

## 构建、启动与联调

以下命令使用 Bash 语法。每个终端都从仓库根目录进入相同环境：

```bash
guix shell -L guix/modules -m guix/env.scm -- bash --noprofile --norc
```

普通 shell 保留宿主桌面、网络、用户 home 和 `/dev/kvm`，适合同时运行后端、Metro 和模拟器。容器工具不放入 manifest。先在宿主机安装并配置 Podman/Compose，执行 `make infra-up` 单独启动容器和完成初始化；开发环境内使用 `make backend-run` 启动 Rust 服务。环境错误应在宿主机修正，不在项目中添加兼容脚本。

### 后端

先按根目录 [README](../README.md#启动本地服务) 准备 `deploy/standalone/.env` 和 JWT 文件。只使用 Android 模拟器时，在 `.env` 中设置：

```bash
LOOP_S3_ENDPOINT_URL='http://127.0.0.1:9000'
LOOP_S3_PRESIGN_ENDPOINT_URL='http://10.0.2.2:9000'
LOOP_S3_PUBLIC_BASE_URL='http://10.0.2.2:9000/loop-local'
```

`10.0.2.2` 在模拟器内指向宿主机；后端自身仍使用 `127.0.0.1`。若 bucket 名不是 `loop-local`，同步修改公开地址中的 bucket。真机联调则使用宿主局域网 IP。

先在宿主机的独立终端执行 `make infra-up`，成功后在已进入 Guix 环境的终端构建并启动后端：

```bash
cd srv
cargo build --workspace --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cd ..
make backend-run
```

`make backend-run` 只加载配置并运行 API、realtime 与 worker，不启动或管理容器。PostgreSQL 库用于 Diesel 的本机编译和链接，数据库服务由宿主机的 Compose 启动。若 Cargo 报工具链版本过低，更新 Guix channel 并重新进入环境；不要用忽略 Rust 版本检查的方式继续构建。

### 前端与 Android 原生构建

在另一个终端按前文启动模拟器，然后在前端终端执行：

```bash
adb devices -l
cd app
npm ci
npm run build:rich-editor

export EXPO_PUBLIC_API_BASE_URL=http://10.0.2.2:3000/loop
export EXPO_PUBLIC_REALTIME_URL=ws://10.0.2.2:3010/loop/realtime

# NDK 使用自己的 Android sysroot，不继承本机 GCC 的头文件和链接路径。
env -u C_INCLUDE_PATH -u CPLUS_INCLUDE_PATH -u CPATH -u LIBRARY_PATH \
  npm run android -- --device
```

新增的本机 C/C++ 工具链会设置 `C_INCLUDE_PATH`、`CPLUS_INCLUDE_PATH` 和 `LIBRARY_PATH`。它们用于后端 native crates 及 Node 本机模块编译，但会干扰 NDK 的 Android sysroot。因此只在 Android 构建命令前清除这些变量，不要在整个 shell 中全局取消。Gradle 会接收启动它的命令所传递的环境。

Expo 首次运行会生成被 Git 忽略的 `app/android/`，构建 Debug APK、安装并启动 Metro。已安装开发客户端后，在保留上述 `EXPO_PUBLIC_*` 环境变量的前端终端运行 `npm start -- --localhost` 继续 JS 调试；必要时运行 `adb reverse tcp:8081 tcp:8081`，多个设备连接时加 `adb -s <serial>`。

### Guix System 上的 FHS 构建

manifest 提供软件依赖，普通 `guix shell` 不会创建 `/usr/bin`、`/lib64` 等标准路径。SDK 自身已由 Guix 打包，但 Gradle/npm 下载的额外 Linux 预编译程序（例如 Maven AAPT2、Hermes、esbuild）可能仍依赖这些路径。遇到文件存在却报 `No such file or directory` 或缺少 ELF loader 时，前端构建使用同一份 manifest 的 FHS 模式：

```bash
# 模拟器与后端继续在宿主普通 shell 中运行，先在宿主执行 adb start-server。
guix shell -C -N -F -L guix/modules -m guix/env.scm -- bash --noprofile --norc

# 容器 home 是临时目录，缓存放在仓库中保留。
export CARGO_HOME="$PWD/.cache/guix/cargo"
export GRADLE_USER_HOME="$PWD/.cache/guix/gradle"
export npm_config_cache="$PWD/.cache/guix/npm"

adb devices -l
```

进入后执行上面的前端安装、构建与启动命令。`-N` 共享宿主网络，ADB 连接宿主已启动的 server；`10.0.2.2` 地址配置仍然适用。FHS 环境也可执行 `cd srv && cargo build --workspace --locked`，但模拟器、桌面与 Podman 的运行使用宿主普通环境。这两种进入方式共享 `env.scm`，无需维护两份软件清单。

仅构建供 x86_64 模拟器使用的 APK：

```bash
cd app
npm run build:rich-editor
npx expo prebuild --platform android --no-install
cd android
env -u C_INCLUDE_PATH -u CPLUS_INCLUDE_PATH -u CPATH -u LIBRARY_PATH \
  ./gradlew :app:assembleDebug -PreactNativeArchitectures=x86_64 --console=plain --stacktrace
```

产物位于 `app/android/app/build/outputs/apk/debug/app-debug.apk`。普通 shell 默认使用用户缓存；FHS 容器使用上面设置的仓库内缓存。设备日志用 `adb logcat` 查看。

Google、Maven、Gradle、crates.io 或 npm 下载失败时，先修正宿主网络。Guix daemon 的下载网络和交互式 shell 的代理配置需要分别确认。构建容器可通过 `-E '^(https?|all|no)_proxy$|^(HTTPS?|ALL|NO)_PROXY$'` 保留已配置的代理；Java/Gradle 的代理需按其规则配置到用户的 `gradle.properties`，不会自动使用 curl 的代理变量。

## 更新包

Google 元数据来自 [SDK 仓库](https://dl.google.com/android/repository/repository2-3.xml) 和 [Google APIs 镜像仓库](https://dl.google.com/android/repository/sys-img/google_apis/sys-img2-3.xml)。更新时选择 stable channel，核对归档和校验值，并同步 `package.xml` 对应的 revision、SDK path、type-details 及依赖。构建阶段不调用 sdkmanager 联网下载。

升级 Expo / React Native 后，重新核对依赖中的 Android 版本配置，并同步组合包。更新 Guix 依赖时同步 `channels.scm`。验证应包含 Rust/Node 工具版本、后端构建、`sdkmanager --list_installed`、NDK/CMake 执行、模拟器启动、项目实际 APK 构建和安装。
