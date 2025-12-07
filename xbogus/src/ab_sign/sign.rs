//! AB-Sign 主签名函数

use super::rc4::rc4_encrypt;
use super::sm3::SM3;
use super::base64_custom::result_encrypt;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;

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
    // 使用固定的随机值（与 Python 版本一致）
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

    // 三次加密处理
    // 1: url_search_params 两次 sm3 的结果
    let combined = format!("{}{}", url_search_params, suffix);
    let url_params_hash1 = sm3.sum(Some(combined.as_bytes()));
    let url_search_params_list = sm3.sum(Some(&url_params_hash1));

    // 2: 对后缀两次 sm3 的结果
    let suffix_hash1 = sm3.sum(Some(suffix.as_bytes()));
    let cus = sm3.sum(Some(&suffix_hash1));

    // 3: 对 ua 处理之后的结果
    let ua_key = format!("{}{}{}", '\u{0}', '\u{1}', '\u{e}'); // [0, 1, 14]
    let encrypted_ua = rc4_encrypt(user_agent, &ua_key);
    let encoded_ua = result_encrypt(&encrypted_ua, "s3");
    let ua = sm3.sum(Some(encoded_ua.as_bytes()));

    let end_time = start_time + 100;
    let start_time_32 = (start_time & 0xFFFFFFFF) as u32;
    let end_time_32 = (end_time & 0xFFFFFFFF) as u32;

    // 配置参数
    let aid: u32 = 6383;
    let page_id: u32 = 110624;

    // 使用 HashMap 模拟 Python 的字典
    let mut b: HashMap<usize, u32> = HashMap::new();

    b.insert(8, 3);
    b.insert(10, end_time_32);
    b.insert(16, start_time_32);
    b.insert(18, 44);

    // 处理时间戳
    let start_time_bytes = split_to_bytes(b[&16]);
    b.insert(20, start_time_bytes[0] as u32);
    b.insert(21, start_time_bytes[1] as u32);
    b.insert(22, start_time_bytes[2] as u32);
    b.insert(23, start_time_bytes[3] as u32);
    b.insert(24, ((start_time >> 32) & 255) as u32);
    b.insert(25, ((start_time >> 40) & 255) as u32);

    // 处理 Arguments 参数
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

    // 处理加密结果
    b.insert(38, url_search_params_list[21] as u32);
    b.insert(39, url_search_params_list[22] as u32);
    b.insert(40, cus[21] as u32);
    b.insert(41, cus[22] as u32);
    b.insert(42, ua[23] as u32);
    b.insert(43, ua[24] as u32);

    // 处理结束时间
    let end_time_bytes = split_to_bytes(b[&10]);
    b.insert(44, end_time_bytes[0] as u32);
    b.insert(45, end_time_bytes[1] as u32);
    b.insert(46, end_time_bytes[2] as u32);
    b.insert(47, end_time_bytes[3] as u32);
    b.insert(48, b[&8]);
    b.insert(49, ((end_time >> 32) & 255) as u32);
    b.insert(50, ((end_time >> 40) & 255) as u32);

    // 处理配置项
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

    // 处理环境信息
    let window_env_list: Vec<u8> = window_env_str.chars().map(|c| c as u8).collect();
    let env_len = window_env_list.len() as u32;
    b.insert(64, env_len);
    b.insert(65, env_len & 255);
    b.insert(66, (env_len >> 8) & 255);

    b.insert(69, 0);
    b.insert(70, 0);
    b.insert(71, 0);

    // 计算校验和
    let checksum = b[&18] ^ b[&20] ^ b[&26] ^ b[&30] ^ b[&38] ^ b[&40] ^ b[&42] ^ b[&21] ^
        b[&27] ^ b[&31] ^ b[&35] ^ b[&39] ^ b[&41] ^ b[&43] ^ b[&22] ^ b[&28] ^
        b[&32] ^ b[&36] ^ b[&23] ^ b[&29] ^ b[&33] ^ b[&37] ^ b[&44] ^ b[&45] ^
        b[&46] ^ b[&47] ^ b[&48] ^ b[&49] ^ b[&50] ^ b[&24] ^ b[&25] ^ b[&52] ^
        b[&53] ^ b[&54] ^ b[&55] ^ b[&57] ^ b[&58] ^ b[&59] ^ b[&60] ^ b[&65] ^
        b[&66] ^ b[&70] ^ b[&71];
    b.insert(72, checksum);

    // 构建最终字节数组 - 按照 Python 版本的顺序
    let mut bb: Vec<u8> = vec![
        b[&18] as u8, b[&20] as u8, b[&52] as u8, b[&26] as u8, b[&30] as u8, b[&34] as u8,
        b[&58] as u8, b[&38] as u8, b[&40] as u8, b[&53] as u8, b[&42] as u8, b[&21] as u8,
        b[&27] as u8, b[&54] as u8, b[&55] as u8, b[&31] as u8, b[&35] as u8, b[&57] as u8,
        b[&39] as u8, b[&41] as u8, b[&43] as u8, b[&22] as u8, b[&28] as u8, b[&32] as u8,
        b[&60] as u8, b[&36] as u8, b[&23] as u8, b[&29] as u8, b[&33] as u8, b[&37] as u8,
        b[&44] as u8, b[&45] as u8, b[&59] as u8, b[&46] as u8, b[&47] as u8, b[&48] as u8,
        b[&49] as u8, b[&50] as u8, b[&24] as u8, b[&25] as u8, b[&65] as u8, b[&66] as u8,
        b[&70] as u8, b[&71] as u8,
    ];
    bb.extend_from_slice(&window_env_list);
    bb.push(b[&72] as u8);

    // RC4 加密，key 是 'y' (chr(121))
    let bb_str: String = bb.iter().map(|&byte| byte as char).collect();
    rc4_encrypt(&bb_str, "y")
}

