# CLAUDE - platform-api

Breadcrumb: Home / platform-api

## 角色

- 多平台 Web API 客户端与签名实现（当前包含 Douyin/TikTok/Bilibili），向 `extractor` 等自研解析提供 HTTP/签名/反爬参数能力。

## 关键文件

- `src/lib.rs`：导出 `BdClient`、`DouyinApi`、`DouyinLiveApi`、`BilibiliApi`、`ab_sign/xbogus_sign` 等。
- `src/client.rs`：统一 HTTP 客户端，预置 DEFAULT/LIVE/MOBILE UA，超时/代理/重定向配置，GET/POST/JSON 封装。
- `src/douyin/api.rs`：构建通用查询参数，使用 `ab_sign` 生成 `a_bogus`，附带 Cookie/Referer 拉取 JSON；提供短链解析、aweme_id/sec_user_id 提取、作品详情 `get_aweme_info`、用户信息与作品列表获取。
- `src/douyin/live.rs`：直播 API，使用移动端 UA 与 `ab_sign_live`；支持代理；短链/直播间 URL 提取 room_id，获取 `web_rid` 与直播流信息。
- `src/douyin/endpoints.rs`：抖音 Web 端常见接口常量（信息流、用户、作品、评论、搜索、直播、登录）。
- `src/douyin/types.rs`：`AwemeInfo`/`AuthorInfo`/`VideoData` 等数据模型。
- `src/bilibili/*`：WBI 签名与 Bilibili 端点封装，提供视频详情、播放地址（progressive）与 UP 投稿分页。
- `src/sign/*`：A-Bogus/Live A-Bogus、X-Bogus、SM3、RC4、定制 base64，导出 `ab_sign`、`ab_sign_live`、`xbogus_sign`。

## 数据流与要点

- Douyin：`DouyinApi::new/with_config` 构建 `BdClient`（默认 Cookie 仅用于测试），可 `set_cookie` 覆盖；`sign_url` 拼接查询+签名并发起请求，空响应视为 Cookie 失效。
- Bilibili：`BilibiliApi` 对 WBI 接口会通过 `NAV` 获取 `wbi_img` 并构造签名（带简单缓存），调用方需注入有效 Cookie（如 SESSDATA 等）以提升稳定性。

## 依赖

- `reqwest` + `tokio`、`serde/serde_json`、`indexmap/regex`、`tracing`、`rquickjs`（JS 签名）、`rand/base64/md5/urlencoding` 等；dev 依赖 `pretty_assertions`。

## 注意事项

- 默认 Cookie 仅示例，生产必须传入有效用户 Cookie，否则接口易返回空/403/风控。
- 签名逻辑依赖 UA 与查询参数，修改 UA/参数需同步签名实现。

## 推荐下一步

- 将错误类型从“平台前缀”逐步抽象为更通用的 `PlatformError`（目前已在 `src/lib.rs` 提供别名），并补齐 Bilibili/WBI 的最小可复现测试向量。
