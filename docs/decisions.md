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

## Measurements: Milestone 2 (clock + Now Playing)
Same method as before (5 min idle, 60 s sampled, `scripts/measure-overlay.ps1`), release build, one run each, Windows 11, AMD RX 9070 XT. Two widgets, not yet the three-widget target scenario. "Playing" used the fake media player in `--forever` mode (a real Windows media session, redrawing every second).

| Metric | No media session | Media playing | Target |
|---|---|---|---|
| Private working set (Task Manager "Memory") | 15.6 MB | 15.7 MB | under 30 MB |
| CPU (% of all cores) | 0.002 | 0.008 | under 1 |
| GPU engine use (avg / max %) | 0.008 / 0.014 | 0.007 / 0.008 | none |
| Exe size | 0.39 MB | 0.39 MB | n/a |
| Threads / handles | 20 / 776 | 22 / 782 | n/a |

Hide/show stress: 80 toggles with a session active kept threads at about 30 and handles at about 830 (no growth), so the start/stop of the media worker does not leak.

## D10: Media source design (Milestone 2)
- API: Windows `GlobalSystemMediaTransportControlsSessionManager`. Fully event-driven: we subscribe to `CurrentSessionChanged` plus, on the current session, `MediaPropertiesChanged`, `PlaybackInfoChanged` and `TimelinePropertiesChanged`. Nothing is polled.
- A worker thread (WinRT MTA) blocks on a channel. Events are debounced (50 ms, they usually arrive in bursts), then the session is read and `WM_MEDIA_CHANGED` is posted to the control window. The UI thread never blocks on WinRT.
- Position: the API reports a position and the time it was valid (`LastUpdatedTime`). We add the age (ignoring values over 24 h old), then the widget ticks forward itself from `MediaSnapshot::at` while playing. No polling. Players that report seeks late will show the new position late.
- Lazy: the source exists only if a widget declares `Source::Media`, and runs only while the overlay is visible. Hiding the overlay ends the worker and clears the snapshot; showing it starts a fresh one. A generation counter stops a worker that is shutting down from overwriting newer state.
- Which session: the system's "current" session (what Windows shows in the volume flyout). Choosing between several players is not supported yet.
- Widget refresh interval is 1 s only while playing; paused or idle it asks for one hour (media events still redraw immediately), so a paused player costs nothing.
- The widget redraws only when something visible changes (title, artist, album, play state, whole-second position or duration).
- `windows` crate 0.62 uses `join()` on async operations (in the separate `windows-future` crate), not `get()`.
- New `windows` features: `Foundation`, `Media_Control`, `Win32_System_WinRT`; `Media` for the dev-only example. No new crates beyond what `windows` already pulls in.

## D11: Now Playing widget layout (Milestone 2)
- Fixed width (300 px at scale 1.0) so the window never resizes; long titles and artists are cut with "..." (DirectWrite trimming).
- Rows: state label (NOW PLAYING / PAUSED), title, artist (optionally "artist - album", `show_album`), progress bar, elapsed time left and total right. Unknown duration (live streams) shows an empty bar and `--:--`.
- No media session: the panel stays and shows "Nothing playing". Hiding the panel entirely when idle needs a dynamic window size; deferred to the settings milestone as an option.
- Album art is not shown (would need image decoding and memory); revisit only if wanted.
- Config: `[widgets.now_playing] enabled`, `show_album`. Enabled by default.
- Testing without a real player: `crates/lwo/examples/fake_media.rs` registers a real Windows media session and `scripts/media-test.ps1` screenshots the overlay through play, pause, a long title and the session disappearing. The example is dev-only and not shipped.

## Measurements: Milestone 3 (clock, Now Playing, CPU, memory)
Same method as before (5 min idle, 60 s sampled, `scripts/measure-overlay.ps1`), release build, one run, Windows 11, AMD RX 9070 XT. This is the real default config on this machine: four panels visible, so it covers the "three widgets" target. The GPU panel was hidden (no NVIDIA driver) and the machine's Spotify session was paused, so Now Playing was in its cheap idle state.

| Metric | Result | Target |
|---|---|---|
| Private working set (Task Manager "Memory") | 16.4 MB | under 30 MB |
| CPU (% of all cores) | 0.006 | under 1 |
| GPU engine use (avg / max %) | 0.008 / 0.016 | none |
| Exe size | 0.41 MB | n/a |
| Threads / handles | 25 / 807 | n/a |

Previous milestone for comparison: 15.6 MB and 0.002% CPU with two widgets. The two new panels add about 1 MB and a CPU/memory read per second. Not measured: the NVIDIA path on real hardware, and CPU/RAM with the NVML library actually loaded and polled (only done against the fake DLL, which is not representative of real NVML cost).

