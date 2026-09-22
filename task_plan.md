# Streamlink-only 录制切换

## 目标

增加一个可配置的“仅使用 Streamlink”模式。开启时隐藏/禁用旧的原生录制入口，并让录制流程只走 Streamlink，方便用户测试；默认保留现有兼容行为，避免影响已有配置。

## 阶段

- [complete] 梳理配置、设置页、录制页和录制调度链路
- [complete] 增加配置字段并接入持久化/设置 UI
- [complete] 在入口和调度层应用 Streamlink-only 行为
- [complete] 编译、测试、检查 warning 并记录结果

## 本轮修复（SOOP 录制与首次刷新）

- [complete] 复现并定位 Streamlink worker 退出和首次状态刷新问题
- [complete] 修复 worker 错误传播、进度状态和启动检查时序
- [complete] 针对 SOOP 做 URL/请求参数兼容处理并验证
- [complete] 编译、测试并检查 warning

## 本轮修复（启动闪退与失效封面）

- [complete] 定位图片 403/404 与 Taffy 布局 panic 的触发路径
- [complete] 保留启动时的历史封面缓存，并为远程图片增加失败占位
- [complete] 刷新成功后用新封面替换旧缓存，完成编译、测试和运行回归

## 本轮修正（保留历史封面过渡）

- [complete] 恢复启动时的历史封面显示
- [complete] 仅在拿到新封面时替换缓存，失败时由 fallback 回到占位图
- [complete] 完成回归并确认无 panic

## 决策

- 优先做可切换开关，而不是删除原生录制代码；这样本次测试可以只启用 Streamlink，后续仍可恢复。
- 用户当前明确要测试 Streamlink，因此开关默认开启；旧配置中缺少该字段时也默认开启，确保本次启动不会意外回退到原生录制。

## 错误记录

| 错误 | 尝试 | 处理 |
| --- | --- | --- |
| 录制页链式 GPUI 表达式语法错误 | 首次插入引擎标签后运行 `cargo fmt` | 将标签放回同一个链式 `div()` 表达式后通过格式化 |
| `IconName::Video` 不存在 | 编译检查发现枚举没有该成员 | 改用已存在的 `IconName::Globe` |
| SOOP 录制失败原因不可见 | worker 将 pull/ffmpeg stderr 丢弃，UI 将进度通道关闭当作完成 | 已保留安全错误摘要，并让 UI 等待最终状态后标记失败 |
