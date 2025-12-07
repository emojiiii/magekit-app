/// 测试抖音直播间信息获取
///
/// 测试 DouyinHandler 能否正确获取直播间信息

#[cfg(test)]
mod tests {
    use live_recorder::platforms::{douyin::DouyinHandler, PlatformHandler};
    use tokio;
    use tracing_subscriber;

    #[tokio::test]
    async fn test_get_live_stream_info() {
        // 初始化日志
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_test_writer()
            .try_init();

        // 使用之前测试的直播间
        let room_id = "313899056971";

        println!("\n🧪 测试获取直播间信息: {}", room_id);

        let handler = DouyinHandler::new();

        match handler.get_stream_info(room_id).await {
            Ok(stream_info) => {
                println!("✅ 成功获取直播间信息:");
                println!("  房间ID: {}", stream_info.room.room_id);
                println!("  主播: {}", stream_info.room.anchor_name);
                println!("  标题: {}", stream_info.room.title);
                println!("  状态: {:?}", stream_info.room.status);
                println!("  流数量: {}", stream_info.streams.len());

                if let Some(first_stream) = stream_info.streams.first() {
                    println!("  第一个流质量: {:?}", first_stream.quality);
                    if let Some((w, h)) = first_stream.resolution {
                        println!("  分辨率: {}x{}", w, h);
                    }
                }

                // 基本断言 - 注意：web_rid 和 room_id 是不同的
                // web_rid 是 URL 中的 ID (如 313899056971)
                // room_id 是内部 ID (如 7580889628614347560)
                assert!(!stream_info.room.room_id.is_empty(), "房间ID不应为空");
                assert!(!stream_info.room.anchor_name.is_empty(), "主播名不应为空");
            }
            Err(e) => {
                println!("❌ 获取失败: {}", e);
                println!("   这可能是由于:");
                println!("   1. AB-Sign 签名算法还需要调试");
                println!("   2. 直播间已关闭");
                println!("   3. API 参数变化");

                // 即使失败也打印错误信息用于调试
                panic!("获取直播间信息失败: {}", e);
            }
        }
    }

    #[tokio::test]
    async fn test_platform_detection() {
        let handler = DouyinHandler::new();

        println!("\n🧪 测试平台检测");
        println!("  平台名称: {}", handler.platform_name());

        let url = "https://live.douyin.com/313899056971";
        assert!(handler.supports_url(url), "应该支持抖音URL");

        let invalid_url = "https://www.bilibili.com/12345";
        assert!(!handler.supports_url(invalid_url), "不应该支持B站URL");

        println!("✅ 平台检测通过");
    }
}
