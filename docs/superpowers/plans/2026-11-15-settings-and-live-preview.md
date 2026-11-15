# Settings Application & Named-Pipe Live Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a widget-slotted configuration schema, a Named Pipe live-preview IPC protocol and session state machine, Named Pipe server integration with an "Ayarlar" context menu entry in `bar`, and a standalone native Settings application (`crates/settings` -> `taskbar-settings.exe`) with hit-tested controls and live preview.

**Architecture:** `core` owns the widget-slotted `Config` extensions (`WidgetSettings`, `KNOWN_WIDGETS`), configurable `ClockModule`/`BatteryModule`, and the JSON-lines IPC protocol (`IpcRequest`, `IpcResponse`, `PreviewSessionState`). `bar` hosts a Named Pipe server (`\\.\pipe\TaskbarLiveConfigV1`) that feeds `WM_IPC_EVENT` into `PreviewSessionState`, applying previews instantaneously and reverting cleanly on client disconnect/cancel. `crates/settings` renders a native `windows-rs` + DWM Mica + `tiny-skia` + `fontdue` UI driven by a pure hit-tested `SettingsAppState` and `PipeClient`.

**Tech Stack:** Rust (edition 2024), `windows` 0.62.2, `tiny-skia` 0.11, `fontdue` 0.9, `serde` + `serde_json` + `toml` 0.8.

**Spec:** [2026-11-15-taskbar-architecture-design.md](file:///C:/Users/Mert/Documents/Taskbar/docs/superpowers/specs/2026-11-15-taskbar-architecture-design.md)

---

### Task 1: Widget-Slotted Config Schema & Configurable Modules (`crates/core`)
- Modify `crates/core/src/config.rs`, `crates/core/src/module.rs`, `crates/core/src/modules/clock.rs`, `crates/core/src/modules/battery.rs`.
- Add `WidgetSettings`, `WidgetCatalogEntry`, `KNOWN_WIDGETS`, `Config::module_config`, and helper methods (`widget_settings`, `widget_settings_mut`, `is_module_enabled`, `set_module_enabled`, `move_module`).
- Add `with_settings` constructors to `ClockModule` (supporting `format = "12h"` / `"24h"` and `show_label`) and `BatteryModule` (supporting `show_icon` and `params["show_bolt"]`).
- Add `build_modules_with_config`.
- Verify: `cargo test -p core`.

### Task 2: Live-Preview IPC Protocol & Session State Machine (`crates/core`)
- Create `crates/core/src/ipc.rs` and export from `crates/core/src/lib.rs`.
- Implement `DEFAULT_PIPE_NAME`, `IPC_PROTOCOL_VERSION`, `IpcRequest`, `IpcResponse`, line-based JSON encode/decode with version check, and `PreviewSessionState`.
- Verify: `cargo test -p core ipc`.

### Task 3: Named Pipe Server, Preview Session & "Ayarlar" Menu (`crates/bar`)
- Enable `Win32_System_Pipes` in `crates/bar/Cargo.toml`.
- Create `crates/bar/src/ipc_server.rs` (`WM_IPC_EVENT`, `IpcEvent`, `start_ipc_server`, `send_ipc_request_once`).
- Update `crates/bar/src/app.rs` to use `PreviewSessionState`, handle `WM_IPC_EVENT`, rebuild modules via `build_modules_with_config`, and add `"Ayarlar"` to the right-click context menu spawning `taskbar-settings.exe`.
- Verify: `cargo test -p bar`.

### Task 4: Settings Application (`crates/settings` -> `taskbar-settings.exe`)
- Add `crates/settings` to root `Cargo.toml`.
- Create `crates/settings/Cargo.toml`, `src/ipc_client.rs`, `src/state.rs`, `src/ui.rs`, `src/canvas.rs`, `src/main.rs`.
- Implement Appearance, Widgets (toggle, reorder, widget slot card), and About/Updates tabs plus Revert/Save footer bar.
- Verify: `cargo test -p taskbar-settings`.

### Task 5: Workspace Build & End-to-End Verification
- Run `cargo test --workspace` and `cargo build --workspace --release`.
- Confirm `bar.exe`, `taskbar-updater.exe`, and `taskbar-settings.exe` build cleanly and commit changes.
