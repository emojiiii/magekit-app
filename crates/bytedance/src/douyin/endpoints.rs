//! 抖音 Web API Endpoints 定义
//!
//! 包含抖音 Web 端所有常见接口 URL

/// 抖音 Web 端所有常见接口
pub struct DouyinEndpoints;

impl DouyinEndpoints {
    // ========== 域名定义 ==========
    pub const DOUYIN_DOMAIN: &str = "https://www.douyin.com";
    pub const IESDOUYIN_DOMAIN: &str = "https://www.iesdouyin.com";
    pub const LIVE_DOMAIN: &str = "https://live.douyin.com";
    pub const LIVE_DOMAIN2: &str = "https://webcast.amemv.com";
    pub const SSO_DOMAIN: &str = "https://sso.douyin.com";
    pub const WEBCAST_WSS_DOMAIN: &str = "wss://webcast5-ws-web-lf.douyin.com";

    // ========== 信息流接口 ==========
    /// 首页推荐信息流
    pub const TAB_FEED: &str = "https://www.douyin.com/aweme/v1/web/tab/feed/";
    /// 好友信息流
    pub const FRIEND_FEED: &str = "https://www.douyin.com/aweme/v1/web/familiar/feed/";
    /// 关注信息流
    pub const FOLLOW_FEED: &str = "https://www.douyin.com/aweme/v1/web/follow/feed/";
    /// 视频频道信息流
    pub const VIDEO_CHANNEL: &str = "https://www.douyin.com/aweme/v1/web/channel/feed/";

    // ========== 用户接口 ==========
    /// 用户简要信息
    pub const USER_SHORT_INFO: &str = "https://www.douyin.com/aweme/v1/web/im/user/info/";
    /// 用户详细信息
    pub const USER_DETAIL: &str = "https://www.douyin.com/aweme/v1/web/user/profile/other/";
    /// 用户发布作品列表
    pub const USER_POST: &str = "https://www.douyin.com/aweme/v1/web/aweme/post/";
    /// 用户喜欢作品列表 (方式A)
    pub const USER_FAVORITE_A: &str = "https://www.douyin.com/aweme/v1/web/aweme/favorite/";
    /// 用户喜欢作品列表 (方式B)
    pub const USER_FAVORITE_B: &str = "https://www.iesdouyin.com/web/api/v2/aweme/like/";
    /// 用户关注列表
    pub const USER_FOLLOWING: &str = "https://www.douyin.com/aweme/v1/web/user/following/list/";
    /// 用户粉丝列表
    pub const USER_FOLLOWER: &str = "https://www.douyin.com/aweme/v1/web/user/follower/list/";
    /// 用户观看历史
    pub const USER_HISTORY: &str = "https://www.douyin.com/aweme/v1/web/history/read/";
    /// 用户收藏作品
    pub const USER_COLLECTION: &str = "https://www.douyin.com/aweme/v1/web/aweme/listcollection/";
    /// 用户收藏夹列表
    pub const USER_COLLECTS: &str = "https://www.douyin.com/aweme/v1/web/collects/list/";
    /// 用户收藏夹视频
    pub const USER_COLLECTS_VIDEO: &str =
        "https://www.douyin.com/aweme/v1/web/collects/video/list/";
    /// 用户收藏音乐
    pub const USER_MUSIC_COLLECTION: &str =
        "https://www.douyin.com/aweme/v1/web/music/listcollection/";

    // ========== 作品接口 ==========
    /// 作品基础接口
    pub const BASE_AWEME: &str = "https://www.douyin.com/aweme/v1/web/aweme/";
    /// 作品详情
    pub const POST_DETAIL: &str = "https://www.douyin.com/aweme/v1/web/aweme/detail/";
    /// 作品弹幕
    pub const POST_DANMAKU: &str = "https://www.douyin.com/aweme/v1/web/danmaku/get_v2/";
    /// 相关作品推荐
    pub const POST_RELATED: &str = "https://www.douyin.com/aweme/v1/web/aweme/related/";
    /// 地点作品
    pub const LOCATE_POST: &str = "https://www.douyin.com/aweme/v1/web/locate/post/";
    /// 合辑作品
    pub const MIX_AWEME: &str = "https://www.douyin.com/aweme/v1/web/mix/aweme/";

    // ========== 评论接口 ==========
    /// 作品评论列表
    pub const POST_COMMENT: &str = "https://www.douyin.com/aweme/v1/web/comment/list/";
    /// 评论回复列表
    pub const POST_COMMENT_REPLY: &str = "https://www.douyin.com/aweme/v1/web/comment/list/reply/";
    /// 发布评论
    pub const POST_COMMENT_PUBLISH: &str = "https://www.douyin.com/aweme/v1/web/comment/publish";
    /// 删除评论
    pub const POST_COMMENT_DELETE: &str = "https://www.douyin.com/aweme/v1/web/comment/delete/";
    /// 评论点赞
    pub const POST_COMMENT_DIGG: &str = "https://www.douyin.com/aweme/v1/web/comment/digg";

    // ========== 搜索接口 ==========
    /// 综合搜索
    pub const GENERAL_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/general/search/single/";
    /// 视频搜索
    pub const VIDEO_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/search/item/";
    /// 用户搜索
    pub const USER_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/discover/search/";
    /// 直播搜索
    pub const LIVE_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/live/search/";
    /// 搜索建议词
    pub const SUGGEST_WORDS: &str = "https://www.douyin.com/aweme/v1/web/api/suggest_words/";
    /// 热搜榜
    pub const HOT_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/hot/search/list/";

    // ========== 直播接口 ==========
    /// 直播间信息
    pub const LIVE_INFO: &str = "https://live.douyin.com/webcast/room/web/enter/";
    /// 通过 room_id 获取直播信息
    pub const LIVE_INFO_ROOM_ID: &str = "https://webcast.amemv.com/webcast/room/reflow/info/";
    /// 直播礼物排行榜
    pub const LIVE_GIFT_RANK: &str = "https://live.douyin.com/webcast/ranklist/audience/";
    /// 直播用户信息
    pub const LIVE_USER_INFO: &str = "https://live.douyin.com/webcast/user/me/";
    /// 关注用户直播列表
    pub const FOLLOW_USER_LIVE: &str = "https://www.douyin.com/webcast/web/feed/follow/";

    // ========== 登录接口 ==========
    /// 获取登录二维码
    pub const SSO_LOGIN_GET_QR: &str = "https://sso.douyin.com/get_qrcode/";
    /// 检查二维码状态
    pub const SSO_LOGIN_CHECK_QR: &str = "https://sso.douyin.com/check_qrconnect/";
    /// 检查登录状态
    pub const SSO_LOGIN_CHECK_LOGIN: &str = "https://sso.douyin.com/check_login/";
    /// 登录重定向
    pub const SSO_LOGIN_REDIRECT: &str = "https://www.douyin.com/login/";
    /// 登录回调
    pub const SSO_LOGIN_CALLBACK: &str = "https://www.douyin.com/passport/sso/login/callback/";
}
