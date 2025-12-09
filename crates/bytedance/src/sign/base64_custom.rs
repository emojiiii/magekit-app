//! 自定义 Base64 编码实现

const ENCODING_TABLES: [&str; 5] = [
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=",
    "Dkdpgh4ZKsQB80/Mfvw36XI1R25+WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=",
    "Dkdpgh4ZKsQB80/Mfvw36XI1R25-WUAlEi7NLboqYTOPuzmFjJnryx9HVGcaStCe=",
    "ckdp1h4ZKsUB80/Mfvw36XIgR25+WQAlEi7NLboqYTOPuzmFjJnryx9HVGDaStCe",
    "Dkdpgh2ZmsQB80/MfvV36XI1R45-WUAlEixNLwoqYTOPuzKFjJnry79HbGcaStCe",
];

const MASKS: [u32; 4] = [16515072, 258048, 4032, 63];
const SHIFTS: [u32; 4] = [18, 12, 6, 0];

/// 使用自定义编码表进行 Base64 编码
/// 注意：输入是一个"伪字符串"，每个字符实际上是一个字节值 (0-255)
pub fn result_encrypt(long_str: &str, table_name: &str) -> String {
    let table_index = match table_name {
        "s0" => 0,
        "s1" => 1,
        "s2" => 2,
        "s3" => 3,
        "s4" => 4,
        _ => 0,
    };

    let encoding_table = ENCODING_TABLES[table_index];
    let table_chars: Vec<char> = encoding_table.chars().collect();

    // 收集所有字符（每个字符代表一个字节值）
    let chars: Vec<char> = long_str.chars().collect();
    let char_count = chars.len();

    let mut result = String::new();
    let mut round_num = 0;
    let mut long_int = get_long_int_from_chars(round_num, &chars);

    let total_chars = ((char_count as f64 / 3.0) * 4.0).ceil() as usize;

    for i in 0..total_chars {
        if i / 4 != round_num {
            round_num += 1;
            long_int = get_long_int_from_chars(round_num, &chars);
        }

        let index = i % 4;
        let char_index = ((long_int & MASKS[index]) >> SHIFTS[index]) as usize;

        result.push(table_chars[char_index]);
    }

    // 添加 padding
    let padding = (4 - result.len() % 4) % 4;
    for _ in 0..padding {
        result.push('=');
    }

    result
}

/// 从字符数组中获取 long int（用于字符串输入）
fn get_long_int_from_chars(round_num: usize, chars: &[char]) -> u32 {
    let round_num = round_num * 3;

    // 获取字符的 Unicode 码点作为字节值
    let char1 = if round_num < chars.len() {
        chars[round_num] as u32
    } else {
        0
    };
    let char2 = if round_num + 1 < chars.len() {
        chars[round_num + 1] as u32
    } else {
        0
    };
    let char3 = if round_num + 2 < chars.len() {
        chars[round_num + 2] as u32
    } else {
        0
    };

    (char1 << 16) | (char2 << 8) | char3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_result_encrypt() {
        let input = "test";
        let result = result_encrypt(input, "s4");
        assert!(!result.is_empty());
    }
}
