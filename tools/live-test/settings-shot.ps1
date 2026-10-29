# Opens Nab's settings with a throwaway config, captures only that window, closes everything.
param([string]$Exe, [string]$OutDir)
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public static class S {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  public static IntPtr Win(string cls) { return FindWindow(cls, null); }
  public static IntPtr Titled(string t) { return FindWindow(null, t); }
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, int d, UIntPtr e);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int s);
  public static RECT Bounds(IntPtr h) { RECT r; DwmGetWindowAttribute(h, 9, out r, 16); return r; }
}
"@
[S]::SetProcessDPIAware() | Out-Null
$tmp = Join-Path $env:TEMP 'nab-settings-shot'
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$tmp\appdata\Nab" | Out-Null
$env:APPDATA = "$tmp\appdata"
@'
paused = false
card = true
flash = true
sound = false
pointer = false
corners = true
jpg_quality = 90
hello_card = false

[[nabs]]
name = "Window"
kind = "window"
hotkey = "Ctrl+Shift+Q"
folder = 'C:\Users\You\Pictures\Nab'
pattern = "{app} {date} {time}"
format = "png"
save = true
copy = true

[[nabs]]
name = "Area"
kind = "area"
hotkey = "Ctrl+Shift+A"
folder = 'C:\Users\You\Pictures\Nab'
pattern = "{app} {date} {time}"
format = "png"
save = true
copy = true

[[nabs]]
name = "Bug reports"
kind = "window"
hotkey = "Ctrl+Shift+B"
folder = 'C:\Users\You\Documents\Bug reports'
pattern = "{app}/{date} {n}"
format = "png"
save = true
copy = true
'@ | Set-Content "$tmp\appdata\Nab\settings.toml" -Encoding UTF8

$cur = New-Object S+POINT; [S]::GetCursorPos([ref]$cur) | Out-Null
$res = Start-Process $Exe -ArgumentList '--autostart', '--no-hello' -PassThru
Start-Sleep -Milliseconds 900
$set = Start-Process $Exe -ArgumentList '--settings' -PassThru
$h = [IntPtr]::Zero
foreach ($i in 1..40) { Start-Sleep -Milliseconds 100; $h = [S]::Titled('Nab Settings'); if ($h -ne [IntPtr]::Zero) { break } }
Start-Sleep -Milliseconds 1600
function Grab($name) {
    [S]::SetForegroundWindow($h) | Out-Null
    Start-Sleep -Milliseconds 350
    $r = [S]::Bounds($h)
    $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
    $bmp.Save((Join-Path $OutDir $name))
    $g.Dispose(); $bmp.Dispose()
    "$name $($r.R - $r.L)x$($r.B - $r.T)"
}
if ($h -ne [IntPtr]::Zero) {
    Grab 'settings-top.png'
    $r = [S]::Bounds($h)
    [S]::SetCursorPos(($r.L + $r.R) / 2, ($r.T + $r.B) / 2) | Out-Null
    foreach ($i in 1..9) { [S]::mouse_event(0x0800, 0, 0, -120, [UIntPtr]::Zero); Start-Sleep -Milliseconds 40 }
    Start-Sleep -Milliseconds 500
    Grab 'settings-scrolled.png'
    foreach ($i in 1..30) { [S]::mouse_event(0x0800, 0, 0, -120, [UIntPtr]::Zero); Start-Sleep -Milliseconds 20 }
    Start-Sleep -Milliseconds 500
    Grab 'settings-bottom.png'
    [S]::SetCursorPos($cur.X, $cur.Y) | Out-Null
    [S]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
} else { 'settings window not found' }
Start-Sleep -Milliseconds 400
[S]::PostMessage([S]::Win('Nab.Resident'), 0x8004, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
Start-Sleep -Milliseconds 600
Get-Process -Id $res.Id, $set.Id -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
'closed'
