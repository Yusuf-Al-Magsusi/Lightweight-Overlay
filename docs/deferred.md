# Deferred: to come back to

Things the owner chose to postpone. Nothing here is cancelled. Check this file when planning a milestone.

## CPU temperature (deferred in Milestone 6)

**Decision:** skip it for now, and add it later. CPU **power** shipped in Milestone 6; temperature did not.

**What blocks it** (details in `docs/decisions.md` D28 and `docs/sensor-investigation.md`):
- No driver-free way to read it was found. The one Windows temperature seen on the dev PC (16.9 °C) was clearly not the CPU.
- The owner's rule: the app must not rely on software the user already has, unless it downloads what it needs itself. So reading the user's own LibreHardwareMonitor is ruled out.
- The only route found is to download and install the **PawnIO** driver (GPL-2.0-or-later) plus a PawnIO module (LGPL-2.1), used by a small elevated helper. Those licenses are outside hard constraint 6, so it needs the owner's explicit exception first.

**Before building it, redo the research (it will be out of date):**
1. Is there now a driver-free source (Windows adding one, or a vendor user-mode library whose license we have **read** and is compatible)? AMD's Ryzen Master Monitoring SDK license could not be read (its page timed out); read it before considering it.
2. PawnIO: current license, whether its installer may be redistributed or only downloaded from its author, and who may load modules.
3. Anti-cheat: look again. As of this investigation, FACEIT was reported (2025) to block PawnIO and its author reported it fixed (2026), unconfirmed by FACEIT; nothing was found about Easy Anti-Cheat, BattlEye or Vanguard. Check the games the owner plays (Minecraft, Rocket League, Fortnite).
4. Which module reads the temperature on this CPU (Ryzen 7 7800X3D) and on Intel, and prove it works before building the rest.
5. Ask the owner for the license exception, plainly, before writing code.

**If approved, it must have** (hard constraints 3 and 4): off by default; a separate helper process that is the only thing asking for admin; clear warnings in the settings window and README, including anti-cheat; consent before anything downloads or installs; one pinned release verified by SHA-256 and signature; a clean off switch that stops our helper (and does not silently remove a driver other programs may use).

**Where it would plug in:** a new value in `Stats` and `Source`, shown on the CPU panel beside the power line, with the temperature unit setting that already exists (°C / °F). The CPU power code (`platform/windows/power.rs`) does not need to change.

## AMD GPU temperature and power (deferred earlier)

The generic GPU panel shows load and VRAM only on AMD and Intel (D16). A Windows per-adapter performance-data query is an untested idea for getting temperature without a vendor library; its license and whether the AMD driver fills it in both need checking first.
