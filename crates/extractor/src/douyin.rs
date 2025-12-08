use crate::error::{ExtractError, ExtractResult};
use indexmap::IndexMap;
use magekit_shared::{PlatformCookie, VideoFormat, VideoInfo};
use regex::Regex;
use serde_json::Value;
use std::time::Duration;

mod abogus;
mod xbogus;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.0.0 Safari/537.36";
const COOKIE: &str = "UIFID_TEMP=2eb4f745f9fe6544447c1d68cb43a44931f67e23b1951fd9ca8b76ce94a62236cd2737882cf5c51c465353be5a7ac0d57116d5c2d0808c689b2e2bde2e22a5c0a47581015a699aa54a32a98b34d66655; hevc_supported=true; bd_ticket_guard_client_web_domain=2; n_mh=WJJizOI1AeKSbz11BaKvElmRAa_ryfixssm0oqRsx1w; uid_tt=e754755e336353a56fc4ef37c6e40339; uid_tt_ss=e754755e336353a56fc4ef37c6e40339; sid_tt=db7245cc0a10561681e41ad11156d862; sessionid=db7245cc0a10561681e41ad11156d862; sessionid_ss=db7245cc0a10561681e41ad11156d862; is_staff_user=false; store-region=cn-gd; store-region-src=uid; UIFID=2eb4f745f9fe6544447c1d68cb43a44931f67e23b1951fd9ca8b76ce94a62236cd2737882cf5c51c465353be5a7ac0d5f27185e79989df235d82d79f95cb35253d3b5c4bd1a311973647432a0870989766e7cecc986f006a587ed827f4fc41f1e1fda69726b6e61c0776647ba5a456c4589be4f599be9f1edc45f9cf8a04c7dc4165ae6387bbfce10fe084a393f423f07080f71be5540c3809e3b92cc98a46d3; is_dash_user=1; SelfTabRedDotControl=%5B%5D; live_use_vvc=%22false%22; xgplayer_device_id=45829625152; xgplayer_user_id=807199643196; fpk1=U2FsdGVkX19Vqqx9nKt+at8mmdFCzFUiuIzCQj/lbiwldcn8pmynTA4NzLtVu2J6OylOgDGDdq4GBCgfD8gjBg==; fpk2=3fa31b52dd6ebc517e5492d43d77e61c; post_dropdown_capcut_link=true; my_rd=2; SEARCH_RESULT_LIST_TYPE=%22multi%22; SearchMultiColumnLandingAbVer=2; enter_pc_once=1; h265ErrorNum=-1; passport_assist_user=Cjxi8W8lbCM9DHw23xyVydPPIZ9WIHhTdhQiEMCE6Q4p0ZnkVarH2nrAwdfVXFEuWQmjCrb_vziWRh_qQlwaSgo8AAAAAAAAAAAAAE9jHo9AKI4sTD8W-HxoL8C3mvWVBjB47oKtI4pFyQLbM3vcZZlAcQR5xOSRIDk40cS4ENqb-g0Yia_WVCABIgEDAQDdVA%3D%3D; login_time=1755932571304; _bd_ticket_crypt_cookie=1fb58812d98e2279a421dee6ca8b5543; __druidClientInfo=JTdCJTIyY2xpZW50V2lkdGglMjIlM0EzMDQlMkMlMjJjbGllbnRIZWlnaHQlMjIlM0E2OTQlMkMlMjJ3aWR0aCUyMiUzQTMwNCUyQyUyMmhlaWdodCUyMiUzQTY5NCUyQyUyMmRldmljZVBpeGVsUmF0aW8lMjIlM0ExLjI1JTJDJTIydXNlckFnZW50JTIyJTNBJTIyTW96aWxsYSUyRjUuMCUyMChXaW5kb3dzJTIwTlQlMjAxMC4wJTNCJTIwV2luNjQlM0IlMjB4NjQpJTIwQXBwbGVXZWJLaXQlMkY1MzcuMzYlMjAoS0hUTUwlMkMlMjBsaWtlJTIwR2Vja28pJTIwQ2hyb21lJTJGMTM5LjAuMC4wJTIwU2FmYXJpJTJGNTM3LjM2JTIyJTdE; theme=%22dark%22; manual_theme=%22dark%22; __ac_signature=_02B4Z6wo00f01t6y4jgAAIDBiHK7tTOCXB7ekuaAAN-HbOZ2HHs2H2s5mX4lfaRuw4A0kNOQwsB3o2nHqYmD.i1d3wiGpFl1s6oPbldBYxtNjZ75gwF2yCHV-ZvNZmSIzQ7DTsYwpezxur-feb; s_v_web_id=verify_mgvvbr4x_bc23d55a_3f38_dac7_75bf_9f3304fb4282; passport_csrf_token=f198c0e773748c2df1ccfb56c1ac42bc; passport_csrf_token_default=f198c0e773748c2df1ccfb56c1ac42bc; sid_guard=db7245cc0a10561681e41ad11156d862%7C1764248523%7C5184000%7CMon%2C+26-Jan-2026+13%3A02%3A03+GMT; session_tlb_tag=sttt%7C11%7C23JFzAoQVhaB5BrREVbYYv________-94rjTgnEwizU__hNlNdSIUIQmEfuISvkuFiulsegdvjU%3D; session_tlb_tag_bk=sttt%7C11%7C23JFzAoQVhaB5BrREVbYYv________-94rjTgnEwizU__hNlNdSIUIQmEfuISvkuFiulsegdvjU%3D; sid_ucp_v1=1.0.0-KDk1ZjNkZjU2MzUzYjJhNjM5ZGMzNjcxOGRhNWIxNjAxZjkyZTE3YjUKHwjC69LF2gIQy5ehyQYY7zEgDDCa74vUBTgFQPsHSAQaAmhsIiBkYjcyNDVjYzBhMTA1NjE2ODFlNDFhZDExMTU2ZDg2Mg; ssid_ucp_v1=1.0.0-KDk1ZjNkZjU2MzUzYjJhNjM5ZGMzNjcxOGRhNWIxNjAxZjkyZTE3YjUKHwjC69LF2gIQy5ehyQYY7zEgDDCa74vUBTgFQPsHSAQaAmhsIiBkYjcyNDVjYzBhMTA1NjE2ODFlNDFhZDExMTU2ZDg2Mg; vdg_s=1; PhoneResumeUidCacheV1=%7B%2293024728514%22%3A%7B%22time%22%3A1764994120924%2C%22noClick%22%3A2%7D%7D; __live_version__=%221.1.4.5166%22; publish_badge_show_info=%220%2C0%2C0%2C1765045316463%22; volume_info=%7B%22isMute%22%3Afalse%2C%22isUserMute%22%3Afalse%2C%22volume%22%3A0.153%7D; live_can_add_dy_2_desktop=%220%22; strategyABtestKey=%221765200495.617%22; ttwid=1%7C-vONDUndXj8EBDFLCFmwtzhs05qnVK_NNEMSPvDm3ls%7C1765200495%7C3b9210a26bc5c0ffa6477c3384eeb135d79200781404c9315fbe57106536dd70; playRecommendGuideTagCount=2; totalRecommendGuideTagCount=2; FOLLOW_NUMBER_YELLOW_POINT_INFO=%22MS4wLjABAAAAUN-DYPJkfbJFW5bLRaTVA2EZ3ZslESOPTxHynG8Z6P8%2F1765209600000%2F0%2F0%2F1765203619365%22; odin_tt=f5cf5ccd9dca098a4d57d414fc42a28622461de52a2daa774184861eb7b834802d8e7376940f812e79f55f7798c700de; biz_trace_id=7ba98a1b; __security_mc_1_s_sdk_crypt_sdk=78414f46-4a72-bf6c; __security_mc_1_s_sdk_cert_key=ffd2516f-474c-942d; __security_mc_1_s_sdk_sign_data_key_web_protect=89a611bb-41e3-8efa; stream_player_status_params=%22%7B%5C%22is_auto_play%5C%22%3A1%2C%5C%22is_full_screen%5C%22%3A0%2C%5C%22is_full_webscreen%5C%22%3A0%2C%5C%22is_mute%5C%22%3A0%2C%5C%22is_speed%5C%22%3A1%2C%5C%22is_visible%5C%22%3A0%7D%22; IsDouyinActive=true; stream_recommend_feed_params=%22%7B%5C%22cookie_enabled%5C%22%3Atrue%2C%5C%22screen_width%5C%22%3A2048%2C%5C%22screen_height%5C%22%3A1152%2C%5C%22browser_online%5C%22%3Atrue%2C%5C%22cpu_core_num%5C%22%3A32%2C%5C%22device_memory%5C%22%3A8%2C%5C%22downlink%5C%22%3A7.75%2C%5C%22effective_type%5C%22%3A%5C%224g%5C%22%2C%5C%22round_trip_time%5C%22%3A0%7D%22; FOLLOW_LIVE_POINT_INFO=%22MS4wLjABAAAAUN-DYPJkfbJFW5bLRaTVA2EZ3ZslESOPTxHynG8Z6P8%2F1765209600000%2F0%2F1765203834677%2F0%22; bd_ticket_guard_client_data=eyJiZC10aWNrZXQtZ3VhcmQtdmVyc2lvbiI6MiwiYmQtdGlja2V0LWd1YXJkLWl0ZXJhdGlvbi12ZXJzaW9uIjoxLCJiZC10aWNrZXQtZ3VhcmQtcmVlLXB1YmxpYy1rZXkiOiJCRUVkU3BrQVI2bURQQ2RyaHNXdlV2eVlIR1d1OXpIOFJJdnlpbGYrdTUxMnlDcmxOS0g4VEZkSTl2MGFUa0REK29FZkZTcm1oRUZjeW9XUTJjc3J3aXc9IiwiYmQtdGlja2V0LWd1YXJkLXdlYi12ZXJzaW9uIjoyfQ%3D%3D; bd_ticket_guard_client_data_v2=eyJyZWVfcHVibGljX2tleSI6IkJFRWRTcGtBUjZtRFBDZHJoc1d2VXZ5WUhHV3U5ekg4Ukl2eWlsZit1NTEyeUNybE5LSDhURmRJOXYwYVRrREQrb0VmRlNybWhFRmN5b1dRMmNzc3J3aXc9IiwiZ2VuZXJhdGVfdGltZSI6MTc2NDI0ODUyMjQyOH0%3D; bd_ticket_guard_client_web=eyJiZC10aWNrZXQtZ3VhcmQtdmVyc2lvbiI6MiwiYmQtdGlja2V0LWd1YXJkLWl0ZXJhdGlvbi12ZXJzaW9uIjoxLCJiZC10aWNrZXQtZ3VhcmQtcmVlLXB1YmxpYy1rZXkiOiJCRUVkU3BrQVI2bURQQ2RyaHNXdlV2eVlIR1d1OXpIOFJJdnlpbGYrdTUxMnlDcmxOS0g4VEZkSTl2MGFUa0REK29FZkZTcm1oRUZjeW9XUTJjc3J3aXc9IiwiYmQtdGlja2V0LWd1YXJkLXdlYi12ZXJzaW9uIjoyfQ%3D%3D; home_can_add_dy_2_desktop=%221%22";

