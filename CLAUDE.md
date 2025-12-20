# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

Last updated: 2025-12-20

## Project Overview

MageKit is a cross-platform video downloader desktop application built with Rust and GPUI. It provides concurrent downloads, task queuing, tool management (yt-dlp/ffmpeg), theme customization, and live recording capabilities.

**Tech Stack**: Rust 2024, GPUI (GPU-accelerated UI), Tokio + smol (async runtime), yt-dlp, ffmpeg

**Binary Entry**: `src/main.rs` (package name: `magekit-app`, binary name: `magekit`)

## Common Commands

### Building and Running

```bash
# Development run (recommended for iteration)
cargo run --bin magekit

# Release build (optimized)
cargo build --release --bin magekit
# Binary location: target/release/magekit.exe (Windows) or target/release/magekit (Linux/macOS)

# Check code without building
cargo check

# Build specific crate
cargo build -p magekit-tool-manager
cargo build -p magekit-shared
```

### Testing

```bash
# Run all tests
cargo test

# Run tests for specific crate
cargo test -p magekit-download
cargo test -p magekit-tool-manager

# Run single test by name
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run integration tests only
cargo test --test integration
```

### Code Quality

```bash
# Format code
cargo fmt

# Format check (CI)
cargo fmt --all -- --check

# Lint with Clippy
cargo clippy

# Clippy as error (CI)
cargo clippy -- -D warnings
```

### Debugging

```bash
# Enable verbose logging
RUST_LOG=debug cargo run

# Set specific module logging
RUST_LOG=magekit_tool_manager=debug cargo run
```

## Architecture

### Workspace Structure

```
magekit-app/                    # Root workspace & GUI application
├── src/                        # GPUI desktop app (main binary)
│   ├── app/                   # Application state and core logic
│   ├── ui/                    # UI components and pages
│   ├── theme/                 # Theme configuration
│   └── main.rs                # Entry point
├── crates/
│   ├── shared/                # Shared types, config, utilities
│   ├── tool_manager/          # Tool management & download scheduling
│   ├── download/              # Unified download abstraction (direct/yt-dlp/ffmpeg/HLS)
│   ├── capture/               # Web resource capture via CDP
│   ├── extractor/             # Custom extractors + yt-dlp bridge
│   ├── bytedance/             # Douyin/TikTok API & signing
│   ├── live_recorder/         # Live streaming recording
│   └── xbogus/                # X-Bogus/AB-Sign signature
└── themes/                    # Theme files (.toml)
```

### Module Dependencies

```
magekit-app (GUI)
├─→ shared (types, config, utils)
├─→ tool_manager (task scheduling)
│   ├─→ shared
│   ├─→ download (new unified download layer)
│   └─→ extractor
│       ├─→ shared
│       └─→ bytedance
├─→ capture (web resource capture)
│   └─→ shared
└─→ live_recorder
    ├─→ shared
    └─→ xbogus
```

### Key Flow: Application Startup

1. `main.rs` initializes:
   - Tracing logger
   - GPUI app with assets and HTTP client
   - Theme hot-reloading from `themes/` directory
   - Router system
2. `AppState::new_sync()` creates:
   - Tokio runtime
   - `ToolManager` with tool detection
   - Config loading
   - Task restoration from persistence
3. Sets `GlobalAppState` for route access
4. Opens `MainWindow` with custom titlebar (frameless window)

### Key Flow: Download Task

1. **GUI Event** → `AppState` → `ToolManager`
2. **Tool Manager**:
   - Generates `TaskId`, creates `TaskStatus`
   - Queries video info (with platform-specific cookies if available)
   - Spawns download task via `download` crate
   - Progress updates via mpsc channel
3. **Download Crate** (`crates/download/`):
   - Dispatcher selects strategy: `Direct | YtDlp | Ffmpeg | HlsDash`
   - Executes with callbacks for progress/completion/error
   - Supports cancellation via `CancellationToken`
4. **Task Queue**:
   - Controls concurrency limits
   - Priority-based scheduling
   - Supports pause/resume/cancel operations
5. **Persistence**:
   - Auto-saves task state to disk
   - Restores on application restart

### Key Modules

#### `crates/shared/` - Shared Library

**Purpose**: Common types, configuration, and utilities used across all crates.

**Key Files**:
- `src/types/` - Domain types (download, task, config, platform, event)
- `src/utils.rs` - Path helpers, formatting, validation, tool resolution
- `src/constants.rs` - Default values and constants

**Important Functions**:
- `get_app_config_dir()`, `get_app_data_dir()`, `get_tools_dir()` - Platform-aware directory resolution
- `load_app_config_or_default()` - Config loading with fallback
- `sanitize_filename()`, `generate_output_path()` - Safe file naming
- `create_tokio_command()` - Windows-compatible command spawning (no console window)

#### `crates/tool_manager/` - Tool & Task Management

**Purpose**: Manages yt-dlp/ffmpeg lifecycle and download task scheduling.

**Key Files**:
- `src/task_manager.rs` - Core `ToolManager` with task orchestration
- `src/download_adapter.rs` - Bridges to new `download` crate
- `src/downloader.rs` - Legacy yt-dlp/ffmpeg wrappers (being migrated)
- `src/config.rs` - Tool configuration (`tool_manager.toml`)
- `src/task_queue.rs` - Concurrent task queue with priorities
- `src/task_persistence.rs` - Task state persistence
- `src/storage.rs` - Tool binary storage
- `src/updater.rs` - Tool download and update

**Main API**:
- `new_sync()` - Synchronous initialization
- `ensure_tools()`, `check_for_updates()` - Tool management
- `get_video_info()`, `start_download()` - Download operations
- `pause_download()`, `resume_download()`, `cancel_download()` - Task control
- `enqueue_download()`, `set_task_priority()` - Queue management
- `subscribe()` - Event stream (`ToolManagerEvent`)

