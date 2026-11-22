# Game frame-time test checklist (manual)

Frame times can't be measured automatically, so this is run by hand.

## Setup
- Install PresentMon or CapFrameX.
- Set the game to **borderless windowed** (exclusive fullscreen is unsupported).
- Use offline or practice modes only. No anti-cheat guarantee exists for any third-party overlay.

## Per game (Minecraft single-player, Rocket League freeplay or training, Fortnite creative or lobby)
1. Stand in a fixed spot. Record 60 seconds of frame times with no overlay. Note average FPS and 1% low.
2. Start one overlay from `spikes/target/release/`: `spike-win32.exe`, `spike-winit-skia.exe` or `spike-egui-wgpu.exe`.
3. Record another 60 seconds in the same spot.
4. Check: is the overlay visible over the game? Do mouse clicks reach the game? Any stutter?
5. Stop it: `Stop-Process -Name spike-* -Force`.
6. Repeat for each overlay, then compare average FPS and 1% low to the baseline.

## Report back
For each game and overlay: baseline FPS / 1% low, FPS / 1% low with overlay, visible (yes/no), clicks pass through (yes/no).
