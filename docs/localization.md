# 界面语言 / Interface language

设置 → 语言提供「跟随系统 / System」「中文」「English」。新配置和缺少语言字段的旧配置默认 `system`。明确选择会保存到 `config.toml` 的 `ui.language`（`system`、`zh` 或 `en`），重启后保持该选择。

- 系统语言由 `sys-locale` 读取。`zh` 的地区/文字变体（例如 `zh-CN`、`zh-TW`、`zh_Hant_TW.UTF-8`）使用中文
- `en` 变体使用英文。缺失、空或不支持的系统语言均回退到英文
- 单个翻译条目缺失时回退到英文目录；两份目录都有覆盖检查
- 切换语言不重建页面或下载/录制任务。输入框的提示原地更新，保留输入、光标和撤销记录；平台选择保留原值
- 视频/频道标题、用户输入、URL、Cookie、工具参数、主题注册键和录制目录名不翻译。内部日志和第三方返回的原始错误保留原文

## 维护翻译

`locales/en.json` / `locales/zh.json` 是翻译源。中文源文案作为大多数键；英文起源的界面文案保留英文键。格式参数（`{}`、`{:.1}`、`{name}` 等）必须保持顺序和格式不变。

```sh
python3 scripts/check_i18n.py --write
python3 scripts/check_i18n.py
```

生成的 `src/i18n/catalog.rs` 被静态编译进应用，不依赖运行目录、网络或运行时文件。不要手动编辑它。

- 渲染时静态文案使用 `i18n::tr`
- 动态文案使用 `i18n::format`，参数先交给 Rust 按原格式格式化
- 需要缓存并随语言切换的运行时提示使用 `i18n::Message` 保存键和参数，在渲染时调用 `render`
- `i18n::text` 只适合应用自己生成的已知静态文案，不用于外部标题或用户内容
- 带提示的持久输入框使用 `i18n::input`，避免切换时重建输入状态

## 验证

不依赖 GUI 或网络的核心测试：

```sh
rustc --edition=2024 --test src/i18n/core.rs -o /tmp/magekit-i18n-tests
/tmp/magekit-i18n-tests
```

集成测试：

```sh
cargo test -p magekit-shared language_tests --locked
cargo test --bin magekit i18n --locked
cargo test --bin magekit format_label_tests --locked
```

手动验收：

1. 使用中文系统语言和新配置启动，确认首页、侧栏、任务、频道、嗅探、录制、工具、设置及对话框显示中文
2. 在各页面输入未提交的文本、选择格式/平台，然后切换 English，确认文案和输入提示更新，输入、选项、活动任务不变
3. 重启确认 English 仍生效；选择 System 并确认配置保存的是 `system`
4. 用英语和不支持的系统语言重复首次启动，确认英文回退
5. 保持录制/下载运行期间切换两次语言，确认任务 ID、进度、录制文件路径不变；检查中文和英文长文案布局
