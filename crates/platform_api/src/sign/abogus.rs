//! A-Bogus 签名算法实现
//!
//! 移植自 Python 抖音爬虫实现

use super::base64_custom::result_encrypt;
use super::rc4::rc4_encrypt;
use super::sm3::SM3;
use rand::Rng;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const END_STRING: &str = "cus";
const DEFAULT_BROWSER: &str = "1536|742|1536|864|0|0|0|0|1536|864|1536|864|1536|742|24|24|MacIntel";

/// A-Bogus 签名选项
#[derive(Debug, Clone)]
pub struct AbogusOptions {
    pub method: String,
    pub start_time: Option<u64>,
    pub end_time: Option<u64>,
    pub random_num_1: Option<f64>,
    pub random_num_2: Option<f64>,
    pub random_num_3: Option<f64>,
    pub browser: Option<String>,
}

impl Default for AbogusOptions {
    fn default() -> Self {
        Self {
            method: "GET".to_string(),
            start_time: None,
            end_time: None,
            random_num_1: None,
            random_num_2: None,
            random_num_3: None,
            browser: None,
        }
    }
}

/// 生成 A-Bogus 签名
pub fn generate_abogus(query: &str, _user_agent: &str, opts: Option<AbogusOptions>) -> String {
    let options = opts.unwrap_or_default();
    let ab = Abogus::new(options.browser.clone());
    ab.get_value(query, &options)
}

struct Abogus {
    ua_code: [u32; 32],
    _browser: String,
    browser_len: u32,
    browser_code: Vec<u32>,
}

impl Abogus {
    fn new(browser: Option<String>) -> Self {
        let browser = browser.unwrap_or_else(|| DEFAULT_BROWSER.to_string());
        let browser_len = browser.chars().count() as u32;
        let browser_code = char_code_at(&browser);

        const UA_CODE: [u32; 32] = [
            76, 98, 15, 131, 97, 245, 224, 133, 122, 199, 241, 166, 79, 34, 90, 191, 128, 126, 122,
            98, 66, 11, 14, 40, 49, 110, 110, 173, 67, 96, 138, 252,
        ];

        Self {
            ua_code: UA_CODE,
            _browser: browser,
            browser_len,
            browser_code,
        }
    }

    fn get_value(&self, url_params: &str, opts: &AbogusOptions) -> String {
        let mut rng = rand::thread_rng();

        let string_1 = self.generate_string_1(
            opts.random_num_1,
            opts.random_num_2,
            opts.random_num_3,
            &mut rng,
        );
        let string_2 = self.generate_string_2(
            url_params,
            &opts.method,
            opts.start_time,
            opts.end_time,
            &mut rng,
        );

        result_encrypt(&(string_1 + &string_2), "s4")
    }

    fn generate_string_1(
        &self,
        random_num_1: Option<f64>,
        random_num_2: Option<f64>,
        random_num_3: Option<f64>,
        rng: &mut impl Rng,
    ) -> String {
        let mut s = String::new();
        s.push_str(&from_char_code(&list_1(random_num_1, rng)));
        s.push_str(&from_char_code(&list_2(random_num_2, rng)));
        s.push_str(&from_char_code(&list_3(random_num_3, rng)));
        s
    }

    fn generate_string_2(
        &self,
        url_params: &str,
        method: &str,
        start_time: Option<u64>,
        end_time: Option<u64>,
        rng: &mut impl Rng,
    ) -> String {
        let mut a = self.generate_string_2_list(url_params, method, start_time, end_time, rng);
        let e = end_check_num(&a);
        a.extend(self.browser_code.iter().copied());
        a.push(e);
        let plain = from_char_code(&a);
        rc4_encrypt(&plain, "y")
    }

    fn generate_string_2_list(
        &self,
        url_params: &str,
        method: &str,
        start_time: Option<u64>,
        end_time: Option<u64>,
        rng: &mut impl Rng,
    ) -> Vec<u32> {
        let start_time = start_time.unwrap_or_else(current_millis);
        let end_time = end_time.unwrap_or_else(|| start_time + rng.gen_range(4..=8));

        let params_array = generate_params_code(url_params);
        let method_array = generate_method_code(method);

        list_4(
            ((end_time >> 24) & 255) as u32,
            params_array[21],
            self.ua_code[23],
            ((end_time >> 16) & 255) as u32,
            params_array[22],
            self.ua_code[24],
            ((end_time >> 8) & 255) as u32,
            (end_time & 255) as u32,
            ((start_time >> 24) & 255) as u32,
            ((start_time >> 16) & 255) as u32,
            ((start_time >> 8) & 255) as u32,
            (start_time & 255) as u32,
            method_array[21],
            method_array[22],
            (end_time / 4294967296) as u32,
            (start_time / 4294967296) as u32,
            self.browser_len,
        )
    }
}

