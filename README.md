# Tapo Controller

<p align="center">
  <img src="docs/screenshot.png" alt="Tapo Controller" width="400">
</p>

<p align="center">
  <strong>A modern desktop app for controlling TP-Link Tapo LED strips</strong>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#development">Development</a> •
  <a href="#supported-devices">Supported Devices</a>
</p>

---

## Features

- 🎨 **Color Control** - Full HSV color picker with real-time preview
- ☀️ **Brightness** - Smooth brightness adjustment (1-100%)
- 🌡️ **White Temperature** - Warm to cool white (2500K-6500K)
- ⚡ **Quick Colors** - 8 preset colors for instant access
- 🔌 **Power Control** - Turn on/off with one click
- 💾 **Auto-reconnect** - Remembers your device on restart
- 🖥️ **System Tray** - Runs in background with quick controls
- 🌙 **Dark Theme** - Easy on the eyes

## Installation

### Download

Download the latest release for your platform:

| Platform | Download |
|----------|----------|
| Windows | [Tapo-Controller-Setup.exe](../../releases/latest) |
| macOS | [Tapo-Controller.dmg](../../releases/latest) |
| Linux | [Tapo-Controller.AppImage](../../releases/latest) |

### From Source

```bash
# Clone the repository
git clone https://github.com/BartaG512/tapo-controller.git
cd tapo-controller

# Install frontend dependencies
npm install

# Install backend dependencies
cd backend
pip install -r requirements.txt
cd ..

# Run in development mode
npm run electron:dev
```

## Usage

1. **Find your device IP**: Open the Tapo app → Select your LED strip → ⚙️ Settings → Device Info
2. **Connect**: Enter your Tapo account email, password, and device IP
3. **Control**: Adjust colors, brightness, and temperature in real-time

## Development

### Prerequisites

- Node.js 18+
- Python 3.10+
- npm or yarn

### Architecture

```
tapo-controller/
├── electron/          # Electron main process
│   ├── main.js        # App entry, window & tray management
│   └── preload.js     # IPC bridge
├── src/               # React frontend
│   ├── App.tsx        # Main component
│   ├── components/    # UI components
│   └── styles/        # CSS styles
├── backend/           # Python FastAPI backend
│   ├── main.py        # API endpoints
│   ├── device.py      # Tapo device wrapper
│   ├── storage.py     # JSON persistence
│   └── scheduler.py   # Schedule management
└── resources/         # App icons
```

### Running

```bash
# Terminal 1: Start backend
cd backend
uvicorn main:app --port 8742

# Terminal 2: Start frontend + Electron
npm run electron:dev
```

### Building

```bash
# Build for current platform
npm run electron:build

# Build output: dist-electron/
```

## Supported Devices

| Model | Color | Brightness | Temperature | Effects |
|-------|-------|------------|-------------|---------|
| L900 | ✅ | ✅ | ✅ | ❌ |
| L920 | ✅ | ✅ | ✅ | ✅ |
| L930 | ✅ | ✅ | ✅ | ✅ |

## API Reference

The backend exposes a REST API on `http://localhost:8742`:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/auth/login` | POST | Connect to device |
| `/auth/logout` | POST | Disconnect |
| `/auth/status` | GET | Check connection status |
| `/device/info` | GET | Get device state |
| `/device/power` | POST | Turn on/off |
| `/device/brightness` | POST | Set brightness |
| `/device/color` | POST | Set color (hue, saturation) |
| `/device/temperature` | POST | Set white temperature |
| `/device/effect` | POST | Set lighting effect |

## Tech Stack

- **Frontend**: React 19, Tailwind CSS, Vite
- **Desktop**: Electron
- **Backend**: Python, FastAPI, [tapo](https://github.com/mihai-dinculescu/tapo)
- **Icons**: Lucide React

## License

MIT License - see [LICENSE](LICENSE) for details.

## Credits

- [tapo](https://github.com/mihai-dinculescu/tapo) - Unofficial Tapo API client
- [react-colorful](https://github.com/omgovich/react-colorful) - Color picker component