/// AB-Sign 主函数
pub fn ab_sign(url_search_params: &str, user_agent: &str) -> String {
    let window_env_str = "1920|1080|1920|1040|0|30|0|0|1872|92|1920|1040|1857|92|1|24|Win32";
    let suffix = "cus";
    let arguments = [0u32, 1, 14];

    // 1. 生成随机字符串前缀
    // 2. 生成 RC4 加密的主体部分
    // 3. 对结果进行最终加密并添加等号后缀
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ab_sign() {
        let query = "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=313899056971&msToken=";
        let ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";
        
        let result = ab_sign(query, ua);
        println!("AB-Sign result: {}", result);
        println!("Result length: {}", result.len());
        
        assert!(!result.is_empty());
        assert!(result.ends_with('='));
    }

    #[test]
    fn test_intermediate_values() {
        let query = "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=313899056971&msToken=";
        let ua = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/116.0.5845.97 Safari/537.36 Core/1.116.567.400 QQBrowser/19.7.6764.400";
        let suffix = "cus";
        
        let mut sm3 = SM3::new();
        
        // 1. URL params + suffix
        let combined = format!("{}{}", query, suffix);
        let url_params_hash1 = sm3.sum(Some(combined.as_bytes()));
        println!("1. url_params_hash1 (first 8): {:?}", &url_params_hash1[..8]);
        
        let url_search_params_list = sm3.sum(Some(&url_params_hash1));
        println!("2. url_search_params_list (first 8): {:?}", &url_search_params_list[..8]);
        println!("   Index 21, 22: {}, {}", url_search_params_list[21], url_search_params_list[22]);
        
        // 2. suffix SM3
        let suffix_hash1 = sm3.sum(Some(suffix.as_bytes()));
        println!("\n3. suffix_hash1 (first 8): {:?}", &suffix_hash1[..8]);
        
        let cus = sm3.sum(Some(&suffix_hash1));
        println!("4. cus (first 8): {:?}", &cus[..8]);
        println!("   Index 21, 22: {}, {}", cus[21], cus[22]);
        
        // 3. UA processing
        let ua_key = format!("{}{}{}", '\u{0}', '\u{1}', '\u{e}');
        let encrypted_ua = rc4_encrypt(ua, &ua_key);
        println!("\n5. encrypted_ua length: {}", encrypted_ua.chars().count());
        let ua_bytes: Vec<u8> = encrypted_ua.chars().take(10).map(|c| c as u8).collect();
        println!("   encrypted_ua bytes (first 10): {:?}", ua_bytes);
        
        let encoded_ua = result_encrypt(&encrypted_ua, "s3");
        println!("6. encoded_ua: {}...", &encoded_ua[..50.min(encoded_ua.len())]);
        
        let ua_hash = sm3.sum(Some(encoded_ua.as_bytes()));
        println!("7. ua_hash (first 8): {:?}", &ua_hash[..8]);
        println!("   Index 23, 24: {}, {}", ua_hash[23], ua_hash[24]);
        
        // 验证与 Python 的值匹配
        assert_eq!(&url_params_hash1[..8], &[75, 86, 209, 161, 203, 85, 160, 130]);
        assert_eq!(&url_search_params_list[..8], &[153, 33, 102, 41, 195, 105, 130, 182]);
        assert_eq!(url_search_params_list[21], 199);
        assert_eq!(url_search_params_list[22], 124);
    }

    #[test]
    fn test_generate_random_str() {
        let s1 = generate_random_str();
        let s2 = generate_random_str();
        // 由于使用固定随机值，应该相同
        assert_eq!(s1, s2);
        assert_eq!(s1.len(), 12);
    }
}
