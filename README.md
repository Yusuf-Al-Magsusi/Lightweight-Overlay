# Lightweight Overlay

> Working title. The name is a placeholder.

A free, open-source, lightweight game overlay for Windows. It draws a transparent, click-through, always-on-top window with small widgets (now playing, clock, FPS, CPU/GPU stats) over games and full-screen apps. Portable single `.exe`, no installer, no account.

**Status: pre-alpha (Milestone 5). Clock, Now Playing, CPU, memory, GPU and (opt-in) FPS widgets, with a settings window and a drag-to-position edit mode.**

Widgets:
- **Clock**: local time.
- **CPU** and **Memory**: overall CPU load and physical memory use, read with ordinary Windows calls (no driver, no admin).
- **GPU**: load and VRAM on any GPU (AMD, Intel or NVIDIA) from the same Windows counters Task Manager uses. On NVIDIA it also shows temperature and power through NVIDIA's NVML library, loaded at runtime. Hidden quietly if no GPU source works (for example in a virtual machine). The AMD path is tested on real hardware; the NVIDIA path is not yet (see `docs/nvidia-test-checklist.md`).
- **Now Playing**: title, artist, progress bar with times and play state, from whatever app Windows treats as the current media session (Spotify, browsers, VLC and others). No network access; it reads the Windows media session only.

## Try it (from source)

Needs Rust (stable, MSVC toolchain).

```
cargo run --release -p lwo
```

- The widgets appear stacked at the top-left of your primary monitor, each in its own small window.
- **Ctrl+Alt+O** shows and hides the overlay.
- **Ctrl+Alt+E** turns edit mode on and off (see below).
- The tray icon: left-click shows or hides the overlay; right-click opens a menu with **Show / hide**, **Edit layout**, **Settings...** and **Quit**.

### Settings

Open **Settings...** from the tray menu. You can turn each widget on or off, set how often it refreshes (1 to 60 seconds), and choose a theme (Dark, Light or Midnight), temperature unit (Celsius or Fahrenheit), scale, opacity, font, optional colour overrides (hex, like `#FFFFFF`) and both hotkeys. **Apply** takes effect immediately with no restart; if something is invalid, a message says what and nothing changes. **Reset layout** puts the widgets back in the default stack.

### Moving the widgets (edit mode)

Press **Ctrl+Alt+E** (or use the tray menu or the settings window). Every widget gets a green outline; drag each one wherever you like, on any monitor. Press the hotkey again to save. While editing, clicks go to the widgets instead of whatever is underneath, so keep it short. Saved positions follow the nearest screen corner, so they survive a resolution or display-scaling change.

### Config file

Everything is saved in `%APPDATA%\lightweight-overlay\config.toml`, created on first run. You can edit it by hand and restart, or use the settings window. An invalid file is never overwritten: the app uses defaults for that run and tells you.
## FPS widget (opt-in)

Shows the frames per second of whatever program is in front. It is **off by default**; switch it on in Settings.

**It needs a Windows privilege.** It reads Windows' own "a frame was presented" events (ETW, the method PresentMon uses). Windows only allows that for administrators and for members of the built-in **Performance Log Users** group. The overlay **never asks for admin rights** and never adds you to the group. If your account isn't allowed, the FPS option is greyed out with an explanation; everything else works normally.

**Think before joining the group.** The privilege belongs to your *account*, not to this app: any program you run could then start system traces, which can show what other programs are doing. If you accept that, run this in PowerShell **as administrator** (replace `YourUserName`), then sign out and back in:

```
Add-LocalGroupMember -SID S-1-5-32-559 -Member "YourUserName"
```

To undo it, use `Remove-LocalGroupMember` with the same arguments, then sign out and back in. (`S-1-5-32-559` is the group's fixed ID in every Windows language. Windows 11 Home has no Computer Management screen for this.)

Know before relying on it:
- The number describes roughly the last 2.5 to 3.5 seconds, not this instant, because Windows delivers frame events in batches.
- It reads the program that owns the window in front. Programs that draw from a helper process (many browsers) show `--`.
- Checked exactly against a test program at 30, 100 and 144 fps. With real games it is **not yet verified**; compare with your game's own counter.
- **Anti-cheat: unknown.** The overlay doesn't touch or open the game, but nothing we found says how anti-cheat systems treat reading these events. Use at your own risk, especially in competitive games.

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

`scripts/smoke-test.ps1` runs the built exe against a temporary config and checks the window, hotkey, single-instance and quit behaviour. `scripts/media-test.ps1` tests the Now Playing widget with a fake media player (`cargo build --example fake_media` first). `scripts/stats-test.ps1` tests the CPU, memory and GPU panels, using a fake `nvml.dll` (`crates/fake-nvml`) for the NVIDIA path, and `cargo run --example gpu_load` generates real GPU load for the generic path. Run `cargo build --workspace` before `cargo test` so the fake library exists (the NVML test is skipped locally, and fails when `CI` is set, if it is missing). `scripts/fps-test.ps1` checks the FPS widget end to end (needs the privilege and `cargo build --release --example present_rate`); `cargo test --release -p lwo accuracy -- --ignored --nocapture` compares the frame counter with that known-rate program. `scripts/edit-test.ps1` drags a widget with the real mouse in edit mode and checks that the layout is saved and restored, and `scripts/settings-test.ps1` drives the real settings window (20 checks, including bad input). Run the test scripts in separate PowerShell sessions. See `docs/decisions.md` for design decisions and `docs/game-test-checklist.md` for manual game testing.

## License

MIT, see [LICENSE](LICENSE).
