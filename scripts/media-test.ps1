# Test of the Now Playing widget using a fake media player.
# Usage: scripts/media-test.ps1 -OutDir <folder for screenshots>
# Needs: cargo build --release -p lwo ; cargo build --example fake_media
# Close other media players first: Windows shows only its one "current" session, so a real
# player that is open would be shown instead of the fake one.
# Uses a temporary %APPDATA%. Takes about 40 seconds. The fake player script:
#   0 s playing (0:30 / 3:00), 10 s paused, 20 s playing a very long title, 30 s gone.
# Besides saving screenshots, it MEASURES the progress bar's fill (accent-coloured pixels in the
# Now Playing window), so a frozen time display is caught automatically:
#   - playing: the bar must grow between two captures 3 s apart
#   - paused: it must not move
#   - resumed after a pause: it must grow again (this is the case that once froze)
param(
    [Parameter(Mandatory)] [string] $OutDir,
    [string] $Exe = "target\release\lwo.exe"
)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class MediaWin {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool CB(IntPtr h, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool EnumWindows(CB cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  // Screen rectangle of a process's visible widget window with this title (each is titled with its name).
  public static RECT Panel(uint pid, string title) { RECT found = new RECT();
    EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p!=pid || !IsWindowVisible(h)) return true;
      var c = new StringBuilder(128); GetClassName(h,c,128); if(!c.ToString().EndsWith("-overlay")) return true;
      var t = new StringBuilder(128); GetWindowText(h,t,128); if(t.ToString()!=title) return true;
      GetWindowRect(h, out found); return true;}, IntPtr.Zero); return found; }
}
"@
$results = @()
function Check($name, $ok, $detail = "") { $script:results += [pscustomobject]@{ Check = $name; Result = $(if ($ok) { "PASS" } else { "FAIL" }); Detail = $detail } }
function Snap($name) {
    $b = New-Object System.Drawing.Bitmap 520, 320
    $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0, 0, 0, 0, $b.Size)
    $b.Save((Join-Path $OutDir "$name.png")); $g.Dispose(); $b.Dispose()
}
# Number of accent-green pixels inside the Now Playing window: proportional to how full the bar is.
function Bar-Fill($procId) {
    $r = [MediaWin]::Panel([uint32]$procId, "Now Playing")
    $w = $r.R - $r.L; $h = $r.B - $r.T
    if ($w -le 0 -or $h -le 0) { return -1 }
    $b = New-Object System.Drawing.Bitmap $w, $h
    $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($r.L, $r.T, 0, 0, $b.Size); $g.Dispose()
    $n = 0
    for ($y = 0; $y -lt $h; $y++) { for ($x = 0; $x -lt $w; $x++) { $c = $b.GetPixel($x, $y)
        if ([math]::Abs($c.R - 77) -lt 30 -and [math]::Abs($c.G - 204) -lt 30 -and [math]::Abs($c.B - 128) -lt 30) { $n++ } } }
    $b.Dispose(); return $n
}

$real = $env:APPDATA
$env:APPDATA = Join-Path $env:TEMP "lwo-media-test-$PID"
try {
    $fake = Start-Process "target\debug\examples\fake_media.exe" -PassThru -WindowStyle Hidden
    $t0 = Get-Date
    Start-Sleep -Seconds 1
    $app = Start-Process $Exe -PassThru
    function WaitUntil($sec) { $left = $sec - ((Get-Date) - $t0).TotalSeconds; if ($left -gt 0) { Start-Sleep -Seconds $left } }
    WaitUntil 5;  Snap "1-playing";        $fill1 = Bar-Fill $app.Id
    WaitUntil 9;  Snap "2-playing-later";  $fill2 = Bar-Fill $app.Id      # about 4 s later: the bar must have grown
    WaitUntil 14; Snap "3-paused";         $fill3 = Bar-Fill $app.Id
    WaitUntil 18; Snap "4-paused-later";   $fill4 = Bar-Fill $app.Id      # paused: must not have moved
    WaitUntil 22; Snap "5-long-title";     $fill5 = Bar-Fill $app.Id      # playing again after the pause (the case that froze)
    WaitUntil 26;                          $fill6 = Bar-Fill $app.Id      # and it must keep growing
    WaitUntil 34; Snap "6-session-gone"
    Check "Now Playing panel is on screen" ($fill1 -ge 0) "fill=$fill1"
    Check "playing: the progress bar grows over 4 s" ($fill2 -gt $fill1) "$fill1 -> $fill2 accent pixels"
    Check "paused: the progress bar does not move" ($fill4 -eq $fill3) "$fill3 -> $fill4 (if this fails while another media player is open, Windows may have switched to that player: close it and re-run)"
    Check "resumed after a pause: the bar is growing again" ($fill6 -gt $fill5) "$fill5 -> $fill6"
    [void][MediaWin]::PostMessage([MediaWin]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $app.WaitForExit(5000) | Out-Null
    Check "lwo exits cleanly" $app.HasExited
}
finally {
    $env:APPDATA = $real
    Get-Process lwo, fake_media -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item -Recurse -Force (Join-Path $env:TEMP "lwo-media-test-$PID") -ErrorAction SilentlyContinue
}
$results | Format-Table -AutoSize | Out-String -Width 200
if ($results | Where-Object Result -eq "FAIL") { exit 1 }
