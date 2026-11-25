# End-to-end test of the FPS widget. Needs "Performance Log Users" (or admin).
# Usage: scripts/fps-test.ps1 -OutDir <screenshot folder>
# Needs: cargo build --release -p lwo ; cargo build --release --example present_rate
# Uses a temporary %APPDATA%. Briefly moves the mouse to bring the test window to the front,
# and presses Ctrl+Alt+O. Checks:
#   - the Windows tracing session exists only while the FPS widget is on screen
#   - nothing is left behind after Quit, and a killed run's leftover is cleared next start
#   - with a program presenting at 100 fps in front, a screenshot is saved to read the number
param([Parameter(Mandatory)] [string] $OutDir)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class FpsWin {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool CB(IntPtr h, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte v, byte s, uint f, UIntPtr e);
  [DllImport("user32.dll")] static extern bool EnumWindows(CB cb, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  public static RECT Panel(uint pid, string title) { RECT found = new RECT();
    EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p!=pid || !IsWindowVisible(h)) return true;
      var c = new StringBuilder(128); GetClassName(h,c,128); if(!c.ToString().EndsWith("-overlay")) return true;
      var t = new StringBuilder(128); GetWindowText(h,t,128); if(t.ToString()!=title) return true;
      GetWindowRect(h, out found); return true;}, IntPtr.Zero); return found; }
}
"@
$results = @()
function Check($name, $ok, $detail = "") { $script:results += [pscustomobject]@{ Check = $name; Result = $(if ($ok) { "PASS" } else { "FAIL" }); Detail = $detail } }
function Session-Count { @(logman query -ets 2>&1 | Select-String "lightweight-overlay-fps").Count }
function Hotkey { [FpsWin]::keybd_event(0x11,0,0,[UIntPtr]::Zero); [FpsWin]::keybd_event(0x12,0,0,[UIntPtr]::Zero); [FpsWin]::keybd_event(0x4F,0,0,[UIntPtr]::Zero); [FpsWin]::keybd_event(0x4F,0,2,[UIntPtr]::Zero); [FpsWin]::keybd_event(0x12,0,2,[UIntPtr]::Zero); [FpsWin]::keybd_event(0x11,0,2,[UIntPtr]::Zero); Start-Sleep -Milliseconds 1200 }
function Quit-App($p) { [void][FpsWin]::PostMessage([FpsWin]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero); $p.WaitForExit(8000) | Out-Null }
function Write-Config($fpsOn) {
    $dir = Join-Path $env:APPDATA "lightweight-overlay"; New-Item -ItemType Directory -Force $dir | Out-Null
    "[widgets.fps]`nenabled = $($fpsOn.ToString().ToLower())`n" | Set-Content (Join-Path $dir "config.toml")
}

$real = $env:APPDATA
$root = Join-Path $env:TEMP "lwo-fps-test-$PID"
$env:APPDATA = $root
try {
    Check "no tracing session before starting" ((Session-Count) -eq 0)

    # FPS off (the default): no session at all
    Write-Config $false
    $app = Start-Process "target\release\lwo.exe" -PassThru; Start-Sleep 4
    Check "FPS off: no tracing session and no FPS panel" (((Session-Count) -eq 0) -and ([FpsWin]::Panel([uint32]$app.Id, "FPS").R -eq 0))
    Quit-App $app

    # FPS on
    Write-Config $true
    $app = Start-Process "target\release\lwo.exe" -PassThru; Start-Sleep 4
    Check "FPS on: the panel is shown" ([FpsWin]::Panel([uint32]$app.Id, "FPS").R -gt 0)
    Check "FPS on: exactly one tracing session" ((Session-Count) -eq 1)

    Hotkey
    Check "hiding the overlay stops the session (lazy)" ((Session-Count) -eq 0)
    Hotkey
    Check "showing it again starts a session" ((Session-Count) -eq 1)

    # A program presenting at exactly 100 fps, brought to the front with a real click
    $game = Start-Process "target\release\examples\present_rate.exe" -ArgumentList "100","25" -PassThru -RedirectStandardOutput (Join-Path $OutDir "present_rate.txt")
    Start-Sleep -Seconds 2
    $game.Refresh(); $h = $game.MainWindowHandle
    $r = New-Object FpsWin+RECT; [void][FpsWin]::GetWindowRect($h, [ref]$r)
    [void][FpsWin]::SetCursorPos([int](($r.L + $r.R) / 2), [int](($r.T + $r.B) / 2)); Start-Sleep -Milliseconds 200
    [FpsWin]::mouse_event(2,0,0,0,[UIntPtr]::Zero); [FpsWin]::mouse_event(4,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 400
    $fg = 0; [void][FpsWin]::GetWindowThreadProcessId([FpsWin]::GetForegroundWindow(), [ref]$fg)
    Check "test program is the front window" ($fg -eq $game.Id) "front pid $fg, test pid $($game.Id)"
    Start-Sleep -Seconds 7
    $panel = [FpsWin]::Panel([uint32]$app.Id, "FPS")
    $b = New-Object System.Drawing.Bitmap ($panel.R - $panel.L), ($panel.B - $panel.T)
    $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($panel.L, $panel.T, 0, 0, $b.Size)
    $b.Save((Join-Path $OutDir "fps-panel-100.png")); $g.Dispose(); $b.Dispose()
    $b = New-Object System.Drawing.Bitmap 360, 700
    $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0, 0, 0, 0, $b.Size)
    $b.Save((Join-Path $OutDir "overlay-with-fps.png")); $g.Dispose(); $b.Dispose()
    $game.WaitForExit(25000) | Out-Null
    Check "test program really ran at 100 fps" ((Get-Content (Join-Path $OutDir "present_rate.txt") -Raw) -match "actual_fps=(99|100)\.")

    Quit-App $app
    Check "Quit stops the session (nothing left behind)" ((Session-Count) -eq 0)

    # A killed run leaves its session behind (Windows keeps it); the next start must clear it
    $app = Start-Process "target\release\lwo.exe" -PassThru; Start-Sleep 4
    Stop-Process -Id $app.Id -Force; Start-Sleep 1
    Check "after being killed, the session is left behind (as expected)" ((Session-Count) -eq 1)
    Write-Config $false
    $app = Start-Process "target\release\lwo.exe" -PassThru; Start-Sleep 4
    Check "the next start clears the leftover, even with FPS now off" ((Session-Count) -eq 0)
    Quit-App $app
}
catch {
    Write-Host "SCRIPT ERROR: $($_.Exception.Message)"; Write-Host $_.InvocationInfo.PositionMessage
    $results += [pscustomobject]@{ Check = "script ran to the end"; Result = "FAIL"; Detail = $_.Exception.Message }
}
finally {
    $env:APPDATA = $real
    Get-Process lwo, present_rate -ErrorAction SilentlyContinue | Stop-Process -Force
    logman stop "lightweight-overlay-fps-$((Get-Process -Id $PID).SessionId)" -ets 2>&1 | Out-Null
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
$results | Format-Table -AutoSize | Out-String -Width 200
if ($results | Where-Object Result -eq "FAIL") { exit 1 }
