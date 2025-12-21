//! Bilibili API 接口实现

use super::endpoints::BilibiliEndpoints;
use super::wrid::WridSigner;
use crate::client::{BdClient, ClientConfig};
use crate::error::{BdError, BdResult};
use serde_json::Value;
use std::collections::BTreeMap;

// 对齐 crawlers/bilibili/web/config.yaml 中的 UA（部分风控/票据可能与 UA 指纹相关）
const BILIBILI_WEB_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/90.0.4430.212 Safari/537.36";

fn max_accept_quality(resp: &Value) -> Option<u32> {
    resp.get("data")
        .and_then(|d| d.get("accept_quality"))
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.iter().filter_map(|x| x.as_u64()).max())
        .map(|m| m as u32)
}

/// Bilibili API 客户端
pub struct BilibiliApi {
    client: BdClient,
    cookie: String,
    user_agent: String,
    wrid: WridSigner,
}

impl BilibiliApi {
    pub fn new() -> BdResult<Self> {
        let client = BdClient::with_config(ClientConfig {
            user_agent: BILIBILI_WEB_UA.to_string(),
            ..Default::default()
        })?;
        let ua = client.user_agent().to_string();
        Ok(Self {
            client,
            cookie: String::new(),
            user_agent: ua,
            wrid: WridSigner::new(),
        })
    }

    pub fn set_cookie(&mut self, cookie: impl Into<String>) {
        self.cookie = cookie.into();
    }

    pub async fn resolve_short_url(&self, url: &str) -> BdResult<String> {
        let resp = self
            .client
            .inner()
            .get(url)
            .send()
            .await
            .map_err(BdError::from)?;
        Ok(resp.url().to_string())
    }

