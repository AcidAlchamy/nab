' Starts the Nab test harness with no console window.
CreateObject("WScript.Shell").Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -STA -File C:\nab-test\harness.ps1", 0, True