/// 生成请求参数
fn build_params(aweme_id: &str) -> IndexMap<&'static str, String> {
    let mut params = base_request_params();
    params.insert("aweme_id", aweme_id.to_string());
    params
}

/// 基础参数（不含 aweme_id）
fn base_request_params() -> IndexMap<&'static str, String> {
    let mut params = IndexMap::new();
    params.insert("device_platform", "webapp".to_string());
    params.insert("aid", "6383".to_string());
    params.insert("channel", "channel_pc_web".to_string());
    params.insert("pc_client_type", "1".to_string());
    params.insert("version_code", "290100".to_string());
    params.insert("version_name", "29.1.0".to_string());
    params.insert("cookie_enabled", "true".to_string());
    params.insert("screen_width", "1920".to_string());
    params.insert("screen_height", "1080".to_string());
    params.insert("browser_language", "zh-CN".to_string());
    params.insert("browser_platform", "Win32".to_string());
    params.insert("browser_name", "Chrome".to_string());
    params.insert("browser_version", "103.0.0.0".to_string());
    params.insert("browser_online", "true".to_string());
    params.insert("engine_name", "Blink".to_string());
    params.insert("engine_version", "103.0.0.0".to_string());
    params.insert("os_name", "Windows".to_string());
    params.insert("os_version", "10".to_string());
    params.insert("cpu_core_num", "12".to_string());
    params.insert("device_memory", "8".to_string());
    params.insert("platform", "PC".to_string());
    params.insert("downlink", "10".to_string());
    params.insert("effective_type", "4g".to_string());
    params.insert("round_trip_time", "0".to_string());
    params.insert("from_user_page", "1".to_string());
    params.insert("locate_query", "false".to_string());
    params.insert("need_time_list", "1".to_string());
    params.insert("pc_libra_divert", "Windows".to_string());
    params.insert("publish_video_strategy_type", "2".to_string());
    params.insert("show_live_replay_strategy", "1".to_string());
    params.insert("time_list_query", "0".to_string());
    params.insert("whale_cut_token", "".to_string());
    params.insert("update_version_code", "170400".to_string());
    params.insert("msToken", "".to_string()); // 必须为空，a_bogus 会处理
    params
}

