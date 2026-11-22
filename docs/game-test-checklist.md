# Game test checklist (manual)

Frame times and in-game behaviour can't be measured automatically, so this is run by hand.

## Setup
- Build: `cargo build --release -p lwo`, then run `target\release\lwo.exe`.
- Install PresentMon or CapFrameX.
- Set the game to **borderless windowed** (exclusive fullscreen is unsupported).
- Use offline or practice modes only. No anti-cheat guarantee exists for any third-party overlay.

## Per game (Minecraft single-player, Rocket League freeplay or training, Fortnite creative or lobby)
1. Stand in a fixed spot. Record 60 seconds of frame times with the overlay **not running**. Note average FPS and 1% low.
2. Start `lwo.exe`. The clock should appear at the top-left over the game.
3. Record another 60 seconds in the same spot.
4. Check:
   - Is the clock visible over the game?
   - Do mouse clicks near the clock reach the game?
   - Does the game keep focus (no alt-tab flicker, no lost mouse capture)?
   - Press Ctrl+Alt+O: does the clock hide and show? Does the game keep running smoothly?
   - Any stutter when the clock changes each second?
5. Quit from the tray icon (right-click, Quit).
6. Compare average FPS and 1% low to the baseline.

## Report back
For each game: baseline FPS / 1% low, FPS / 1% low with overlay, clock visible (yes/no), clicks pass through (yes/no), hotkey works (yes/no), anything odd.

## Now Playing widget (Milestone 2)
With a real player (Spotify, a browser tab, VLC, and so on) running:
- Title, artist, elapsed and total time match the player, and the bar moves while playing.
- Pause in the player: the label changes to PAUSED and the time stops. Resume: it moves again.
- Skip a track: title and artist change within about a second.
- Seek in the player: the elapsed time jumps to the new position (some players report seeks late).
- Close the player: the widget shows "Nothing playing".
- With two players open, the widget follows the one Windows treats as current (the one shown in the volume flyout).

## Rendering decision
The Milestone 0 spikes compared rendering approaches (see `docs/decisions.md`, D4). The frame-time comparison between candidates was never run; only the chosen raw Win32 approach is in the codebase now.
