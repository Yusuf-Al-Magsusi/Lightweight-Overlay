# Visual test of the CPU / memory / GPU widgets.
# Usage: scripts/stats-test.ps1 -OutDir <folder for screenshots>
# Needs: cargo build --workspace ; cargo build --release -p lwo
#  1. release build, real machine: CPU, memory and GPU panels show. Without an NVIDIA
#     driver the GPU panel has only load and VRAM (generic Windows counters).
#     (For real GPU load run `cargo run --example gpu_load -- 15` meanwhile.)
#  2. debug build with the fake nvml.dll (debug builds only honour LWO_NVML_PATH):
#     the GPU panel must appear and its numbers must change every second.
# Uses a temporary %APPDATA%. Takes about 25 seconds.
param([Parameter(Mandatory)] [string] $OutDir)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class S {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
}
"@
function Snap($name, $h) {
    $b = New-Object System.Drawing.Bitmap 360, $h
    $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0, 0, 0, 0, $b.Size)
    $b.Save((Join-Path $OutDir "$name.png")); $g.Dispose(); $b.Dispose()
}
function Quit-App($p) {
    [void][S]::PostMessage([S]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $p.WaitForExit(5000) | Out-Null
}
function Overlay-Height {
    $r = New-Object S+RECT
    [void][S]::GetWindowRect([S]::FindWindow("lightweight-overlay-overlay", [NullString]::Value), [ref]$r)
    return ($r.B - $r.T)
}

$real = $env:APPDATA
$env:APPDATA = Join-Path $env:TEMP "lwo-stats-test-$PID"
try {
    # 1. Release build on this machine
    $app = Start-Process "target\release\lwo.exe" -PassThru
    Start-Sleep -Seconds 4
    $h1 = Overlay-Height
    "release: process alive=$(-not $app.HasExited) overlay height=$h1"
    Snap "1-release-real-machine" ([math]::Min(900, $h1 + 60))
    Quit-App $app

    # 2. Debug build with fake NVML, values varying every second
    $env:LWO_NVML_PATH = (Resolve-Path "target\debug\fake_nvml.dll").Path
    $env:FAKE_NVML_VARY = "1"
    $app = Start-Process "target\debug\lwo.exe" -PassThru
    Start-Sleep -Seconds 4
    $h2 = Overlay-Height
    "debug+fake nvml: process alive=$(-not $app.HasExited) overlay height=$h2 (taller than release by the temperature/power row: $($h2 - $h1) px)"
    Snap "2-fake-nvml-a" ([math]::Min(900, $h2 + 60))
    Start-Sleep -Seconds 3
    Snap "3-fake-nvml-b" ([math]::Min(900, $h2 + 60))
    Quit-App $app
}
finally {
    $env:APPDATA = $real
    Remove-Item Env:LWO_NVML_PATH, Env:FAKE_NVML_VARY -ErrorAction SilentlyContinue
    Get-Process lwo -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item -Recurse -Force (Join-Path $env:TEMP "lwo-stats-test-$PID") -ErrorAction SilentlyContinue
}
