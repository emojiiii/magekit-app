# CLAUDE - bytedance

Breadcrumb: Home / bytedance

## 角色

- 字节跳动 Web API 客户端与签名实现（抖音为主，声明支持 TikTok），向 `extractor` 等自研解析提供 HTTP/签名能力。

## 关键文件

- `src/lib.rs`：导出 `BdClient`、`DouyinApi`、`DouyinLiveApi`、`ab_sign/xbogus_sign` 等。
- `src/client.rs`：统一 HTTP 客户端，预置 DEFAULT/LIVE/MOBILE UA，超时/代理/重定向配置，GET/POST/JSON 封装。
- `src/douyin/api.rs`：构建通用查询参数，使用 `ab_sign` 生成 `a_bogus`，附带 Cookie/Referer 拉取 JSON；提供短链解析、aweme_id/sec_user_id 提取、作品详情 `get_aweme_info`、用户信息与作品列表获取。
- `src/douyin/live.rs`：直播 API，使用移动端 UA 与 `ab_sign_live`；支持代理；短链/直播间 URL 提取 room_id，获取 `web_rid` 与直播流信息。
- `src/douyin/endpoints.rs`：抖音 Web 端常见接口常量（信息流、用户、作品、评论、搜索、直播、登录）。
- `src/douyin/types.rs`：`AwemeInfo`/`AuthorInfo`/`VideoData` 等数据模型。
- `src/sign/*`：A-Bogus/Live A-Bogus、X-Bogus、SM3、RC4、定制 base64，导出 `ab_sign`、`ab_sign_live`、`xbogus_sign`。

## 数据流与要点

- `DouyinApi::new/with_config` 构建 `BdClient`（默认 Cookie 仅用于测试），可 `set_cookie` 覆盖；`sign_url` 拼接查询+签名并发起请求，空响应视为 Cookie 失效。
- 作品解析：`get_aweme_info` 在 `get_post_detail` JSON 中提取核心字段并转为 `AwemeInfo`；辅助静态方法解析 aweme_id/sec_user_id、短链。
- 直播解析：`DouyinLiveApi::extract_room_id` 处理 live.douyin.com 直接房间或短链跳转，随后 `get_live_room_id`/`get_live_room_info` 获取 `web_rid` 与流。

## 依赖

- `reqwest` + `tokio`、`serde/serde_json`、`indexmap/regex`、`tracing`、`rquickjs`（JS 签名）、`rand/base64/md5/urlencoding` 等；dev 依赖 `pretty_assertions`。

## 注意事项

- 默认 Cookie 仅示例，生产必须传入有效用户 Cookie，否则易返回空/403。
- A-Bogus/Live 签名依赖 UA 与查询参数，修改 UA/参数需同步签名逻辑；响应 JSON 若很大，错误路径会打印前 500 字符。
- 当前代码集中于抖音，TikTok 具体实现尚未在本次扫描中确认。

## 推荐下一步

- 审阅 `sign/*` 性能/安全（特别是 QuickJS 与 RC4 实现），为 TikTok 落地补接口与测试；增加 live API 正/反例测试与 Cookie 失效提示。
