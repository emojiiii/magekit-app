# Streamlink / uv 直播录制迁移

## 本次改动

直播录制按平台分流：抖音继续使用原来的 Rust 原生录制器，其他平台交给 Streamlink 插件。Streamlink 没有对应插件、账号没有观看权限或网络请求失败时会直接返回错误，不会回退到原生录制器。快手不在当前支持范围内。
`LiveRecorder` 的原有启动、查询、进度、停止和等待接口保留，现有 GUI 监控/录制入口继续使用这些接口。

```text
Rust GPUI / CLI
  ├─ 工具页：安装、更新/修复、状态刷新
  ├─ uv 运行环境管理：私有 Python 3.12 + 独立 Streamlink 环境
  └─ LiveRecorder facade
       ├─ 抖音域名：原有 Rust 原生录制流程
       └─ 其他平台：Streamlink 支持时交给 Python supervisor
            ├─ 每次连接启动新的 Streamlink pull 子进程
            ├─ FFmpeg copy → MPEG-TS 标准化
            ├─ 常驻 FFmpeg copy → TS / MP4 / MKV / FLV 文件
            └─ 无对应插件：明确报不支持，不调用其他原生平台处理器
```

Streamlink 分支由 Rust 管理 Python 子进程，而不是把 Python 库编译成 Rust。该分支增加了 Python 适配进程和 TS 标准化进程，换取跨容器重连、可取消任务和清晰的升级边界；两段 FFmpeg 都使用 `-c copy`，不主动重编码。抖音分支沿用原录制链路。部分 Streamlink 协议还可能启动自己的内部 muxer。

## 构建与内嵌 uv

初始版本：uv **0.12.17**，Streamlink **8.6.1**，managed Python **3.12**。

正常 `cargo build --release` 会执行 `live_recorder/build.rs`，按 Cargo 的 **TARGET** 下载官方固定版本 uv，校验官方 `.sha256`，仅提取可执行文件，使用 `include_bytes!` 嵌入 Rust 二进制。
不执行远程 shell 安装脚本。现有 Windows/macOS/Linux release workflow 不需要另行改打包步骤；macOS 两个架构分别内嵌各自的 uv，再合成 universal binary。

**构建主机**需要 Python 3（仅使用标准库）。Windows 默认寻找 `python`，其他平台默认寻找 `python3`；可通过 `MAGEKIT_BUILD_PYTHON` 指定。
uv 的 MIT 许可文本随程序嵌入，在释放 uv 时一并写入工具目录。Python / Streamlink 本身不是编译进主程序的，它们在首次使用时安装。

离线或受限 CI 可准备与目标架构/版本匹配的 uv 可执行文件：

```bash
MAGEKIT_UV_BUNDLE=/absolute/path/to/uv cargo build --release --locked
```

```powershell
$env:MAGEKIT_UV_BUNDLE = 'C:\build-tools\uv.exe'
cargo build --release --locked
```

`MAGEKIT_UV_BUNDLE` 是受信任的构建输入，手工提供时由构建者负责验证其版本、来源与架构。
没有此覆盖值时，release 构建在下载/校验失败时明确失败，不生成悄悄缺少 uv 的发行包。
`MAGEKIT_SKIP_UV_BUNDLE=1` 是供开发者使用的显式跳过开关，不应对普通发行包使用。

开发构建不下载/内嵌 uv；使用 `MAGEKIT_UV_PATH` 指定可执行文件，或将 uv 加入 PATH：

```powershell
$env:MAGEKIT_UV_PATH = 'C:\tools\uv.exe'
cargo run --bin magekit
```

首次运行仍需要网络下载 Python 和 Python 依赖。**内嵌 uv 不等于完整离线发行版。** 后续已安装的环境可直接使用；缓存位于应用私有目录，不会每次录制重新安装。

## 环境安装与更新

在「工具管理」页使用 Streamlink 卡片。也可以调用独立 CLI：

```bash
cargo run -p live_recorder -- --runtime status
cargo run -p live_recorder -- --runtime install
cargo run -p live_recorder -- --runtime update
```

运行环境位于 `get_tools_dir()/streamlink/`：

```text
streamlink/
  uv-<version>-<os>-<arch>/uv[.exe]
  uv-<version>-<os>-<arch>/LICENSE-MIT.txt
  python/                 # uv managed Python
  cache/                  # uv dependency cache
  env-<uuid>/             # 不原地修改的虚拟环境
  activations/*.json      # 安装、导入验证成功后发布的启用记录
```

首次安装固定 `streamlink==8.6.1`；「更新/修复」在一个**全新的**环境中解析 `streamlink>=8.6.1,<9`，验证导入和版本后再发布启用记录。
运行中的录制器持有其原 Python 路径，不被更新影响。失败不替换原可用环境；同进程内安装/更新通过异步锁串行化。
跨进程使用独立环境和唯一启用记录，不重命名 venv（venv 中存在绝对路径），也不依赖 Windows 的覆盖式 rename。
旧环境暂不自动清理，避免误删其他应用实例仍在使用的解释器。uv 自身随应用发行更新，不运行 `uv self update` 去改写内嵌工具。

此实现固定顶层初始版本，但没有声称 Python 的全部传递依赖已经形成可跨平台重现的完整锁文件。

## 录制、重连、停止

```bash
cargo run -p live_recorder -- 'https://www.twitch.tv/example' -o ./recording.mp4 -f mp4
cargo run -p live_recorder -- 'https://www.twitch.tv/example' -f ts --duration 600 --retries 5
```

