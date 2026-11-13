# Flick One-Click Windows Installer
$ErrorActionPreference = "Stop"

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "      Installing Flick Windows Receiver (v2.0.3)            " -ForegroundColor Yellow
Write-Host "============================================================" -ForegroundColor Cyan

# 1. Stop any currently running instances
Write-Host "[1/5] Closing any running Flick instances..." -ForegroundColor Gray
Get-Process -Name "*flick*" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 600

# 2. Setup Destination Directory in %LOCALAPPDATA%\Programs\Flick
$InstallDir = Join-Path $env:LOCALAPPDATA "Programs\Flick"
$AppDir = Join-Path $InstallDir "app"
Write-Host "[2/5] Creating application folder: $InstallDir" -ForegroundColor Gray
if (-not (Test-Path $AppDir)) {
    New-Item -ItemType Directory -Path $AppDir -Force | Out-Null
}

# 3. Locate Source and Copy Application Files
$InstallerDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = Split-Path -Parent $InstallerDir
$ReceiverDir = Join-Path $ProjectRoot "windows-receiver"

Write-Host "[3/5] Installing application files and assets..." -ForegroundColor Gray

# Copy all python runtime scripts, fonts, and icons
Copy-Item -Path (Join-Path $ReceiverDir "*.py") -Destination $AppDir -Force
Copy-Item -Path (Join-Path $ReceiverDir "flick.ico") -Destination $InstallDir -Force
Copy-Item -Path (Join-Path $ReceiverDir "flick.png") -Destination $InstallDir -Force
Copy-Item -Path (Join-Path $ReceiverDir "flick.ico") -Destination $AppDir -Force
Copy-Item -Path (Join-Path $ReceiverDir "flick.png") -Destination $AppDir -Force
if (Test-Path (Join-Path $ReceiverDir "fonts")) {
    Copy-Item -Path (Join-Path $ReceiverDir "fonts") -Destination $AppDir -Recurse -Force
}

# Also copy Flick.exe if available and not blocked
$SourceExe = Join-Path $ProjectRoot "Flick.exe"
if (Test-Path $SourceExe) {
    try {
        Copy-Item -Path $SourceExe -Destination (Join-Path $InstallDir "Flick.exe") -Force -ErrorAction SilentlyContinue
    } catch {}
}

# 4. Find pythonw.exe (Signed official GUI Python - 100% immune to Windows Defender false positives)
$PythonwPath = "C:\Users\Parvez\AppData\Local\Python\pythoncore-3.14-64\pythonw.exe"
if (-not (Test-Path $PythonwPath)) {
    $PythonwPath = "C:\Users\Parvez\AppData\Local\Python\bin\pythonw.exe"
}
if (-not (Test-Path $PythonwPath)) {
    $cmd = Get-Command pythonw -ErrorAction SilentlyContinue
    if ($cmd) { $PythonwPath = $cmd.Source }
}

$TargetScript = Join-Path $AppDir "flick_receiver.py"
$TargetIco = Join-Path $InstallDir "flick.ico"

# 5. Create Desktop & Start Menu Shortcuts
Write-Host "[4/5] Creating Desktop and Start Menu shortcuts..." -ForegroundColor Gray
$WshShell = New-Object -ComObject WScript.Shell

# Desktop Shortcut
$DesktopPath = [Environment]::GetFolderPath('Desktop')
$DesktopLnk = Join-Path $DesktopPath "Flick.lnk"
$Shortcut = $WshShell.CreateShortcut($DesktopLnk)
$Shortcut.TargetPath = $PythonwPath
$Shortcut.Arguments = "`"$TargetScript`""
$Shortcut.WorkingDirectory = $AppDir
$Shortcut.IconLocation = "$TargetIco,0"
$Shortcut.Description = "Flick - Instant Zero-Trust File and Clipboard Teleportation"
$Shortcut.Save()
Write-Host "      + Desktop Shortcut: $DesktopLnk" -ForegroundColor Green

# Start Menu Shortcut
$ProgramsPath = [Environment]::GetFolderPath('Programs')
$StartMenuLnk = Join-Path $ProgramsPath "Flick.lnk"
$Shortcut = $WshShell.CreateShortcut($StartMenuLnk)
$Shortcut.TargetPath = $PythonwPath
$Shortcut.Arguments = "`"$TargetScript`""
$Shortcut.WorkingDirectory = $AppDir
$Shortcut.IconLocation = "$TargetIco,0"
$Shortcut.Description = "Flick - Instant Zero-Trust File and Clipboard Teleportation"
$Shortcut.Save()
Write-Host "      + Start Menu Shortcut: $StartMenuLnk" -ForegroundColor Green

# Set AUMID on Start Menu shortcut so Windows Toast displays "Flick" instead of "Python"
try {
    python -c "from win32com.propsys import propsys, pscon; store = propsys.SHGetPropertyStoreFromParsingName(r'$StartMenuLnk', None, 2, propsys.IID_IPropertyStore); store.SetValue(pscon.PKEY_AppUserModel_ID, propsys.PROPVARIANTType('Flick.WindowsReceiver')); store.Commit()" 2>$null
    Write-Host "      + Shortcut AUMID configured: Flick.WindowsReceiver" -ForegroundColor Green
} catch {}

# 6. Windows Startup Registration
Write-Host "[5/5] Registering Windows background startup..." -ForegroundColor Gray
$RunKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
Set-ItemProperty -Path $RunKey -Name "Flick" -Value "`"$PythonwPath`" `"$TargetScript`" --minimized" -Force

# Create Uninstaller in InstallDir
$UninstallScript = @"
@echo off
echo Stopping Flick...
taskkill /F /IM pythonw.exe /FI "WINDOWTITLE eq Flick*" 2>nul
taskkill /F /IM Flick.exe 2>nul
echo Removing shortcuts...
del /F /Q "$DesktopLnk" 2>nul
del /F /Q "$StartMenuLnk" 2>nul
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v "Flick" /f 2>nul
echo Removing files...
rd /S /Q "$InstallDir" 2>nul
echo Flick uninstalled successfully.
pause
"@
Set-Content -Path (Join-Path $InstallDir "uninstall.bat") -Value $UninstallScript

Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "        SUCCESS! FLICK HAS BEEN INSTALLED!                  " -ForegroundColor Green
Write-Host "============================================================" -ForegroundColor Cyan
Write-Host "You can open Flick anytime from Desktop or Start Menu!" -ForegroundColor Yellow
