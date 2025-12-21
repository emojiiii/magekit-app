//! Bilibili WBI（wrid）签名实现（对齐 crawlers/bilibili/web/utils.py）。
//!
//! Python 版本逻辑要点：
//! - 对签名用的 `wts` 追加固定盐值（不改变最终请求里的 `wts`）
//! - 按 key 排序
//! - 过滤 value 中的 `!'()*`
//! - 使用 `urlencode`（application/x-www-form-urlencoded，空格编码为 `+`）
//! - `w_rid = md5(encoded_query)`

use std::collections::BTreeMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const WTS_SALT: &str = "ea1db124af3c7062474693fa704f4ff8";

#[derive(Clone, Default)]
pub struct WridSigner;

impl WridSigner {
    pub fn new() -> Self {
        Self
    }

    pub fn sign_query(&self, mut params: BTreeMap<String, String>) -> String {
        let original_wts = params
            .get("wts")
            .cloned()
            .unwrap_or_else(|| now_epoch_secs().to_string());
        params.insert("wts".to_string(), original_wts.clone());

        let mut sign_params = params.clone();
        sign_params.insert("wts".to_string(), format!("{}{}", original_wts, WTS_SALT));

        let encode_query = form_encode_filtered(&sign_params);
        let w_rid = format!("{:x}", md5::compute(encode_query.as_bytes()));

        params.insert("w_rid".to_string(), w_rid);
        form_encode(&params)
    }
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs()
}

fn form_encode(params: &BTreeMap<String, String>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in params.iter() {
        serializer.append_pair(k, v);
    }
    serializer.finish()
}

fn form_encode_filtered(params: &BTreeMap<String, String>) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in params.iter() {
        let filtered = filter_wbi_value(v);
        serializer.append_pair(k, &filtered);
    }
    serializer.finish()
}

fn filter_wbi_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, '!' | '\'' | '(' | ')' | '*'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrid_matches_python_vectors() {
        // 这些期望值由 Python 版（urllib.parse.urlencode + md5）计算得到：
        // see: crawlers/bilibili/web/utils.py + wrid.py
        let signer = WridSigner::new();

        let mut user_post = BTreeMap::new();
        user_post.insert("wts".to_string(), "1700000000".to_string());
        user_post.insert(
            "dm_img_inter".to_string(),
            r#"{"ds":[],"wh":[3557,5674,5],"of":[154,308,154]}"#.to_string(),
        );
        user_post.insert("dm_img_list".to_string(), "[]".to_string());
        user_post.insert("mid".to_string(), "94510621".to_string());
        user_post.insert("pn".to_string(), "1".to_string());
        user_post.insert("ps".to_string(), "20".to_string());
        let signed_user_post = signer.sign_query(user_post);
        assert!(signed_user_post.contains("w_rid=4aec6cbc8fa48024bd8a4410adcb332c"));

        let mut profile = BTreeMap::new();
        profile.insert("wts".to_string(), "1700000000".to_string());
        profile.insert("mid".to_string(), "94510621".to_string());
        let signed_profile = signer.sign_query(profile);
        assert!(signed_profile.contains("w_rid=43f69180db68d75acfc527f8bca6f15f"));

        let mut playurl = BTreeMap::new();
        playurl.insert("wts".to_string(), "1700000000".to_string());
        playurl.insert("bvid".to_string(), "BV1fW411W7cR".to_string());
        playurl.insert("cid".to_string(), "123456".to_string());
        playurl.insert("qn".to_string(), "80".to_string());
        playurl.insert("fnval".to_string(), "0".to_string());
        playurl.insert("fourk".to_string(), "1".to_string());
        let signed_playurl = signer.sign_query(playurl);
        assert!(signed_playurl.contains("w_rid=e8a6cb57abd3c4e6de6f794a40d8d048"));
    }
}
