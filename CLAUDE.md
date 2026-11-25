# CLAUDE.md

Free, open-source, lightweight game overlay for Windows, written in Rust. The product name is a placeholder (`lightweight-overlay`, crate prefix `lwo`); read it from `lwo_core::APP_NAME`.

## Hard constraints (never break these without asking the owner)

1. **Light.** Initial targets: under 30 MB RAM and under 1% CPU on an idle desktop with three widgets on. Measure and report both numbers at every milestone. If a target proves unrealistic, show the measurements and propose a new one.
2. **Never harm the game.** The overlay is its own process. No DLL injection, no DirectX or Vulkan hooks, nothing that touches the game's process. Draw a transparent, click-through, topmost window. Windowed and borderless fullscreen are supported; exclusive fullscreen is out of scope.
3. **Anti-cheat safe by default.** The default install uses only user-mode data sources and neither ships nor installs a driver. CPU package power may come from Windows' own "Energy Meter" counters, whose driver is part of Windows (agreed with the owner in Milestone 6). Anything that needs us to install or load a kernel driver (CPU temperature, and CPU wattage where Windows doesn't report it) is opt-in, runs in a separate helper process, and shows a clear warning in the UI and the README.
4. **No admin rights for the main app.** Only the optional sensor helper may ask for elevation.
5. **Lazy by design.** A disabled widget is not loaded, polled or drawn. Poll sensors about once per second, and only the ones a visible widget needs. Redraw only when a value changes. When nothing changes the app sleeps: no busy loops, no per-frame redraw.
6. **Free and open source.** MIT license. Dependencies must be MIT, Apache-2.0 or BSD-compatible (MPL-2.0 only for a separate helper process). Each new dependency needs a one-line justification: purpose, license, size impact. No telemetry. No network calls except for a widget the user enabled, such as Spotify.
7. **Windows 10 and 11 (x64) first, portable later.** Keep all OS-specific code (overlay window, media, sensors) in per-platform modules behind traits, selected at compile time with cfg. Do not build Linux or macOS ports before v0.1.

## Working rules

- One milestone at a time, one git branch per milestone (`milestone-N`). The owner merges to `main`. Stop after each milestone and report: what changed, how to run it, measured RAM and CPU, what to test by hand.
- Commit small and often. Run `cargo fmt` and `cargo clippy -- -D warnings` before each commit.
- Never claim something works unless it was run. If it can't be tested (a game, a GPU, a driver), say so and give a manual checklist.
- Decisions with real trade-offs: ask the owner. Otherwise decide and record the reason in `docs/decisions.md`.
- Keep the README honest: use-at-your-own-risk with anti-cheat; only the default install is driver-free.
- Prefer boring, simple code. Add an abstraction only when two real widgets need it.
- Commits are authored by the repository owner only. No `Co-Authored-By` or tool-attribution trailers.
- Test hardware: AMD GPU (so no NVML testing on the dev machine). Games available for manual tests: Minecraft, Rocket League, Fortnite.

## Deferred work

Postponed, not cancelled; read `docs/deferred.md` when planning a milestone. Includes **CPU temperature** (skipped in Milestone 6: needs a kernel driver and a license exception the owner has not given).

## Layout

- `crates/` product code (Cargo workspace).
- `spikes/` throwaway rendering spikes (separate workspace), removed after the Milestone 0 decision.
- `docs/decisions.md` decision log.