    pub fn extract_bvid(url: &str) -> Option<String> {
        // BVxxxxxxxxxxx（12 位，含 BV 前缀）
        let re = regex::Regex::new(r"(BV[0-9A-Za-z]{10})").ok()?;
        re.captures(url)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    pub fn extract_mid(url: &str) -> Option<String> {
        let parsed = url::Url::parse(url).ok()?;
        let host = parsed.host_str()?.to_lowercase();
        if host != "space.bilibili.com" {
            return None;
        }
        let mid = parsed.path_segments()?.next()?.to_string();
        (!mid.is_empty()).then_some(mid)
    }

    /// 视频详情（/x/web-interface/view）
    pub async fn get_video_detail(&self, bvid: &str) -> BdResult<Value> {
        let url = format!("{}?bvid={}", BilibiliEndpoints::VIDEO_DETAIL, bvid);
        self.get_json(&url, BiliReferer::Video(Some(bvid))).await
    }

    /// 播放地址（WBI 版 /x/player/wbi/playurl，优先返回 DASH）
    pub async fn get_video_playurl(&self, bvid: &str, cid: u64, qn: u32) -> BdResult<Value> {
        let mut params = BTreeMap::new();
        params.insert("bvid".to_string(), bvid.to_string());
        params.insert("cid".to_string(), cid.to_string());
        params.insert("qn".to_string(), qn.to_string());
        // 对齐 crawlers/bilibili/web/models.py：fnval=4048，返回 DASH（含更高画质）
        // 额外补齐 web 常用参数：fnver/platform，避免默认平台被识别为 html5 导致画质受限。
        params.insert("fnver".to_string(), "0".to_string());
        params.insert("platform".to_string(), "pc".to_string());
        params.insert("fnval".to_string(), "4048".to_string());
        if qn >= 120 {
            params.insert("fourk".to_string(), "1".to_string());
        }
        let query = self.wrid.sign_query(params);
        let url = format!("{}?{}", BilibiliEndpoints::VIDEO_PLAYURL_WBI, query);
        self.get_json(&url, BiliReferer::Video(Some(bvid))).await
    }

    /// UP 主投稿列表（WBI，分页）
    pub async fn get_user_post_videos(&self, mid: &str, pn: u32, ps: u32) -> BdResult<Value> {
        let mut params = BTreeMap::new();
        params.insert("mid".to_string(), mid.to_string());
        params.insert("pn".to_string(), pn.to_string());
        params.insert("ps".to_string(), ps.to_string());
        self.insert_wbi_antibot_params(&mut params);
        let query = self.wrid.sign_query(params);
        let url = format!("{}?{}", BilibiliEndpoints::SPACE_ARC_SEARCH, query);
        self.get_json(&url, BiliReferer::Space(mid)).await
    }

    /// UP 主信息（WBI）
    ///
    /// 说明：该接口在部分环境下对 Cookie 要求较高（可能需要 bili_ticket/SESSDATA 等）。
    /// 调用方应做好失败兜底（例如从投稿列表推断昵称）。
    pub async fn get_user_profile(&self, mid: &str) -> BdResult<Value> {
        let mut params = BTreeMap::new();
        params.insert("mid".to_string(), mid.to_string());
        let query = self.wrid.sign_query(params);
        let url = format!("{}?{}", BilibiliEndpoints::SPACE_ACC_INFO, query);
        self.get_json(&url, BiliReferer::Space(mid)).await
    }

    /// 综合热门（分页）
    pub async fn get_com_popular(&self, pn: u32, ps: u32) -> BdResult<Value> {
        let url = format!(
            "{}?pn={}&ps={}&web_location=333.934",
            BilibiliEndpoints::COM_POPULAR,
            pn,
            ps
        );
        self.get_json(&url, BiliReferer::Video(None)).await
    }

    /// 视频评论
    pub async fn get_video_comments(
        &self,
        oid: &str,
        pn: u32,
        ps: u32,
        sort: u32,
    ) -> BdResult<Value> {
        // type=1：视频评论；oid 传 aid（注意：不是 BV）
        let url = format!(
            "{}?type=1&oid={}&sort={}&nohot=0&ps={}&pn={}",
            BilibiliEndpoints::VIDEO_COMMENTS,
            oid,
            sort,
            ps,
            pn
        );
        self.get_json(&url, BiliReferer::Video(None)).await
    }

    /// 评论的回复
    pub async fn get_comment_reply(
        &self,
        oid: &str,
        root: &str,
        pn: u32,
        ps: u32,
    ) -> BdResult<Value> {
        let url = format!(
            "{}?type=1&oid={}&root={}&ps={}&pn={}",
            BilibiliEndpoints::COMMENT_REPLY,
            oid,
            root,
            ps,
            pn
        );
        self.get_json(&url, BiliReferer::Video(None)).await
    }

    /// 用户动态（WBI）
    pub async fn get_user_dynamic(&self, host_mid: &str, offset: &str) -> BdResult<Value> {
        let mut params = BTreeMap::new();
        params.insert("host_mid".to_string(), host_mid.to_string());
        params.insert("offset".to_string(), offset.to_string());
        let query = self.wrid.sign_query(params);
        let url = format!("{}?{}", BilibiliEndpoints::USER_DYNAMIC, query);
        self.get_json(&url, BiliReferer::Space(host_mid)).await
    }

    /// 直播间信息
    pub async fn get_live_room_detail(&self, room_id: &str) -> BdResult<Value> {
        let url = format!("{}?room_id={}", BilibiliEndpoints::LIVEROOM_DETAIL, room_id);
        self.get_json(&url, BiliReferer::Video(None)).await
    }

    /// 直播分区列表
    pub async fn get_live_areas(&self) -> BdResult<Value> {
        self.get_json(BilibiliEndpoints::LIVE_AREAS, BiliReferer::Video(None))
            .await
    }

    /// 直播流（quality: 1/2/3/4...）
    pub async fn get_live_playurl(&self, cid: &str, quality: u32) -> BdResult<Value> {
        let url = format!(
            "{}?cid={}&quality={}",
            BilibiliEndpoints::LIVE_VIDEOS,
            cid,
            quality
        );
        self.get_json(&url, BiliReferer::Video(None)).await
    }

    /// 正在直播的主播（分区）
    pub async fn get_live_streamers(&self, parent_area_id: &str, page: u32) -> BdResult<Value> {
        let url = format!(
            "{}?platform=web&parent_area_id={}&page={}",
            BilibiliEndpoints::LIVE_STREAMER,
            parent_area_id,
            page
        );
        self.get_json(&url, BiliReferer::Video(None)).await
    }

    fn insert_wbi_antibot_params(&self, params: &mut BTreeMap<String, String>) {
        // 对齐 crawlers/bilibili/web/models.py（尽量少带“指纹字段”）。
        params
            .entry("dm_img_inter".to_string())
            .or_insert_with(|| r#"{"ds":[],"wh":[3557,5674,5],"of":[154,308,154]}"#.to_string());
        params
            .entry("dm_img_list".to_string())
            .or_insert_with(|| "[]".to_string());
    }

    /// 发送 GET 并解析 JSON
    async fn get_json(&self, url: &str, referer: BiliReferer<'_>) -> BdResult<Value> {
        let mut req = self.client.inner().get(url);
        req = req.header(reqwest::header::USER_AGENT, &self.user_agent);
        req = req.header(
            reqwest::header::ACCEPT_LANGUAGE,
            "zh-CN,zh;q=0.9,en;q=0.8,en-GB;q=0.7,en-US;q=0.6",
        );
        req = req.header(reqwest::header::ACCEPT, "application/json,text/plain,*/*");

        match referer {
            BiliReferer::Video(bvid) => {
                let referer_url = bvid
                    .map(|b| format!("https://www.bilibili.com/video/{}", b))
                    .unwrap_or_else(|| "https://www.bilibili.com/".to_string());
                req = req
                    .header(reqwest::header::REFERER, referer_url)
                    .header(reqwest::header::ORIGIN, "https://www.bilibili.com");
            }
            BiliReferer::Space(mid) => {
                let referer_url = format!("https://space.bilibili.com/{}", mid);
                req = req
                    .header(reqwest::header::REFERER, referer_url)
                    .header(reqwest::header::ORIGIN, "https://space.bilibili.com");
            }
        }

        if !self.cookie.trim().is_empty() {
            tracing::debug!("🍪 BilibiliApi 请求携带 Cookie: len={}", self.cookie.len());
            req = req.header(reqwest::header::COOKIE, &self.cookie);
        } else {
            tracing::debug!("🍪 BilibiliApi 请求未携带 Cookie");
        }

        let resp = req.send().await.map_err(BdError::from)?;
        let json: Value = resp.json().await.map_err(BdError::from)?;
        Ok(json)
    }
}

enum BiliReferer<'a> {
    /// Bilibili 视频页 Referer（传入 bvid 时使用更贴近浏览器的完整路径）
    Video(Option<&'a str>),
    Space(&'a str),
}