/// 将参数转换为 URL 编码的查询字符串
fn params_to_query(params: &IndexMap<&str, String>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// 抖音 Web 端所有常见接口
pub struct DouyinEndpoints;

impl DouyinEndpoints {
    pub const DOUYIN_DOMAIN: &str = "https://www.douyin.com";
    pub const IESDOUYIN_DOMAIN: &str = "https://www.iesdouyin.com";
    pub const LIVE_DOMAIN: &str = "https://live.douyin.com";
    pub const LIVE_DOMAIN2: &str = "https://webcast.amemv.com";
    pub const SSO_DOMAIN: &str = "https://sso.douyin.com";
    pub const WEBCAST_WSS_DOMAIN: &str = "wss://webcast5-ws-web-lf.douyin.com";

    pub const TAB_FEED: &str = "https://www.douyin.com/aweme/v1/web/tab/feed/";
    pub const USER_SHORT_INFO: &str = "https://www.douyin.com/aweme/v1/web/im/user/info/";
    pub const USER_DETAIL: &str = "https://www.douyin.com/aweme/v1/web/user/profile/other/";
    pub const BASE_AWEME: &str = "https://www.douyin.com/aweme/v1/web/aweme/";
    pub const USER_POST: &str = "https://www.douyin.com/aweme/v1/web/aweme/post/";
    pub const LOCATE_POST: &str = "https://www.douyin.com/aweme/v1/web/locate/post/";
    pub const GENERAL_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/general/search/single/";
    pub const VIDEO_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/search/item/";
    pub const USER_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/discover/search/";
    pub const LIVE_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/live/search/";
    pub const POST_DETAIL: &str = "https://www.douyin.com/aweme/v1/web/aweme/detail/";
    pub const POST_DANMAKU: &str = "https://www.douyin.com/aweme/v1/web/danmaku/get_v2/";
    pub const USER_FAVORITE_A: &str = "https://www.douyin.com/aweme/v1/web/aweme/favorite/";
    pub const USER_FAVORITE_B: &str = "https://www.iesdouyin.com/web/api/v2/aweme/like/";
    pub const USER_FOLLOWING: &str = "https://www.douyin.com/aweme/v1/web/user/following/list/";
    pub const USER_FOLLOWER: &str = "https://www.douyin.com/aweme/v1/web/user/follower/list/";
    pub const MIX_AWEME: &str = "https://www.douyin.com/aweme/v1/web/mix/aweme/";
    pub const USER_HISTORY: &str = "https://www.douyin.com/aweme/v1/web/history/read/";
    pub const USER_COLLECTION: &str = "https://www.douyin.com/aweme/v1/web/aweme/listcollection/";
    pub const USER_COLLECTS: &str = "https://www.douyin.com/aweme/v1/web/collects/list/";
    pub const USER_COLLECTS_VIDEO: &str = "https://www.douyin.com/aweme/v1/web/collects/video/list/";
    pub const USER_MUSIC_COLLECTION: &str =
        "https://www.douyin.com/aweme/v1/web/music/listcollection/";
    pub const FRIEND_FEED: &str = "https://www.douyin.com/aweme/v1/web/familiar/feed/";
    pub const FOLLOW_FEED: &str = "https://www.douyin.com/aweme/v1/web/follow/feed/";
    pub const POST_RELATED: &str = "https://www.douyin.com/aweme/v1/web/aweme/related/";
    pub const FOLLOW_USER_LIVE: &str = "https://www.douyin.com/webcast/web/feed/follow/";
    pub const LIVE_INFO: &str = "https://live.douyin.com/webcast/room/web/enter/";
    pub const LIVE_INFO_ROOM_ID: &str = "https://webcast.amemv.com/webcast/room/reflow/info/";
    pub const LIVE_GIFT_RANK: &str = "https://live.douyin.com/webcast/ranklist/audience/";
    pub const LIVE_USER_INFO: &str = "https://live.douyin.com/webcast/user/me/";
    pub const SUGGEST_WORDS: &str = "https://www.douyin.com/aweme/v1/web/api/suggest_words/";
    pub const SSO_LOGIN_GET_QR: &str = "https://sso.douyin.com/get_qrcode/";
    pub const SSO_LOGIN_CHECK_QR: &str = "https://sso.douyin.com/check_qrconnect/";
    pub const SSO_LOGIN_CHECK_LOGIN: &str = "https://sso.douyin.com/check_login/";
    pub const SSO_LOGIN_REDIRECT: &str = "https://www.douyin.com/login/";
    pub const SSO_LOGIN_CALLBACK: &str = "https://www.douyin.com/passport/sso/login/callback/";
    pub const POST_COMMENT: &str = "https://www.douyin.com/aweme/v1/web/comment/list/";
    pub const POST_COMMENT_REPLY: &str = "https://www.douyin.com/aweme/v1/web/comment/list/reply/";
    pub const POST_COMMENT_PUBLISH: &str = "https://www.douyin.com/aweme/v1/web/comment/publish";
    pub const POST_COMMENT_DELETE: &str = "https://www.douyin.com/aweme/v1/web/comment/delete/";
    pub const POST_COMMENT_DIGG: &str = "https://www.douyin.com/aweme/v1/web/comment/digg";
    pub const DOUYIN_HOT_SEARCH: &str = "https://www.douyin.com/aweme/v1/web/hot/search/list/";
    pub const DOUYIN_VIDEO_CHANNEL: &str = "https://www.douyin.com/aweme/v1/web/channel/feed/";
}

/// 根据参数生成带 a_bogus 的 URL
fn sign_url(endpoint: &str, params: &IndexMap<&str, String>) -> String {
    let query = params_to_query(params);
    let a_bogus = abogus::generate_abogus(&query, UA, None);
    let a_bogus_encoded = urlencoding::encode(&a_bogus);
    format!("{endpoint}?{query}&a_bogus={a_bogus_encoded}")
}

fn default_cookie(cookies: Option<&[PlatformCookie]>) -> String {
    cookies
        .and_then(|list| {
            list.iter()
                .find(|c| c.enabled && c.platform.to_lowercase().contains("douyin"))
        })
        .map(|c| c.cookie.clone())
        .unwrap_or_else(|| COOKIE.to_string())
}

fn build_client() -> ExtractResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(UA)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| ExtractError::Network(e.to_string()))
}

