# CLAUDE - shared
Breadcrumb: Home / shared

## 角色
跨模块共享的类型、配置与工具函数：任务模型、下载选项、配置结构、路径/格式化工具、工具目录解析。

## 关键文件
- `src/types.rs`：`VideoInfo/VideoFormat`，`DownloadOptions`（默认下载目录/模板/元数据开关等），`TaskState/TaskStatus/TaskUpdate`，`AppConfig`（download/tools/ui/advanced/live_record），`PlatformCookie`，`ToolType/UpdateStatus`，直播相关类型（`LiveRecordConfig`、房间/录制状态）。
- `src/utils.rs`：`create_command/create_tokio_command`（Windows 无控制台）、`sanitize_filename`、`generate_output_path`、`format_*` helpers、配置读写（`load_app_config_or_default`）、路径解析（`get_app_config_dir/get_app_data_dir/get_tools_dir/get_log_dir/get_temp_dir`）、URL 校验。
- `src/constants.rs`：常量与路径命名（未细扫，依赖 utils）。

## 主要接口与约定
- 配置文件：位于平台配置目录（`dirs::config_dir()/MageKit/...`），默认缺失时写入初始配置。
- 工具目录：`get_tools_dir` 生成应用数据目录下的 `tools/`，供 yt-dlp/ffmpeg 安装；日志与临时目录同理。
- 命名/路径安全：`sanitize_filename` 替换非法字符并裁剪首尾空格点；`generate_output_path` 处理重名递增。
- 日志与校验：URL 校验给出非支持域的 warning 而非 hard fail。

## 消费方
- GUI：使用任务/配置/事件类型、路径与格式化工具。
- Tool Manager：使用 `DownloadOptions`、`TaskStatus`、`UpdateChannel`、配置合并与路径工具。
- Live Recorder：重用配置与类型（直播配置/房间状态等）。

## 注意事项
- `DownloadOptions` 默认输出目录取自 `dirs::download_dir()`；UI 若需要自定义路径需覆盖。
- 配置和工具路径创建时会 `create_dir_all`，在沙盒/只读环境需提前检查权限。

