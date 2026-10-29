# Nab live test, run inside the logged-on desktop session by a scheduled task.
# Drives Nab with synthetic keys and mouse, checks files and the clipboard, takes screenshots.
$ErrorActionPreference = 'Continue'
$T = 'C:\nab-test'
$out = "$T\out"
$log = "$T\log.txt"
Remove-Item $log, "$T\shot-*.png", "$T\DONE" -ErrorAction SilentlyContinue
function Log($m) { "$(Get-Date -Format HH:mm:ss.fff) $m" | Add-Content $log }

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class U {
  [StructLayout(LayoutKind.Sequential)] public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; }
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO p);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool r);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("kernel32.dll")] public static extern uint GetTickCount();
  [DllImport("kernel32.dll")] public static extern bool QueryProcessCycleTime(IntPtr h, out ulong c);
  public static uint Idle() { var i = new LASTINPUTINFO(); i.cbSize = 8; GetLastInputInfo(ref i); return GetTickCount() - i.dwTime; }
  public static void Key(byte vk, bool up) { keybd_event(vk, 0, up ? 2u : 0u, UIntPtr.Zero); }
  public static RECT Bounds(IntPtr h) { RECT r; DwmGetWindowAttribute(h, 9, out r, 16); return r; }
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static IntPtr Win(string cls) { return FindWindow(cls, null); }
  public static IntPtr Win(string cls, string title) { return FindWindow(cls, title); }
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  public static IntPtr Starting(string cls, string prefix) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, l) => {
      var c = new System.Text.StringBuilder(256); GetClassName(h, c, 256);
      var t = new System.Text.StringBuilder(256); GetWindowText(h, t, 256);
      if (c.ToString() == cls && t.ToString().StartsWith(prefix)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
[U]::SetProcessDPIAware() | Out-Null

$idle = [U]::Idle()
Log "idle ms before start: $idle"
if ($idle -lt 120000) { Log 'Someone used this PC in the last 2 minutes: not testing.'; 'ABORT' | Set-Content "$T\DONE"; exit }

function Combo([byte[]]$keys) {
    foreach ($k in $keys) { [U]::Key($k, $false); Start-Sleep -Milliseconds 25 }
    [array]::Reverse($keys)
    foreach ($k in $keys) { [U]::Key($k, $true); Start-Sleep -Milliseconds 25 }
}
function Shot($name) {
    $b = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Left, $b.Top, 0, 0, $bmp.Size)
    $bmp.Save("$T\shot-$name.png")
    $g.Dispose(); $bmp.Dispose()
}
function Files { Get-ChildItem $out -Recurse -File -ErrorAction SilentlyContinue | Where-Object Extension -in '.png', '.jpg' }
function Describe($f) {
    try { $img = [System.Drawing.Image]::FromFile($f.FullName); $s = "$($img.Width)x$($img.Height) $($img.PixelFormat)"; $img.Dispose() } catch { $s = "unreadable: $_" }
    "$($f.FullName.Substring($out.Length + 1)) | $s | $($f.Length) bytes"
}
function NewFiles($before) { Files | Where-Object { $before -notcontains $_.FullName } }
function Click($x, $y) { [U]::SetCursorPos($x, $y) | Out-Null; Start-Sleep -Milliseconds 120; [U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60; [U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero) }
function ClipInfo {
    try {
        $d = [System.Windows.Forms.Clipboard]::GetDataObject()
        $fmts = ($d.GetFormats() -join ', ')
        $png = $d.GetData('PNG')
        $n = if ($png -is [System.IO.MemoryStream]) { $png.Length } else { 'none' }
        "formats: $fmts | PNG bytes: $n"
    } catch { "clipboard error: $_" }
}

# --- fresh start -----------------------------------------------------------------
# Only test windows on screen: minimise everything that was open (restored at the end).
$shell = New-Object -ComObject Shell.Application
$shell.MinimizeAll(); Start-Sleep -Milliseconds 800
Get-Process nab -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $out | Out-Null
$cfgDir = "$env:APPDATA\Nab"; New-Item -ItemType Directory -Force $cfgDir | Out-Null
$o = $out -replace '\\', '\\'
@"
paused = false
card = true
flash = true
sound = false
pointer = false
corners = true
jpg_quality = 90
hello_card = true

[[nabs]]
name = "Window"
kind = "window"
hotkey = "Ctrl+Shift+Q"
folder = "$o"
pattern = "{app} {date} {time}"
format = "png"
save = true
copy = true

[[nabs]]
name = "Area"
kind = "area"
hotkey = "Ctrl+Shift+A"
folder = "$o"
pattern = "{app} {date} {time}"
format = "png"
save = true
copy = true

[[nabs]]
name = "Screen"
kind = "screen"
hotkey = ""
folder = "$o"
pattern = "{nab} {n}"
format = "png"
save = true
copy = true

[[nabs]]
name = "Docs"
kind = "window"
hotkey = "Ctrl+Shift+J"
folder = "$o"
pattern = "{app}/{title} {n}"
format = "jpg"
save = true
copy = false
"@ | Set-Content "$cfgDir\settings.toml" -Encoding UTF8

$p = Start-Process "$T\Nab.exe" -PassThru
Start-Sleep -Milliseconds 2500
Shot '01-hello'
$res = [U]::Win('Nab.Resident')
Log "resident window: $res  pid $($p.Id)"

# A window to nab: File Explorer on a test folder.
New-Item -ItemType Directory -Force "$T\target" | Out-Null
'Nab test file' | Set-Content "$T\target\nab-test-file.txt"
Start-Process explorer.exe "$T\target"
$nh = [IntPtr]::Zero
foreach ($i in 1..30) { Start-Sleep -Milliseconds 200; $nh = [U]::Starting('CabinetWClass', 'target'); if ($nh -ne [IntPtr]::Zero) { break } }
[U]::ShowWindow($nh, 9) | Out-Null
[U]::MoveWindow($nh, 260, 160, 1100, 650, $true) | Out-Null
[U]::SetForegroundWindow($nh) | Out-Null
Start-Sleep -Milliseconds 800
$r = [U]::Bounds($nh)
Log "explorer bounds: $($r.L),$($r.T) - $($r.R),$($r.B)  = $($r.R - $r.L)x$($r.B - $r.T)  foreground ok: $([U]::GetForegroundWindow() -eq $nh)"

# 1. Window nab (Ctrl+Shift+Q)
$before = @(Files | ForEach-Object FullName)
Combo @(0x11, 0x10, 0x51)
Start-Sleep -Milliseconds 260
Shot '02-card-fading-in'
Start-Sleep -Milliseconds 1200
Shot '03-card'
NewFiles $before | ForEach-Object {
    Log "WINDOW (expect $($r.R - $r.L)x$($r.B - $r.T), 32bpp with clear corners): $(Describe $_)"
    try {
        $bmp = New-Object System.Drawing.Bitmap $_.FullName
        $a = @($bmp.GetPixel(0, 0).A, $bmp.GetPixel(2, 2).A, $bmp.GetPixel($bmp.Width - 1, $bmp.Height - 1).A, $bmp.GetPixel(20, 0).A, $bmp.GetPixel(10, 10).A)
        Log "  corner alphas (0,0) (2,2) (w-1,h-1) | edge (20,0) | inside (10,10): $($a -join ' ')"
        $bmp.Dispose()
    } catch { Log "  pixel check failed: $_" }
}
Log "clipboard after window nab: $(ClipInfo)"

# 1b. The card: "Show in folder" opens the folder, then the card goes away
$wa = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
$cx = $wa.Right - 384 - 16; $cy = $wa.Bottom - 106 - 16
Log "card window visible: $([U]::IsWindowVisible([U]::Win('Nab.Card')))"
Click ($cx + 280) ($cy + 84)
Start-Sleep -Milliseconds 1500
$folder = [U]::Starting('CabinetWClass', 'out')
Log "after Show in folder: folder window open: $($folder -ne [IntPtr]::Zero) | card visible: $([U]::IsWindowVisible([U]::Win('Nab.Card')))"
if ($folder -ne [IntPtr]::Zero) { [U]::PostMessage($folder, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
Start-Sleep -Milliseconds 500

# 2. JPG nab into a per-app subfolder (Ctrl+Shift+J), copy off
[U]::SetForegroundWindow($nh) | Out-Null; Start-Sleep -Milliseconds 400
$before = @(Files | ForEach-Object FullName)
Combo @(0x11, 0x10, 0x4A)
Start-Sleep -Milliseconds 1500
NewFiles $before | ForEach-Object { Log "JPG: $(Describe $_)" }

# 3. Area: drag a rectangle
$before = @(Files | ForEach-Object FullName)
Combo @(0x11, 0x10, 0x41)
Start-Sleep -Milliseconds 900
[U]::SetCursorPos(700, 500) | Out-Null; Start-Sleep -Milliseconds 300
Shot '04-area-hover'
[U]::SetCursorPos(400, 300) | Out-Null; Start-Sleep -Milliseconds 150
[U]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero)
foreach ($i in 1..10) { [U]::SetCursorPos(400 + 50 * $i, 300 + 35 * $i) | Out-Null; Start-Sleep -Milliseconds 30 }
Start-Sleep -Milliseconds 300
Shot '05-area-drag'
[U]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 1500
NewFiles $before | ForEach-Object { Log "AREA DRAG (expect 501x351): $(Describe $_)" }

# 4. Area: click a window
$before = @(Files | ForEach-Object FullName)
Combo @(0x11, 0x10, 0x41)
Start-Sleep -Milliseconds 900
Click (($r.L + $r.R) / 2) (($r.T + $r.B) / 2)
Start-Sleep -Milliseconds 1500
NewFiles $before | ForEach-Object { Log "AREA CLICK (expect $($r.R - $r.L)x$($r.B - $r.T)): $(Describe $_)" }

# 5. Area: Esc cancels
$before = @(Files | ForEach-Object FullName)
Combo @(0x11, 0x10, 0x41)
Start-Sleep -Milliseconds 900
Combo @(0x1B)
Start-Sleep -Milliseconds 800
$n = @(NewFiles $before).Count
Log "AREA ESC: new files $n (expect 0); area windows left: $([U]::Win('Nab.Area'))"

# 6. --nab screen from the command line
$before = @(Files | ForEach-Object FullName)
Start-Process "$T\Nab.exe" -ArgumentList '--nab', 'screen' -Wait
Start-Sleep -Milliseconds 1800
NewFiles $before | ForEach-Object { Log "SCREEN via --nab: $(Describe $_)" }
$before = @(Files | ForEach-Object FullName)
Start-Process "$T\Nab.exe" -ArgumentList '--nab', 'all' -Wait
Start-Sleep -Milliseconds 1800
NewFiles $before | ForEach-Object { Log "ALL SCREENS via --nab: $(Describe $_)" }

# 7. Cost while a card is up, then idle cost of the resident
$proc = Get-Process -Id $p.Id -ErrorAction SilentlyContinue
if ($proc) {
    [U]::SetForegroundWindow($nh) | Out-Null; Start-Sleep -Milliseconds 300
    Combo @(0x11, 0x10, 0x51)
    Start-Sleep -Milliseconds 400
    $c1 = [uint64]0; [U]::QueryProcessCycleTime($proc.Handle, [ref]$c1) | Out-Null
    Start-Sleep -Seconds 3
    $c2 = [uint64]0; [U]::QueryProcessCycleTime($proc.Handle, [ref]$c2) | Out-Null
    Log ("CARD UP: {0:N0} CPU cycles in 3 s" -f ($c2 - $c1))
}
Start-Sleep -Seconds 6
Log "card visible after its time: $([U]::IsWindowVisible([U]::Win('Nab.Card')))"
if ($proc) {
    $c1 = [uint64]0; [U]::QueryProcessCycleTime($proc.Handle, [ref]$c1) | Out-Null
    Start-Sleep -Seconds 10
    $c2 = [uint64]0; [U]::QueryProcessCycleTime($proc.Handle, [ref]$c2) | Out-Null
    $proc.Refresh()
    Log ("IDLE: {0:N0} CPU cycles in 10 s | private {1:N1} MB | working set {2:N1} MB" -f ($c2 - $c1), ($proc.PrivateMemorySize64 / 1MB), ($proc.WorkingSet64 / 1MB))
}

# 8. Quit: the PNG must still be on the clipboard afterwards
[U]::SetForegroundWindow($nh) | Out-Null; Start-Sleep -Milliseconds 300
Combo @(0x11, 0x10, 0x51)
Start-Sleep -Milliseconds 1500
[U]::PostMessage([U]::Win('Nab.Resident'), 0x8004, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 1500
Log "after quit: nab running: $([bool](Get-Process nab -ErrorAction SilentlyContinue)) | $(ClipInfo)"

foreach ($t in 'target', 'out', 'Windows Explorer') {
    $w = [U]::Starting('CabinetWClass', $t)
    if ($w -ne [IntPtr]::Zero) { [U]::PostMessage($w, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
}
Start-Sleep -Milliseconds 500
$shell.UndoMinimizeALL()
Get-Process nab -ErrorAction SilentlyContinue | Stop-Process -Force
Log 'done'
'DONE' | Set-Content "$T\DONE"
