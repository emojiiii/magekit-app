# magekit-xbogus

X-Bogus 签名生成库，使用 QuickJS 执行 JS 版本的签名算法，并附带 A-Bogus 的纯 Rust 实现。

## 主要 API

- `XBogus::new()` / `Default` 创建签名器
- `XBogus::sign(query, user_agent)` 生成 X-Bogus 字符串
- 便捷函数 `sign(query, user_agent)` 直接生成
- A-Bogus：`ab_sign(query, user_agent, seed)`
- 导出的内部模块（测试/高级用）：`ab_sign::base64_custom`、`rc4`、`sm3`

## 快速示例

```rust
use magekit_xbogus::sign;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sig = sign(
        "device_platform=webapp&aid=6383&channel=channel_pc_web",
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
    )?;
    println!("X-Bogus={sig}");
    Ok(())
}
```

## 优点

- 将 JS 逻辑封装在 QuickJS，行为与浏览器端一致。
- 同时提供 A-Bogus Rust 实现，方便抖音 Web API 场景。
- API 简单，可直接嵌入到请求构造流程。

## 局限 / 风险

- QuickJS 运行时初始化有开销，高并发场景需池化或重用实例。
- 目前仅暴露基础签名能力，缺少输入校验与性能指标。
- 依赖 JS 源码文件，升级时需同步校验正确性。

## 改进建议

- 提供 `XBogusPool` 以复用运行时并降低创建开销。
- 增加基准测试与 profiling，评估签名吞吐与延迟。
- 为签名输入添加参数校验/归一化，避免非法输入导致 panic 或空签名。
- 允许注入自定义日志/trace，便于调试线上问题。