async fn fetch_json(
    endpoint: &str,
    params: IndexMap<&str, String>,
    cookies: Option<&[PlatformCookie]>,
    use_post: bool,
) -> ExtractResult<Value> {
    let client = build_client()?;
    let cookie_header = default_cookie(cookies);
    let url = sign_url(endpoint, &params);

    let req = if use_post {
        client.post(&url)
    } else {
        client.get(&url)
    }
    .header("Referer", "https://www.douyin.com/")
    .header("Accept", "application/json, text/plain, */*")
    .header("Accept-Language", "zh-CN,zh;q=0.9")
    .header("Cookie", cookie_header);

    let resp = req
        .send()
        .await
        .map_err(|e| ExtractError::Network(e.to_string()))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(ExtractError::Network(format!("status {}", status)));
    }

    let text = resp
        .text()
        .await
        .map_err(|e| ExtractError::Parse(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| ExtractError::Parse(e.to_string()))
}

pub async fn extract_video_info(
    url: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<VideoInfo> {
    tracing::info!("🔍 开始解析抖音视频: {}", url);

    let client = reqwest::Client::builder()
        .user_agent(UA)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| ExtractError::Network(e.to_string()))?;

    let resolved = resolve_short_link(&client, url).await.unwrap_or_else(|_| url.to_string());
    tracing::debug!("📍 解析后的 URL: {}", resolved);

    let aweme_id = extract_aweme_id(&resolved)
        .or_else(|| extract_aweme_id(url))
        .ok_or_else(|| ExtractError::InvalidUrl(resolved.clone()))?;
    tracing::info!("📝 提取到 aweme_id: {}", aweme_id);

    // 优先使用用户提供的 Cookie，否则使用默认 Cookie
    let cookie_header = default_cookie(cookies);
    tracing::debug!("🍪 使用 Cookie 长度: {} bytes", cookie_header.len());

    // 构建参数
    let params = build_params(&aweme_id);
    let query = params_to_query(&params);

    // 使用移植自 Python 的 A-Bogus 签名（X-Bogus 已经失效）
    let a_bogus = abogus::generate_abogus(&query, UA, None);
    let a_bogus_encoded = urlencoding::encode(&a_bogus);

    let api_url = format!(
        "https://www.douyin.com/aweme/v1/web/aweme/detail/?{}&a_bogus={}",
        query, a_bogus_encoded
    );
    tracing::debug!("🌐 API URL: {}", api_url);

    let request = client
        .get(&api_url)
        .header("Referer", "https://www.douyin.com/")
        .header("Accept", "application/json, text/plain, */*")
        .header("Accept-Language", "zh-CN,zh;q=0.9")
        .header("Cookie", cookie_header);

    let resp = request
        .send()
        .await
        .map_err(|e| ExtractError::Network(e.to_string()))?;

    let status = resp.status();
    tracing::debug!("📡 响应状态: {}", status);

    if !status.is_success() {
        return Err(ExtractError::Network(format!(
            "status {}",
            status
        )));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| ExtractError::Parse(e.to_string()))?;
    tracing::debug!("📦 响应长度: {} bytes", body.len());

    // 调试：如果响应为空或很短，打印出来
    if body.len() < 100 {
        tracing::warn!("⚠️ 响应内容很短或为空: {}", body);
        if body.is_empty() {
            return Err(ExtractError::Parse("API 返回空响应，需要提供有效的 Cookie".to_string()));
        }
    }

    let api_resp: DouyinApiResponse =
        serde_json::from_str(&body).map_err(|e| {
            tracing::error!("❌ JSON 解析失败: {}, 响应前 500 字符: {}", e, &body[..body.len().min(500)]);
            ExtractError::Parse(format!("JSON 解析失败: {}", e))
        })?;

    // 调试：打印响应状态码
    if let Some(code) = api_resp.status_code {
        tracing::debug!("📊 API status_code: {}", code);
        if code != 0 {
            // 打印错误信息
            tracing::warn!("⚠️ API 返回错误: status_code={}, 响应: {}", code, &body[..body.len().min(500)]);
        }
    }

    // 检查是否有过滤信息
    if api_resp.aweme_detail.is_none() {
        // 尝试解析过滤原因
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) {
            if let Some(filter) = json.get("filter_detail") {
                if let Some(reason) = filter.get("filter_reason").and_then(|v| v.as_str()) {
                    let msg = match reason {
                        "core_dep" => "视频不可用（可能已删除、受地区限制或受版权保护）",
                        "video_deleted" => "视频已被删除",
                        "region_block" => "该视频在您的地区不可用",
                        _ => "视频不可用",
                    };
                    tracing::warn!("⚠️ 视频被过滤: filter_reason={}, aweme_id={}", reason, aweme_id);
                    return Err(ExtractError::Parse(format!("{}: filter_reason={}", msg, reason)));
                }
            }
        }
        tracing::error!("❌ 响应中缺少 aweme_detail，响应: {}", &body[..body.len().min(1000)]);
        return Err(ExtractError::Parse("missing aweme_detail".to_string()));
    }

    let detail = api_resp.aweme_detail.unwrap();

    let video = detail
        .video
        .ok_or_else(|| ExtractError::Parse("missing video".to_string()))?;

    let mut formats = Vec::new();
    if let Some(bit_rates) = video.bit_rate {
        for br in bit_rates {
            if let Some(addr) = br.play_addr {
                let url = addr.url_list.as_ref().and_then(|l| l.first()).cloned();
                let resolution = match (addr.width, addr.height) {
                    (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                    _ => None,
                };
                let format_id = br
                    .gear_name
                    .clone()
                    .or_else(|| br.quality_type.map(|q| format!("q{}", q)))
                    .unwrap_or_else(|| "default".to_string());
                formats.push(VideoFormat {
                    format_id,
                    ext: addr
                        .format
                        .clone()
                        .unwrap_or_else(|| "mp4".to_string()),
                    resolution,
                    fps: None,
                    filesize: addr.data_size,
                    vcodec: None,
                    acodec: None,
                    quality: br.gear_name.map(|g| g.to_string()),
                    download_url: url,
                });
            }
        }
    }

    if formats.is_empty() {
        if let Some(addr) = video.play_addr {
            let url = addr.url_list.as_ref().and_then(|l| l.first()).cloned();
            let resolution = match (addr.width, addr.height) {
                (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                _ => None,
            };
            formats.push(VideoFormat {
                format_id: "play".to_string(),
                ext: addr
                    .format
                    .clone()
                    .unwrap_or_else(|| "mp4".to_string()),
                resolution,
                fps: None,
                filesize: addr.data_size,
                vcodec: None,
                acodec: None,
                quality: None,
                download_url: url,
            });
        }
    }

    let thumbnail = video
        .cover
        .and_then(|c| c.url_list.and_then(|l| l.first().cloned()));

    Ok(VideoInfo {
        id: detail.aweme_id.unwrap_or_else(|| aweme_id.clone()),
        title: detail
            .desc
            .clone()
            .unwrap_or_else(|| "抖音视频".to_string()),
        description: detail.desc,
        duration: video
            .duration
            .map(|d| if d > 10_000 { Duration::from_millis(d) } else { Duration::from_secs(d) }),
        uploader: detail.author.and_then(|a| a.nickname),
        upload_date: None,
        thumbnail,
        formats,
        url: url.to_string(),
    })
}

/// 直接获取作品详情 JSON（等价于 web_crawler.py 的 fetch_one_video）
pub async fn fetch_post_detail_api(
    aweme_id: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = build_params(aweme_id);
    // 与 Python 逻辑保持一致，msToken 置空
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::POST_DETAIL, params, cookies, false).await
}

