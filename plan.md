# MageKit 视频下载器 - 功能实现计划

## 项目概述

**项目名称**: MageKit - 基于 GPUI 的跨平台视频下载器
**技术栈**: Rust + GPUI + GPUI Component + gpui-router + Tokio
**目标**: 构建用户友好的桌面视频下载应用，自动管理 yt-dlp/ffmpeg 工具

---

## 📁 当前项目结构

```
magekit-app/
├── Cargo.toml (workspace配置)
├── shared/                      # 共享库
│   └── src/
│       ├── lib.rs
│       ├── types.rs             # 数据类型定义
│       ├── constants.rs         # 应用常量
│       └── utils.rs             # 工具函数
├── tool_manager/                # 工具管理器
│   └── src/
│       ├── lib.rs
│       ├── config.rs            # 配置管理
│       ├── downloader.rs        # 视频下载器 (核心)
│       ├── error.rs             # 错误处理
│       ├── storage.rs           # 工具存储
│       ├── task_manager.rs      # 任务管理器
│       ├── task_persistence.rs  # 任务持久化
│       ├── task_queue.rs        # 任务队列
│       ├── updater.rs           # 工具更新
│       └── history.rs           # 下载历史
└── gui_app/                     # GUI应用
    └── src/
        ├── main.rs              # 应用入口
        ├── app.rs               # 应用状态管理
        ├── theme/
        │   └── mod.rs           # 主题系统
        ├── logic/               # (空) 业务逻辑层
        └── ui/
            ├── mod.rs           # 模块导出
            ├── main_window.rs   # 主窗口 + 路由
            ├── layout.rs        # 布局组件
            ├── widgets/         # 公共小组件
            │   ├── card.rs
            │   ├── checkbox.rs
            │   ├── section.rs
            │   └── status_badge.rs
            └── pages/           # 页面组件 (重构后)
                ├── home/        # 首页 (下载页)
                │   ├── page.rs
                │   └── widgets/
                ├── tools/       # 工具管理页
                │   ├── page.rs
                │   └── widgets/
                └── settings/    # 设置页
                    ├── page.rs
                    └── widgets/
```

---

## ✅ 已完成的基础工作

### 基础架构

- ✅ Cargo workspace 配置
- ✅ 三个 crate 拆分 (shared, tool_manager, gui_app)
- ✅ 路由系统集成 (gpui-router)
- ✅ 主题系统基础框架
- ✅ 页面组件重构 (pages/ 目录结构)

### Tool Manager

- ✅ ToolStorage - 工具存储管理
- ✅ VideoDownloader - 视频下载器（结构已完成）
- ✅ ConfigManager - 配置管理
- ✅ TaskManager - 任务管理器（结构已完成）
- ✅ TaskQueue - 任务队列
- ✅ TaskPersistence - 任务持久化
- ✅ HistoryManager - 下载历史

### GUI 界面

- ✅ MainWindow - 主窗口 + 路由系统
- ✅ AppLayout - 布局组件（侧边栏导航）
- ✅ HomePage - 首页（下载面板）
- ✅ ToolsPage - 工具管理页
- ✅ SettingsPage - 设置页

---

## 🎯 功能实现计划

### 阶段 1: 工具管理 - 让 yt-dlp/ffmpeg 真正工作 ✅ 已完成

**目标**: 实现工具自动安装、检测、更新功能

#### 1.1 ToolsPage 功能实现

当前状态: ✅ 全部完成

- [x] **检测系统已安装的工具**

  - ✅ 使用 `which::which()` 检测 yt-dlp 和 ffmpeg
  - ✅ 获取已安装工具的版本号 (`--version` 命令)
  - ✅ 更新 ToolInfo 状态 (ToolInstallState)
  - ✅ 区分系统安装和应用安装的工具 (`is_system` 字段)

- [x] **实现工具自动下载**

  - ✅ 连接 `tool_manager/src/updater.rs` 的下载逻辑
  - ✅ 从 GitHub Releases 下载 yt-dlp
  - ✅ 使用 brew 安装 ffmpeg（macOS）
  - ✅ 显示实时下载进度（百分比、速度、已下载/总大小）
  - ✅ 下载过程不阻塞 UI（使用 smol::unblock）

- [x] **工具管理功能**
  - ✅ 删除已安装的工具（非系统工具）
  - ✅ 红色危险按钮样式
  - ✅ 版本号显示（ffmpeg/yt-dlp 版本解析）

