# Smoke test for the overlay executable. Usage:
#   scripts/smoke-test.ps1 [-Exe target\release\lwo.exe] [-Screenshot out.png]
# Runs against a temporary %APPDATA%, so your real config is never touched.
# Briefly shows the overlay at the top-left of the screen and presses Ctrl+Alt+O.
param(
    [string] $Exe = "target\release\lwo.exe",
    [string] $Screenshot = ""
)
$ErrorActionPreference = "Stop"
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class W {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
}
"@

$results = @()
function Check($name, $ok, $detail = "") {
    $script:results += [pscustomobject]@{ Check = $name; Result = $(if ($ok) { "PASS" } else { "FAIL" }); Detail = $detail }
}
function Press-Hotkey {
    [W]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero); [W]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero)
    [W]::keybd_event(0x4F, 0, 0, [UIntPtr]::Zero); [W]::keybd_event(0x4F, 0, 2, [UIntPtr]::Zero)
    [W]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero); [W]::keybd_event(0x11, 0, 2, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 800
}
function Start-App($appdata) {
    $env:APPDATA = $appdata
    $p = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds 3
    return $p
}

$root = Join-Path $env:TEMP "lwo-smoke-$PID"
New-Item -ItemType Directory -Force $root | Out-Null
$realAppData = $env:APPDATA
try {
    # --- Normal run ---
    $p = Start-App $root
    Check "process stays running" (-not $p.HasExited)
    $cfg = Join-Path $root "lightweight-overlay\config.toml"
    Check "default config created" (Test-Path $cfg) $cfg

    $ov = [W]::FindWindow("lightweight-overlay-overlay", [NullString]::Value)
    $ctl = [W]::FindWindow("lightweight-overlay-control", [NullString]::Value)
    Check "overlay window exists and is visible" ($ov -ne [IntPtr]::Zero -and [W]::IsWindowVisible($ov))
    Check "control window exists" ($ctl -ne [IntPtr]::Zero)

    $ex = [W]::GetWindowLong($ov, -20)
    $want = 0x80000 -bor 0x20 -bor 0x8 -bor 0x80 -bor 0x8000000   # LAYERED TRANSPARENT TOPMOST TOOLWINDOW NOACTIVATE
    Check "window has all 5 overlay styles" (($ex -band $want) -eq $want) ("0x{0:X}" -f $ex)

    $r = New-Object W+RECT; [void][W]::GetWindowRect($ov, [ref]$r)
    $pt = New-Object W+POINT; $pt.X = [int](($r.L + $r.R) / 2); $pt.Y = [int](($r.T + $r.B) / 2)
    Check "click-through (hit test skips overlay)" ([W]::WindowFromPoint($pt) -ne $ov) "rect=($($r.L),$($r.T),$($r.R),$($r.B))"

    if ($Screenshot) {
        Add-Type -AssemblyName System.Drawing, System.Windows.Forms
        $bmp = New-Object System.Drawing.Bitmap 520, 220
        $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen(0, 0, 0, 0, $bmp.Size)
        $bmp.Save($Screenshot); $g.Dispose(); $bmp.Dispose()
    }

    Press-Hotkey
    $hidden = -not [W]::IsWindowVisible($ov)
    Check "hotkey Ctrl+Alt+O hides overlay" $hidden "(fails if another app owns this hotkey)"
    Press-Hotkey
    Check "hotkey again shows overlay" ([W]::IsWindowVisible($ov))

    # --- Second instance exits quietly ---
    $p2 = Start-Process -FilePath $Exe -PassThru
    $p2.WaitForExit(5000) | Out-Null
    Check "second instance exits immediately" $p2.HasExited
    Check "first instance unaffected" (-not $p.HasExited)

    # --- Quit via the tray menu command (ID_QUIT = 2) ---
    [void][W]::PostMessage($ctl, 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $p.WaitForExit(5000) | Out-Null
    Check "Quit command exits the app" $p.HasExited
    Check "overlay window gone after quit" ([W]::FindWindow("lightweight-overlay-overlay", [NullString]::Value) -eq [IntPtr]::Zero)

    # --- Invalid config: still runs, file untouched ---
    Set-Content -Path $cfg -Value "this is = not [valid toml" -Encoding utf8
    $before = Get-Content $cfg -Raw
    $p3 = Start-App $root
    Check "runs with invalid config" (-not $p3.HasExited)
    Check "invalid config left untouched" ((Get-Content $cfg -Raw) -eq $before)
    [void][W]::PostMessage([W]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $p3.WaitForExit(5000) | Out-Null

    # --- start_visible = false ---
    Set-Content -Path $cfg -Value "[overlay]`nstart_visible = false`n" -Encoding utf8
    $p4 = Start-App $root
    $ov4 = [W]::FindWindow("lightweight-overlay-overlay", [NullString]::Value)
    Check "start_visible=false starts hidden" (-not [W]::IsWindowVisible($ov4))
    [void][W]::PostMessage([W]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $p4.WaitForExit(5000) | Out-Null
}
finally {
    $env:APPDATA = $realAppData
    Get-Process lwo -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
$results | Format-Table -AutoSize
if ($results | Where-Object Result -eq "FAIL") { exit 1 }