`retry_count` 表示初次连接之外的重试次数，配合有限指数退避；取流结束/卡住时会重新解析直播间，不重复使用过期的签名流地址。
每次连接先标准化为 TS，再送入同一个最终 muxer，避免直接拼接原始 FLV/fMP4。文件存在时明确报错，不覆盖之前的录像。
`max_duration` 是包含连接/重连时间的任务时长上限；结束时仍会等待管线排空和文件收尾。

进度通过有界的 latest-value watch 通道传递，未消费的进度不会无限堆积。开始时间保持稳定，结束时保留实际文件大小。
GUI 的 `stop().await` 和 CLI 的 Ctrl+C 都等待子进程结束、文件写完后才返回。
正常停止先结束取流、排空管线并关闭输出；超时才清理进程树/Unix 进程组，并把无法保证收尾的情况报告为错误。
关闭进度通道会退出 CLI 监控循环，不再无限空转。句柄被丢弃也会发送停止信号。

MP4 使用 `frag_keyframe+default_base_moof`。没有使用 `empty_moov`，因为真实 FFmpeg 测试发现它会使 copy 模式下的 AAC ADTS 自动转换失效。
分片能降低异常中断损坏的风险，但不保证任意时刻强杀或机器断电都能恢复全部内容。

**重连不是补录**：断网期间未取得的视频无法凭空恢复；编码、分辨率或轨道布局切换仍需真实平台长时间验证。

## Cookie 与认证

复用应用已有 `PlatformCookie` 配置。Rust 在传给 Python 前筛选当前主机对应的配置，Python 再施加域名边界。
Cookie 经 stdin JSON 传递，不进入进程命令行、不写临时 Cookie 文件、不回显原始插件异常。
普通自定义请求头和代理仍通过 `RecordConfig` 传入；Cookie 请求头转换成域内 cookie jar，而不是全局发送给所有 CDN。
全局 Authorization / Proxy-Authorization / Host 头会被明确拒绝，不能用它们代替插件认证。

SOOP Cookie 可用，但公开房间通常不需要登录。韩国 `sooplive.co.kr` 与 Global `sooplive.com` Cookie 仍按各自域名隔离；`sooplive` / `soop` 别名指向韩国域，Global Cookie 可将平台名称设为 `soop_global`。Streamlink 8.6.1 的插件会把认证检查发往 `.sooplive.com`，因此这个明确的 Global 别名也会用于韩国房间的认证检查，但 Cookie 仍绑定在 `.sooplive.com`，不会发给韩国站点或其他域名。插件源码：[Streamlink SOOP 插件](https://github.com/streamlink/streamlink/blob/8.6.1/src/streamlink/plugins/soop.py)。
用户名和密码是可选登录方式；配置的凭据经 worker stdin 传递，不进命令行或日志，但保存在 MageKit 本地配置文件中。若房间要求登录，可使用有效的 Global Cookie 或账号密码。19+ 房间仍要求账号本身完成成人认证并拥有该房间的观看权限；应用不能代替账号完成认证或绕过房间权限。
此适配器不加载用户侧加载插件，不持久化 Streamlink 插件缓存，也不自动启动浏览器来处理挑战。

## 兼容范围

- Streamlink 后端实现 TS、MP4、MKV、FLV；具体编码必须被目标容器支持，不会为了兼容而偷偷重编码。抖音仍使用原来的原生录制流程。
- `segment_duration`、弹幕保存尚未由 Streamlink 后端实现；其他 Streamlink 平台启用时会明确报错。抖音继续按原生录制器现有行为处理。
- GUI 的分流范围包括抖音原生录制，以及 Bilibili、斗鱼、虎牙、SOOP 等 Streamlink 插件平台；快手和没有插件的平台会报不支持。平台封面等字段不保证齐全。

## Rust 依赖审查

移除了 GPUI 相关 `*`，沿用当前 Cargo.lock 的兼容版本系列：`gpui 0.2.2`、`gpui-component 0.5.0`、`gpui-component-assets 0.5.0`，与 `reqwest_client` 的 `v0.5.0` tag 保持配套。
**Cargo.lock 未做全量更新，本 PR 不是所有依赖已经追到最新版本的声明。**

建议下一批独立变更分别处理 GPUI/组件/资源/HTTP client 的联动升级，以及 networking / thiserror / rand 等依赖与调用点。
这些升级需要 workspace 锁文件重新解析、Windows/macOS GUI 构建和实际页面验证，不应与媒体管线迁移一起无验证地跨版本替换。
仓库已经是 edition 2024，不需要为升级而重写整套 Rust 架构；保留 Tokio 后台任务与 GPUI 的异步边界。

## 验证

本地离线测试使用**模拟 Streamlink 源 + 真实 FFmpeg/ffprobe**，覆盖域名/Cookie 隔离、清晰度选择、容器参数、Windows 命令行长度、归档处理、重试预算、两次媒体连接后的帧数/时长、停止收尾和错误脱敏。

```bash
python3 -m unittest discover -s crates/live_recorder/tests -p 'test_streamlink_worker.py' -v
cargo check -p live_recorder --all-targets --locked
cargo test -p live_recorder --lib --locked recorder::tests::
```

新增 CI 进行离线媒体测试、真实 Streamlink SDK 导入/API 冒烟检查，以及 Windows/macOS/Linux recorder 编译检查。
提交环境没有 Rust 工具链，因此不能把这些 CI 检查或 GPUI 构建声称为本地已通过。真实平台、多房间并行、长期断流恢复、发行包首次安装/更新仍需发布前实测。

参考：
- https://streamlink.github.io/api_guide/quickstart.html
- https://streamlink.github.io/api/session.html
- https://streamlink.github.io/plugins.html
- https://docs.astral.sh/uv/guides/install-python/
- https://docs.astral.sh/uv/reference/cli/
- https://github.com/astral-sh/uv/releases/tag/0.12.17
