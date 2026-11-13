@echo off
title Add Flick to Windows Startup
cd /d "%~dp0"

set "TARGET=%~dp0Flick.exe"
if not exist "%TARGET%" set "TARGET=%~dp0..\Flick.exe"

set "SHORTCUT=%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\Flick.lnk"
set "ICO=%~dp0flick.ico"

powershell -NoProfile -Command "$ws = New-Object -COM WScript.Shell; $s = $ws.CreateShortcut('%SHORTCUT%'); $s.TargetPath = '%TARGET%'; $s.WorkingDirectory = '%~dp0'; $s.IconLocation = '%ICO%'; $s.Description = 'Flick Windows Receiver'; $s.Save()"

echo ========================================================
echo [Flick] Added to Windows Startup successfully!
echo Shortcut created at:
echo %SHORTCUT%
echo.
echo Flick will now start automatically whenever your PC boots.
echo ========================================================
pause
