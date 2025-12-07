//! RC4 加密算法实现

/// RC4 加密
pub fn rc4_encrypt(plaintext: &str, key: &str) -> String {
    // 初始化状态数组
    let mut s: Vec<u8> = (0..=255).collect();

    // 使用密钥对状态数组进行置换
    let key_bytes = key.as_bytes();
    let mut j: usize = 0;

    for i in 0..256 {
        j = (j + s[i] as usize + key_bytes[i % key_bytes.len()] as usize) % 256;
        s.swap(i, j);
    }

    // 生成密钥流并加密
    let mut i: usize = 0;
    let mut j: usize = 0;
    let mut result = String::new();

    for ch in plaintext.chars() {
        i = (i + 1) % 256;
        j = (j + s[i] as usize) % 256;
        s.swap(i, j);

        let t = (s[i] as usize + s[j] as usize) % 256;
        let keystream_byte = s[t];

        // XOR 操作
        let encrypted_byte = (ch as u8) ^ keystream_byte;
        result.push(encrypted_byte as char);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rc4_encrypt() {
        let plaintext = "Hello, World!";
        let key = "secret";
        let encrypted = rc4_encrypt(plaintext, key);

        // RC4 是对称加密，再次加密应该得到原文
        let decrypted = rc4_encrypt(&encrypted, key);
        assert_eq!(decrypted, plaintext);
    }
}
