# magekit-bytedance

字节跳动平台 API 封装，提供抖音 Web 端接口调用与签名能力（A-Bogus / X-Bogus）。适合作为上层解析器或录制/下载工具的基础依赖。

## 模块与主要 API

- `BdClient`：HTTP 客户端封装
  - `new()` / `with_config(ClientConfig)` 创建客户端
  - `inner()` 获取底层 `reqwest::Client`
  - `user_agent()` 读取 UA
  - `get` / `get_with_headers` / `post` / `post_json` 基础请求方法
- `ClientConfig`
  - `timeout_secs`、`user_agent`、`proxy`、`max_redirects`
  - 便捷构造：`ClientConfig::live()`、`ClientConfig::mobile()`、`with_proxy()`、`with_timeout()`
- `DouyinApi`
  - 构造：`new()`、`with_config(ClientConfig)`，支持 `set_cookie`
  - 作品：`get_post_detail`、`get_aweme_info`
  - 用户：`get_user_profile`、`get_user_info`、`get_user_posts`、`get_user_likes`、`get_user_following`、`get_user_followers`
  - 评论：`get_post_comments`、`get_comment_replies`
  - 搜索：`search_general`、`search_video`、`search_user`、`get_hot_search`
  - 合辑：`get_mix_aweme`
  - URL 工具：`resolve_short_url`、`extract_aweme_id`、`extract_sec_user_id`
- 签名模块
  - `ab_sign(query, user_agent, seed)` 生成 A-Bogus
  - `xbogus_sign(query, user_agent)` 生成 X-Bogus

## 快速示例

```rust
use magekit_bytedance::{DouyinApi, ClientConfig};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut api = DouyinApi::with_config(ClientConfig::mobile())?;
    api.set_cookie("your_cookie_here");
    let info = api.get_aweme_info("7321613070743663893").await?;
    println!("标题: {}", info.desc);
    Ok(())
}
```

## 优点

- 自带完整的抖音 Web 参数模板与签名流程，开箱可用。
- HTTP 客户端支持代理、UA 预设，便于切换场景（直播 / 移动端）。
- 提供已解析的模型（如 `AwemeInfo`、`UserInfo`），减少上层解析工作量。

## 局限 / 风险

- 默认 Cookie 仅用于测试，生产必须注入有效 Cookie，否则接口易返回空。
- 仅覆盖抖音 Web 端接口，TikTok 逻辑尚未暴露/完善。
- 错误类型目前偏简单，对 HTTP 与 JSON 细节的诊断有限。

## 改进建议

- 将 Cookie、UA、a_bogus seed 统一放入配置结构，支持持久化和热更新。
- 补充 TikTok API 模块与统一的 `PlatformApi` trait，便于多平台复用。
- 增强错误与日志：区分网络、签名、数据缺失，并输出关键信息片段。
- 为高频接口加上限流/重试策略，并允许注入自定义 middleware（如缓存）。