/// 获取用户发布作品列表
pub async fn fetch_user_posts_api(
    sec_user_id: &str,
    max_cursor: i64,
    count: i64,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("sec_user_id", sec_user_id.to_string());
    params.insert("max_cursor", max_cursor.to_string());
    params.insert("count", count.to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::USER_POST, params, cookies, false).await
}

/// 获取用户点赞作品列表
pub async fn fetch_user_like_api(
    sec_user_id: &str,
    max_cursor: i64,
    count: i64,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("sec_user_id", sec_user_id.to_string());
    params.insert("max_cursor", max_cursor.to_string());
    params.insert("count", count.to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::USER_FAVORITE_A, params, cookies, false).await
}

/// 获取用户合辑作品列表
pub async fn fetch_user_mix_api(
    mix_id: &str,
    cursor: i64,
    count: i64,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("mix_id", mix_id.to_string());
    params.insert("cursor", cursor.to_string());
    params.insert("count", count.to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::MIX_AWEME, params, cookies, false).await
}

/// 获取直播间信息
pub async fn fetch_live_info_api(
    web_rid: &str,
    room_id_str: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = IndexMap::new();
    params.insert("aid", "6383".to_string());
    params.insert("app_name", "douyin_web".to_string());
    params.insert("live_id", "1".to_string());
    params.insert("device_platform", "web".to_string());
    params.insert("language", "zh-CN".to_string());
    params.insert("cookie_enabled", "true".to_string());
    params.insert("screen_width", "1920".to_string());
    params.insert("screen_height", "1080".to_string());
    params.insert("browser_language", "zh-CN".to_string());
    params.insert("browser_platform", "Win32".to_string());
    params.insert("browser_name", "Edge".to_string());
    params.insert("browser_version", "119.0.0.0".to_string());
    params.insert("enter_source", "".to_string());
    params.insert("is_need_double_stream", "false".to_string());
    params.insert("web_rid", web_rid.to_string());
    params.insert("room_id_str", room_id_str.to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::LIVE_INFO, params, cookies, false).await
}

