# CLAUDE - xbogus
Breadcrumb: Home / xbogus

## 角色
生成 X-Bogus 签名（抖音/快手等接口常用），基于 QuickJS 执行 JS 代码，并提供 Rust AB-Sign 备用实现。

## 关键文件
- `src/lib.rs`：`XBogus` 结构与错误类型，加载内置 `x-bogus.js`，暴露 `XBogus::sign(query, ua)` 与便捷函数 `sign(...)`；导出 `ab_sign`/`base64_custom`/`rc4`/`sm3`。
- `src/ab_sign/*`：AB-Sign Rust 实现（base64/RC4/SM3）。
- `x-bogus.js`：JS 版签名算法（未细扫，运行时通过 QuickJS eval）。

## 依赖与运行
- 使用 `rquickjs`（`full-async`/`parallel`/`macro`）创建 Runtime/Context 并执行 JS；Tokio 仅用于依赖。
- API：`XBogus::new()` 初始化 JS 环境后调用 `sign` 函数；错误分层（运行时/上下文/JS 执行/函数调用）。
- 测试：`tests` 覆盖构造与基本签名生成，确保非空结果。

## 推荐下一步
- 审阅并记录 `x-bogus.js` 算法与参数要求（UA/Query），补充示例输入输出。
- 若在 GUI/服务侧调用，注意线程安全：当前实例持有 `Context`，可按请求创建或加池化。

