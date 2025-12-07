/// 测试 SM3 哈希算法
/// 
/// 对比 Rust 实现与 Python 版本的输出
use xbogus::ab_sign::sm3::SM3;

fn main() {
    println!("🔍 测试 SM3 哈希算法\n");
    
    // 测试1: 标准测试向量 "abc"
    let mut sm3 = SM3::new();
    let result = sm3.sum(Some(b"abc"));
    let hex: String = result.iter().map(|b| format!("{:02x}", b)).collect();
    println!("测试1: \"abc\"");
    println!("  结果: {}", hex);
    println!("  预期: 66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0");
    println!("  匹配: {}\n", hex == "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0");
    
    // 测试2: URL参数 + suffix (从 Python ab_sign 提取)
    let url_params = "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=313899056971&msToken=";
    let suffix = "cus";
    let combined = format!("{}{}", url_params, suffix);
    
    let mut sm3 = SM3::new();
    let hash1 = sm3.sum(Some(combined.as_bytes()));
    let hex1: String = hash1.iter().map(|b| format!("{:02x}", b)).collect();
    println!("测试2: url_params + suffix (第一次哈希)");
    println!("  输入长度: {} bytes", combined.len());
    println!("  结果: {}", hex1);
    println!("  (需与 Python 对比)\n");
    
    // 测试3: 对哈希结果再次哈希
    let mut sm3 = SM3::new();
    let hash2 = sm3.sum(Some(&hash1));
    let hex2: String = hash2.iter().map(|b| format!("{:02x}", b)).collect();
    println!("测试3: hash(hash1) (第二次哈希)");
    println!("  结果: {}", hex2);
    println!("  (需与 Python 对比)\n");
    
    // 测试4: suffix 单独哈希
    let mut sm3 = SM3::new();
    let suffix_hash1 = sm3.sum(Some(suffix.as_bytes()));
    let hex3: String = suffix_hash1.iter().map(|b| format!("{:02x}", b)).collect();
    println!("测试4: hash(\"cus\")");
    println!("  结果: {}", hex3);
    
    let mut sm3 = SM3::new();
    let suffix_hash2 = sm3.sum(Some(&suffix_hash1));
    let hex4: String = suffix_hash2.iter().map(|b| format!("{:02x}", b)).collect();
    println!("  hash(hash(\"cus\")): {}", hex4);
    println!("  (需与 Python 对比)\n");
    
    println!("📝 请在 Python 中运行以下代码对比:");
    println!("  from ab_sign import SM3");
    println!("  sm3 = SM3()");
    println!("  print(sm3.sum('{}cus'.encode()).hex())", url_params);
    println!("  # 然后对比上面的结果");
}
