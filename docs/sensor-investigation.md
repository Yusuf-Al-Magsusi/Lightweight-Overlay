# Milestone 6 investigation: CPU temperature and power

Status: **investigation done; waiting for the owner's decision. Nothing built, installed or changed on the PC.**

## Summary
1. **CPU package power can be read without our shipping or installing any driver, on the dev PC.** Windows' "Energy Meter" performance counters give it, with no admin rights. The driver behind them is Microsoft's own processor driver. This contradicts the assumption in hard constraint 3 that CPU wattage needs a kernel driver, at least on this hardware, so the owner has to decide how to treat it.
2. **CPU temperature is different.** I found no driver-free way to read it. Tools that show it use a kernel driver; LibreHardwareMonitor uses PawnIO.
3. **Anti-cheat:** the one anti-cheat I found evidence about is FACEIT, which blocked the PawnIO driver in 2025. The PawnIO author reports it fixed in 2026. I found **nothing** about Easy Anti-Cheat, BattlEye or Vanguard either way.

## Checked on the dev PC (read-only)
Windows 11, AMD Ryzen 7 7800X3D, not elevated.

| Check | Result |
|---|---|
| `\Energy Meter(rapl_package0_pkg)\Power` | readable without admin; milliwatts (see below) |
| Per-core channels (`..._coreN_core`) | present. They are parts of the package; adding them to it would count power twice |
| Provider of the Windows energy meter device | service `AmdPPM`, provider **Microsoft**, `cpu.inf` |
| `Thermal Zone Information` temperature | 16.9 °C: clearly not the CPU |
| ACPI thermal zone through WMI | access denied without admin |
| Drivers already installed (by other software, not by us) | `PawnIO` 2.2.0 (running), `AMDRyzenMasterDriverV32` (running), `RTCore64` from MSI Afterburner (running) |

