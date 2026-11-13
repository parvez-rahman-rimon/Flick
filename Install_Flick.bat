@echo off
title Installing Flick...
cd /d "%~dp0"
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0installer\install.ps1"
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo Installation encountered an error.
    pause
) else (
    echo.
    echo Flick is running and ready!
    timeout /t 3 >nul
)