fn current_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn list_1(random_num: Option<f64>, rng: &mut impl Rng) -> [u32; 4] {
    random_list(random_num, 170, 85, 1, 2, 5, 45 & 170, rng)
}

fn list_2(random_num: Option<f64>, rng: &mut impl Rng) -> [u32; 4] {
    random_list(random_num, 170, 85, 1, 0, 0, 0, rng)
}

fn list_3(random_num: Option<f64>, rng: &mut impl Rng) -> [u32; 4] {
    random_list(random_num, 170, 85, 1, 0, 5, 0, rng)
}

fn random_list(
    random_num: Option<f64>,
    b: i64,
    c: i64,
    d: i64,
    e: i64,
    f: i64,
    g: i64,
    rng: &mut impl Rng,
) -> [u32; 4] {
    let r = random_num.unwrap_or_else(|| rng.gen_range(0.0..1.0) * 10000.0);
    let int_r = r.floor() as i64;
    let v1 = int_r & 255;
    let v2 = int_r >> 8;

    [
        (v1 & b | d) as u32,
        (v1 & c | e) as u32,
        (v2 & b | f) as u32,
        (v2 & c | g) as u32,
    ]
}

fn from_char_code(codes: &[u32]) -> String {
    codes
        .iter()
        .filter_map(|&c| char::from_u32(c))
        .collect::<String>()
}

fn char_code_at(s: &str) -> Vec<u32> {
    s.chars().map(|c| c as u32).collect()
}

fn generate_params_code(params: &str) -> Vec<u32> {
    sm3_twice(&(params.to_string() + END_STRING))
}

fn generate_method_code(method: &str) -> Vec<u32> {
    sm3_twice(&(method.to_string() + END_STRING))
}

fn sm3_twice(input: &str) -> Vec<u32> {
    let mut sm3 = SM3::new();
    let first = sm3.sum(Some(input.as_bytes()));
    let second = sm3.sum(Some(&first));
    second.into_iter().map(|b| b as u32).collect()
}

fn list_4(
    a: u32,
    b: u32,
    c: u32,
    d: u32,
    e: u32,
    f: u32,
    g: u32,
    h: u32,
    i: u32,
    j: u32,
    k: u32,
    m: u32,
    n: u32,
    o: u32,
    p: u32,
    q: u32,
    r: u32,
) -> Vec<u32> {
    vec![
        44, a, 0, 0, 0, 0, 24, b, n, 0, c, d, 0, 0, 0, 1, 0, 239, e, o, f, g, 0, 0, 0, 0, h, 0, 0,
        14, i, j, 0, k, m, 3, p, 1, q, 1, r, 0, 0, 0,
    ]
}

fn end_check_num(data: &[u32]) -> u32 {
    data.iter().fold(0, |acc, &v| acc ^ v)
}

/// 使用 live_recorder 中验证过的 AB-Sign 算法
/// 移植自 xbogus crate 的 ab_sign 函数
pub fn ab_sign_live(url_search_params: &str, user_agent: &str) -> String {
    let window_env_str = "1920|1080|1920|1040|0|30|0|0|1872|92|1920|1040|1857|92|1|24|Win32";
    let suffix = "cus";
    let arguments = [0u32, 1, 14];

    let random_str = generate_random_str();
    let rc4_bb_str = generate_rc4_bb_str(
        url_search_params,
        user_agent,
        window_env_str,
        suffix,
        &arguments,
    );

    let combined = format!("{}{}", random_str, rc4_bb_str);
    format!("{}=", result_encrypt(&combined, "s4"))
}

/// 生成随机字节
fn gener_random(random_num: u16, option: &[u8; 2]) -> [u8; 4] {
    let byte1 = (random_num & 255) as u8;
    let byte2 = ((random_num >> 8) & 255) as u8;

    [
        (byte1 & 170) | (option[0] & 85),
        (byte1 & 85) | (option[0] & 170),
        (byte2 & 170) | (option[1] & 85),
        (byte2 & 85) | (option[1] & 170),
    ]
}

