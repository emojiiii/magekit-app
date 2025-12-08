/// 测试 ab_sign 生成的签名
///
/// 用于验证我们的 Rust 实现是否与 Python 版本一致
use xbogus::ab_sign;

fn main() {
    let query = "aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=116.0.0.0&web_rid=313899056971&msToken=";
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

    println!("🔑 测试 ab_sign:");
    println!("  query: {}", query);
    println!("  user_agent: {}", user_agent);

    println!("\n📞 调用 ab_sign...");
    let signature = ab_sign(query, user_agent);
    println!("✅ ab_sign 返回");

    println!("\n✅ 生成的签名:");
    println!("  {}", signature);
    println!("  长度: {} 字符", signature.len());

    println!("\n📝 Python 预期签名 (作为参考):");
    println!(
        "  E7mhBmg6mEVNgf6X51/LfY3q6lB3Y68s0HViMD2fdVfoBy39HMYd9exoRdhvGC6jiT/QIeYjy4hbO3xprQAjM36UHWwEUdQ2mgWkKl5Q5I0j53iruyRDntmF4vj3SFlm5XNAEOk="
    );
    println!("  长度: 133 字符");

    // 打印前20个字符进行快速比较
    println!("\n🔍 前20个字符对比:");
    let rust_prefix: String = signature.chars().take(20).collect();
    let py_prefix = "E7mhBmg6mEVNgf6X51/L";
    println!("  Rust: {}", rust_prefix);
    println!("  Python: {}", py_prefix);

    if signature.starts_with("E7mhBmg6mEVNgf6X51/L") {
        println!("\n✅ 前缀匹配！签名可能正确！");
    } else {
        println!("\n❌ 前缀不匹配！需要调试签名生成算法");
    }
}
