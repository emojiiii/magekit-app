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

### 阶段 1: 工具管理 - 让 yt-dlp/ffmpeg 真正工作 (高优先级)

**目标**: 实现工具自动安装、检测、更新功能

#### 1.1 ToolsPage 功能实现

当前状态: ✅ 基础功能已完成

- [x] **检测系统已安装的工具**

  - ✅ 使用 `which::which()` 检测 yt-dlp 和 ffmpeg
  - ✅ 获取已安装工具的版本号 (`--version` 命令)
  - ✅ 更新 ToolInfo 状态 (ToolInstallState)
  - ✅ 区分系统安装和应用安装的工具 (`is_system` 字段)

- [ ] **实现工具自动下载**

  - 连接 `tool_manager/src/updater.rs` 的下载逻辑
  - 从 GitHub Releases 下载 yt-dlp
  - 从官方源下载 ffmpeg（或使用预编译包）
  - 显示下载进度

- [ ] **工具版本更新检查**
  - 检查远程最新版本
  - 比较本地版本
  - 提供更新选项

**涉及文件**:

- `gui_app/src/ui/pages/tools/page.rs` - 页面逻辑 ✅
- `gui_app/src/app.rs` - AppState 工具检测方法 ✅
- `tool_manager/src/updater.rs` - 下载更新逻辑
- `tool_manager/src/storage.rs` - 工具存储路径

---

### 阶段 2: 视频信息获取 - URL 解析功能 (高优先级)

**目标**: 输入 URL 能获取视频信息

#### 2.1 HomePage 解析功能

当前状态: `on_parse()` 返回硬编码的模拟数据

- [ ] **连接 VideoDownloader.get_video_info()**

  - 调用 yt-dlp --dump-json 获取视频信息
  - 解析返回的 JSON 数据
  - 处理各种错误情况（无效 URL、网络错误等）

- [ ] **异步任务处理**

  - 使用 GPUI 的异步机制
  - 显示加载状态 (DownloadState::Fetching)
  - 错误处理和用户提示

- [ ] **视频信息展示**
  - 显示视频标题、时长、上传者
  - 显示缩略图（如果可用）
  - 显示可用的格式列表

**涉及文件**:

- `gui_app/src/ui/pages/home/page.rs` - 页面逻辑
- `gui_app/src/app.rs` - AppState 方法调用
- `tool_manager/src/downloader.rs` - get_video_info()

---

### 阶段 3: 视频下载功能 - 核心功能 (高优先级)

**目标**: 实现完整的视频下载流程

#### 3.1 下载任务创建

- [ ] **格式选择**

  - 解析视频可用格式
  - 用户选择画质/格式
  - 构建 DownloadOptions

- [ ] **启动下载任务**
  - 调用 ToolManager.start_download()
  - 创建 TaskHandle
  - 添加到任务队列

#### 3.2 下载进度监控

- [ ] **实时进度更新**

  - 从 yt-dlp 输出解析进度
  - 更新 UI 进度条
  - 显示下载速度、剩余时间

- [ ] **任务状态同步**
  - 连接 ToolManagerEvent 到 UI
  - 使用 mpsc channel 传递更新
  - 处理任务完成/失败状态

#### 3.3 任务管理

- [ ] **暂停/恢复下载**

  - 实现 yt-dlp 进程控制
  - 保存断点信息
  - 恢复下载

- [ ] **取消下载**
  - 终止 yt-dlp 进程
  - 清理临时文件
  - 更新任务状态

**涉及文件**:

- `gui_app/src/ui/pages/home/page.rs` - 下载 UI
- `gui_app/src/app.rs` - start_download() 方法
- `tool_manager/src/task_manager.rs` - 任务管理
- `tool_manager/src/downloader.rs` - 下载执行

---

### 阶段 4: 任务列表页面 (中优先级)

**目标**: 显示所有下载任务，支持任务管理

#### 4.1 创建 TasksPage

当前状态: 使用静态的 tasks_page() 函数

- [ ] **创建有状态的 TasksPage 组件**

  - 在 `pages/tasks/` 目录下创建
  - 订阅 AppState 的任务列表
  - 实时更新显示

- [ ] **任务列表展示**

  - 显示所有任务（进行中、已完成、失败）
  - 显示任务详情（文件名、大小、进度）
  - 支持任务分组/筛选

- [ ] **任务操作**
  - 暂停/恢复单个任务
  - 删除任务
  - 重试失败任务
  - 打开下载目录

**涉及文件**:

- `gui_app/src/ui/pages/tasks/` (新建)
- `gui_app/src/ui/layout.rs` - 添加路由
- `tool_manager/src/task_manager.rs`

---

### 阶段 5: 设置持久化 (中优先级)

**目标**: 用户设置能保存和加载

#### 5.1 SettingsPage 功能完善

当前状态: UI 已完成，设置不持久化

- [ ] **配置文件读写**

  - 使用 tool_manager 的 ConfigManager
  - 保存用户设置到本地文件
  - 应用启动时加载配置

- [ ] **设置项实现**

  - 下载路径选择（实现文件夹选择对话框）
  - 最大并发数
  - 自动更新检查
  - 代理设置

- [ ] **设置同步到 AppState**
  - 修改设置后更新 AppState.config
  - 通知相关组件配置变更

**涉及文件**:

- `gui_app/src/ui/pages/settings/page.rs`
- `gui_app/src/app.rs` - update_config()
- `tool_manager/src/config.rs`

---

### 阶段 6: 通知系统 (中优先级)

**目标**: 用户操作反馈

#### 6.1 Toast 通知

- [ ] **集成 notification.rs**

  - 下载开始/完成/失败通知
  - 错误提示
  - 操作确认

- [ ] **系统通知**
  - 下载完成时发送系统通知
  - 可配置通知选项

**涉及文件**:

- `gui_app/src/ui/notification.rs` (已有代码待集成)
- `gui_app/src/ui/main_window.rs`

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
| 1    | 工具管理   | ⏳ 待开始 | 0%     |
| 2    | 视频解析   | ⏳ 待开始 | 0%     |
| 3    | 视频下载   | ⏳ 待开始 | 0%     |
| 4    | 任务列表   | ⏳ 待开始 | 0%     |
| 5    | 设置持久化 | ⏳ 待开始 | 0%     |
| 6    | 通知系统   | ⏳ 待开始 | 0%     |
| 7    | 高级功能   | ⏳ 待开始 | 0%     |

---

## 🚀 下一步行动

**立即开始**: 阶段 1 - 工具管理

1. 在 `ToolsPage` 中调用 `ToolManager.ensure_tools()` 检测工具状态
2. 实现工具下载进度显示
3. 测试 yt-dlp 和 ffmpeg 的自动安装

完成工具管理后，用户才能使用后续的下载功能。

---

**最后更新**: 2025-12-05
**版本**: v0.3.0-dev
**状态**: UI 框架完成，功能待实现