**技术实现亮点**:

- 使用 `AtomicU64` 在后台线程和 UI 之间共享下载进度
- 使用 `smol::unblock()` 避免阻塞主线程
- 使用 `reqwest` 的 streaming API 实现流式下载
- 进度回调使用特殊标记 (`speed = u64::MAX`) 表示进入安装阶段

**涉及文件**:

- `gui_app/src/ui/pages/tools/page.rs` - 页面逻辑 ✅
- `gui_app/src/ui/pages/tools/widgets/tool_card.rs` - 工具卡片组件 ✅
- `gui_app/src/app.rs` - AppState 工具检测方法 ✅
- `tool_manager/src/updater.rs` - 下载更新逻辑 ✅
- `tool_manager/src/storage.rs` - 工具存储和删除 ✅

---

### 阶段 2: 视频信息获取 - URL 解析功能 ✅ 已完成

**目标**: 输入 URL 能获取视频信息

#### 2.1 HomePage 解析功能

当前状态: ✅ 全部完成

- [x] **连接 VideoDownloader.get_video_info()**

  - ✅ 调用 yt-dlp --dump-json 获取视频信息
  - ✅ 解析返回的 JSON 数据
  - ✅ 处理各种错误情况（无效 URL、网络错误等）

- [x] **异步任务处理**

  - ✅ 使用后台线程 + smol::unblock 避免阻塞主线程
  - ✅ 显示加载状态 (DownloadState::Fetching)
  - ✅ 错误处理和用户提示

- [x] **视频信息展示**
  - ✅ 显示视频标题、时长、上传者
  - ✅ 格式化时长显示 (HH:MM:SS)
  - ✅ 缩略图 URL 获取（显示待实现）

**技术实现亮点**:

- 在 `AppState` 中添加 `get_video_info_in_background()` 方法
- 使用 `smol::unblock()` 将同步阻塞操作移到线程池
- 将 shared 的 `VideoInfo` 转换为 GUI 的简化版 `VideoInfo`
- 时长格式化函数 `format_duration()`

**涉及文件**:

- `gui_app/src/ui/pages/home/page.rs` - 页面逻辑 ✅
- `gui_app/src/app.rs` - AppState 方法调用 ✅
- `tool_manager/src/downloader.rs` - get_video_info() ✅

---

### 阶段 3: 视频下载功能 - 核心功能 ✅ 已完成

**目标**: 实现完整的视频下载流程

#### 3.1 下载任务创建

- [x] **格式选择**

  - ✅ 预设画质选项（最佳、1080p、720p、480p、仅音频）
  - ✅ 用户选择画质/格式
  - ✅ 构建 yt-dlp format 参数

- [x] **启动下载任务**
  - ✅ 调用 `download_video_in_background()` 执行下载
  - ✅ 后台线程运行 yt-dlp 进程
  - ✅ 支持下载选项（元数据、缩略图、字幕）

#### 3.2 下载进度监控

- [x] **实时进度更新**

  - ✅ 从 yt-dlp 输出解析进度（使用 --progress-template）
  - ✅ 更新 UI 进度条
  - ✅ 显示下载速度

- [x] **任务状态同步**
  - ✅ 使用 AtomicU32/AtomicU64 共享进度
  - ✅ cx.spawn 轮询更新 UI
  - ✅ 处理任务完成/失败状态

#### 3.3 任务管理（基础版）

- [ ] **暂停/恢复下载** - 待后续实现
- [ ] **取消下载** - 待后续实现

**技术实现亮点**:

- 在 `AppState` 中添加 `download_video_in_background()` 方法
- 使用 `std::process::Command` 同步运行 yt-dlp
- 解析 yt-dlp 的 `--progress-template` 输出获取进度
- `parse_size_string()` 函数解析大小字符串（如 "1.5MiB"）
- `format_speed()` 函数格式化下载速度显示
- `DownloadVideoOptions` 结构体封装下载选项

**涉及文件**:

- `gui_app/src/ui/pages/home/page.rs` - 下载 UI 和进度更新 ✅
- `gui_app/src/app.rs` - download_video_in_background() ✅

---

### 阶段 4: 任务列表页面 ✅ 已完成

**目标**: 显示所有下载任务，支持任务管理

#### 4.1 创建 TasksPage

当前状态: ✅ 已完成基础框架

