# GPUI Fast / GPUI Kit migration

## Dependency identity and reproducibility

MageKit uses [GPUI Fast](https://github.com/longbridge/gpui-fast) and the
[GPUI Kit facade](https://github.com/longbridge/gpui-kit). GPUI Fast is an
experimental retained-mode fork; this is not a stable GPUI release upgrade.

GPUI Kit's default branch currently uses `gpui-pre` snapshots, not GPUI Fast.
This application therefore pins Kit's upstream `gpui-fast-main` integration
commit `0067bafcf3160cb871eff2521e98a51e526da175`. All GPUI Fast crates use the
same Git source, with commit `10d005166a9f9e079d55ebfb700035f3860c3e71`
recorded in `Cargo.lock`. The crates.io GPUI patch also routes `gpui-router`
through this source, avoiding two incompatible sets of GPUI types. A small
vendored router compatibility patch disambiguates `RenderOnce::render` from
GPUI Fast’s blanket `View::render`; see `vendor/gpui-router/PATCHES.md`.

Build and test with `--locked`. Update the Kit integration revision and GPUI
Fast lockfile entries together, then check all supported desktop targets.
A future Kit release may replace this temporary upstream integration branch.

The application initializes its platform with `gpui_kit::application()` and
uses Kit's component and asset modules. Kit's Root owns dialog/notification
layers. Theme updates must use `Theme::update` to synchronize the unstyled
base layer's theme tokens as well as styled components.

## Language

Settings offers System, English, and Chinese. New configurations default to
System; an explicit choice is saved in the existing `ui.language` field.
Chinese and English regional/script locale variants resolve to the supported
language. Missing or unsupported locales fall back to English. Choosing a
language refreshes visible text without recreating pages or active tasks.
Platform identifiers, paths, format IDs, user text, and external-tool output
are not translated.

## Validation

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test -p magekit-shared --locked
cargo test --bin magekit i18n --locked
```

Desktop smoke checks should cover:

- First launch with Chinese, English, unsupported, and absent system locale
- Switching System → English → Chinese → System; restart after each explicit choice
- Switching with text entered in Home, Channel, Capture, and Settings inputs
- Navigate between cached pages after switching; verify status labels and placeholders
- Open and dismiss dialogs/menus; verify translated component controls and no duplicate layers
- Toggle themes repeatedly and verify both inputs and surrounding components update
- Continue active downloads and recordings while changing language
- Windows/macOS/Linux titlebar dragging, maximize, minimize, and close
