# CLAUDE - GUI（根 crate）

Breadcrumb: Home / src

## 角色

GPUI 桌面前端：路由、状态、页面/组件、工具与任务操作入口。

## 关键入口

- `src/main.rs`：初始化 tracing、主题热加载、路由注册；构造 `AppState::new_sync()` 后打开主窗口。
- `src/ui/main_window.rs`：使用 `gpui_router` 配置路由与页面缓存（home/tasks/tools/settings/channel/record + 404）。
- `src/app/state.rs`：`AppState`（Tokio runtime、`ToolManager`、配置、任务 map、事件通道、取消/暂停标志）。
- `src/app/types.rs`：应用事件与通知类型、工具状态、下载选项。
- `src/app/download.rs`：后台获取信息/下载，按平台生成 Cookie 文件，封装取消/暂停/续传，解析 yt-dlp 进度与输出。
- `src/app/tasks.rs`：任务增删改查与通知桥接（调用 `ToolManager` 的 pause/resume/cancel）。

## 依赖与交互

- 依赖 `magekit-tool-manager`（获取信息、任务队列、工具存储）、`magekit-shared`（类型/配置/路径工具）、`live_recorder`（可扩展录制）、GPUI/`gpui-component`/`gpui-router`。
- 配置：启动时 `load_app_config_or_default`，变更通过 `save_app_config` 持久化；工具配置下沉到工具管理器。
- 工具路径：优先应用内存储（`ToolStorage`），否则降级系统 PATH（yt-dlp/ffmpeg）。

## 状态与事件流

- 全局状态通过 `GlobalAppState` 注入 GPUI；事件通道 `event_tx/event_rx` 转发任务更新、配置变更、工具更新、通知。
- 下载过程：UI 调用 → `download_video_in_background` 创建取消/暂停标志与 Cookie 文件 → spawn 子进程监控 stdout/stderr → 进度回调更新 UI。
- 任务管理：`start_download` 目前示例化默认 output_dir 与标题；与 `ToolManager` 的 `start_download/pause/resume/cancel/get_all_tasks` 对接。

## UI/路由

- 页面：home（下载）、tasks（列表）、tools（工具管理）、settings、channel（批量/订阅）、record（直播录制）。组件分布在 `ui/pages/**` 与 `ui/widgets/**`。
- 布局/通知：Root 组件附加对话框层与通知层；主题来自 `cx.theme()`。

## 风险与待补充

- 未覆盖的 UI 组件绑定与表单校验（`ui/pages/**`）；任务/工具更新的实际 UI 响应逻辑未审阅。
- `tasks.rs` 示例化下载标题/路径，真实逻辑需对齐 `ToolManager` 任务创建与输出路径生成。

## 推荐下一步

- 补扫 `ui/pages` 细节，梳理事件/路由参数与状态更新。
- 对齐任务创建路径与 `shared::generate_output_path`，确保 UI 选项生效。
- 添加集成测试/冒烟脚本：启动 → 添加任务 → 进度 → 取消/恢复路径。
