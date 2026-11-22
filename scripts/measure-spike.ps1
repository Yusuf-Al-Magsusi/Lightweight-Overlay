# Measures one overlay executable. Usage:
#   scripts/measure-spike.ps1 -Exe <path> -Name <label> [-IdleSeconds 300] [-CpuSeconds 60]
# Reports: exe size, private bytes after idle, CPU over a window, GPU engine use,
# and whether the overlay window is click-through (WindowFromPoint skips it).
param(
    [Parameter(Mandatory)] [string] $Exe,
    [Parameter(Mandatory)] [string] $Name,
    [int] $IdleSeconds = 300,
    [int] $CpuSeconds = 60,
    [string] $Screenshot = ""
)

Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class Win {
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X; public int Y; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr h, int i);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public static IntPtr[] ForPid(uint pid) {
    var list = new System.Collections.Generic.List<IntPtr>();
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) list.Add(h); return true; }, IntPtr.Zero);
    return list.ToArray();
  }
}
"@

$ErrorActionPreference = "Stop"
$size = [math]::Round((Get-Item $Exe).Length / 1MB, 2)
$p = Start-Process -FilePath $Exe -PassThru
Start-Sleep -Seconds 3

# Window / click-through / style checks
$all = [Win]::ForPid([uint32]$p.Id)
# Helper windows (IME, message-only) are tiny or empty; the overlay is the largest visible one.
$hwnds = @($all | Sort-Object -Descending {
        $q = New-Object Win+RECT; [void][Win]::GetWindowRect($_, [ref]$q); ($q.R - $q.L) * ($q.B - $q.T)
    } | Select-Object -First 1)
$ct = "no visible window"
$styles = ""
foreach ($h in $hwnds) {
    $ex = [Win]::GetWindowLong($h, -20)
    $flags = @()
    if ($ex -band 0x80000) { $flags += "LAYERED" }
    if ($ex -band 0x20) { $flags += "TRANSPARENT" }
    if ($ex -band 0x8) { $flags += "TOPMOST" }
    if ($ex -band 0x80) { $flags += "TOOLWINDOW" }
    if ($ex -band 0x8000000) { $flags += "NOACTIVATE" }
    $r = New-Object Win+RECT
    [void][Win]::GetWindowRect($h, [ref]$r)
    $pt = New-Object Win+POINT
    $pt.X = [int](($r.L + $r.R) / 2); $pt.Y = [int](($r.T + $r.B) / 2)
    $hit = [Win]::WindowFromPoint($pt)
    $ct = if ($hit -ne $h) { "yes (hit-test passes through)" } else { "NO (overlay intercepts)" }
    $styles = ($flags -join "|") + " rect=($($r.L),$($r.T),$($r.R),$($r.B))"
}

if ($Screenshot) {
    Add-Type -AssemblyName System.Drawing, System.Windows.Forms
    $bmp = New-Object System.Drawing.Bitmap 420, 220
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen(0, 0, 0, 0, $bmp.Size)
    $bmp.Save($Screenshot, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}

Write-Host "[$Name] idling $IdleSeconds s ..."
Start-Sleep -Seconds ([math]::Max(0, $IdleSeconds - 3))

$p.Refresh()
$priv = [math]::Round($p.PrivateMemorySize64 / 1MB, 1)
$ws = [math]::Round($p.WorkingSet64 / 1MB, 1)
# Private working set = the "Memory" column in Task Manager's Details tab.
$pws = $null
try {
    $c = Get-Counter -Counter "\Process V2(*)\Working Set - Private" -ErrorAction Stop
    $s = $c.CounterSamples | Where-Object { $_.InstanceName -like "*:$($p.Id)" } | Select-Object -First 1
    if ($s) { $pws = [math]::Round($s.CookedValue / 1MB, 1) }
} catch { }

# CPU + GPU over the window
$cores = [Environment]::ProcessorCount
$cpu0 = $p.TotalProcessorTime.TotalSeconds
$gpuSamples = @()
$t0 = Get-Date
for ($i = 0; $i -lt $CpuSeconds; $i += 5) {
    try {
        $c = Get-Counter -Counter "\GPU Engine(pid_$($p.Id)_*)\Utilization Percentage" -ErrorAction Stop
        $gpuSamples += ($c.CounterSamples | Measure-Object CookedValue -Sum).Sum
    } catch { $gpuSamples += 0 }
    Start-Sleep -Seconds 4
}
$p.Refresh()
$elapsed = ((Get-Date) - $t0).TotalSeconds
$cpuPct = [math]::Round((($p.TotalProcessorTime.TotalSeconds - $cpu0) / $elapsed / $cores) * 100, 3)
$gpuAvg = if ($gpuSamples.Count) { [math]::Round(($gpuSamples | Measure-Object -Average).Average, 3) } else { 0 }
$gpuMax = if ($gpuSamples.Count) { [math]::Round(($gpuSamples | Measure-Object -Maximum).Maximum, 3) } else { 0 }
$threads = $p.Threads.Count
$handles = $p.HandleCount

Stop-Process -Id $p.Id -Force
[pscustomobject]@{
    Name = $Name; ExeMB = $size; PrivateBytesMB = $priv; PrivateWorkingSetMB = $pws; WorkingSetMB = $ws
    CpuPctOfAllCores = $cpuPct; GpuAvgPct = $gpuAvg; GpuMaxPct = $gpuMax
    Threads = $threads; Handles = $handles; ClickThrough = $ct; Styles = $styles
} | ConvertTo-Json -Compress