- [x] **创建有状态的 TasksPage 组件**

  - ✅ 在 `pages/tasks/` 目录下创建完整结构
  - ✅ 连接 AppState 获取任务列表
  - ✅ 支持任务筛选（全部/下载中/已完成/失败）

- [x] **任务列表展示**

  - ✅ TaskItem 组件显示单个任务详情
  - ✅ 显示任务状态、进度条、下载速度
  - ✅ 空状态提示界面

- [x] **任务操作（已完成）**

  - ✅ 暂停/恢复按钮（更新任务状态）
  - ✅ 取消/删除按钮（更新并移除任务）
  - ✅ 打开文件夹按钮（跨平台支持）
  - ✅ 清空已完成任务

- [x] **任务状态同步**
  - ✅ 下载开始时创建任务并添加到列表
  - ✅ 下载进度实时更新到任务列表
  - ✅ 下载完成/失败时更新任务状态
  - ✅ TasksPage 每秒自动刷新任务状态

**技术实现亮点**:

- `TasksPage` 有状态组件，支持任务筛选
- `TaskItem` 使用 `IntoElement` trait 实现可复用组件
- 使用 `gpui_component` 的 `Sizable` trait 控制按钮大小
- 相对宽度进度条 `w(relative(progress_percent))`
- 定时器自动刷新任务列表 (`cx.spawn` + `Timer::after`)
- 使用 `blocking_write()`/`blocking_read()` 在同步上下文访问 RwLock
- 跨平台打开文件夹 (`open`/`explorer`/`xdg-open`)

**涉及文件**:

- `gui_app/src/ui/pages/tasks/mod.rs` - 模块导出 ✅
- `gui_app/src/ui/pages/tasks/page.rs` - TasksPage 组件 ✅
- `gui_app/src/ui/pages/tasks/widgets/mod.rs` - 小组件模块 ✅
- `gui_app/src/ui/pages/tasks/widgets/task_item.rs` - TaskItem 组件 ✅
- `gui_app/src/ui/pages/mod.rs` - 添加 tasks 模块导出 ✅
- `gui_app/src/ui/main_window.rs` - 路由集成 ✅
- `gui_app/src/ui/pages/home/page.rs` - 下载时创建任务 ✅

---

### 阶段 5: 设置持久化 ✅ 已完成

**目标**: 用户设置能保存和加载

#### 5.1 SettingsPage 功能完善

当前状态: ✅ 全部完成

- [x] **配置文件读写**

  - ✅ 在 shared/src/utils.rs 添加 load_app_config() 和 save_app_config()
  - ✅ 配置文件路径: ~/Library/Application Support/MageKit/config.toml
  - ✅ 首次运行自动创建默认配置文件
  - ✅ 应用启动时自动加载已保存配置
  - ✅ 修改设置后自动保存到文件

- [x] **设置项实现**

  - ✅ 下载路径选择（使用 rfd crate 实现文件夹选择对话框）
  - ✅ 最大并发数设置
  - ✅ 嵌入元数据/缩略图选项
  - ✅ 自动更新检查开关
  - ✅ 调试模式开关

- [x] **设置同步到 AppState**
  - ✅ SettingsPage 从 AppState 读取初始配置
  - ✅ 修改设置后更新 AppState.config
  - ✅ AppState.update_config() 自动保存到文件

**技术实现亮点**:

- 使用 `rfd` crate 实现跨平台原生文件夹选择对话框
- 使用 `toml` crate 序列化/反序列化配置
- 使用 `smol::unblock()` 避免文件对话框阻塞 UI
- 配置自动保存，无需手动点击保存按钮

**涉及文件**:

- `shared/Cargo.toml` - 添加 toml 依赖 ✅
- `shared/src/utils.rs` - 配置文件读写函数 ✅
- `gui_app/Cargo.toml` - 添加 rfd 依赖 ✅
- `gui_app/src/ui/pages/settings/page.rs` - 连接 AppState ✅
- `gui_app/src/app.rs` - 加载/保存配置 ✅

---

### 阶段 6: 通知系统 ✅ 已完成

**目标**: 用户操作反馈

#### 6.1 Toast 通知

当前状态: ✅ 已完成

- [x] **集成 gpui_component 内置通知系统**

  - ✅ 使用 `window.push_notification()` 推送通知
  - ✅ 支持 Info、Success、Warning、Error 四种类型
  - ✅ 通知自动淡出