/// 生成随机字符串
fn generate_random_str() -> String {
    let random_values = [0.123456789, 0.987654321, 0.555555555];

    let mut random_bytes = Vec::new();
    random_bytes.extend_from_slice(&gener_random((random_values[0] * 10000.0) as u16, &[3, 45]));
    random_bytes.extend_from_slice(&gener_random((random_values[1] * 10000.0) as u16, &[1, 0]));
    random_bytes.extend_from_slice(&gener_random((random_values[2] * 10000.0) as u16, &[1, 5]));

    random_bytes.iter().map(|&b| b as char).collect()
}

/// 将数字拆分为4个字节 (大端序)
fn split_to_bytes(num: u32) -> [u8; 4] {
    [
        ((num >> 24) & 255) as u8,
        ((num >> 16) & 255) as u8,
        ((num >> 8) & 255) as u8,
        (num & 255) as u8,
    ]
}

/// 生成 RC4 加密的 BB 字符串
fn generate_rc4_bb_str(
    url_search_params: &str,
    user_agent: &str,
    window_env_str: &str,
    suffix: &str,
    arguments: &[u32; 3],
) -> String {
    let mut sm3 = SM3::new();
    let start_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    // 1: url_search_params 两次 sm3 的结果
    let combined = format!("{}{}", url_search_params, suffix);
    let url_params_hash1 = sm3.sum(Some(combined.as_bytes()));
    let url_search_params_list = sm3.sum(Some(&url_params_hash1));

    // 2: 对后缀两次 sm3 的结果
    let suffix_hash1 = sm3.sum(Some(suffix.as_bytes()));
    let cus = sm3.sum(Some(&suffix_hash1));

    // 3: 对 ua 处理之后的结果
    let ua_key = format!("{}{}{}", '\u{0}', '\u{1}', '\u{e}');
    let encrypted_ua = rc4_encrypt(user_agent, &ua_key);
    let encoded_ua = result_encrypt(&encrypted_ua, "s3");
    let ua = sm3.sum(Some(encoded_ua.as_bytes()));

    let end_time = start_time + 100;
    let start_time_32 = (start_time & 0xFFFFFFFF) as u32;
    let end_time_32 = (end_time & 0xFFFFFFFF) as u32;

    let aid: u32 = 6383;
    let page_id: u32 = 110624;

    let mut b: HashMap<usize, u32> = HashMap::new();

    b.insert(8, 3);
    b.insert(10, end_time_32);
    b.insert(16, start_time_32);
    b.insert(18, 44);

    let start_time_bytes = split_to_bytes(b[&16]);
    b.insert(20, start_time_bytes[0] as u32);
    b.insert(21, start_time_bytes[1] as u32);
    b.insert(22, start_time_bytes[2] as u32);
    b.insert(23, start_time_bytes[3] as u32);
    b.insert(24, ((start_time >> 32) & 255) as u32);
    b.insert(25, ((start_time >> 40) & 255) as u32);

    let arg0_bytes = split_to_bytes(arguments[0]);
    b.insert(26, arg0_bytes[0] as u32);
    b.insert(27, arg0_bytes[1] as u32);
    b.insert(28, arg0_bytes[2] as u32);
    b.insert(29, arg0_bytes[3] as u32);

    b.insert(30, ((arguments[1] / 256) & 255) as u32);
    b.insert(31, (arguments[1] % 256) as u32);

    let arg1_bytes = split_to_bytes(arguments[1]);
    b.insert(32, arg1_bytes[0] as u32);
    b.insert(33, arg1_bytes[1] as u32);

    let arg2_bytes = split_to_bytes(arguments[2]);
    b.insert(34, arg2_bytes[0] as u32);
    b.insert(35, arg2_bytes[1] as u32);
    b.insert(36, arg2_bytes[2] as u32);
    b.insert(37, arg2_bytes[3] as u32);

    b.insert(38, url_search_params_list[21] as u32);
    b.insert(39, url_search_params_list[22] as u32);
    b.insert(40, cus[21] as u32);
    b.insert(41, cus[22] as u32);
    b.insert(42, ua[23] as u32);
    b.insert(43, ua[24] as u32);

    let end_time_bytes = split_to_bytes(b[&10]);
    b.insert(44, end_time_bytes[0] as u32);
    b.insert(45, end_time_bytes[1] as u32);
    b.insert(46, end_time_bytes[2] as u32);
    b.insert(47, end_time_bytes[3] as u32);
    b.insert(48, b[&8]);
    b.insert(49, ((end_time >> 32) & 255) as u32);
    b.insert(50, ((end_time >> 40) & 255) as u32);

    b.insert(51, page_id & 255);

    let page_id_bytes = split_to_bytes(page_id);
    b.insert(52, page_id_bytes[0] as u32);
    b.insert(53, page_id_bytes[1] as u32);
    b.insert(54, page_id_bytes[2] as u32);
    b.insert(55, page_id_bytes[3] as u32);

    b.insert(56, aid);
    b.insert(57, aid & 255);
    b.insert(58, (aid >> 8) & 255);
    b.insert(59, (aid >> 16) & 255);
    b.insert(60, (aid >> 24) & 255);

    let window_env_list: Vec<u8> = window_env_str.chars().map(|c| c as u8).collect();
    let env_len = window_env_list.len() as u32;
    b.insert(64, env_len);
    b.insert(65, env_len & 255);
    b.insert(66, (env_len >> 8) & 255);

    b.insert(69, 0);
    b.insert(70, 0);
    b.insert(71, 0);

    let checksum = b[&18]
        ^ b[&20]
        ^ b[&26]
        ^ b[&30]
        ^ b[&38]
        ^ b[&40]
        ^ b[&42]
        ^ b[&21]
        ^ b[&27]
        ^ b[&31]
        ^ b[&35]
        ^ b[&39]
        ^ b[&41]
        ^ b[&43]
        ^ b[&22]
        ^ b[&28]
        ^ b[&32]
        ^ b[&36]
        ^ b[&23]
        ^ b[&29]
        ^ b[&33]
        ^ b[&37]
        ^ b[&44]
        ^ b[&45]
        ^ b[&46]
        ^ b[&47]
        ^ b[&48]
        ^ b[&49]
        ^ b[&50]
        ^ b[&24]
        ^ b[&25]
        ^ b[&52]
        ^ b[&53]
        ^ b[&54]
        ^ b[&55]
        ^ b[&57]
        ^ b[&58]
        ^ b[&59]
        ^ b[&60]
        ^ b[&65]
        ^ b[&66]
        ^ b[&70]
        ^ b[&71];
    b.insert(72, checksum);

    let mut bb: Vec<u8> = vec![
        b[&18] as u8,
        b[&20] as u8,
        b[&52] as u8,
        b[&26] as u8,
        b[&30] as u8,
        b[&34] as u8,
        b[&58] as u8,
        b[&38] as u8,
        b[&40] as u8,
        b[&53] as u8,
        b[&42] as u8,
        b[&21] as u8,
        b[&27] as u8,
        b[&54] as u8,
        b[&55] as u8,
        b[&31] as u8,
        b[&35] as u8,
        b[&57] as u8,
        b[&39] as u8,
        b[&41] as u8,
        b[&43] as u8,
        b[&22] as u8,
        b[&28] as u8,
        b[&32] as u8,
        b[&60] as u8,
        b[&36] as u8,
        b[&23] as u8,
        b[&29] as u8,
        b[&33] as u8,
        b[&37] as u8,
        b[&44] as u8,
        b[&45] as u8,
        b[&59] as u8,
        b[&46] as u8,
        b[&47] as u8,
        b[&48] as u8,
        b[&49] as u8,
        b[&50] as u8,
        b[&24] as u8,
        b[&25] as u8,
        b[&65] as u8,
        b[&66] as u8,
        b[&70] as u8,
        b[&71] as u8,
    ];
    bb.extend_from_slice(&window_env_list);
    bb.push(b[&72] as u8);

    let bb_str: String = bb.iter().map(|&byte| byte as char).collect();
    rc4_encrypt(&bb_str, "y")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abogus_matches_python_vector() {
        let query = "aid=6383&device_platform=webapp&channel=channel_pc_web&msToken=";
        let opts = AbogusOptions {
            method: "GET".to_string(),
            start_time: Some(1_700_000_000_000),
            end_time: Some(1_700_000_000_005),
            random_num_1: Some(1111.0),
            random_num_2: Some(2222.0),
            random_num_3: Some(3333.0),
            browser: None,
        };

        let result = generate_abogus(query, "", Some(opts));
        assert_eq!(
            result,
            "D7WhBQugdDDkkfyh56KLfY3q6VfVYmQI0SVkMD2fAPDOqL39HMYh9exoIBGvXY8jwG/-IeEjy4hbT3ohrQ2y0Hwf9W0L/25ksDSkKl5Q5xSSs1X9eghgJ04qmkt5SMx2RvB-rOXmqhZHKRbp09oHmhK4b1dzFgf3qJLz6j=="
        );
    }

    #[test]
    fn test_ab_sign_live() {
        let query = "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=313899056971&msToken=";
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

        let result = ab_sign_live(query, ua);
        assert!(!result.is_empty());
        assert!(result.ends_with('='));
    }
}
