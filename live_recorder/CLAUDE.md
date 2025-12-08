# CLAUDE - live_recorder
Breadcrumb: Home / live_recorder

## 角色
直播录制/探测库，封装平台工厂、流信息获取与录制控制，为 GUI 或服务端提供 API。

## 关键文件
- `src/lib.rs`：导出 `LiveRecorder`（别名 `LiveRecorderCore`）、错误与类型模块。
- `src/core.rs`：`LiveRecorderCore` 封装 `recorder::LiveRecorder`，提供 `start_recording`、`check_room_status`、`get_stream_info`、`quick_record`、`record`，支持自定义 `PlatformFactory`。
- `src/types.rs`：直播模型与配置：`LiveStatus`、`StreamInfo/StreamData/StreamUrl`、`RecordConfig`（输出模板、质量、格式、分段、Headers/代理、重试/超时）、`VideoQuality`、`RecordProgress/RecordStatus`。
- 其他（未细扫）：`platforms/*`（bilibili/douyin/douyu/huya/kuaishou/soop 等平台实现）、`recorder.rs`（录制流程）、`stream.rs`（流处理）、`examples/` 与 `tests/`（douyin 测试）。

## 依赖与扩展
- 使用 `rquickjs`、`reqwest`（带压缩）、`async-trait/futures`、`rand/hex/md-5/base64` 等处理平台逻辑；依赖 `xbogus` 进行签名。
- 配置默认输出模板：`./downloads/{platform}/{anchor_name}_{room_id}_{timestamp}.mp4`，质量默认 `Original`，重试 3、超时 30s，UA 默认浏览器字符串。

## 集成提示
- GUI 可调用 `LiveRecorderCore::start_recording/check_room_status` 获取房间信息与流质量列表，再用 `record` 启动。
- 关注平台实现对 Cookie/Headers 的需求（未审阅），必要时复用 `magekit-shared::PlatformCookie`。

## 推荐下一步
- 深入 `platforms/*` 与 `recorder.rs`，确认签名、抓流、断线重连与分段逻辑；补充跨平台权限与存储路径校验。
- 运行 `examples/basic_usage.rs`/`tests/douyin_test.rs` 做兼容性验证。