#### `crates/download/` - Unified Download Layer

**Purpose**: Pluggable download system supporting multiple strategies (direct HTTP, yt-dlp, ffmpeg, HLS/DASH).

**Architecture**:
- `DownloadClient` - Main dispatcher
- `Downloader` trait - Pluggable downloader interface
- Built-in implementations: `DirectDownloader`, `YtDlpDownloader`, `FfmpegDownloader`, `HlsDashDownloader`
- Strategy selection: `Auto | Direct | YtDlp | Ffmpeg | HlsDash | Custom(String)`
- Callback model: Sync callbacks send to channels for async UI updates
- Cancellation via `CancellationToken`
- Retry policy with configurable backoff

**Key Types**:
- `DownloadRequest` - URL, strategy, output, headers, cookies, timeouts, retries
- `DownloadEvent` - Progress | Complete | Error | Log
- `DownloadOutcome` - Result with output path and metadata

#### `crates/capture/` - Web Resource Capture

**Purpose**: Captures video/audio/image resources from web pages using Chrome DevTools Protocol (CDP).

**Features**:
- Static HTML scanning + live CDP network monitoring
- Resource filtering by type/size/pattern
- Ad detection and auto-speedup
- Cloudflare bypass mechanisms (headless detection evasion)

**Important**: Capture sessions run continuously until manually cancelled with `session.cancel()`.

#### `crates/extractor/` - Platform Extractors

**Purpose**: Custom video extractors and yt-dlp bridge for platform-specific parsing.

**Supported Platforms**: Douyin, TikTok, and fallback to yt-dlp for others.

#### `crates/live_recorder/` - Live Streaming

**Purpose**: Platform-agnostic live stream recording with `LiveRecorderCore` interface.

**Features**: Room status checking, stream info extraction, recording with quality/segment config.

#### `crates/bytedance/` - Douyin/TikTok API

**Purpose**: Douyin/TikTok Web API integration and signing.

#### `crates/xbogus/` - Signature Library

**Purpose**: X-Bogus and AB-Sign implementations using QuickJS and native Rust fallback.

## Development Conventions

### Code Style

- **Comments and logs**: Use Chinese
- **Logging emojis**: 🚀 (start), ✅ (success), ❌ (error), ⚠️ (warning)
- **GPUI async patterns**:
  - Use `cx.spawn()` or `smol::unblock()` for async work
  - Update state with `this.update(...); cx.notify()`
- **File naming**: Use `sanitize_filename()` from `shared::utils`
- **Path conflicts**: Auto-append sequence numbers via `generate_output_path()`

### Themes

- Theme files in `themes/` directory are hot-reloaded via `ThemeRegistry::watch_dir`
- UI colors accessed via `cx.theme()`
- Theme format: TOML with `[colors]` section

### Configuration & Tools

- Config files stored in platform config directory (via `dirs::config_dir()`)
- Tool binaries (yt-dlp/ffmpeg) stored in app data directory
- Primary config: `AppConfig` (app-level)
- Tool config: `tool_manager.toml` (tool-level)
- All configs support merge, validation, and defaults

### Platform Considerations

- **Windows**: Uses `windows_subsystem = "windows"` in release to hide console
- **Command spawning**: Always use `create_tokio_command()` to prevent console flashing
- **Paths**: Use `shared::utils` path helpers for cross-platform compatibility

## Build Profiles

The project uses optimized dev profile settings for GPUI performance:

```toml
[profile.dev]
codegen-units = 16
debug = "limited"
split-debuginfo = "unpacked"

[profile.dev.package]
resvg = { opt-level = 3 }
rustybuzz = { opt-level = 3 }
taffy = { opt-level = 3 }
ttf-parser = { opt-level = 3 }
```

These settings optimize GPUI rendering dependencies while keeping dev builds fast.

## CI/CD

The project uses GitHub Actions:

- **Format Check**: `cargo fmt --all -- --check`
- **Lint**: `cargo clippy -- -D warnings`
- **Test**: `cargo test`
- **Release**: Automated builds for Windows/macOS/Linux

Workflows located in `.github/workflows/`.

## Important Notes

### Tool Manager Migration

The `tool_manager` crate is in transition:
- New downloads should use the `download` crate (via `download_adapter.rs`)
- Legacy `downloader.rs` is being phased out
- Some task options not yet persisted (use `DownloadOptions::default()` for retries)

### Event System

- `ToolManager` broadcasts events via `tokio::sync::broadcast`
- Subscribe with `tool_manager.subscribe()` to receive `ToolManagerEvent`s
- Events include task updates, tool updates, and queue changes

### Cookie Support

- Cookies stored per-platform in config
- Matched by URL domain when making requests
- Supports accessing member-only content

### Task Persistence

- Tasks auto-saved to disk via `TaskPersistence`
- Restored on startup via `restore_tasks()`
- Use `clear_completed_tasks()` to clean up finished tasks

## Troubleshooting

### Build Issues

- Ensure Rust toolchain is up-to-date: `rustup update`
- Install system dependencies (see README.md for platform-specific requirements)
- For GPUI-related issues, ensure OpenGL 3.3+ / Metal / DirectX 11+ support

### Tool Detection

- yt-dlp and ffmpeg checked in app data directory first, then system PATH
- Use Settings page to manually install/update tools
- Check tool status via `ToolManager::ensure_tools()`

### Performance

- Use release builds for production: `cargo build --release`
- Dev builds have some optimizations enabled for GPUI (see Build Profiles)
- Enable `RUST_LOG=info` for performance diagnostics
