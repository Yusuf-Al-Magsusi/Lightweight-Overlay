# Lightweight Overlay

> Working title. The name is a placeholder.

A free, open-source, lightweight game overlay for Windows. It draws a transparent, click-through, always-on-top window with small widgets (now playing, clock, FPS, CPU/GPU stats) over games and full-screen apps. Portable single `.exe`, no installer, no account.

**Status: pre-alpha (Milestone 0, scaffolding and rendering spike). Nothing usable yet.**

## Design goals

- Light: target under 30 MB RAM and under 1% CPU idle with three widgets.
- Separate process only. No injection, no graphics API hooks. Windowed and borderless fullscreen only; exclusive fullscreen is not supported.
- Disabled widgets cost nothing. The app sleeps when nothing changes.
- No telemetry. No network calls unless you enable a widget that needs one.

## Anti-cheat: use at your own risk

This overlay never touches the game process, but **no one can promise that every anti-cheat system will accept any third-party overlay.** You use it at your own risk, especially in competitive online games.

- The **default install is driver-free**: it uses only user-mode data sources.
- Anything needing a kernel driver (CPU temperature and wattage) will be an **opt-in separate helper** with its own warning. Only the default install is driver-free.

## License

MIT, see [LICENSE](LICENSE).
