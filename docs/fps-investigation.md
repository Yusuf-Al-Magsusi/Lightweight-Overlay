# Milestone 5 investigation: what an FPS widget needs

Status: **investigation done; waiting for the owner's decision before the widget is built.**

## Summary
- The only practical way to read another program's frame rate **without touching it** is ETW (Event Tracing for Windows), the approach PresentMon uses.
- ETW needs a privilege: **Administrator, or membership of the local "Performance Log Users" group** (or a system service). A normal account outside that group gets "access denied".
- Our hard constraint 4 says the main app never needs admin rights. It does **not** need admin if the user is in "Performance Log Users", but **most accounts are not in that group** (general Windows knowledge; I did not measure this), and adding yourself needs admin once, then signing out and in.
- Measured on the dev PC (whose account is already in the group): a non-elevated process can start the session, receive other programs' frame events, and lose none at idle load, for a negligible cost.

## Official rules (Microsoft Learn)
- `StartTrace` and `OpenTrace` (real-time): allowed for administrators, members of Performance Log Users, and services running as LocalSystem, LocalService or NetworkService. Others get `ERROR_ACCESS_DENIED`.
- There is a system-wide limit on tracing sessions (usually 64). A real-time session can outlive a crashed program, so we must stop ours on exit **and** clear a leftover one with our name on start.
- Sources: [StartTraceW](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-starttracew), [OpenTraceW](https://learn.microsoft.com/en-us/windows/win32/api/evntrace/nf-evntrace-opentracew).

## PresentMon
- MIT-licensed (Intel; the session source file carries an NVIDIA copyright and an MIT SPDX tag). [Repository](https://github.com/GameTechDev/PresentMon).
- Needs the same privilege: its README says the user must be in Performance Log Users, and its console app has `--restart_as_admin`.
- Uses providers including DXGI, D3D9, DxgKrnl, Win32k and Dwm-Core, filters by event id, and records lost events. Its README says it works with DirectX, OpenGL and Vulkan, but the pages read did not say which events each needs.
- Bundling it would **not** reduce the privilege needed. It would add a second program and output parsing. I did not check its download size.

## What I measured on the dev PC
Windows 11, non-elevated shell, account in Performance Log Users (enabled in the normal token; Administrators is "deny only"). I don't know what added the account to the group (it may have been the CapFrameX or PresentMon install, but that is a guess). Tool: `crates/lwo/examples/etw_probe.rs`.

| Providers | Events received | Lost | Probe CPU | Frame events seen |
|---|---|---|---|---|
| DXGI + D3D9 | about 395/s | 0 | about 0.2% of one core or less | DXGI id 42 (present start), paired 1:1 with id 43 |
| plus DxgKrnl, filtered to ids 168 and 184 | about 396/s | 0 | below the measurement resolution | DxgKrnl id 184 matched DXGI id 42 one for one |

Limits of these numbers: the only program drawing frames was a desktop app at about 2 frames per second. **No game was running**, so load, event loss under load, OpenGL and Vulkan games, and accuracy against a known FPS are all untested.

Other findings:
- In one run the presenting process was not marked as owning the foreground window. A likely reason is that multi-process apps draw from a helper process, but I haven't confirmed it. A widget that shows "the game's FPS" by foreground window must be checked against real games.
- The probe's `--names` option opens each presenting process with a limited query right to read its file name. It is off by default, and the real widget **must not open the game's process at all** (hard constraint 2): it only needs the process id from the foreground window and from the events.
- Your earlier PresentMon error ("ETW events were lost", 554585) is not explained yet. PresentMon enables many more providers than the probe. The probe's two filtered kernel events lost nothing at idle, but that says nothing about load.

## Game test: Java Minecraft (run by the owner)
Minecraft 1.21.1 (NeoForge, Sodium, OpenGL 4.6) on the RX 9070 XT, vsync with a 120 FPS limit. Command: `etw_probe -- 15 --dxgkrnl --names`.

| Result | Value |
|---|---|
| Minecraft (`java.exe`), DXGI id 42 | 116.9 frames/s (15 s average) |
| Minecraft (`java.exe`), DxgKrnl id 184 | 116.9 frames/s, the same count |
| Minecraft's F3 screen (a screenshot, one moment) | 109 fps, limit 120 |
| Events received | 22,476 (about 1,500/s) |
| Events lost / real-time buffers lost | 0 / 0 |
| Probe CPU | below the measurement resolution (shown as 0.00 s) |

What it shows:
- Java Minecraft's frames are visible, and **events were not lost while a game was running**.
- Although Minecraft uses OpenGL, its frames appeared in the plain DXGI provider on this AMD driver. That may not hold for other drivers or games, so the DxgKrnl fallback stays in the design.
- The two sources counted identically, which confirms they must not be added together.
- The rate is **consistent with** F3, but this is not an accuracy proof: F3 is one instant, the probe is a 15 s average, they were not read at the same moment, and the rate was moving under a 120 limit. Accuracy still needs the known-rate test program.
- The foreground mark landed on `Photos.exe`, not Minecraft. The probe reads the foreground window once, **at the end** of the capture, so most likely another window was in front by then. Picking the game by foreground window is therefore still **unverified**.
- DXGI sends many other events for each frame (about 1,500 events/s in total here). The widget should filter DXGI to the present event only, as the probe already does for DxgKrnl.

Still untested: Rocket League and Fortnite (anti-cheat), frame rates well above 120, long sessions, other GPU vendors, and an account outside Performance Log Users.

## Anti-cheat
**None of the pages read says how anti-cheat systems treat ETW consumers.** What can be said: ETW is a Windows feature where we read an event stream the OS produces; we load nothing into the game, hook nothing and (in the intended design) open no handle to it. That is not a guarantee, and the README must keep saying the overlay is use-at-your-own-risk.

## Security note about the group
Membership of Performance Log Users is a real privilege: a program running as that user can start system-wide traces, which can reveal activity of other programs. We should say this plainly wherever we tell users to join it.

## Options
**A. Read ETW directly from the main app, never elevating (recommended).**
Works for accounts already in the group. Otherwise the FPS widget stays off and the settings window explains, once, what it needs and what the group allows. Pure Rust through the `windows` crate we already use; no new program; the session runs only while the FPS widget is visible.
Cost: most users must do a one-time admin step and sign out and in, or go without FPS.

**B. A separate, opt-in elevated helper** that runs the ETW session and sends only FPS numbers to the overlay over a local pipe.
Works for any admin user with a UAC prompt at each start, with no group change. It needs your permission, because constraint 4 says only the optional *sensor* helper may ask for elevation. More code, plus a local pipe that must be secured. It could be combined with the Milestone 6 sensor helper, so there is only one elevated program.

**C. Bundle PresentMon as an optional helper.** Same privilege as A, plus a second program and output parsing. Not recommended.

**D. Drop the FPS widget** for v0.1.

Not options: injecting into the game or hooking its graphics API (forbidden by constraint 2). Windows' compositor timing only reports the desktop's refresh, not a game's frame rate, and I know of no per-game FPS performance counter.

## Proposed design if A is chosen
- Providers: DXGI and D3D9 present events; DxgKrnl filtered to the present and flip ids, so OpenGL and Vulkan games are covered. If a process produces both, count DXGI/D3D9 only, so frames aren't counted twice.
- Which process: the one owning the foreground window, checked against real games first (see above).
- Show frames per second over the last second, updated once per second. 1% low is a possible later option.
- Lazy: the session exists only while the FPS widget is visible; stopped on hide and quit; a leftover is cleared on start.
- If access is denied, the widget is hidden and the settings window explains why, without error pop-ups.
- Report events lost, so we notice if it happens under load.
- Test with a small program that presents at a known rate, then compare with Minecraft's F3 screen.

## How the Minecraft test was run (can be repeated)
Do this in single player, in a borderless window. Don't use `--names` with games that have anti-cheat, and don't run it with Rocket League or Fortnite yet.

```
cargo run --release --example etw_probe -- 15 --dxgkrnl --names
```

Click into the game within a few seconds and keep it in front. Send back the whole output, and the FPS shown on Minecraft's F3 screen. This answers whether Java Minecraft's frames are visible, which event shows them, whether they match F3, and whether events are lost under load.
