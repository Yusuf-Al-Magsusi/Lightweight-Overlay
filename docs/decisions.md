# Decisions

Newest at the bottom. Each entry: decision, reason.

## D1: Cargo workspace; spikes in a separate workspace
Product code lives in `crates/`. The rendering spikes pulled in heavy crates (eframe, winit, windows), so they lived in `spikes/` with their own workspace. They were deleted at the start of Milestone 1 as planned; they remain in git history on the `milestone-0` branch.

## D2: Placeholder name `lightweight-overlay` / `lwo`
No name chosen yet. The name is exposed as `lwo_core::APP_NAME`; config dir, window class and tray tooltip must use it.

## D3: Dependency policy
MIT, Apache-2.0 or BSD-compatible only (MPL-2.0 only in a separate helper process). Each new dependency is listed here with purpose, license and size impact.

### Dependencies added so far
| Crate | Purpose | License | Size impact |
|---|---|---|---|
| `windows` 0.62 | Win32 bindings (window, Direct2D, DirectWrite, tray, hotkey) | MIT/Apache-2.0 | Only used APIs link in; part of the exe size below |
| `serde` 1 | Derive (de)serialization for the config | MIT/Apache-2.0 | Compile-time only, negligible at runtime |
| `toml` 0.9 | Read and write `config.toml` | MIT/Apache-2.0 | Roughly 100-200 KB in the exe |

## D4: Rendering approach: raw Win32 behind a platform trait

### Measurements
Machine: Windows 11, AMD Radeon RX 9070 XT + integrated AMD graphics. Scene: clock, label, progress bar (three widgets), updated once per second. 5 minutes idle, then CPU and GPU sampled for 60 s. Measured with `scripts/measure-spike.ps1`. One run each, release builds with `opt-level = "s"`, LTO.

| | (a) Win32 + Direct2D | (b) winit + tiny-skia | (c) egui + wgpu |
|---|---|---|---|
| Exe size | 0.11 MB | 0.68 MB | 6.82 MB |
| Private working set (Task Manager "Memory") | **15.1 MB** | **21.7 MB** | 170.3 MB |
| Private bytes (commit) | 85.9 MB | 74.7 MB | 640.2 MB |
| Working set | 38.9 MB | 36.4 MB | 238.6 MB |
| CPU (% of all cores, 60 s) | 0.006 | 0.006 | 0.003 |
| GPU engine use (avg / max %) | 0.007 / 0.008 | 0 / 0 | 0.074 / 0.106 |
| Threads / handles | 19 / 741 | 3 / 196 | 27 / 954 |
| Per-pixel transparency | works (screenshot) | works (screenshot) | works (screenshot) |
| Click-through (WindowFromPoint skips it) | yes | yes | yes |
| Style flags (LAYERED, TRANSPARENT, TOPMOST, TOOLWINDOW, NOACTIVATE) | all five | all five | no TOOLWINDOW or NOACTIVATE |

Other findings:
- (c) egui with the glow (OpenGL) backend started but drew nothing on this machine, even opaque, with no error output. Dropped.
- Memory target: the 30 MB goal is judged on private working set. Private bytes (commit) is much larger for all of them (a Windows commit-charge artifact) and is recorded only for completeness.
- All three idle at about 0% CPU: they block in the message loop or event loop between 1 s ticks.

### Not measured (needs the owner's machine and games)
Game frame times with and without the overlay (Minecraft, Rocket League, Fortnite in borderless windowed). Checklist in `docs/game-test-checklist.md`. This decision should be revisited if frame times show a difference between candidates.

### Decision
Use **raw Win32** (layered window, `UpdateLayeredWindow`, Direct2D + DirectWrite) on Windows, behind a platform trait so other platforms can plug in later.