- [x] **下载状态通知**

  - ✅ 下载开始通知
  - ✅ 下载完成通知（Success 类型）
  - ✅ 下载失败通知（Error 类型）

- [x] **工具管理通知**

  - ✅ 工具安装成功通知
  - ✅ 工具安装失败通知
  - ✅ 工具删除成功/失败通知

- [ ] **系统通知** (待后续实现)
  - 下载完成时发送系统级通知
  - 可配置通知选项

**技术实现亮点**:

- 使用 `gpui_component::notification::{Notification, NotificationType}` 内置组件
- 使用 `gpui_component::WindowExt` trait 的 `push_notification()` 方法
- MainWindow 已集成 `Root::render_notification_layer()` 渲染通知层
- 在 `cx.spawn` 闭包中通过 `this.update()` 获取 window 发送通知

**涉及文件**:

- `gui_app/src/ui/pages/home/page.rs` - 下载通知 ✅
- `gui_app/src/ui/pages/tools/page.rs` - 工具安装/删除通知 ✅
- `gui_app/src/ui/main_window.rs` - 通知层渲染 (已有) ✅

---

### 阶段 7: 高级功能 (低优先级)

#### 7.1 批量下载

- [ ] 支持播放列表下载
- [ ] 批量 URL 导入
- [ ] 下载模板

#### 7.2 格式转换

- [ ] 视频格式转换
- [ ] 音频提取
- [ ] 字幕下载

#### 7.3 下载历史

- [ ] 历史记录展示
- [ ] 重新下载
- [ ] 历史搜索

---

## 📋 优先级总结

### 🔴 高优先级 (必须完成)

1. **工具管理** - 让用户能安装 yt-dlp/ffmpeg
2. **视频解析** - URL 能获取视频信息
3. **视频下载** - 核心下载功能

### 🟡 中优先级 (重要功能)

4. **任务列表** - 管理下载任务
5. **设置持久化** - 保存用户配置
6. **通知系统** - 操作反馈

### 🟢 低优先级 (锦上添花)

7. **批量下载** - 播放列表等
8. **格式转换** - 后处理功能
9. **下载历史** - 历史记录

---

## 🔧 技术要点

### GPUI 异步任务模式

```rust
// 在组件中执行异步任务
fn fetch_video_info(&mut self, cx: &mut Context<Self>) {
    let url = self.get_url(cx);
    let app_state = self.app_state.clone();

    cx.spawn(|this, mut cx| async move {
        // 执行异步操作
        let result = app_state.tool_manager.get_video_info(&url).await;

        // 更新 UI
        this.update(&mut cx, |this, cx| {
            match result {
                Ok(info) => this.download_state = DownloadState::Ready(info),
                Err(e) => this.download_state = DownloadState::Error(e.to_string()),
            }
            cx.notify();
        })
    }).detach();
}
```

### 事件通道模式

```rust
// AppState 中的事件发送
pub async fn start_download(&self, url: String) -> Result<TaskId> {
    let task_id = self.tool_manager.start_download(url, options).await?;

    // 发送通知事件
    self.event_tx.send(AppEvent::ShowNotification(...)).await?;

    Ok(task_id)
}

// 组件中订阅事件
// 使用 cx.subscribe() 或轮询 event_rx
```

---

## 📊 进度跟踪

| 阶段 | 功能       | 状态      | 完成度 |
| ---- | ---------- | --------- | ------ |
| 1    | 工具管理   | ✅ 已完成 | 100%   |
| 2    | 视频解析   | ✅ 已完成 | 100%   |
| 3    | 视频下载   | ✅ 已完成 | 95%    |
| 4    | 任务列表   | ✅ 已完成 | 95%    |
| 5    | 设置持久化 | ✅ 已完成 | 100%   |
| 6    | 通知系统   | ✅ 已完成 | 90%    |
| 7    | 高级功能   | ⏳ 待开始 | 0%     |

---

## 🚀 下一步行动

**当前任务**: 阶段 7 - 高级功能

1. ✅ 阶段 1-6 全部完成
2. 核心功能已具备，可以进入高级功能开发

下一步可选任务：

- 批量下载/播放列表支持
- 格式转换
- 下载历史
- 系统通知
- 性能优化

---

**最后更新**: 2025-12-05
**版本**: v0.10.0-dev
**状态**: 阶段 1-6 完成，核心功能就绪
