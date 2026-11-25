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

## CPU power (Milestone 6)
- The CPU panel shows a watts line under the load bar. Compare it with another monitor you trust, idle and in a game; small differences from timing are normal.
- Settings, CPU row, untick **Show power**, Apply: the line disappears and the panel gets shorter. Tick it again to bring it back.
- On a PC that doesn't report power: no line, no error, and the setting is greyed out. If you have another PC, please try it and tell us the CPU, Windows version and what you saw.
## FPS widget (Milestone 5)
Needs "Performance Log Users". Turn FPS on in Settings. **Start with Minecraft single player**; don't try it with games that use anti-cheat unless you accept that its behaviour there is unknown.
1. Put the game in borderless windowed, in front, and set a frame cap you can check (for example 60, then 120).
2. The FPS panel should settle within about 3 seconds to a number close to the game's own counter (Minecraft: F3). Note both, a few times, at each cap.
3. Uncapped: does it still follow the game's counter? Does it stay steady?
4. Alt+Tab to the desktop: it should show `--` or the new front program's rate; back in the game it should recover.
5. Watch for stutter in the game, and for a tray warning about dropped frame events.
6. Report: game and graphics API, the cap, the game's counter and our number, and anything odd.

## Settings, edit mode and tray (Milestone 4)
The dialog and edit mode are covered by `scripts/settings-test.ps1` and `scripts/edit-test.ps1`. These are the parts only a person can check:

**Tray menu** (right-click the tray icon; it may be under the "^" overflow):
- It shows Show / hide overlay, Edit layout, Settings... and Quit, with your hotkeys in the first two labels.
- "Edit layout" has a check mark while edit mode is on.
- Clicking the menu's Settings... opens the dialog; Quit closes the app and saves the layout if you were editing.
- Clicking elsewhere closes the menu (it should not stay stuck open).

**Settings window** (look and feel):
- Controls look modern (themed), the text is readable at your display scaling, and nothing is cut off.
- Tab moves between fields in a sensible order, Enter presses OK, Esc cancels.
- Change the theme to Light and Midnight and press Apply: colours change at once, text stays readable.
- Type a colour override such as `#FF8800` for Accent: the progress bars change. Type `orange`: you get a message and nothing changes.
- Set a very small refresh (1) and a large one (30) for different widgets; the slow ones update less often.
- Change the show/hide hotkey, Apply: the new one works, the old one does not.
- Try a hotkey another program uses: you get a warning, the overlay keeps working.

**Click to finish editing:** with edit mode on, the screen dims. Dragging and clicking widgets keeps it on; clicking anywhere else (desktop, another window, the taskbar) ends it and saves, and that click does not also act on what was clicked. With the settings window open, clicking inside it should not end edit mode.

**Edit mode over a game** (borderless windowed):
- Ctrl+Alt+E while the game is running: the widgets get outlines and can be dragged; the game keeps running.
- Drag a widget onto a second monitor if you have one; press Ctrl+Alt+E again; restart the app: it comes back in the same place.
- With Now Playing set to "Hide when idle": close all media players, the panel disappears; start one, it appears. In edit mode it is shown so you can place it.

**Temperature units:** on an NVIDIA PC, switching to Fahrenheit changes the GPU panel's temperature to `°F`.

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
