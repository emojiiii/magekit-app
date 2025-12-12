# CLAUDE - capture, applicable for all files within /Users/cyk/Documents/emojiiii/magekit-app/crates/capture

# MageKit Capture - 资源捕捉库

## 核心功能

- **持续运行**：捕捉任务不会自动结束，需要手动调用 `session.cancel()` 停止
- **广告检测**：自动识别广告资源（URL 关键词、域名、文件大小等）
- **自动加速播放**：检测到广告时自动通过 CDP 加速播放视频
- **Cloudflare 绕过**：通过浏览器参数和 CDP 脚本隐藏自动化特征

## Cloudflare 绕过机制

### 浏览器启动参数

为了绕过 Cloudflare 检测，启动浏览器时添加了以下关键参数：

#### 1. 隐藏自动化特征

- `--disable-blink-features=AutomationControlled` - 禁用自动化控制特征
- `--exclude-switches=enable-automation` - 排除自动化开关
- `--disable-features=IsolateOrigins,site-per-process,AutomationControlled` - 禁用自动化相关特征

#### 2. 沙箱和 GPU 配置

- `--no-sandbox` - 禁用沙箱（某些环境需要）
- `--disable-setuid-sandbox` - 禁用 setuid 沙箱
- `--disable-gpu` - 禁用 GPU（headless 模式推荐）
- `--disable-software-rasterizer` - 禁用软件光栅化
- `--disable-dev-shm-usage` - 禁用共享内存（避免 /dev/shm 问题）

#### 3. 减少指纹特征

- `--disable-extensions` - 禁用扩展
- `--disable-plugins` - 禁用插件
- `--disable-plugins-discovery` - 禁用插件发现
- `--user-agent=...` - 设置正常的用户代理

#### 4. 正常浏览器行为

- `--disable-background-timer-throttling` - 禁用后台定时器节流
- `--disable-backgrounding-occluded-windows` - 禁用遮挡窗口后台化
- `--disable-renderer-backgrounding` - 禁用渲染器后台化
- `--enable-features=NetworkService,NetworkServiceInProcess` - 启用网络服务

### CDP 脚本注入

在页面加载时通过 `Page.addScriptToEvaluateOnNewDocument` 注入脚本，隐藏 webdriver 特征：

1. **删除 `navigator.webdriver`** - 这是最关键的检测点
2. **修改 `window.chrome`** - 添加 chrome 对象
3. **修改 `navigator.permissions.query`** - 覆盖权限查询 API
4. **修改 `navigator.plugins`** - 返回正常的插件列表
5. **修改 `navigator.languages`** - 设置语言列表
6. **修改 `navigator.platform`** - 设置平台信息
7. **修改 WebGL 参数** - 覆盖 WebGL 渲染器信息

## 使用示例

```rust
use magekit_capture::{CaptureRequest, ResourceFilter, start_capture};
use std::time::Duration;

let request = CaptureRequest {
    target_url: "https://example.com".to_string(),
    custom_browser_path: None,
    headless: true,
    timeout: Duration::from_secs(30),
    filter: ResourceFilter::video_only(),
    auto_speedup_ads: true, // 启用自动加速广告
    speedup_rate: 2.0, // 2倍速播放
};

let mut session = start_capture(request).await?;

// 捕捉会持续运行，不会自动结束
while let Some(event) = session.rx.recv().await {
    match event {
        CaptureEvent::Found(resource) => {
            println!("发现资源: {}", resource.url);
        }
        CaptureEvent::Log(msg) => {
            println!("日志: {}", msg);
        }
        _ => {}
    }
}

// 需要手动停止
// session.cancel();
```

## 注意事项

1. **持续运行**：捕捉任务不会自动结束，需要手动调用 `session.cancel()` 停止
2. **Cloudflare 检测**：虽然添加了绕过机制，但 Cloudflare 的检测在不断更新，可能仍会被拦截
3. **沙箱安全**：`--no-sandbox` 参数会降低安全性，仅在必要时使用
4. **广告检测**：基于启发式规则，可能误判，建议根据实际情况调整
