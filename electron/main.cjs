const { app, BrowserWindow, Tray, Menu, nativeImage, ipcMain } = require('electron');
const path = require('path');
const { spawn } = require('child_process');

let mainWindow = null;
let tray = null;
let backendProcess = null;

const isDev = !app.isPackaged;
const BACKEND_URL = 'http://127.0.0.1:8742';

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 420,
    height: 700,
    minWidth: 380,
    minHeight: 600,
    frame: false,
    resizable: true,
    transparent: false,
    backgroundColor: '#1a1a2e',
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      contextIsolation: true,
      nodeIntegration: false,
    },
    icon: path.join(__dirname, '../resources/icon.png'),
  });

  if (isDev) {
    mainWindow.loadURL('http://localhost:5173');
    mainWindow.webContents.openDevTools({ mode: 'detach' });
  } else {
    mainWindow.loadFile(path.join(__dirname, '../dist/index.html'));
  }

  mainWindow.on('close', (event) => {
    if (!app.isQuitting) {
      event.preventDefault();
      mainWindow.hide();
    }
  });
}

function createTray() {
  // Create a simple tray icon (16x16 colored square)
  const iconPath = path.join(__dirname, '../resources/icon.png');
  let icon;

  try {
    icon = nativeImage.createFromPath(iconPath);
    if (icon.isEmpty()) {
      // Create a simple colored icon if file doesn't exist
      icon = nativeImage.createEmpty();
    }
  } catch {
    icon = nativeImage.createEmpty();
  }

  tray = new Tray(icon.isEmpty() ? createDefaultIcon() : icon);

  const contextMenu = Menu.buildFromTemplate([
    {
      label: 'Show App',
      click: () => {
        mainWindow?.show();
        mainWindow?.focus();
      },
    },
    {
      label: 'Quick Actions',
      submenu: [
        {
          label: 'Turn On',
          click: () => sendQuickAction('on'),
        },
        {
          label: 'Turn Off',
          click: () => sendQuickAction('off'),
        },
        { type: 'separator' },
        {
          label: 'Brightness 25%',
          click: () => sendQuickAction('brightness', 25),
        },
        {
          label: 'Brightness 50%',
          click: () => sendQuickAction('brightness', 50),
        },
        {
          label: 'Brightness 100%',
          click: () => sendQuickAction('brightness', 100),
        },
      ],
    },
    { type: 'separator' },
    {
      label: 'Quit',
      click: () => {
        app.isQuitting = true;
        app.quit();
      },
    },
  ]);

  tray.setToolTip('Tapo Controller');
  tray.setContextMenu(contextMenu);

  tray.on('click', () => {
    mainWindow?.show();
    mainWindow?.focus();
  });
}

function createDefaultIcon() {
  // Create a simple 16x16 blue square as default icon
  const size = 16;
  const canvas = Buffer.alloc(size * size * 4);

  for (let i = 0; i < size * size; i++) {
    canvas[i * 4] = 100;      // R
    canvas[i * 4 + 1] = 150;  // G
    canvas[i * 4 + 2] = 255;  // B
    canvas[i * 4 + 3] = 255;  // A
  }

  return nativeImage.createFromBuffer(canvas, { width: size, height: size });
}

async function sendQuickAction(action, value) {
  try {
    if (action === 'on' || action === 'off') {
      await fetch(`${BACKEND_URL}/device/power`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ on: action === 'on' }),
      });
    } else if (action === 'brightness') {
      await fetch(`${BACKEND_URL}/device/brightness`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ value }),
      });
    }
  } catch (err) {
    console.error('Quick action failed:', err);
  }
}

function startBackend() {
  const backendPath = isDev
    ? path.join(__dirname, '../backend')
    : path.join(process.resourcesPath, 'backend');

  const pythonCmd = process.platform === 'win32' ? 'python' : 'python3';

  backendProcess = spawn(pythonCmd, ['-m', 'uvicorn', 'main:app', '--host', '127.0.0.1', '--port', '8742'], {
    cwd: backendPath,
    stdio: 'pipe',
  });

  backendProcess.stdout?.on('data', (data) => {
    console.log(`[Backend] ${data}`);
  });

  backendProcess.stderr?.on('data', (data) => {
    console.error(`[Backend] ${data}`);
  });

  backendProcess.on('error', (err) => {
    console.error('Failed to start backend:', err);
  });
}

function stopBackend() {
  if (backendProcess) {
    backendProcess.kill();
    backendProcess = null;
  }
}

// IPC handlers for window controls
ipcMain.on('window:minimize', () => mainWindow?.hide()); // Minimize to tray
ipcMain.on('window:maximize', () => {
  if (mainWindow?.isMaximized()) {
    mainWindow.unmaximize();
  } else {
    mainWindow?.maximize();
  }
});
ipcMain.on('window:close', () => mainWindow?.hide());

// App lifecycle
app.whenReady().then(() => {
  startBackend();
  createWindow();
  createTray();

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    // Don't quit, keep running in tray
  }
});

app.on('before-quit', () => {
  app.isQuitting = true;
  stopBackend();
});
