use rand::Rng;
use std::time::{SystemTime, UNIX_EPOCH};
use xbogus::sm3::SM3;

const UA_CODE: [u32; 32] = [
    76, 98, 15, 131, 97, 245, 224, 133, 122, 199, 241, 166, 79, 34, 90, 191, 128, 126, 122, 98,
    66, 11, 14, 40, 49, 110, 110, 173, 67, 96, 138, 252,
];

const END_STRING: &str = "cus";
const DEFAULT_BROWSER: &str = "1536|742|1536|864|0|0|0|0|1536|864|1536|864|1536|742|24|24|MacIntel";

const S0: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=";
const S1: &str = "Dkdpgh4ZKsQB80/Mfvw36XI1R25+WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=";
const S2: &str = "Dkdpgh4ZKsQB80/Mfvw36XI1R25-WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=";
const S3: &str = "ckdp1h4ZKsUB80/Mfvw36XIgR25+WQAlEi7NLboqYTOPuzmFjJnryx9HVGDaStCe";
const S4: &str = "Dkdpgh2ZmsQB80/MfvV36XI1R45-WUAlEixNLwoqYTOPuzKFjJnry79HbGcaStCe";

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

/// 生成 A-Bogus 签名（移植自 crawlers/douyin/web/abogus.py）
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
        let string_2 =
            self.generate_string_2(url_params, &opts.method, opts.start_time, opts.end_time, &mut rng);

        generate_result(&(string_1 + &string_2), "s4")
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
            (end_time / 4294967296) as u32,   // 256^4
            (start_time / 4294967296) as u32, // 256^4
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

fn rc4_encrypt(plaintext: &str, key: &str) -> String {
    let mut s: Vec<u32> = (0..256).collect();
    let key_bytes: Vec<u32> = key.chars().map(|c| c as u32).collect();

    let mut j = 0usize;
    for i in 0..256 {
        j = (j + s[i] as usize + key_bytes[i % key_bytes.len()] as usize) % 256;
        s.swap(i, j);
    }

    let mut i = 0usize;
    j = 0usize;
    let mut cipher = String::new();

    for ch in plaintext.chars() {
        i = (i + 1) % 256;
        j = (j + s[i] as usize) % 256;
        s.swap(i, j);
        let t = (s[i] + s[j]) % 256;
        let val = s[t as usize] ^ (ch as u32);
        if let Some(c) = char::from_u32(val) {
            cipher.push(c);
        }
    }

    cipher
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

fn pick_charset(key: &str) -> &'static str {
    match key {
        "s0" => S0,
        "s1" => S1,
        "s2" => S2,
        "s3" => S3,
        _ => S4,
    }
}

fn generate_result(s: &str, key: &str) -> String {
    let charset = pick_charset(key);
    let mut r = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    let mut idx = 0;

    while idx < len {
        let c0 = chars[idx] as u32;
        let c1 = if idx + 1 < len { chars[idx + 1] as u32 } else { 0 };
        let c2 = if idx + 2 < len { chars[idx + 2] as u32 } else { 0 };

        let n = if idx + 2 < len {
            (c0 << 16) | (c1 << 8) | c2
        } else if idx + 1 < len {
            (c0 << 16) | (c1 << 8)
        } else {
            c0 << 16
        };

        for (shift, mask) in [
            (18, 0xFC0000),
            (12, 0x03F000),
            (6, 0x0FC0),
            (0, 0x3F),
        ] {
            if (shift == 6 && idx + 1 >= len) || (shift == 0 && idx + 2 >= len) {
                break;
            }
            let pos = ((n & mask) >> shift) as usize;
            if let Some(ch) = charset.chars().nth(pos) {
                r.push(ch);
            }
        }

        idx += 3;
    }

    let padding = (4 - r.len() % 4) % 4;
    for _ in 0..padding {
        r.push('=');
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abogus_matches_python_vector() {
        let query =
            "aid=6383&device_platform=webapp&channel=channel_pc_web&msToken=";
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
}

