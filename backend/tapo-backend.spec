# -*- mode: python ; coding: utf-8 -*-
# PyInstaller spec file for Tapo Controller backend (optimized)

import sys
from PyInstaller.utils.hooks import collect_data_files, collect_submodules

block_cipher = None

# Only include what we actually need
hiddenimports = [
    'uvicorn.logging',
    'uvicorn.loops',
    'uvicorn.loops.auto',
    'uvicorn.protocols',
    'uvicorn.protocols.http',
    'uvicorn.protocols.http.auto',
    'uvicorn.protocols.websockets',
    'uvicorn.protocols.websockets.auto',
    'uvicorn.lifespan',
    'uvicorn.lifespan.on',
    'uvicorn.lifespan.off',
    'apscheduler.schedulers.asyncio',
    'apscheduler.triggers.cron',
    'apscheduler.executors.asyncio',
    'apscheduler.jobstores.memory',
    'tapo',
    'tapo.requests',
    'pydantic',
    'pydantic_settings',
]

# Exclude unnecessary modules to reduce size and startup time
excludes = [
    # Data science / visualization (not needed)
    'matplotlib',
    'numpy',
    'pandas',
    'scipy',
    'PIL',
    'Pillow',
    'cv2',
    'opencv',

    # IPython / Jupyter (not needed)
    'IPython',
    'ipython',
    'jupyter',
    'notebook',
    'ipykernel',
    'jedi',
    'parso',

    # Testing (not needed in production)
    'pytest',
    'unittest',
    'test',
    '_pytest',
    'py',

    # Development tools (not needed)
    'setuptools',
    'pip',
    'wheel',
    'pkg_resources',

    # GUI toolkits (not needed for API server)
    'tkinter',
    '_tkinter',
    'Tkinter',
    'wx',
    'PyQt5',
    'PyQt6',
    'PySide2',
    'PySide6',

    # Other unnecessary
    'pygments',
    'docutils',
    'sphinx',
    'lxml',
    'html5lib',
    'bs4',
    'beautifulsoup4',
]

a = Analysis(
    ['main.py'],
    pathex=[],
    binaries=[],
    datas=[],
    hiddenimports=hiddenimports,
    hookspath=[],
    hooksconfig={},
    runtime_hooks=[],
    excludes=excludes,
    win_no_prefer_redirects=False,
    win_private_assemblies=False,
    cipher=block_cipher,
    noarchive=False,
)

pyz = PYZ(a.pure, a.zipped_data, cipher=block_cipher)

exe = EXE(
    pyz,
    a.scripts,
    a.binaries,
    a.zipfiles,
    a.datas,
    [],
    name='tapo-backend',
    debug=False,
    bootloader_ignore_signals=False,
    strip=False,
    upx=True,
    upx_exclude=[],
    runtime_tmpdir=None,
    console=True,  # Keep console for logging
    disable_windowed_traceback=False,
    argv_emulation=False,
    target_arch=None,
    codesign_identity=None,
    entitlements_file=None,
)