Reasons:
- Smallest in every measure: 15.1 MB vs 21.7 MB (b) private working set, 0.11 MB exe vs 0.68 MB. egui is ruled out at 170 MB.
- Rule from the brief: prefer the portable option only if within a few MB of raw Win32 with no GPU load. (b) is 6.6 MB (about 44%) heavier, which is more than "a few".
- (b) is less portable than it looks: its present path (`UpdateLayeredWindow`) is Windows-only, so only the tiny-skia drawing is portable.
- Win32 uses more threads (19) than (b) (3), from Direct2D/DirectWrite. Idle CPU is the same, so this is acceptable.

Accepted by the owner. (b) remains the fallback if portability ever matters more than about 6 MB; switching later is contained to the platform and render modules.

Note: the Win32 spike's overlay window sets all five styles. If the egui path is ever revisited, its missing TOOLWINDOW and NOACTIVATE would need to be fixed.

## D5: Crate layout and platform boundary (Milestone 1)
- `lwo-core`: no OS code. Config, hotkey parsing, the `Widget` trait, the `Canvas` / `TextMeasure` traits and the clock widget. Fully unit-tested.
- `lwo`: the binary. `platform/mod.rs` selects the platform module with `cfg`; `platform/windows/` holds the Win32 code (window, Direct2D canvas, tray, hotkey). A non-Windows build fails with `compile_error!` until a port exists.
- The boundary between widgets and the OS is `Canvas`/`TextMeasure` (draw rounded rect, draw text, measure text). Widgets never see OS types. A `Platform` trait was not added: there is one platform, and the contract is just `platform::run(App)`. Add the trait when a second platform exists.
- `Widget::size` is computed once at startup from the widest possible content (clock uses all 8s), so the window never resizes and text never jitters.
- Registry and lazy source manager are deferred to Milestone 3, when a second widget with a real data source exists.

## D6: Control window separate from the overlay window
The overlay window is `WS_EX_NOACTIVATE | WS_EX_TRANSPARENT` and hidden at times, which makes it a poor owner for the tray menu (a popup menu needs a foreground-capable window to dismiss correctly). A never-shown control window owns the hotkey, timer and tray icon; the overlay window only displays pixels.

## D7: Lazy behaviour of the Win32 loop
- The thread blocks in `GetMessageW`; no polling.
- Hidden overlay: the timer is killed, nothing is drawn.
- Visible: one timer lined up to the next second boundary (clock only), redraw only if a widget's `update` reports a change.
- Reasserts `HWND_TOPMOST` every time the overlay is shown, since a borderless game can take the top spot.

## D8: Config behaviour
- Location `%APPDATA%\lightweight-overlay\config.toml`. Missing file: defaults are written. Invalid file: defaults are used for that run, the file is **not** overwritten, and a tray balloon tells the user. Out-of-range values (scale, opacity) are clamped.
- Saves are atomic (temp file then rename).
- Per-monitor placement, themes, units and per-widget refresh come in Milestone 4. Milestone 1 supports anchor + offset on the primary monitor, scale, opacity, font, start visibility and the toggle hotkey.
- A single instance runs at a time (named mutex); a second launch exits silently.

## Measurements: Milestone 1 (clock widget only)
Same method as D4 (5 min idle, then 60 s sampled, `scripts/measure-overlay.ps1`), release build, one run, Windows 11, AMD RX 9070 XT. Only one widget exists, so this is not yet the "three widgets" target scenario.

| Metric | Result | Target |
|---|---|---|
| Private working set (Task Manager "Memory") | 15.0 MB | under 30 MB |
| CPU (% of all cores) | 0.003 | under 1 |
| GPU engine use (avg / max %) | 0.007 / 0.008 | none |
| Exe size | 0.33 MB | n/a |
| Private bytes (commit) | 85.9 MB | n/a |
| Threads / handles | 19 / 742 | n/a |

## D9: Known limits in Milestone 1
- DPI is read once at startup (system DPI). Moving the overlay between monitors of different DPI or changing scale at runtime is not handled yet.
- Tray icon is the stock Windows application icon; a custom icon comes later.
- Edit mode (drag to position) is Milestone 4, so the overlay is always fully click-through.
