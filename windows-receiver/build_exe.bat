@echo off
title Build Flick Standalone Executable
cd /d "%~dp0"
echo ========================================================
echo Building Flick.exe Standalone Windows Executable
echo ========================================================
python build_exe.py
pause
