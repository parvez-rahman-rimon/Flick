# -*- mode: python ; coding: utf-8 -*-
from PyInstaller.utils.hooks import collect_submodules
from PyInstaller.utils.hooks import collect_all

datas = [('D:/Sandbox/EkSend/windows-receiver/flick.ico', '.'), ('D:/Sandbox/EkSend/windows-receiver/flick.png', '.'), ('D:/Sandbox/EkSend/windows-receiver/fonts', 'fonts')]
binaries = []
hiddenimports = ['pystray._win32', 'win32clipboard', 'win32con', 'win32gui', 'win32api', 'win32crypt', 'crypto_manager', 'desktop_dashboard']
hiddenimports += collect_submodules('PIL')
hiddenimports += collect_submodules('qrcode')
tmp_ret = collect_all('cryptography')
datas += tmp_ret[0]; binaries += tmp_ret[1]; hiddenimports += tmp_ret[2]


a = Analysis(
    ['flick_receiver.py'],
    pathex=[],
    binaries=binaries,
    datas=datas,
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=[],
    noarchive=False,
    optimize=0,
)
pyz = PYZ(a.pure)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.datas,
    [],
    name='Flick',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=False,
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
    icon=['D:/Sandbox/EkSend/windows-receiver/flick.ico'],
)
