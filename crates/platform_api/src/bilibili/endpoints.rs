//! Bilibili API 端点定义

pub struct BilibiliEndpoints;

impl BilibiliEndpoints {
    pub const API_DOMAIN: &'static str = "https://api.bilibili.com";
    pub const LIVE_DOMAIN: &'static str = "https://api.live.bilibili.com";

    /// 视频详情（view）
    pub const VIDEO_DETAIL: &'static str = "https://api.bilibili.com/x/web-interface/view";

    /// 视频分 P（cid 列表）
    pub const VIDEO_PAGES: &'static str = "https://api.bilibili.com/x/player/pagelist";

    /// 视频播放地址（progressive，返回 durl）
    pub const VIDEO_PLAYURL: &'static str = "https://api.bilibili.com/x/player/playurl";
    /// 视频播放地址（WBI 版本）
    pub const VIDEO_PLAYURL_WBI: &'static str = "https://api.bilibili.com/x/player/wbi/playurl";

    /// WBI：UP 主投稿列表（分页 + 分区 tid）
    pub const SPACE_ARC_SEARCH: &'static str = "https://api.bilibili.com/x/space/wbi/arc/search";

    /// WBI：UP 主信息
    pub const SPACE_ACC_INFO: &'static str = "https://api.bilibili.com/x/space/wbi/acc/info";

    /// 收藏夹列表
    pub const COLLECT_FOLDERS: &'static str =
        "https://api.bilibili.com/x/v3/fav/folder/created/list-all";
    /// 收藏夹内容列表
    pub const COLLECT_VIDEOS: &'static str = "https://api.bilibili.com/x/v3/fav/resource/list";

    /// 综合热门
    pub const COM_POPULAR: &'static str = "https://api.bilibili.com/x/web-interface/popular";
    /// 每周必看
    pub const WEEKLY_POPULAR: &'static str =
        "https://api.bilibili.com/x/web-interface/popular/series/one";
    /// 入站必刷
    pub const PRECIOUS_POPULAR: &'static str =
        "https://api.bilibili.com/x/web-interface/popular/precious";

    /// 视频评论
    pub const VIDEO_COMMENTS: &'static str = "https://api.bilibili.com/x/v2/reply";
    /// 评论回复
    pub const COMMENT_REPLY: &'static str = "https://api.bilibili.com/x/v2/reply/reply";
    /// 用户动态
    pub const USER_DYNAMIC: &'static str =
        "https://api.bilibili.com/x/polymer/web-dynamic/v1/feed/space";

    /// 直播间信息
    pub const LIVEROOM_DETAIL: &'static str = "https://api.live.bilibili.com/room/v1/Room/get_info";
    /// 直播分区列表
    pub const LIVE_AREAS: &'static str = "https://api.live.bilibili.com/room/v1/Area/getList";
    /// 直播流
    pub const LIVE_VIDEOS: &'static str = "https://api.live.bilibili.com/room/v1/Room/playUrl";
    /// 正在直播的主播
    pub const LIVE_STREAMER: &'static str =
        "https://api.live.bilibili.com/xlive/web-interface/v1/second/getList";

    /// WBI key 获取
    pub const NAV: &'static str = "https://api.bilibili.com/x/web-interface/nav";
}
