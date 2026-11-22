# Lightweight Overlay

> Working title. The name is a placeholder.

A free, open-source, lightweight game overlay for Windows. It draws a transparent, click-through, always-on-top window with small widgets (now playing, clock, FPS, CPU/GPU stats) over games and full-screen apps. Portable single `.exe`, no installer, no account.

**Status: pre-alpha (Milestone 2). Clock and Now Playing widgets exist so far.**

Widgets:
- **Clock**: local time.
- **Now Playing**: title, artist, progress bar with times and play state, from whatever app Windows treats as the current media session (Spotify, browsers, VLC and others). No network access; it reads the Windows media session only.

## Try it (from source)

Needs Rust (stable, MSVC toolchain).

```
cargo run --release -p lwo
```

- A clock appears at the top-left of your primary monitor.
- **Ctrl+Alt+O** shows and hides it.
- The tray icon: left-click toggles, right-click opens the menu (Show / hide, Quit).
- Settings live in `%APPDATA%\lightweight-overlay\config.toml`, created on first run. Edit it and restart to apply changes (a settings UI comes later).

## Design goals

- Light: target under 30 MB RAM and under 1% CPU idle with three widgets.
- Separate process only. No injection, no graphics API hooks. Windowed and borderless fullscreen only; exclusive fullscreen is not supported.
- Disabled widgets cost nothing. The app sleeps when nothing changes.
- No telemetry. No network calls unless you enable a widget that needs one.

## Anti-cheat: use at your own risk

This overlay never touches the game process, but **no one can promise that every anti-cheat system will accept any third-party overlay.** You use it at your own risk, especially in competitive online games.

- The **default install is driver-free**: it uses only user-mode data sources.
- Anything needing a kernel driver (CPU temperature and wattage) will be an **opt-in separate helper** with its own warning. Only the default install is driver-free.

## Development

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`scripts/smoke-test.ps1` runs the built exe against a temporary config and checks the window, hotkey, single-instance and quit behaviour. `scripts/media-test.ps1` tests the Now Playing widget with a fake media player (`cargo build --example fake_media` first). See `docs/decisions.md` for design decisions and `docs/game-test-checklist.md` for manual game testing.

## License

MIT, see [LICENSE](LICENSE).
