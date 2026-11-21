# Tessera

A minimal grid window placer for Windows 10/11, inspired by [gTile](https://github.com/gTile/gTile).

Press **Win+Shift+G**: a semi-transparent 6×6 grid appears in the middle of the monitor that holds the focused window. Pick two corners and the window snaps to that part of the monitor's work area (the screen minus the taskbar).

## Usage

| Action | Keyboard | Mouse |
|---|---|---|
| Move the highlighted cell | Arrow keys | Hover |
| Set corner A | Space | Click |
| Set corner B and apply | Space again (or Enter) | Second click, or drag from A to B |
| Undo corner A / close | Esc | Right click |

- The highlighted cell starts where the window's top-left corner currently is.
- The grid closes after applying, when it loses focus, or when you press the hotkey again.
- Maximized windows are restored first. Invisible window borders are compensated, so neighbouring windows meet without gaps.
- Windows of programs running as administrator cannot be moved by a non-elevated program: Tessera just beeps.
- If Win+Shift+G is taken, Tessera falls back to **Win+Ctrl+Alt+G** and says so in a notification.

Tray icon menu: **Start with Windows** (enabled on first launch) and **Exit**.

## Log

`%LOCALAPPDATA%\Tessera\tessera.log`, rewritten at every start. It records the target window, the rectangles involved and the result of every move: attach it when reporting a problem.

## Building

Cross-compiling from Linux:

    rustup target add x86_64-pc-windows-gnu
    apt-get install -y gcc-mingw-w64-x86-64
    cargo build --release --target x86_64-pc-windows-gnu

The executable is `target/x86_64-pc-windows-gnu/release/tessera.exe`. On Windows, a plain `cargo build --release` works too (MSVC toolchain).

The executable is not signed, so SmartScreen warns on first launch: choose *More info → Run anyway*.

## Layout

- `src/main.rs` — startup, single instance, hidden window, message loop
- `src/hotkey.rs` — global hotkey and fallback
- `src/overlay.rs` — grid window, drawing, mouse and keyboard selection
- `src/placement.rs` — target validation, grid geometry, moving the window
- `src/tray.rs` — tray icon, notifications, menu
- `src/autostart.rs` — Run key handling
- `src/log.rs`, `src/util.rs` — logging and helpers
