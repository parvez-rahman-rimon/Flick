@echo off
title Flick Windows Receiver
cd /d "%~dp0"

if exist "Flick.exe" (
    echo Launching Flick.exe...
    start "" "Flick.exe"
    exit /b
)

if exist "dist\Flick.exe" (
    echo Launching dist\Flick.exe...
    start "" "dist\Flick.exe"
    exit /b
)

echo Starting Flick Python Receiver...
python flick_receiver.py
pause
