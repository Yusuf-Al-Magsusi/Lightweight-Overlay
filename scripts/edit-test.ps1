# Edit-mode test: drag a widget with the real mouse, check the layout is saved and restored.
# Usage: scripts/edit-test.ps1 [-Exe target\release\lwo.exe] [-OutDir <screenshot folder>]
# Uses a temporary %APPDATA%. Briefly takes over the mouse pointer (about 5 seconds).
param(
    [string] $Exe = "target\release\lwo.exe",
    [string] $OutDir = ""
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Collections.Generic; using System.Text;
public static class E {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool P(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(P cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte v, byte s, uint f, UIntPtr e);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  public static IntPtr[] Panels(uint pid) { var r = new List<IntPtr>();
    EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p==pid && IsWindowVisible(h)){ var sb=new StringBuilder(256); GetClassName(h,sb,256); if(sb.ToString().EndsWith("-overlay")) r.Add(h);} return true;}, IntPtr.Zero); return r.ToArray(); }
}
"@
$results = @()
function Check($name, $ok, $detail = "") { $script:results += [pscustomobject]@{ Check = $name; Result = $(if ($ok) { "PASS" } else { "FAIL" }); Detail = $detail } }
function Hotkey($vk) {   # Ctrl+Alt+<vk>
    [E]::keybd_event(0x11,0,0,[UIntPtr]::Zero); [E]::keybd_event(0x12,0,0,[UIntPtr]::Zero)
    [E]::keybd_event($vk,0,0,[UIntPtr]::Zero); [E]::keybd_event($vk,0,2,[UIntPtr]::Zero)
    [E]::keybd_event(0x12,0,2,[UIntPtr]::Zero); [E]::keybd_event(0x11,0,2,[UIntPtr]::Zero)
    Start-Sleep -Milliseconds 900
}
function Rects($procId) { [E]::Panels([uint32]$procId) | ForEach-Object { $r = New-Object E+RECT; [void][E]::GetWindowRect($_, [ref]$r); [pscustomobject]@{ H = $_; L = $r.L; T = $r.T; W = $r.R - $r.L; Ht = $r.B - $r.T; Ex = [E]::GetWindowLong($_, -20) } } }
function Snap($name) { if ($OutDir) { New-Item -ItemType Directory -Force $OutDir | Out-Null; $b = New-Object System.Drawing.Bitmap 700, 700; $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0,0,0,0,$b.Size); $b.Save((Join-Path $OutDir "$name.png")); $g.Dispose(); $b.Dispose() } }

$real = $env:APPDATA
$root = Join-Path $env:TEMP "lwo-edit-test-$PID"
$env:APPDATA = $root
$cfg = Join-Path $root "lightweight-overlay\config.toml"
try {
    $p = Start-Process $Exe -PassThru; Start-Sleep 4
    $before = @(Rects $p.Id)
    Check "one window per widget (4 or 5 expected)" ($before.Count -ge 4) "found $($before.Count)"
    $TRANSPARENT = 0x20
    Check "normal mode: all windows click-through" (@($before | Where-Object { -not ($_.Ex -band $TRANSPARENT) }).Count -eq 0)

    Hotkey 0x45   # E: edit mode on
    $edit = @(Rects $p.Id)
    Check "edit mode: windows are no longer click-through" (@($edit | Where-Object { $_.Ex -band $TRANSPARENT }).Count -eq 0)
    Snap "1-edit-mode"

    # Drag the first panel 300 px right and 200 px down with the real mouse.
    $target = $edit | Sort-Object T | Select-Object -First 1
    $sx = $target.L + [int]($target.W / 2); $sy = $target.T + [int]($target.Ht / 2)
    [void][E]::SetCursorPos($sx, $sy); Start-Sleep -Milliseconds 300
    [E]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 300     # left down
    foreach ($i in 1..20) { [void][E]::SetCursorPos($sx + 15 * $i, $sy + 10 * $i); Start-Sleep -Milliseconds 30 }
    [E]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 600     # left up
    $moved = @(Rects $p.Id) | Where-Object { $_.H -eq $target.H }
    $dx = $moved.L - $target.L; $dy = $moved.T - $target.T
    Check "dragging moved the window" ([math]::Abs($dx - 300) -le 6 -and [math]::Abs($dy - 200) -le 6) "moved by ($dx, $dy), wanted about (300, 200)"
    Snap "2-after-drag"

    Hotkey 0x45   # E: edit mode off, saves layout
    $after = @(Rects $p.Id)
    Check "leaving edit mode: windows are click-through again" (@($after | Where-Object { -not ($_.Ex -band $TRANSPARENT) }).Count -eq 0)
    $text = if (Test-Path $cfg) { Get-Content $cfg -Raw } else { "" }
    Check "layout saved to config.toml" ($text -match "\[layout\.") "$(([regex]::Matches($text, '\[layout\.')).Count) entries"
    $moved2 = $after | Where-Object { $_.H -eq $target.H }
    Check "window stayed where it was dropped" ($moved2.L -eq $moved.L -and $moved2.T -eq $moved.T)

    # Quit, start again: positions must be restored from the file.
    [void][E]::PostMessage([E]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $p.WaitForExit(5000) | Out-Null
    $p = Start-Process $Exe -PassThru; Start-Sleep 4
    $restored = @(Rects $p.Id)
    $same = 0
    foreach ($m in @(@($after) | Sort-Object T)) { if (@($restored | Where-Object { [math]::Abs($_.L - $m.L) -le 2 -and [math]::Abs($_.T - $m.T) -le 2 -and $_.W -eq $m.W }).Count -ge 1) { $same++ } }
    Check "after restart every widget is back where it was" ($same -eq @($after).Count) "$same of $(@($after).Count) matched"
    Snap "3-after-restart"
    [void][E]::PostMessage([E]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $p.WaitForExit(5000) | Out-Null
}
finally {
    $env:APPDATA = $real
    Get-Process lwo -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
$results | Format-Table -AutoSize | Out-String -Width 200
if ($results | Where-Object Result -eq "FAIL") { exit 1 }