/// 通过 room_id 获取直播信息
pub async fn fetch_live_info_by_room_api(
    room_id: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = IndexMap::new();
    params.insert("verifyFp", "".to_string());
    params.insert("type_id", "0".to_string());
    params.insert("live_id", "1".to_string());
    params.insert("sec_user_id", "".to_string());
    params.insert("version_code", "99.99.99".to_string());
    params.insert("app_id", "1128".to_string());
    params.insert("msToken", "".to_string());
    params.insert("room_id", room_id.to_string());
    fetch_json(DouyinEndpoints::LIVE_INFO_ROOM_ID, params, cookies, false).await
}

/// 获取直播送礼排行榜
pub async fn fetch_live_gift_rank_api(
    room_id: &str,
    rank_type: i64,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("webcast_sdk_version", "2450".to_string());
    params.insert("room_id", room_id.to_string());
    params.insert("rank_type", rank_type.to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::LIVE_GIFT_RANK, params, cookies, false).await
}

/// 获取用户 profile 信息
pub async fn fetch_user_profile_api(
    sec_user_id: &str,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("sec_user_id", sec_user_id.to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::USER_DETAIL, params, cookies, false).await
}

/// 获取视频评论
pub async fn fetch_video_comments_api(
    aweme_id: &str,
    cursor: i64,
    count: i64,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("aweme_id", aweme_id.to_string());
    params.insert("cursor", cursor.to_string());
    params.insert("count", count.to_string());
    params.insert("item_type", "0".to_string());
    params.insert("insert_ids", "".to_string());
    params.insert("whale_cut_token", "".to_string());
    params.insert("cut_version", "1".to_string());
    params.insert("rcFT", "".to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::POST_COMMENT, params, cookies, false).await
}

/// 获取评论回复
pub async fn fetch_video_comments_reply_api(
    item_id: &str,
    comment_id: &str,
    cursor: i64,
    count: i64,
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("item_id", item_id.to_string());
    params.insert("comment_id", comment_id.to_string());
    params.insert("cursor", cursor.to_string());
    params.insert("count", count.to_string());
    params.insert("item_type", "0".to_string());
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::POST_COMMENT_REPLY, params, cookies, false).await
}

