//! X-Bogus 签名算法实现
//!
//! 移植自 Python 抖音爬虫实现

#![allow(dead_code)]

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use std::time::{SystemTime, UNIX_EPOCH};

const CHARSET: &str = "Dkdpgh4ZKsQB80/Mfvw36XI1R25-WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=";
const UA_KEY: [u8; 3] = [0, 1, 12];
const EMPTY_MD5: &str = "d41d8cd98f00b204e9800998ecf8427e";

/// X-Bogus 签名选项
#[derive(Debug, Clone)]
pub struct XBogusOptions {
    pub timestamp: Option<u64>,
    pub user_agent: Option<String>,
}

impl Default for XBogusOptions {
    fn default() -> Self {
        Self {
            timestamp: None,
            user_agent: None,
        }
    }
}

/// X-Bogus 签名结果
#[derive(Debug, Clone)]
pub struct XBogusResult {
    pub params: String,
    pub signature: String,
    pub user_agent: String,
}

/// 生成 X-Bogus 签名
pub fn generate_xbogus(
    url_path: &str,
    user_agent: &str,
    opts: Option<XBogusOptions>,
) -> XBogusResult {
    let options = opts.unwrap_or_default();
    let ua = options
        .user_agent
        .clone()
        .unwrap_or_else(|| user_agent.to_string());
    let timestamp = options.timestamp.unwrap_or_else(current_ts);

    // array1
    let ua_rc4 = rc4_encrypt(&UA_KEY, ua.as_bytes());
    let ua_base64 = STANDARD.encode(&ua_rc4);
    let ua_md5 = md5_hex(latin1_bytes(&ua_base64));
    let array1 = md5_str_to_array(&ua_md5);

    // array2
    let empty_bytes = md5_str_to_array(EMPTY_MD5);
    let empty_md5 = md5_hex(empty_bytes);
    let array2 = md5_str_to_array(&empty_md5);

    // url path
    let url_path_array = md5_encrypt(url_path);

    let ct: u64 = 536_919_696;
    let mut new_array: Vec<f64> = vec![
        64.0,
        0.00390625,
        1.0,
        12.0,
        url_path_array[14] as f64,
        url_path_array[15] as f64,
        array2[14] as f64,
        array2[15] as f64,
        array1[14] as f64,
        array1[15] as f64,
        ((timestamp >> 24) & 255) as f64,
        ((timestamp >> 16) & 255) as f64,
        ((timestamp >> 8) & 255) as f64,
        (timestamp & 255) as f64,
        ((ct >> 24) & 255) as f64,
        ((ct >> 16) & 255) as f64,
        ((ct >> 8) & 255) as f64,
        (ct & 255) as f64,
    ];

    let mut xor_result = new_array[0] as i64;
    for b in new_array.iter().skip(1) {
        xor_result ^= *b as i64;
    }
    new_array.push(xor_result as f64);

    let mut array3 = Vec::new();
    let mut array4 = Vec::new();
    let mut idx = 0usize;
    while idx < new_array.len() {
        array3.push(new_array[idx] as u8);
        if let Some(v) = new_array.get(idx + 1) {
            array4.push(*v as u8);
        }
        idx += 2;
    }

    let mut merge_array = array3;
    merge_array.extend(array4);

    let merged_str = encoding_conversion(&merge_array);
    let garbled = encoding_conversion2(
        2,
        255,
        latin1_string(&rc4_encrypt(&[0xFF], latin1_bytes(&merged_str).as_slice())),
    );

    let mut xb = String::new();
    let mut i = 0usize;
    let bytes = latin1_bytes(&garbled);
    while i + 2 < bytes.len() {
        xb.push_str(&calculation(bytes[i], bytes[i + 1], bytes[i + 2]));
        i += 3;
    }

    XBogusResult {
        params: format!("{url_path}&X-Bogus={xb}"),
        signature: xb,
        user_agent: ua,
    }
}

fn md5_hex<T: AsRef<[u8]>>(data: T) -> String {
    format!("{:x}", md5::compute(data))
}

fn md5_str_to_array(s: &str) -> Vec<u8> {
    if s.len() > 32 {
        return s.chars().map(|c| c as u8).collect();
    }

    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        let high = (bytes[i] as char).to_digit(16).unwrap_or(0) as u8;
        let low = (bytes[i + 1] as char).to_digit(16).unwrap_or(0) as u8;
        out.push((high << 4) | low);
        i += 2;
    }
    out
}

fn md5_encrypt(url_path: &str) -> Vec<u8> {
    let first = md5_hex(url_path.as_bytes());
    let first_arr = md5_str_to_array(&first);
    let second = md5_hex(first_arr);
    md5_str_to_array(&second)
}

fn rc4_encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s: Vec<u8> = (0u8..=255).collect();
    let mut j: usize = 0;
    for i in 0..256 {
        j = (j + s[i] as usize + key[i % key.len()] as usize) % 256;
        s.swap(i, j);
    }

    let mut i = 0usize;
    j = 0usize;
    let mut out = Vec::with_capacity(data.len());
    for byte in data {
        i = (i + 1) % 256;
        j = (j + s[i] as usize) % 256;
        s.swap(i, j);
        let t = (s[i] as usize + s[j] as usize) % 256;
        let k = s[t] as usize;
        out.push((*byte as usize ^ k) as u8);
    }
    out
}

fn encoding_conversion(values: &[u8]) -> String {
    if values.len() < 19 {
        return String::new();
    }
    let y = vec![
        values[0], values[10], values[1], values[11], values[2], values[12], values[3], values[13],
        values[4], values[14], values[5], values[15], values[6], values[16], values[7], values[17],
        values[8], values[18], values[9],
    ];
    latin1_string(&y)
}

fn encoding_conversion2(a: u8, b: u8, c: String) -> String {
    let mut out = String::new();
    out.push(a as char);
    out.push(b as char);
    out.push_str(&c);
    out
}

fn calculation(a1: u8, a2: u8, a3: u8) -> String {
    let x1 = ((a1 as u32) & 255) << 16;
    let x2 = ((a2 as u32) & 255) << 8;
    let x3 = x1 | x2 | a3 as u32;
    let chars: Vec<char> = CHARSET.chars().collect();
    let mut out = String::new();
    out.push(chars[((x3 & 16515072) >> 18) as usize]);
    out.push(chars[((x3 & 258048) >> 12) as usize]);
    out.push(chars[((x3 & 4032) >> 6) as usize]);
    out.push(chars[(x3 & 63) as usize]);
    out
}

fn latin1_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| *b as char).collect()
}

fn latin1_bytes(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u8).collect()
}

fn current_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xbogus_matches_python_vector() {
        let url = "device_platform=webapp&aid=6383&channel=channel_pc_web";
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/103.0.0.0 Safari/537.36";
        let res = generate_xbogus(
            url,
            ua,
            Some(XBogusOptions {
                timestamp: Some(1_700_000_000),
                user_agent: None,
            }),
        );

        assert_eq!(res.signature, "DFSzswVYEmGANjultmWx-e9WX7jq");
        assert_eq!(
            res.params,
            format!("{url}&X-Bogus=DFSzswVYEmGANjultmWx-e9WX7jq")
        );
    }
}
