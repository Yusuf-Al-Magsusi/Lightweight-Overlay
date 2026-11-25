# Settings-dialog test: opens the real dialog from the tray command, fills it in through
# Win32 messages, applies, and checks the overlay and config.toml.
# Usage: scripts/settings-test.ps1 [-Exe target\release\lwo.exe] [-OutDir <screenshot folder>]
# Uses a temporary %APPDATA%. Shows the dialog and the overlay for about 30 seconds.
param(
    [string] $Exe = "target\release\lwo.exe",
    [string] $OutDir = ""
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public static class S {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool CB(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(CB cb, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr p, CB cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll", EntryPoint="SendMessageW")] public static extern IntPtr Send(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll", EntryPoint="SendMessageW", CharSet=CharSet.Unicode)] public static extern IntPtr SendText(IntPtr h, uint m, IntPtr w, string l);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetAncestor(IntPtr h, uint flags);
  [DllImport("user32.dll")] public static extern void keybd_event(byte v, byte s, uint f, UIntPtr e);
  public class Child { public IntPtr H; public string Cls; public string Text; public int Style; public int L, T; }
  public static List<Child> Children(IntPtr parent) { var r = new List<Child>();
    EnumChildWindows(parent, (h,l)=>{ var c=new StringBuilder(64); GetClassName(h,c,64); var t=new StringBuilder(256); GetWindowText(h,t,256); RECT rc; GetWindowRect(h,out rc);
      r.Add(new Child{H=h,Cls=c.ToString(),Text=t.ToString(),Style=GetWindowLong(h,-16),L=rc.L,T=rc.T}); return true;}, IntPtr.Zero); return r; }
  public static List<IntPtr> TopLevel(uint pid, string classSuffix, string exactClass) { var r = new List<IntPtr>();
    EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p!=pid || !IsWindowVisible(h)) return true;
      var c=new StringBuilder(128); GetClassName(h,c,128); var cs=c.ToString();
      if((classSuffix!=null && cs.EndsWith(classSuffix)) || (exactClass!=null && cs==exactClass)) r.Add(h); return true;}, IntPtr.Zero); return r; }
}
"@
$results = @()
function Check($name, $ok, $detail = "") { $script:results += [pscustomobject]@{ Check = $name; Result = $(if ($ok) { "PASS" } else { "FAIL" }); Detail = $detail } }
function Snap($name, $rect) {
    if (-not $OutDir) { return }
    New-Item -ItemType Directory -Force $OutDir | Out-Null
    $w = [math]::Max(1, $rect.R - $rect.L); $h = [math]::Max(1, $rect.B - $rect.T)
    $b = New-Object System.Drawing.Bitmap $w, $h; $g = [System.Drawing.Graphics]::FromImage($b)
    $g.CopyFromScreen($rect.L, $rect.T, 0, 0, $b.Size); $b.Save((Join-Path $OutDir "$name.png")); $g.Dispose(); $b.Dispose()
}
function Screen-Rect($l, $t, $r, $b) { $x = New-Object S+RECT; $x.L = $l; $x.T = $t; $x.R = $r; $x.B = $b; $x }
function Control-Window { [S]::FindWindow("lightweight-overlay-control", [NullString]::Value) }
function Hotkey($vk) { [S]::keybd_event(0x11,0,0,[UIntPtr]::Zero); [S]::keybd_event(0x12,0,0,[UIntPtr]::Zero); [S]::keybd_event($vk,0,0,[UIntPtr]::Zero); [S]::keybd_event($vk,0,2,[UIntPtr]::Zero); [S]::keybd_event(0x12,0,2,[UIntPtr]::Zero); [S]::keybd_event(0x11,0,2,[UIntPtr]::Zero); Start-Sleep -Milliseconds 900 }
function Panels($procId) { @([S]::TopLevel([uint32]$procId, "-overlay", [NullString]::Value) | ForEach-Object { $r = New-Object S+RECT; [void][S]::GetWindowRect($_, [ref]$r); [pscustomobject]@{ H = $_; W = $r.R - $r.L; Ht = $r.B - $r.T; L = $r.L; T = $r.T } }) }
function Dialog($procId) { $d = @([S]::TopLevel([uint32]$procId, [NullString]::Value, "lightweight-overlay-settings")); if ($d.Count) { $d[0] } else { [IntPtr]::Zero } }
function Parts($dlg) {
    $kids = [S]::Children($dlg)
    $edits = @($kids | Where-Object { $_.Cls -eq "Edit" } | Sort-Object T, L)
    $checks = @($kids | Where-Object { $_.Cls -eq "Button" -and (($_.Style -band 0xF) -eq 3) } | Sort-Object T, L)
    $combos = @($kids | Where-Object { $_.Cls -eq "ComboBox" } | Sort-Object T, L)
    $btn = @{}; foreach ($k in ($kids | Where-Object { $_.Cls -eq "Button" -and (($_.Style -band 0xF) -le 1) })) { $btn[$k.Text] = $k.H }
    [pscustomobject]@{ Edits = $edits; Checks = $checks; Combos = $combos; Buttons = $btn }
}
function Set-Text($c, $text) { [void][S]::SendText($c.H, 0x000C, [IntPtr]::Zero, $text) }   # WM_SETTEXT
function Read-Edit($c) { # WM_GETTEXT via a managed buffer
    $len = [S]::Send($c.H, 0x000E, [IntPtr]::Zero, [IntPtr]::Zero).ToInt32()
    $mem = [System.Runtime.InteropServices.Marshal]::AllocHGlobal(($len + 1) * 2)
    try { [void][S]::Send($c.H, 0x000D, [IntPtr]($len + 1), $mem); [System.Runtime.InteropServices.Marshal]::PtrToStringUni($mem) } finally { [System.Runtime.InteropServices.Marshal]::FreeHGlobal($mem) }
}
function Set-Check($c, $on) { [void][S]::Send($c.H, 0x00F1, [IntPtr][int]$on, [IntPtr]::Zero) }   # BM_SETCHECK
function Set-Combo($c, $i) { [void][S]::Send($c.H, 0x014E, [IntPtr]$i, [IntPtr]::Zero) }          # CB_SETCURSEL
function Click-Async($h) { [void][S]::PostMessage($h, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep -Milliseconds 700 }   # BM_CLICK
function Close-MessageBox($procId) {
    $boxes = @([S]::TopLevel([uint32]$procId, [NullString]::Value, "#32770")); if (-not $boxes.Count) { return $null }
    $kids = [S]::Children($boxes[0]); $text = ($kids | Where-Object { $_.Cls -eq "Static" -and $_.Text } | ForEach-Object { $_.Text }) -join " "
    [void][S]::PostMessage($boxes[0], 0x0010, [IntPtr]::Zero, [IntPtr]::Zero); Start-Sleep -Milliseconds 400   # WM_CLOSE
    $text
}

$real = $env:APPDATA
$root = Join-Path $env:TEMP "lwo-settings-test-$PID"
$env:APPDATA = $root
$cfg = Join-Path $root "lightweight-overlay\config.toml"
try {
    $p = Start-Process $Exe -PassThru; Start-Sleep 4
    $before = Panels $p.Id
    Check "overlay running with several panels" ($before.Count -ge 4) "found $($before.Count)"

    # Open the dialog through the tray menu's command (ID_SETTINGS = 4)
    [void][S]::PostMessage((Control-Window), 0x0111, [IntPtr]4, [IntPtr]::Zero); Start-Sleep -Milliseconds 900
    $dlg = Dialog $p.Id
    Check "Settings opens a dialog" ($dlg -ne [IntPtr]::Zero)
    $parts = Parts $dlg
    if ($env:LWO_DEBUG) { Write-Host "dialog handle: $dlg"; ([S]::Children($dlg) | Select-Object Cls, Text, Style | Format-Table | Out-String) | Write-Host }
    Check "dialog has the expected controls (14 edits, 11 check boxes, 2 drop-downs)" ($parts.Edits.Count -eq 14 -and $parts.Checks.Count -eq 11 -and $parts.Combos.Count -eq 2) "$($parts.Edits.Count) / $($parts.Checks.Count) / $($parts.Combos.Count)"
    $dr = New-Object S+RECT; [void][S]::GetWindowRect($dlg, [ref]$dr); Snap "1-dialog-defaults" $dr
    Check "fields start with the saved values" ((Read-Edit $parts.Edits[6]) -eq "100" -and (Read-Edit $parts.Edits[8]) -eq "Segoe UI" -and (Read-Edit $parts.Edits[12]) -eq "Ctrl+Alt+O") "scale '$(Read-Edit $parts.Edits[6])' font '$(Read-Edit $parts.Edits[8])' hotkey '$(Read-Edit $parts.Edits[12])'"

    # Second dialog request must not open another one
    [void][S]::PostMessage((Control-Window), 0x0111, [IntPtr]4, [IntPtr]::Zero); Start-Sleep -Milliseconds 500
    Check "asking again does not open a second dialog" (@([S]::TopLevel([uint32]$p.Id, [NullString]::Value, "lightweight-overlay-settings")).Count -eq 1)

    # --- invalid number: message box, nothing saved
    $cfgBefore = if (Test-Path $cfg) { Get-Content $cfg -Raw } else { "" }
    Set-Text $parts.Edits[6] "5"
    Click-Async $parts.Buttons["Apply"]
    $msg = Close-MessageBox $p.Id
    Check "an invalid scale is refused with a message" ($msg -match "Scale") $msg
    Check "...and nothing was saved or changed" (((Get-Content $cfg -Raw) -eq $cfgBefore) -and ((Panels $p.Id).Count -eq $before.Count))
    Set-Text $parts.Edits[6] "100"

    # --- invalid hotkey: refused by the config validation
    Set-Text $parts.Edits[12] "O"
    Click-Async $parts.Buttons["Apply"]
    $msg = Close-MessageBox $p.Id
    Check "a hotkey without a modifier is refused" ($msg -match "hotkey") $msg
    Set-Text $parts.Edits[12] "Ctrl+Alt+P"

    # --- real change: clock off, light theme, 150 % scale, 2 s CPU refresh, new hotkey
    $cpuBefore = ($before | Sort-Object T)[2]
    Set-Check $parts.Checks[0] $false     # clock off
    Set-Combo $parts.Combos[0] 1          # Light
    Set-Text $parts.Edits[6] "150"        # scale
    Set-Text $parts.Edits[2] "2"          # CPU refresh seconds
    Click-Async $parts.Buttons["Apply"]
    $after = Panels $p.Id
    Check "Apply took effect live: clock panel gone" ($after.Count -eq $before.Count - 1) "$($before.Count) -> $($after.Count) panels"
    $widestBefore = ($before | Measure-Object W -Maximum).Maximum; $widestAfter = ($after | Measure-Object W -Maximum).Maximum
    Check "...and the scale applied (widest panel about 1.5x)" ([math]::Abs($widestAfter / $widestBefore - 1.5) -lt 0.05) "$widestBefore -> $widestAfter px"
    $text = Get-Content $cfg -Raw
    Check "config.toml was written with the new values" (($text -match 'theme = "light"') -and ($text -match "scale = 1\.5") -and ($text -match "(?s)\[widgets\.clock\].*?enabled = false") -and ($text -match "(?s)\[widgets\.cpu\].*?refresh_seconds = 2"))
    Snap "2-light-theme-150pct" (Screen-Rect 0 0 700 900)

    # the new hotkey works and the old one no longer does
    Hotkey 0x4F   # Ctrl+Alt+O (old)
    Check "the old show/hide hotkey no longer works" ((Panels $p.Id).Count -eq $after.Count)
    Hotkey 0x50   # Ctrl+Alt+P (new)
    Check "the new show/hide hotkey works immediately" ((Panels $p.Id).Count -eq 0)
    Hotkey 0x50
    Check "...and shows the overlay again" ((Panels $p.Id).Count -eq $after.Count)

    # Applying while in edit mode must leave edit mode (windows click-through again)
    Hotkey 0x45   # Ctrl+Alt+E: edit mode on
    $TRANSPARENT = 0x20
    $editing = @([S]::TopLevel([uint32]$p.Id, "-overlay", [NullString]::Value) | Where-Object { -not ([S]::GetWindowLong($_, -20) -band $TRANSPARENT) }).Count
    Check "edit mode can be entered while the dialog is open" ($editing -gt 0) "$editing editable windows"
    $dlgNow = Dialog $p.Id
    $wr = New-Object S+RECT; [void][S]::GetWindowRect($dlgNow, [ref]$wr)
    $mid = New-Object S+POINT; $mid.X = [int](($wr.L + $wr.R) / 2); $mid.Y = [int](($wr.T + $wr.B) / 2)
    $top = [S]::GetAncestor([S]::WindowFromPoint($mid), 2)   # GA_ROOT
    Check "the settings window stays above the edit-mode backdrop" ($top -eq $dlgNow) "window at its centre: $top, dialog: $dlgNow"
    $partsNow = Parts $dlgNow
    Click-Async $partsNow.Buttons["Apply"]
    $stillEditable = @([S]::TopLevel([uint32]$p.Id, "-overlay", [NullString]::Value) | Where-Object { -not ([S]::GetWindowLong($_, -20) -band $TRANSPARENT) }).Count
    Check "Apply while editing ends edit mode and keeps the layout" ($stillEditable -eq 0 -and ((Get-Content $cfg -Raw) -match "\[layout\.")) "$stillEditable still editable"
    # Dialog shows what is in effect; OK closes it
    $parts = Parts (Dialog $p.Id)
    Check "dialog still open after Apply and shows the applied values" ((Read-Edit $parts.Edits[6]) -eq "150" -and (Read-Edit $parts.Edits[12]) -eq "Ctrl+Alt+P")
    Click-Async $parts.Buttons["OK"]
    Check "OK closes the dialog" ((Dialog $p.Id) -eq [IntPtr]::Zero)

    # Reset layout button path and Cancel path: reopen, press Cancel
    [void][S]::PostMessage((Control-Window), 0x0111, [IntPtr]4, [IntPtr]::Zero); Start-Sleep -Milliseconds 800
    $parts = Parts (Dialog $p.Id)
    Set-Text $parts.Edits[6] "300"
    Click-Async $parts.Buttons["Cancel"]
    Check "Cancel closes the dialog without applying" (((Dialog $p.Id) -eq [IntPtr]::Zero) -and ((Get-Content $cfg -Raw) -match "scale = 1\.5"))

    # Quit and restart: settings persist
    [void][S]::PostMessage((Control-Window), 0x0111, [IntPtr]2, [IntPtr]::Zero); $p.WaitForExit(5000) | Out-Null
    $p = Start-Process $Exe -PassThru; Start-Sleep 4
    $restarted = Panels $p.Id
    Check "after a restart the saved settings are in effect" ($restarted.Count -eq $after.Count -and [math]::Abs((($restarted | Measure-Object W -Maximum).Maximum) - $widestAfter) -le 1) "$($restarted.Count) panels"
    [void][S]::PostMessage((Control-Window), 0x0111, [IntPtr]2, [IntPtr]::Zero); $p.WaitForExit(5000) | Out-Null
}
catch {
    Write-Host "SCRIPT ERROR: $($_.Exception.Message)"; Write-Host $_.InvocationInfo.PositionMessage
    $results += [pscustomobject]@{ Check = "script ran to the end"; Result = "FAIL"; Detail = $_.Exception.Message }
}
finally {
    $env:APPDATA = $real
    Get-Process lwo -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
}
$results | Format-Table -AutoSize | Out-String -Width 220
if ($results | Where-Object Result -eq "FAIL") { exit 1 }