/// 获取抖音热榜
pub async fn fetch_hot_search_api(
    cookies: Option<&[PlatformCookie]>,
) -> ExtractResult<Value> {
    let mut params = base_request_params();
    params.insert("msToken", "".to_string());
    fetch_json(DouyinEndpoints::DOUYIN_HOT_SEARCH, params, cookies, false).await
}

async fn resolve_short_link(client: &reqwest::Client, url: &str) -> ExtractResult<String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| ExtractError::Network(e.to_string()))?;
    Ok(resp.url().to_string())
}

/// 从网页提取视频信息（无需 Cookie）
async fn extract_from_webpage(client: &reqwest::Client, aweme_id: &str) -> ExtractResult<VideoInfo> {
    let page_url = format!("https://www.douyin.com/video/{}", aweme_id);
    tracing::debug!("📄 尝试从网页获取: {}", page_url);

    let resp = client
        .get(&page_url)
        .header("Referer", "https://www.douyin.com/")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .send()
        .await
        .map_err(|e| ExtractError::Network(e.to_string()))?;

    let status = resp.status();
    tracing::debug!("📡 网页响应状态: {}", status);

    if !status.is_success() {
        return Err(ExtractError::Network(format!("网页请求失败: {}", status)));
    }

    let html = resp.text().await.map_err(|e| ExtractError::Parse(e.to_string()))?;
    tracing::debug!("📦 HTML 长度: {} bytes", html.len());

    // 尝试从页面中提取 SSR 数据
    // 抖音页面包含 <script id="RENDER_DATA" type="application/json">...</script>
    let render_data_re = Regex::new(r#"<script[^>]*id="RENDER_DATA"[^>]*>([^<]+)</script>"#)
        .map_err(|e| ExtractError::Other(e.to_string()))?;

    if let Some(caps) = render_data_re.captures(&html) {
        tracing::debug!("✅ 找到 RENDER_DATA");
        if let Some(data_match) = caps.get(1) {
            let encoded_data = data_match.as_str();
            tracing::debug!("📦 RENDER_DATA 长度: {} bytes", encoded_data.len());
            // URL decode
            let decoded = urlencoding::decode(encoded_data)
                .map_err(|e| ExtractError::Parse(format!("URL decode 失败: {}", e)))?;

            return parse_render_data(&decoded, aweme_id, &page_url);
        }
    } else {
        tracing::debug!("⚠️ 未找到 RENDER_DATA");
    }

    // 备用方式：尝试提取 __NEXT_DATA__
    let next_data_re = Regex::new(r#"<script[^>]*id="__NEXT_DATA__"[^>]*>([^<]+)</script>"#)
        .map_err(|e| ExtractError::Other(e.to_string()))?;

    if let Some(caps) = next_data_re.captures(&html) {
        tracing::debug!("✅ 找到 __NEXT_DATA__");
        if let Some(data_match) = caps.get(1) {
            let json_str = data_match.as_str();
            return parse_next_data(json_str, aweme_id, &page_url);
        }
    } else {
        tracing::debug!("⚠️ 未找到 __NEXT_DATA__");
    }

    // 打印 HTML 前 2000 字符用于调试
    if html.len() > 100 {
        tracing::debug!("📄 HTML 前 2000 字符: {}", &html[..html.len().min(2000)]);
    }

    Err(ExtractError::Parse("无法从网页中提取视频数据".to_string()))
}

/// 解析 RENDER_DATA 中的视频信息
fn parse_render_data(json_str: &str, aweme_id: &str, original_url: &str) -> ExtractResult<VideoInfo> {
    let data: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| ExtractError::Parse(format!("RENDER_DATA JSON 解析失败: {}", e)))?;

    // 遍历数据寻找视频详情
    // 结构通常是: { "xx_uuid": { "aweme": { "detail": {...} } } }
    for (_key, value) in data.as_object().ok_or_else(|| ExtractError::Parse("数据格式错误".to_string()))? {
        if let Some(aweme) = value.get("aweme") {
            if let Some(detail) = aweme.get("detail") {
                return parse_aweme_detail(detail, aweme_id, original_url);
            }
        }
    }

    Err(ExtractError::Parse("RENDER_DATA 中未找到视频详情".to_string()))
}

/// 解析 __NEXT_DATA__ 中的视频信息
fn parse_next_data(json_str: &str, aweme_id: &str, original_url: &str) -> ExtractResult<VideoInfo> {
    let data: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| ExtractError::Parse(format!("__NEXT_DATA__ JSON 解析失败: {}", e)))?;

    // 尝试从 props.pageProps.itemInfo 获取
    if let Some(detail) = data.pointer("/props/pageProps/itemInfo") {
        return parse_aweme_detail(detail, aweme_id, original_url);
    }

    Err(ExtractError::Parse("__NEXT_DATA__ 中未找到视频详情".to_string()))
}

