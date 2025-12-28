//! SOOP (原 AfreecaTV) 直播平台处理器
//!
//! SOOP 韩国版（play.sooplive.co.kr / sooplive.co.kr）直播平台处理器
//!
//! 说明：
//! - 国际版（sooplive.com）已拆分到 `soop_global.rs`，避免无效请求与分支判断。
//! - 推荐 Cookie key：`sooplive`（与国际版 `sooplive.com` 区分）。

use async_trait::async_trait;
use regex::Regex;
use reqwest::Client;
use serde_json;
use std::collections::HashMap;
use std::time::Duration;

use crate::{
    error::{RecorderError, RecorderResult},
    platforms::{PlatformCookies, PlatformHandler},
    types::{LiveRoomInfo, LiveStatus, StreamData, StreamInfo, StreamUrl, VideoQuality},
};

/// SOOP 韩国版处理器
pub struct SoopKrHandler {
    client: Client,
}

#[derive(Debug, Default)]
struct KrPlayPageMeta {
    broad_no: Option<String>,
    title: Option<String>,
    cover_url: Option<String>,
    bj_nick: Option<String>,
}

impl SoopKrHandler {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0",
            )
            .build()
            .expect("Failed to create HTTP client");

        Self { client }
    }

    /// 从 URL 提取 BJ ID 和 broad_no
    fn extract_bj_id_and_broad_no(&self, url: &str) -> RecorderResult<(String, Option<String>)> {
        let parts: Vec<&str> = url.split('/').collect();

        // 支持格式:
        // https://play.sooplive.co.kr/bjid -> (bjid, None)
        // https://play.sooplive.co.kr/bjid/broadno -> (bjid, Some(broadno))

        if parts.len() >= 4 {
            let bj_id = parts[3].split('?').next().unwrap_or(parts[3]);

            // 检查是否有 broad_no（第5个段）
            let broad_no = if parts.len() >= 5 {
                let bn = parts[4].split('?').next().unwrap_or(parts[4]);
                if !bn.is_empty() && bn.chars().all(|c| c.is_ascii_digit()) {
                    Some(bn.to_string())
                } else {
                    None
                }
            } else {
                None
            };

            if !bj_id.is_empty() {
                return Ok((bj_id.to_string(), broad_no));
            }
        }

        Err(RecorderError::InvalidUrlFormat(
            "Cannot extract BJ ID from SOOP URL".to_string(),
        ))
    }

    async fn get_kr_play_page_meta(
        &self,
        bj_id: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<KrPlayPageMeta> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0"
                .parse()
                .unwrap(),
        );
        headers.insert(
            "accept-language",
            "zh-CN,zh;q=0.9,ko;q=0.8,en;q=0.7".parse().unwrap(),
        );
        headers.insert(
            "referer",
            format!("https://play.sooplive.co.kr/{}", bj_id)
                .parse()
                .unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("cookie", val);
            }
        }

        let url = format!("https://play.sooplive.co.kr/{}", bj_id);
        let resp = self.client.get(url).headers(headers).send().await?;
        let html = resp.text().await?;

        let broad_no_re = Regex::new(r"window\.nBroadNo\s*=\s*(\d+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let broad_no = broad_no_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .filter(|n| n != "0");

        let og_title_re = Regex::new(r#"<meta\s+property=["']og:title["']\s+content=["']([^"']*)["']"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let og_image_re = Regex::new(r#"<meta\s+property=["']og:image["']\s+content=["']([^"']*)["']"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let bj_nick_re = Regex::new(r#"window\.szBjNick\s*=\s*['"]([^'"]*)['"]"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let broad_title_re = Regex::new(r#"window\.szBroadTitle\s*=\s*["']([^"']*)["']"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let broad_thum_re = Regex::new(r#"window\.szBroadThumPath\s*=\s*["']([^"']*)["']"#)
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        let title = og_title_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                broad_title_re
                    .captures(&html)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().trim().to_string())
                    .filter(|s| !s.is_empty())
            });

        let cover_url = og_image_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                broad_thum_re
                    .captures(&html)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().trim().to_string())
                    .filter(|s| !s.is_empty())
            })
            .map(|s| {
                if s.starts_with("//") {
                    format!("https:{}", s)
                } else if s.starts_with('/') {
                    format!("https://play.sooplive.co.kr{}", s)
                } else {
                    s
                }
            });

        let bj_nick = bj_nick_re
            .captures(&html)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim().to_string())
            .filter(|s| !s.is_empty());

        Ok(KrPlayPageMeta {
            broad_no,
            title,
            cover_url,
            bj_nick,
        })
    }

    async fn get_kr_bj_nick_and_broad_no_via_player_live_api(
        &self,
        bj_id: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<(Option<String>, Option<String>)> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0"
                .parse()
                .unwrap(),
        );
        headers.insert("accept", "*/*".parse().unwrap());
        headers.insert(
            "accept-language",
            "zh-CN,zh;q=0.9,ko;q=0.8,en;q=0.7".parse().unwrap(),
        );
        headers.insert("origin", "https://play.sooplive.co.kr".parse().unwrap());
        headers.insert(
            "referer",
            format!("https://play.sooplive.co.kr/{}", bj_id)
                .parse()
                .unwrap(),
        );
        headers.insert(
            "content-type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("cookie", val);
            }
        }

        let data = [
            ("bid", bj_id),
            ("bno", ""),
            ("type", "info"),
            ("pwd", ""),
            ("player_type", "html5"),
            ("stream_type", "common"),
            ("quality", "master"),
            ("mode", "landing"),
            ("from_api", "0"),
            ("is_revive", "false"),
        ];

        let url = format!(
            "https://live.sooplive.co.kr/afreeca/player_live_api.php?bjid={}",
            bj_id
        );

        let resp = self
            .client
            .post(url)
            .headers(headers)
            .form(&data)
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;
        let channel = &json["CHANNEL"];

        let bj_nick = channel["BJNICK"].as_str().map(|s| s.to_string());
        let broad_no = channel["BNO"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| channel["BNO"].as_u64().map(|n| n.to_string()));

        tracing::debug!(
            "🧩 player_live_api(info): RESULT={:?}, BJNICK={:?}, BNO={:?}",
            channel["RESULT"],
            bj_nick.as_deref(),
            broad_no.as_deref()
        );

        Ok((bj_nick, broad_no))
    }

    async fn post_kr_watch_api(
        &self,
        bj_id: &str,
        broad_no_param: &str,
        headers: &reqwest::header::HeaderMap,
    ) -> RecorderResult<serde_json::Value> {
        let data = [
            ("bj_id", bj_id),
            ("broad_no", broad_no_param),
            ("agent", "web"),
            ("confirm_adult", "true"),
            ("player_type", "webm"),
            ("mode", "live"),
        ];

        tracing::info!(
            "📡 SOOP KR API 请求: bj_id={}, broad_no={}",
            bj_id,
            if broad_no_param.is_empty() {
                "(auto)"
            } else {
                broad_no_param
            }
        );

        let response = self
            .client
            .post("http://api.m.sooplive.co.kr/broad/a/watch")
            .headers(headers.clone())
            .form(&data)
            .send()
            .await?;

        let json: serde_json::Value = response.json().await?;
        Ok(json)
    }

    /// 获取韩国版流数据
    async fn get_kr_stream_data(
        &self,
        room_id_or_url: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<StreamInfo> {
        // 解析 room_id，可能的格式：
        // 1. "bj_id" - 只有 bj_id
        // 2. "bj_id|broad_no" - bj_id 和 broad_no
        // 3. 完整 URL - 从 URL 提取
        let (bj_id, url_broad_no) = if room_id_or_url.contains('|') {
            // 格式：bj_id|broad_no
            let parts: Vec<&str> = room_id_or_url.split('|').collect();
            (parts[0].to_string(), Some(parts[1].to_string()))
        } else if room_id_or_url.starts_with("http") {
            // 完整 URL
            self.extract_bj_id_and_broad_no(room_id_or_url)
                .unwrap_or_else(|_| (room_id_or_url.to_string(), None))
        } else {
            // 只有 bj_id
            (room_id_or_url.to_string(), None)
        };

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0"
                .parse()
                .unwrap(),
        );
        headers.insert(
            "accept-language",
            "zh-CN,zh;q=0.8,zh-TW;q=0.7,zh-HK;q=0.5,en-US;q=0.3,en;q=0.2"
                .parse()
                .unwrap(),
        );
        headers.insert("referer", "https://m.sooplive.co.kr/".parse().unwrap());
        headers.insert(
            "content-type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("cookie", val);
            }
        }

        // 如果 room_id 中包含 broad_no，则优先使用它；否则为空（让 API 返回当前直播）
        let mut broad_no_param = url_broad_no.as_deref().unwrap_or("");
        let mut json = self.post_kr_watch_api(&bj_id, broad_no_param, &headers).await?;

        // 观测：部分场景下输入的 broad_no 可能已过期，watch API 会返回 -3004。
        // 此时重试 broad_no=(auto)，让服务端返回当前 broad_no。
        let code = json["data"]["code"].as_i64();
        if code == Some(-3004) && !broad_no_param.is_empty() {
            tracing::warn!(
                "⚠️ SOOP KR watch API 返回 -3004，尝试 broad_no=(auto) 重试以纠正过期 broad_no"
            );
            broad_no_param = "";
            json = self.post_kr_watch_api(&bj_id, broad_no_param, &headers).await?;
        }

        // 输出关键字段用于调试
        tracing::info!(
            "📋 SOOP KR API 响应关键字段: result={}, code={:?}, message={:?}",
            json["result"],
            json["data"]["code"],
            json["data"]["message"]
        );

        // 解析主播名称
        let anchor_name = if let Some(nick) = json["data"]["user_nick"].as_str() {
            if let Some(id) = json["data"]["bj_id"].as_str() {
                format!("{}-{}", nick, id)
            } else {
                nick.to_string()
            }
        } else {
            bj_id.to_string()
        };

        // 获取直播标题
        let title = json["data"]["broad_title"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // 获取封面图 - 使用直播缩略图或主播头像
        let thumbnail = json["data"]["thumbnail"].as_str();
        let profile_thumbnail = json["data"]["profile_thumbnail"].as_str();

        tracing::debug!(
            "🖼️ SOOP KR 封面字段: thumbnail={:?}, profile_thumbnail={:?}",
            thumbnail,
            profile_thumbnail
        );

        let cover_url = thumbnail.or(profile_thumbnail).map(|s| {
            // 如果是相对路径，补全为完整URL
            if s.starts_with("//") {
                format!("https:{}", s)
            } else if s.starts_with("/") {
                format!("https://stimg.sooplive.co.kr{}", s)
            } else {
                s.to_string()
            }
        });

        // 获取观看人数 - view_cnt 可能是数字或字符串
        let viewer_count = json["data"]["view_cnt"].as_u64().or_else(|| {
            json["data"]["view_cnt"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
        });

        tracing::info!(
            "📺 SOOP KR 主播: {}, 标题: {}, 封面: {:?}, 观看: {:?}",
            anchor_name,
            title,
            cover_url.is_some(),
            viewer_count
        );

        let room_info = LiveRoomInfo {
            room_id: bj_id.to_string(),
            anchor_name: anchor_name.clone(),
            title,
            status: LiveStatus::Offline, // 先设置为 Offline，后面会更新
            start_time: None,
            viewer_count,
            cover_url,
            extra: HashMap::new(),
        };

        // 检查错误码
        if let Some(code) = json["data"]["code"].as_i64() {
            match code {
                -3001 => {
                    tracing::info!("📴 SOOP 直播刚刚结束");
                    return Ok(StreamInfo {
                        room: room_info,
                        streams: vec![],
                    });
                }
                -3002 => {
                    tracing::warn!("🔒 SOOP 直播需要 19+ 认证（watch API code=-3002），回退解析开播状态");

                    let play_meta = match self.get_kr_play_page_meta(&bj_id, cookies).await {
                        Ok(m) => Some(m),
                        Err(e) => {
                            tracing::warn!("⚠️ 获取 SOOP play 页面信息失败: {}", e);
                            None
                        }
                    };

                    let resolved_broad_no = if !broad_no_param.is_empty() {
                        play_meta
                            .as_ref()
                            .and_then(|m| m.broad_no.clone())
                            .or_else(|| Some(broad_no_param.to_string()))
                    } else {
                        play_meta.as_ref().and_then(|m| m.broad_no.clone())
                    };

                    let mut room_info = room_info;
                    if let Some(ref m) = play_meta {
                        if let Some(ref title) = m.title {
                            room_info.title = title.clone();
                        }
                        if let Some(ref cover) = m.cover_url {
                            room_info.cover_url = Some(cover.clone());
                        }
                        if let Some(ref nick) = m.bj_nick {
                            room_info.anchor_name = format!("{}-{}", nick, bj_id);
                        }
                    }

                    room_info.status = if resolved_broad_no.is_some() {
                        LiveStatus::Live
                    } else {
                        LiveStatus::Offline
                    };
                    room_info.extra.insert(
                        "requires_auth".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    room_info.extra.insert(
                        "auth_reason".to_string(),
                        serde_json::Value::String("soop_19plus".to_string()),
                    );

                    if let Some(bn) = resolved_broad_no {
                        room_info.extra.insert(
                            "bj_id".to_string(),
                            serde_json::Value::String(bj_id.clone()),
                        );
                        room_info.extra.insert(
                            "broad_no".to_string(),
                            serde_json::Value::String(bn.clone()),
                        );
                        room_info.extra.insert(
                            "play_url".to_string(),
                            serde_json::Value::String(format!(
                                "https://play.sooplive.co.kr/{}/{}",
                                bj_id, bn
                            )),
                        );
                    }

                    return Ok(StreamInfo {
                        room: room_info,
                        streams: vec![],
                    });
                }
                -3004 => {
                    tracing::warn!("🔒 SOOP watch API 返回 -3004，尝试通过 player_live_api 获取流信息");

                    // 先补齐 play 页面可见的元数据（标题/封面/当前 broad_no）
                    let play_meta = match self.get_kr_play_page_meta(&bj_id, cookies).await {
                        Ok(m) => Some(m),
                        Err(e) => {
                            tracing::warn!("⚠️ 获取 SOOP play 页面信息失败: {}", e);
                            None
                        }
                    };

                    // 复用 player_live_api 的 BNO（若可用）作为更可靠的当前 broad_no
                    let (bj_nick, broad_no_from_player) =
                        self.get_kr_bj_nick_and_broad_no_via_player_live_api(&bj_id, cookies)
                            .await
                            .unwrap_or((None, None));

                    let resolved_broad_no = broad_no_from_player
                        .or_else(|| play_meta.as_ref().and_then(|m| m.broad_no.clone()))
                        .or_else(|| {
                            if broad_no_param.is_empty() {
                                None
                            } else {
                                Some(broad_no_param.to_string())
                            }
                        });

                    let mut room_info = room_info;
                    if let Some(ref m) = play_meta {
                        if let Some(ref title) = m.title {
                            room_info.title = title.clone();
                        }
                        if let Some(ref cover) = m.cover_url {
                            room_info.cover_url = Some(cover.clone());
                        }
                    }
                    if let Some(nick) = bj_nick.or_else(|| play_meta.as_ref().and_then(|m| m.bj_nick.clone()))
                    {
                        room_info.anchor_name = format!("{}-{}", nick, bj_id);
                    }

                    // 有 broad_no 才能判定为 Live，否则保持 Offline
                    room_info.status = if resolved_broad_no.is_some() {
                        LiveStatus::Live
                    } else {
                        LiveStatus::Offline
                    };

                    if let Some(ref bn) = resolved_broad_no {
                        room_info.extra.insert(
                            "bj_id".to_string(),
                            serde_json::Value::String(bj_id.clone()),
                        );
                        room_info.extra.insert(
                            "broad_no".to_string(),
                            serde_json::Value::String(bn.clone()),
                        );
                        room_info.extra.insert(
                            "play_url".to_string(),
                            serde_json::Value::String(format!(
                                "https://play.sooplive.co.kr/{}/{}",
                                bj_id, bn
                            )),
                        );
                    }

                    // 若没有 Cookie，直接提示需要登录；有 Cookie 则继续尝试拿流（对齐 py_demo）
                    if cookies.is_none() {
                        room_info.extra.insert(
                            "requires_auth".to_string(),
                            serde_json::Value::Bool(true),
                        );
                        room_info.extra.insert(
                            "auth_reason".to_string(),
                            serde_json::Value::String("soop_login".to_string()),
                        );
                        return Ok(StreamInfo {
                            room: room_info,
                            streams: vec![],
                        });
                    }

                    // 尝试获取 AID，拼出 master m3u8，并解析清晰度列表
                    if let Some(ref broad_no) = resolved_broad_no {
                        let aid = match self
                            .get_kr_aid_via_player_live_api(&bj_id, Some(broad_no), cookies)
                            .await?
                        {
                            Some(aid) => aid,
                            None => {
                                room_info.extra.insert(
                                    "requires_auth".to_string(),
                                    serde_json::Value::Bool(true),
                                );
                                room_info.extra.insert(
                                    "auth_reason".to_string(),
                                    serde_json::Value::String("soop_login".to_string()),
                                );
                                return Ok(StreamInfo {
                                    room: room_info,
                                    streams: vec![],
                                });
                            }
                        };

                        let cdn_data = self.get_cdn_url(broad_no, cookies).await?;
                        let view_url = cdn_data["view_url"].as_str().unwrap_or("");
                        if !view_url.is_empty() {
                            let aid_sep = if view_url.contains('?') {
                                if view_url.ends_with('?') || view_url.ends_with('&') {
                                    ""
                                } else {
                                    "&"
                                }
                            } else {
                                "?"
                            };
                            let m3u8_url = format!("{}{}aid={}", view_url, aid_sep, aid);

                            let streams = self
                                .parse_kr_m3u8_playlist(&m3u8_url, cookies, &bj_id, broad_no)
                                .await
                                .unwrap_or_else(|e| {
                                    tracing::warn!("⚠️ 解析 SOOP master m3u8 失败: {}", e);
                                    vec![]
                                });

                            return Ok(StreamInfo {
                                room: room_info,
                                streams,
                            });
                        }
                    }

                    room_info.extra.insert(
                        "requires_auth".to_string(),
                        serde_json::Value::Bool(true),
                    );
                    room_info.extra.insert(
                        "auth_reason".to_string(),
                        serde_json::Value::String("soop_login".to_string()),
                    );
                    return Ok(StreamInfo {
                        room: room_info,
                        streams: vec![],
                    });
                }
                -6001 => {
                    tracing::warn!("❌ SOOP 直播间地址错误");
                    return Err(RecorderError::InvalidUrlFormat(
                        "请检查 SOOP 直播间地址是否正确".to_string(),
                    ));
                }
                _ => {}
            }
        }

        // 检查是否成功获取直播信息
        if json["result"].as_i64() != Some(1) || anchor_name.is_empty() {
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 获取直播流信息 - broad_no 可能是数字或字符串
        let broad_no = json["data"]["broad_no"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| json["data"]["broad_no"].as_u64().map(|n| n.to_string()))
            .unwrap_or_default();

        let hls_auth_key = json["data"]["hls_authentication_key"]
            .as_str()
            .unwrap_or("");

        tracing::info!(
            "📹 SOOP KR 流信息: broad_no={:?}, hls_auth_key 长度={}",
            if broad_no.is_empty() {
                "(空)"
            } else {
                &broad_no
            },
            hls_auth_key.len()
        );

        if broad_no.is_empty() {
            tracing::warn!("⚠️ broad_no 为空，直播未开始");
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        // 获取 CDN URL
        tracing::debug!("🌐 正在获取 CDN URL...");
        let cdn_data = self.get_cdn_url(&broad_no, cookies).await?;
        tracing::debug!("📋 CDN 响应: {:?}", cdn_data);

        let view_url = cdn_data["view_url"].as_str().unwrap_or("");

        if view_url.is_empty() {
            tracing::warn!("⚠️ view_url 为空");
            return Ok(StreamInfo {
                room: room_info,
                streams: vec![],
            });
        }

        let aid_sep = if view_url.contains('?') {
            if view_url.ends_with('?') || view_url.ends_with('&') {
                ""
            } else {
                "&"
            }
        } else {
            "?"
        };

        // 对齐 py_demo：优先使用 player_live_api 的 AID 获取 master playlist。
        // watch API 返回的 hls_authentication_key 在部分房间只会返回单档位。
        let aid = self
            .get_kr_aid_via_player_live_api(&bj_id, Some(&broad_no), cookies)
            .await?
            .ok_or_else(|| RecorderError::StreamNotAvailable("SOOP 获取 AID 失败".to_string()))?;
        tracing::info!(
            "✅ 使用 player_live_api AID 获取 master m3u8: {} (aid_len={})",
            view_url,
            aid.len()
        );

        let m3u8_url = format!("{}{}aid={}", view_url, aid_sep, aid);

        let mut room_info = room_info;
        room_info.status = LiveStatus::Live;
        room_info.extra.insert(
            "bj_id".to_string(),
            serde_json::Value::String(bj_id.clone()),
        );
        room_info.extra.insert(
            "broad_no".to_string(),
            serde_json::Value::String(broad_no.clone()),
        );
        room_info.extra.insert(
            "play_url".to_string(),
            serde_json::Value::String(format!(
                "https://play.sooplive.co.kr/{}/{}",
                bj_id, broad_no
            )),
        );

        // 解析 master playlist 获取多码率流（只解析一次，避免“先 1 档后多档”重复日志）
        let mut streams = self
            .parse_kr_m3u8_playlist(&m3u8_url, cookies, &bj_id, &broad_no)
            .await
            .unwrap_or_default();

        // 最终兜底：依然没有解析出变体时，至少返回当前 m3u8
        if streams.is_empty() {
            streams = vec![StreamData {
                quality: VideoQuality::Original,
                url: StreamUrl {
                    flv_url: None,
                    hls_url: Some(m3u8_url),
                    dash_url: None,
                },
                bitrate: None,
                resolution: None,
                codec: None,
                cdn: None,
            }];
        }

        tracing::info!(
            "🎬 SOOP KR 最终可用流 {} 个: {:?}",
            streams.len(),
            streams
                .iter()
                .map(|s| (format!("{:?}", s.quality), s.bitrate, s.resolution))
                .collect::<Vec<_>>()
        );

        Ok(StreamInfo {
            room: room_info,
            streams,
        })
    }

    /// 通过 player_live_api.php 获取 AID（部分房间需要它才能拿到多清晰度 master playlist）
    async fn get_kr_aid_via_player_live_api(
        &self,
        bj_id: &str,
        broad_no: Option<&str>,
        cookies: Option<&str>,
    ) -> RecorderResult<Option<String>> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0"
                .parse()
                .unwrap(),
        );
        headers.insert("accept", "*/*".parse().unwrap());
        headers.insert(
            "accept-language",
            "zh-CN,zh;q=0.9,ko;q=0.8,en;q=0.7".parse().unwrap(),
        );
        headers.insert("origin", "https://play.sooplive.co.kr".parse().unwrap());
        headers.insert(
            "referer",
            match broad_no {
                Some(bn) if !bn.is_empty() => {
                    format!("https://play.sooplive.co.kr/{}/{}", bj_id, bn)
                }
                _ => format!("https://play.sooplive.co.kr/{}", bj_id),
            }
            .parse()
            .unwrap(),
        );
        headers.insert(
            "content-type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("cookie", val);
            }
        }

        let data = [
            ("bid", bj_id),
            ("bno", broad_no.unwrap_or("")),
            ("type", "aid"),
            ("pwd", ""),
            ("player_type", "html5"),
            ("stream_type", "common"),
            ("quality", "master"),
            ("mode", "landing"),
            ("from_api", "0"),
            ("is_revive", "false"),
        ];

        let url = format!(
            "https://live.sooplive.co.kr/afreeca/player_live_api.php?bjid={}",
            bj_id
        );

        let resp = self
            .client
            .post(url)
            .headers(headers)
            .form(&data)
            .send()
            .await?;

        let json: serde_json::Value = resp.json().await?;
        let aid = json["CHANNEL"]["AID"].as_str().map(|s| s.to_string());
        if let Some(ref aid) = aid {
            tracing::debug!("🔑 player_live_api AID 长度={}", aid.len());
        }
        Ok(aid)
    }

    /// 获取 CDN URL
    async fn get_cdn_url(
        &self,
        broad_no: &str,
        cookies: Option<&str>,
    ) -> RecorderResult<serde_json::Value> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0"
                .parse()
                .unwrap(),
        );
        headers.insert("accept-language", "zh-CN,zh;q=0.8".parse().unwrap());
        headers.insert("origin", "https://play.sooplive.co.kr".parse().unwrap());
        headers.insert("referer", "https://play.sooplive.co.kr/".parse().unwrap());
        headers.insert(
            "content-type",
            "application/x-www-form-urlencoded".parse().unwrap(),
        );

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("cookie", val);
            }
        }

        let params = format!(
            "return_type=gcp_cdn&use_cors=false&cors_origin_url=play.sooplive.co.kr&broad_key={}-common-master-hls&time=8361.086329376785",
            broad_no
        );

        let url = format!(
            "http://livestream-manager.sooplive.co.kr/broad_stream_assign.html?{}",
            params
        );

        let response = self.client.get(&url).headers(headers).send().await?;

        let json: serde_json::Value = response.json().await?;
        Ok(json)
    }

    /// 解析 m3u8 播放列表（韩国版）
    async fn parse_kr_m3u8_playlist(
        &self,
        m3u8_url: &str,
        cookies: Option<&str>,
        bj_id: &str,
        broad_no: &str,
    ) -> RecorderResult<Vec<StreamData>> {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:109.0) Gecko/20100101 Firefox/119.0"
                .parse()
                .unwrap(),
        );
        headers.insert("accept", "*/*".parse().unwrap());
        headers.insert(
            "accept-language",
            "zh-CN,zh;q=0.9,ko;q=0.8,en;q=0.7".parse().unwrap(),
        );
        headers.insert("origin", "https://play.sooplive.co.kr".parse().unwrap());
        if !broad_no.is_empty() {
            headers.insert(
                "referer",
                format!("https://play.sooplive.co.kr/{}/{}", bj_id, broad_no)
                    .parse()
                    .unwrap(),
            );
        }

        if let Some(cookie) = cookies {
            if let Ok(val) = cookie.parse() {
                headers.insert("cookie", val);
            }
        }

        let response = self.client.get(m3u8_url).headers(headers).send().await?;

        let content = response.text().await?;

        tracing::debug!(
            "📄 SOOP KR m3u8 内容 ({} 字节):\n{}",
            content.len(),
            &content[..content.len().min(1000)]
        );

        let mut streams = Vec::new();
        let url_prefix = m3u8_url
            .rsplit('/')
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("/");
        let url_prefix = format!("{}/", url_prefix);

        let bandwidth_re = Regex::new(r"BANDWIDTH=(\d+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;
        let resolution_re = Regex::new(r"RESOLUTION=(\d+)x(\d+)")
            .map_err(|e| RecorderError::InvalidResponseFormat(e.to_string()))?;

        let lines: Vec<&str> = content.lines().collect();
        let mut current_bandwidth = 0u64;
        let mut current_resolution: Option<(u32, u32)> = None;
        let mut expecting_variant_uri = false;

        for line in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
            if line.starts_with("#EXT-X-STREAM-INF") {
                expecting_variant_uri = true;
                if let Some(caps) = bandwidth_re.captures(line) {
                    current_bandwidth = caps[1].parse().unwrap_or(0);
                }
                current_resolution = resolution_re.captures(line).and_then(|c| {
                    let w = c.get(1)?.as_str().parse::<u32>().ok()?;
                    let h = c.get(2)?.as_str().parse::<u32>().ok()?;
                    Some((w, h))
                });
                continue;
            }

            if !expecting_variant_uri || line.starts_with('#') {
                continue;
            }
            expecting_variant_uri = false;

            // 部分 SOOP m3u8 的 URI 行末尾会带 ",," 等分隔符（疑似占位字段），需截断
            let uri = line.split(',').next().unwrap_or(line).trim();

            let full_url = if uri.starts_with("http://") || uri.starts_with("https://") {
                uri.to_string()
            } else {
                format!("{}{}", url_prefix, uri)
            };

            streams.push(StreamData {
                quality: VideoQuality::Standard, // 先占位，后续按码率排序再映射档位
                url: StreamUrl {
                    flv_url: None,
                    hls_url: Some(full_url),
                    dash_url: None,
                },
                bitrate: Some(current_bandwidth),
                resolution: current_resolution,
                codec: None,
                cdn: None,
            });
        }

        Self::apply_quality_by_rank(&mut streams);

        tracing::debug!(
            "🎬 SOOP KR 解析出 {} 个流: {:?}",
            streams.len(),
            streams
                .iter()
                .map(|s| (format!("{:?}", s.quality), s.bitrate, s.resolution))
                .collect::<Vec<_>>()
        );

        Ok(streams)
    }

    /// 将 m3u8 变体按码率从高到低映射到统一档位（对齐 py_demo 的“带宽排序选清晰度”）
    fn apply_quality_by_rank(streams: &mut Vec<StreamData>) {
        streams.sort_by(|a, b| b.bitrate.cmp(&a.bitrate));

        for (idx, s) in streams.iter_mut().enumerate() {
            s.quality = match idx {
                0 => VideoQuality::Original,
                1 => VideoQuality::Ultra,
                2 => VideoQuality::High,
                3 => VideoQuality::Standard,
                _ => VideoQuality::Low,
            };
        }
    }
}

