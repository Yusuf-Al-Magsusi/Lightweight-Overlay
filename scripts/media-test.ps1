# Visual test of the Now Playing widget using a fake media player.
# Usage: scripts/media-test.ps1 -OutDir <folder for screenshots>
# Needs: cargo build --release -p lwo ; cargo build --example fake_media
# Close other media players first: Windows shows only its one "current" session, so a real
# player that is open would be shown instead of the fake one.
# Uses a temporary %APPDATA%. Takes about 35 seconds. The fake player script:
#   0 s playing (0:30 / 3:00), 10 s paused, 20 s playing a very long title, 30 s gone.
param([Parameter(Mandatory)] [string] $OutDir)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class M {
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
}
"@
function Snap($name) {
    $b = New-Object System.Drawing.Bitmap 520, 320
    $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0, 0, 0, 0, $b.Size)
    $b.Save((Join-Path $OutDir "$name.png")); $g.Dispose(); $b.Dispose()
}

$real = $env:APPDATA
$env:APPDATA = Join-Path $env:TEMP "lwo-media-test-$PID"
try {
    $fake = Start-Process "target\debug\examples\fake_media.exe" -PassThru -WindowStyle Hidden
    $t0 = Get-Date
    Start-Sleep -Seconds 1
    $app = Start-Process "target\release\lwo.exe" -PassThru
    function WaitUntil($sec) { $left = $sec - ((Get-Date) - $t0).TotalSeconds; if ($left -gt 0) { Start-Sleep -Seconds $left } }
    WaitUntil 5;  Snap "1-playing"
    WaitUntil 8;  Snap "2-playing-later"      # position should have advanced about 3 s
    WaitUntil 14; Snap "3-paused"
    WaitUntil 18; Snap "4-paused-later"       # position must not have moved
    WaitUntil 25; Snap "5-long-title"
    WaitUntil 34; Snap "6-session-gone"
    [void][M]::PostMessage([M]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $app.WaitForExit(5000) | Out-Null
    "lwo exited cleanly: $($app.HasExited)"
}
finally {
    $env:APPDATA = $real
    Get-Process lwo, fake_media -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item -Recurse -Force (Join-Path $env:TEMP "lwo-media-test-$PID") -ErrorAction SilentlyContinue
}