/// 从 JSON 值解析视频详情
fn parse_aweme_detail(detail: &serde_json::Value, aweme_id: &str, original_url: &str) -> ExtractResult<VideoInfo> {
    let id = detail["aweme_id"].as_str()
        .or_else(|| detail["awemeId"].as_str())
        .unwrap_or(aweme_id)
        .to_string();

    let title = detail["desc"].as_str()
        .unwrap_or("抖音视频")
        .to_string();

    let description = detail["desc"].as_str().map(|s| s.to_string());

    let uploader = detail["author"]["nickname"].as_str()
        .or_else(|| detail["authorInfo"]["nickname"].as_str())
        .map(|s| s.to_string());

    let thumbnail = detail["video"]["cover"]["url_list"][0].as_str()
        .or_else(|| detail["video"]["originCover"].as_str())
        .map(|s| s.to_string());

    let duration = detail["video"]["duration"].as_u64()
        .map(|d| if d > 10_000 { Duration::from_millis(d) } else { Duration::from_secs(d) });

    let mut formats = Vec::new();

    // 尝试提取各种格式
    if let Some(video) = detail.get("video") {
        // bit_rate 数组
        if let Some(bit_rates) = video.get("bit_rate").and_then(|v| v.as_array()) {
            for br in bit_rates {
                if let Some(play_addr) = br.get("play_addr") {
                    let url = play_addr["url_list"].as_array()
                        .and_then(|arr| arr.first())
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let width = play_addr["width"].as_u64().map(|v| v as u32);
                    let height = play_addr["height"].as_u64().map(|v| v as u32);
                    let resolution = match (width, height) {
                        (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                        _ => None,
                    };

                    let format_id = br["gear_name"].as_str()
                        .or_else(|| br["quality_type"].as_i64().map(|q| format!("q{}", q).leak() as &str))
                        .unwrap_or("default")
                        .to_string();

                    formats.push(VideoFormat {
                        format_id,
                        ext: "mp4".to_string(),
                        resolution,
                        fps: None,
                        filesize: play_addr["data_size"].as_u64(),
                        vcodec: None,
                        acodec: None,
                        quality: br["gear_name"].as_str().map(|s| s.to_string()),
                        download_url: url,
                    });
                }
            }
        }

        // 如果没有 bit_rate，尝试 play_addr
        if formats.is_empty() {
            if let Some(play_addr) = video.get("play_addr") {
                let url = play_addr["url_list"].as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let width = play_addr["width"].as_u64().map(|v| v as u32);
                let height = play_addr["height"].as_u64().map(|v| v as u32);
                let resolution = match (width, height) {
                    (Some(w), Some(h)) => Some(format!("{}x{}", w, h)),
                    _ => None,
                };

                formats.push(VideoFormat {
                    format_id: "play".to_string(),
                    ext: "mp4".to_string(),
                    resolution,
                    fps: None,
                    filesize: play_addr["data_size"].as_u64(),
                    vcodec: None,
                    acodec: None,
                    quality: None,
                    download_url: url,
                });
            }
        }
    }

    Ok(VideoInfo {
        id,
        title,
        description,
        duration,
        uploader,
        upload_date: None,
        thumbnail,
        formats,
        url: original_url.to_string(),
    })
}

fn extract_aweme_id(url: &str) -> Option<String> {
    // 支持多种 URL 格式：
    // - https://www.douyin.com/video/7321613070743663893
    // - https://www.douyin.com/note/7321613070743663893
    // - https://www.iesdouyin.com/share/video/7321613070743663893
    // - https://www.douyin.com/discover?modal_id=7321613070743663893
    // - https://www.douyin.com/user/xxx?vid=7321613070743663893
    let patterns = [
        r"/video/(\d+)",
        r"/note/(\d+)",
        r"aweme_id=(\d+)",
        r"/share/video/(\d+)",
        r"modal_id=(\d+)",
        r"[?&]vid=(\d+)",
    ];
    for pat in &patterns {
        let re = Regex::new(pat).ok()?;
        if let Some(caps) = re.captures(url) {
            if let Some(id) = caps.get(1) {
                return Some(id.as_str().to_string());
            }
        }
    }
    None
}

#[derive(Debug, serde::Deserialize)]
struct DouyinApiResponse {
    #[serde(default)]
    status_code: Option<i32>,
    #[serde(default)]
    status_msg: Option<String>,
    #[serde(default)]
    aweme_detail: Option<AwemeDetail>,
}

#[derive(Debug, serde::Deserialize)]
struct AwemeDetail {
    #[serde(default)]
    aweme_id: Option<String>,
    #[serde(default)]
    desc: Option<String>,
    #[serde(default)]
    author: Option<Author>,
    #[serde(default)]
    video: Option<AwemeVideo>,
}

#[derive(Debug, serde::Deserialize)]
struct Author {
    #[serde(default)]
    nickname: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct AwemeVideo {
    #[serde(default)]
    duration: Option<u64>,
    #[serde(default)]
    cover: Option<Cover>,
    #[serde(default)]
    bit_rate: Option<Vec<BitRate>>,
    #[serde(default)]
    play_addr: Option<PlayAddr>,
}

#[derive(Debug, serde::Deserialize)]
struct Cover {
    #[serde(default)]
    url_list: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct BitRate {
    #[serde(default)]
    gear_name: Option<String>,
    #[serde(default)]
    quality_type: Option<i64>,
    #[serde(default)]
    bitrate: Option<u64>,
    #[serde(default)]
    play_addr: Option<PlayAddr>,
}

#[derive(Debug, serde::Deserialize)]
struct PlayAddr {
    #[serde(default)]
    url_list: Option<Vec<String>>,
    #[serde(default)]
    height: Option<u32>,
    #[serde(default)]
    width: Option<u32>,
    #[serde(default)]
    data_size: Option<u64>,
    #[serde(default)]
    format: Option<String>,
}