impl Default for SoopKrHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl PlatformHandler for SoopKrHandler {
    fn platform_name(&self) -> &'static str {
        "sooplive"
    }

    fn supported_url_patterns(&self) -> Vec<&'static str> {
        vec![
            "play.sooplive.co.kr",
            "sooplive.co.kr",
            "afreecatv.com", // 旧域名兼容
        ]
    }

    async fn extract_room_id(&self, url: &str) -> RecorderResult<String> {
        // 提取 bj_id 和可选的 broad_no
        let (bj_id, broad_no) = self.extract_bj_id_and_broad_no(url)?;

        // 如果有 broad_no，返回 "bj_id|broad_no" 格式
        // 否则只返回 bj_id
        if let Some(bn) = broad_no {
            Ok(format!("{}|{}", bj_id, bn))
        } else {
            Ok(bj_id)
        }
    }

    async fn get_stream_info(&self, room_id: &str) -> RecorderResult<StreamInfo> {
        self.get_stream_info_with_cookies(room_id, &PlatformCookies::default())
            .await
    }

    async fn get_stream_info_with_cookies(
        &self,
        room_id: &str,
        cookies: &PlatformCookies,
    ) -> RecorderResult<StreamInfo> {
        tracing::info!("🎮 正在获取 SOOP 直播间信息: {}", room_id);

        let cookie_str = cookies.cookie.as_deref();

        // 打印 Cookie 信息（INFO 级别，只显示长度以保护隐私）
        if let Some(cookie) = cookie_str {
            tracing::info!("🍪 使用 Cookie，长度: {} 字节", cookie.len());
        } else {
            tracing::warn!("⚠️ 未提供 Cookie（某些需要登录的直播间可能无法访问）");
        }

        tracing::info!("🇰🇷 尝试 SOOP KR API...");
        let info = self.get_kr_stream_data(room_id, cookie_str).await?;
        tracing::info!("✅ SOOP 直播间信息获取成功 (KR)");
        Ok(info)
    }
}
