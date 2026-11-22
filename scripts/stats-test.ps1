# Visual test of the CPU / memory / GPU widgets.
# Usage: scripts/stats-test.ps1 -OutDir <folder for screenshots>
# Needs: cargo build --workspace ; cargo build --release -p lwo
#  1. release build, real machine: CPU, memory and GPU panels show. Without an NVIDIA
#     driver the GPU panel has only load and VRAM (generic Windows counters).
#     (For real GPU load run `cargo run --example gpu_load -- 15` meanwhile.)
#  2. debug build with the fake nvml.dll (debug builds only honour LWO_NVML_PATH):
#     the GPU panel must appear with a temperature/power row, and its numbers must
#     change every second.
# Each widget has its own window, so the check compares the GPU panel's height.
# Uses a temporary %APPDATA%. Takes about 25 seconds.
param([Parameter(Mandatory)] [string] $OutDir)
$ErrorActionPreference = "Stop"
New-Item -ItemType Directory -Force $OutDir | Out-Null
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text;
public static class StatsWin {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  public delegate bool CB(IntPtr h, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool EnumWindows(CB cb, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  // Height of the visible widget window with this title (each widget's window is titled with its name), or 0.
  public static int PanelHeight(uint pid, string title) { int found = 0;
    EnumWindows((h,l)=>{ uint p; GetWindowThreadProcessId(h,out p); if(p!=pid || !IsWindowVisible(h)) return true;
      var c = new StringBuilder(128); GetClassName(h,c,128); if(!c.ToString().EndsWith("-overlay")) return true;
      var t = new StringBuilder(128); GetWindowText(h,t,128); if(t.ToString()!=title) return true;
      RECT r; GetWindowRect(h,out r); found = r.B-r.T; return true;}, IntPtr.Zero); return found; }
}
"@
function Snap($name) {
    $b = New-Object System.Drawing.Bitmap 360, 640
    $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen(0, 0, 0, 0, $b.Size)
    $b.Save((Join-Path $OutDir "$name.png")); $g.Dispose(); $b.Dispose()
}
function Quit-App($p) {
    [void][StatsWin]::PostMessage([StatsWin]::FindWindow("lightweight-overlay-control", [NullString]::Value), 0x0111, [IntPtr]2, [IntPtr]::Zero)
    $p.WaitForExit(5000) | Out-Null
}

$real = $env:APPDATA
$env:APPDATA = Join-Path $env:TEMP "lwo-stats-test-$PID"
try {
    # 1. Release build on this machine
    $app = Start-Process "target\release\lwo.exe" -PassThru
    Start-Sleep -Seconds 4
    $h1 = [StatsWin]::PanelHeight([uint32]$app.Id, "GPU")
    "release: process alive=$(-not $app.HasExited), GPU panel height = $h1 px (0 means no GPU panel on this PC)"
    Snap "1-release-real-machine"
    Quit-App $app

    # 2. Debug build with fake NVML, values varying every second
    $env:LWO_NVML_PATH = (Resolve-Path "target\debug\fake_nvml.dll").Path
    $env:FAKE_NVML_VARY = "1"
    $app = Start-Process "target\debug\lwo.exe" -PassThru
    Start-Sleep -Seconds 4
    $h2 = [StatsWin]::PanelHeight([uint32]$app.Id, "GPU")
    "debug + fake nvml: process alive=$(-not $app.HasExited), GPU panel height = $h2 px ($($h2 - $h1) px taller than the generic provider's, from the temperature/power row)"
    Snap "2-fake-nvml-a"
    Start-Sleep -Seconds 3
    Snap "3-fake-nvml-b"
    Quit-App $app
}
finally {
    $env:APPDATA = $real
    Remove-Item Env:LWO_NVML_PATH, Env:FAKE_NVML_VARY -ErrorAction SilentlyContinue
    Get-Process lwo -ErrorAction SilentlyContinue | Stop-Process -Force
    Remove-Item -Recurse -Force (Join-Path $env:TEMP "lwo-stats-test-$PID") -ErrorAction SilentlyContinue
}
