@echo off
title Remove Flick from Windows Startup
set "SHORTCUT=%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup\Flick.lnk"

if exist "%SHORTCUT%" (
    del "%SHORTCUT%"
    echo ========================================================
    echo [Flick] Removed from Windows Startup successfully!
    echo ========================================================
) else (
    echo [Flick] Startup shortcut was not found in Startup folder.
)
pause