## D12: CPU and memory sources: direct Win32 calls, not `sysinfo` (Milestone 3)
- CPU load: `GetSystemTimes`, with load computed from the change between two polls (kernel time includes idle). Memory: `GlobalMemoryStatusEx`, used = total physical minus available (the same figure Task Manager shows as "In use").
- Chosen over the `sysinfo` crate because each is a single system call with no allocation and no extra dependency. `sysinfo` is a general-purpose library that enumerates far more than we need. **We did not benchmark `sysinfo`**: the brief said to pick whichever measures lighter, and this choice is based on the cost of the calls themselves, not a side-by-side run. The measured result for our approach is in the Milestone 3 table.
- Verified against Windows: memory matched `Win32_OperatingSystem` exactly (19.7 / 31.1 GiB), and the CPU panel read 28% while the Windows performance counter read 28, 27 and 26% under a synthetic load.
- The first CPU poll after the overlay is shown only records a baseline, so the CPU panel shows `--%` for about a second.

## D13: NVIDIA GPU through NVML, loaded at runtime with no new dependency
- NVML (`nvml.dll`) comes with the NVIDIA driver and is user-mode. We load it with `LoadLibraryExW` and look up seven functions with `GetProcAddress`, using the signatures from `nvml.h` (`nvmlInit_v2`, `nvmlShutdown`, `nvmlDeviceGetHandleByIndex_v2`, `nvmlDeviceGetTemperature`, `nvmlDeviceGetPowerUsage`, `nvmlDeviceGetUtilizationRates`, `nvmlDeviceGetMemoryInfo`). This avoids a crate such as `nvml-wrapper` (and its own loader dependency) for what is a very small surface.
- Search path: the Windows system folder first, then NVIDIA's old install folder by absolute path. Never the application or current folder, so a planted `nvml.dll` beside the exe is not loaded.
- Shown: GPU 0 only: load %, temperature (C), power (W), VRAM used / total. Clocks are not shown. Power is optional (`-- W` where the driver doesn't report it).
- Lazy: the widget is dropped at startup if NVML can't be loaded and initialised (no NVIDIA driver, or AMD/Intel GPU): no error, no balloon, no empty panel. While the overlay is visible NVML is loaded and initialised; hiding the overlay shuts it down and unloads it (verified: the module is gone while hidden).
- **Not tested against real NVIDIA hardware** (the dev machine has an AMD GPU). Tested against `crates/fake-nvml`, a dev-only fake `nvml.dll` that follows `nvml.h`; this verifies loading, calling, struct plumbing, unloading, and the widget. It cannot catch a mistake in my reading of `nvml.h`. Manual checklist: `docs/nvidia-test-checklist.md`.
- Debug builds only (`cfg(debug_assertions)`) honour `LWO_NVML_PATH` to load a different library. Release builds never read it, so the shipped exe has no environment-variable hook that could load an arbitrary library.

## D14: Lazy source manager (Milestone 3)
- `Sources` in the Windows platform module holds one entry per source type, created only if some enabled widget declares it through `Widget::sources()` (`Media`, `Cpu`, `Memory`, `Gpu`).
- `start` on show, `stop` on hide. While running, polled sources are read once per timer tick, and only the ones that exist; the result is handed to widgets in `Context::stats`. Media stays event-driven (D10).
- A widget whose source is unavailable on this machine is removed before layout (`source_available`).
- Widgets redraw only when their formatted text or whole-percent bar changes, so a steady CPU or memory value costs no drawing.
- The refresh interval is still a fixed 1 s per widget; per-widget refresh rates come with the settings UI (Milestone 4).

## D15: Stat panels (Milestone 3)
- CPU, Memory and GPU are separate widgets sharing one compact layout (label, large value, bar, optional detail lines). Each has a fixed width (150, 200 and 220 px at scale 1.0) so windows never resize.
- Config: `[widgets.cpu]`, `[widgets.ram]`, `[widgets.gpu]`, each `enabled` (default true). The GPU widget is on by default but only appears where NVML exists.
- Temperature is shown in Celsius only until the units setting arrives in Milestone 4.
- Defaults now stack four panels (clock, Now Playing, CPU, memory), about 380 px tall at 100% scale. A layout and "compact mode" are part of the settings milestone.

## D9: Known limits in Milestone 1
- DPI is read once at startup (system DPI). Moving the overlay between monitors of different DPI or changing scale at runtime is not handled yet.
- Tray icon is the stock Windows application icon; a custom icon comes later.
- Edit mode (drag to position) is Milestone 4, so the overlay is always fully click-through.