**Units check.** Two readings about 7 s apart of the separate `Energy` and `Time` counters, converted as energy in picowatt-hours and time in milliseconds, gave 36.5 W idle, 72.1 W with all 16 threads busy, and 37.3 W after. The `Power` counter read 36,454, 72,372 and 36,756 at the same moments, so `Power` is in milliwatts. (Microsoft's driver interface documents time in 100 ns units; the performance counter evidently presents it differently. A first version of my check script also mixed up two variables whose names differed only in case; the figures above were worked out from the raw numbers it printed.) These readings were not compared with an outside power meter or another tool.

**How widely this works is not known.** Microsoft says drivers *can* implement the Energy Metering Interface from Windows 10 on, and does not say which hardware does, or that CPU package power is exposed. Community projects (not Microsoft) report that Windows 11 usually exposes CPU data, that Windows 10 often only does on devices with hardware meters, and that AMD coverage varies. So it must be treated as "shown where present, hidden otherwise". Only this one PC has been checked.

## What LibreHardwareMonitor uses
- LibreHardwareMonitor (MPL-2.0) says some sensors need administrator rights.
- It has moved from the WinRing0 driver to **PawnIO**: v0.9.6 (its latest release when checked) updates PawnIO modules and its installer. The release pages read did not state the exact release that switched. A third-party project reports that the change was made because Microsoft Defender flags WinRing0 as a vulnerable driver (CVE-2020-14979).
- **PawnIO** is a separately distributed kernel driver. Its source is **GPL-2.0-or-later** with an exception for programs that talk to it only through its device control interface; its modules repository is **LGPL-2.1** and includes AMD CPU modules (what each reads is not stated on the pages read). Permission to redistribute its installer is not stated.

## Anti-cheat: what the evidence actually says
No official statement from any anti-cheat vendor could be read (the FACEIT support article returned an error). Everything here is from public issue threads.

- September 2025: users reported FACEIT preventing PawnIO from loading. The PawnIO author wrote that the certificate used to sign the driver appeared to be the reason.
- February 2026: the author reported it fixed in PawnIO 2.1.0 with a new signature; Fan Control added support for it. In May 2026 the author wrote that it was solved. **FACEIT's own confirmation was not found.**
- A LibreHardwareMonitor collaborator wrote in February 2026 that it would not work with FACEIT at that time and that, as far as they knew, it caused no ban, only a failure to start.
- One user reported using it with Battlefield 6 for a month without problems. Another reported an unnamed anti-cheat complaining about `PawnIO.sys`.
- **Easy Anti-Cheat, BattlEye, Vanguard:** not mentioned in any page read. Absence of reports is not evidence of safety.

## AMD's Ryzen Master Monitoring SDK
Not usable for now: its license could not be read (AMD's page timed out repeatedly), and we only use software whose license has been read. AMD has also published security bulletins about this SDK.

## Options

**A. Power only, through Windows' energy meter, inside the main app (recommended first step).**
Add CPU package watts where Windows provides it, hidden quietly where it doesn't. No driver from us, no helper, no admin, no new dependency. Testable on the dev PC now.
*No temperature.*

**C. Temperature from a copy of LibreHardwareMonitor that the user runs themselves (recommended if temperature matters).**
LibreHardwareMonitor has an optional "Remote Web Server" that serves its sensors as JSON. The overlay, only when the user switches this on, would read that from this PC (`127.0.0.1`). We ship no driver and no helper and never ask for admin; the user chooses to install LibreHardwareMonitor (and PawnIO), run it as admin, and turn the server on. This is the "separate, opt-in process" of the plan, and the off switch is ours to turn off or theirs to close the program.
Costs: more setup for the user; a second program running (its memory and CPU would need measuring and are not part of our budget); parsing JSON, which needs either a small new dependency (`serde_json`, MIT/Apache-2.0) or our own parser. Cautions we would have to show: the server can be set to listen on all network interfaces, its data pages can be read by web pages (it allows any origin), and password protection is optional, so users should keep it on `127.0.0.1`. The anti-cheat risk is then that of LibreHardwareMonitor and PawnIO running at all, whether or not the overlay is open.

**B. Our own elevated helper that uses PawnIO.** Not recommended. Reading the sensor needs a PawnIO module; the published ones are LGPL-2.1, which is outside our allowed licenses (hard constraint 6; MPL-2.0 is allowed only for a helper). Reusing LibreHardwareMonitor's library (MPL-2.0) would make a .NET helper, still relying on those modules. It also means more code, an admin program of ours, and a secured pipe.

**D. No temperature** (A alone, or skip the milestone).

## Questions for the owner
1. May CPU **power** from Windows' own energy meter count as a driver-free source in the default app? (Constraint 3 currently lists CPU wattage as needing a driver. If yes, I would update that wording in `CLAUDE.md`.)
2. For **temperature**: C (read your own LibreHardwareMonitor), or none for now?

## Sources
- [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor), [its releases](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases) and [its web server source](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/master/LibreHardwareMonitor.Windows.Forms/Utilities/HttpServer.cs)
- [LibreHardwareMonitor discussion on anti-cheat](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/discussions/2195)
- [Fan Control issue: FACEIT blocking PawnIO](https://github.com/Rem0o/FanControl.Releases/issues/3516) and [PawnIO.Setup issue](https://github.com/namazso/PawnIO.Setup/issues/1)
- [PawnIO](https://github.com/namazso/PawnIO), [PawnIO modules](https://github.com/namazso/PawnIO.Modules), [pawnio.eu](https://pawnio.eu/)
- [Microsoft: Energy Meter Interface](https://learn.microsoft.com/en-us/windows-hardware/drivers/powermeter/energy-meter-interface) and [its measurement structure](https://learn.microsoft.com/en-us/windows/win32/api/emi/ns-emi-emi_channel_measurement_data)
- [A project's note on replacing WinRing0 with PawnIO](https://github.com/brunomourasoares/RMMonitor/pull/38)
- [AMD security bulletin on the Ryzen Master Monitoring SDK](https://www.amd.com/en/resources/product-security/bulletin/amd-sb-9004.html)
