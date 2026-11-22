# Decisions

Newest at the bottom. Each entry: decision, reason.

## D1: Cargo workspace; spikes in a separate workspace
Product code lives in `crates/`. The rendering spikes pull in heavy crates (eframe, winit, windows), so they live in `spikes/` with their own workspace. They are kept on the `milestone-0` branch as a record and will be deleted at the start of Milestone 1.

## D2: Placeholder name `lightweight-overlay` / `lwo`
No name chosen yet. The name is exposed as `lwo_core::APP_NAME`; config dir, window class and tray tooltip must use it.

## D3: Dependency policy
MIT, Apache-2.0 or BSD-compatible only (MPL-2.0 only in a separate helper process). Each new dependency is listed here with purpose, license and size impact.

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
