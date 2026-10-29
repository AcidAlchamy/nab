# Live test

Drives a real Nab on a **test PC** (it minimises every window, sends keys and mouse clicks, and quits Nab when done):

1. Copy `Nab.exe`, `harness.ps1` and `run.vbs` to `C:\nab-test` on the test PC.
2. Run `run.vbs` inside the signed-in desktop session (an interactive scheduled task works: `schtasks /Create /TN NabTest /TR "wscript.exe C:\nab-test\run.vbs" /SC ONCE /ST 23:59 /IT`, then `schtasks /Run /TN NabTest`).
3. Read `C:\nab-test\log.txt`.

It refuses to start if the PC was used in the last two minutes. It checks a window nab against the window's own
edges and corner transparency, the card's "Show in folder", a JPG into a per-app subfolder, an area drag, an area
click on a window, Esc, `--nab screen` / `--nab all`, idle CPU and memory, and that the PNG stays on the clipboard
after Nab quits. Its screenshots (`shot-*.png`) show whatever is on that PC's screen: delete them when done.

`settings-shot.ps1` opens the settings window with a throwaway config and captures just that window (for the README).
