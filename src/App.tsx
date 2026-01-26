import { useEffect, useState, useRef } from 'react';
import { HsvaColor, HsvaColorPicker } from 'react-colorful';
import { TitleBar } from './components/TitleBar';
import { Power, Sun, Wifi, Lock, Mail, Loader2, LogOut, Thermometer, Sparkles, Search, Plus, Trash2, RefreshCw, ChevronLeft } from 'lucide-react';

const API = 'http://127.0.0.1:8742';

interface DeviceInfo {
  ip: string;
  nickname: string;
  model: string | null;
  connected: boolean;
  device_on: boolean | null;
  brightness: number | null;
  hue?: number | null;
  saturation?: number | null;
}

export default function App() {
  const [loggedIn, setLoggedIn] = useState(false);
  const [loading, setLoading] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState('');

  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [deviceIp, setDeviceIp] = useState('');
  const [scanning, setScanning] = useState(false);
  const [discoveredDevices, setDiscoveredDevices] = useState<string[]>([]);

  // Multi-device state
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState<string | null>(null);
  const [showAddDevice, setShowAddDevice] = useState(false);

  // Control state for selected device
  const [color, setColor] = useState<HsvaColor>({ h: 200, s: 100, v: 100, a: 1 });
  const [brightness, setBrightness] = useState(100);
  const [colorTemp, setColorTemp] = useState(4000);
  const [mode, setMode] = useState<'color' | 'white'>('color');

  const colorTimeout = useRef<NodeJS.Timeout>();
  const brightnessTimeout = useRef<NodeJS.Timeout>();
  const tempTimeout = useRef<NodeJS.Timeout>();

  const loadDevices = async () => {
    try {
      const res = await fetch(`${API}/devices`);
      const data = await res.json();
      setDevices(data.devices);
      if (data.devices.length > 0 && !selectedDevice) {
        setSelectedDevice(data.devices[0].ip);
      }
    } catch (err) {
      console.error('Failed to load devices:', err);
    }
  };

  useEffect(() => {
    fetch(`${API}/auth/status`)
      .then(r => r.json())
      .then(async (statusData) => {
        if (statusData.authenticated) {
          setLoggedIn(true);
          await loadDevices();
        }
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  // Load device info when selected device changes
  useEffect(() => {
    if (selectedDevice) {
      const device = devices.find(d => d.ip === selectedDevice);
      if (device && device.connected) {
        fetch(`${API}/device/info?ip=${selectedDevice}`)
          .then(r => r.json())
          .then(info => {
            setBrightness(info.brightness || 100);
            if (info.hue !== null) {
              setColor(c => ({ ...c, h: info.hue, s: info.saturation }));
            }
          })
          .catch(() => {});
      }
    }
  }, [selectedDevice, devices]);

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setConnecting(true);
    setError('');
    try {
      const res = await fetch(`${API}/auth/login`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.detail);
      setLoggedIn(true);
      // Go straight to device discovery
      setShowAddDevice(true);
      scanNetwork();
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setConnecting(false);
    }
  };

  const handleAddDevice = async (e: React.FormEvent) => {
    e.preventDefault();
    setConnecting(true);
    setError('');
    try {
      const res = await fetch(`${API}/auth/login`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email, password, device_ip: deviceIp }),
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.detail);
      setShowAddDevice(false);
      setDeviceIp('');
      await loadDevices();
      setSelectedDevice(deviceIp);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setConnecting(false);
    }
  };

  const handleLogout = async () => {
    await fetch(`${API}/auth/logout`, { method: 'POST' });
    setLoggedIn(false);
    setDevices([]);
    setSelectedDevice(null);
  };

  const handleRemoveDevice = async (ip: string) => {
    await fetch(`${API}/devices/${encodeURIComponent(ip)}`, { method: 'DELETE' });
    await loadDevices();
    if (selectedDevice === ip) {
      setSelectedDevice(devices.find(d => d.ip !== ip)?.ip || null);
    }
  };

  const handleReconnect = async (ip: string) => {
    try {
      await fetch(`${API}/devices/${encodeURIComponent(ip)}/reconnect`, { method: 'POST' });
      await loadDevices();
    } catch (err) {
      console.error('Reconnect failed:', err);
    }
  };

  const scanNetwork = async () => {
    setScanning(true);
    setDiscoveredDevices([]);
    try {
      const res = await fetch(`${API}/discover`);
      const data = await res.json();
      const existingIps = devices.map(d => d.ip);
      setDiscoveredDevices(
        data.potential_devices
          .map((d: { ip: string }) => d.ip)
          .filter((ip: string) => !existingIps.includes(ip))
      );
    } catch (err) {
      console.error('Scan failed:', err);
    } finally {
      setScanning(false);
    }
  };

  const currentDevice = devices.find(d => d.ip === selectedDevice);

  const togglePower = async () => {
    if (!currentDevice) return;
    const newState = !currentDevice.device_on;
    await fetch(`${API}/device/power`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ on: newState, ip: selectedDevice }),
    });
    await loadDevices();
  };

  const handleBrightness = (val: number) => {
    setBrightness(val);
    clearTimeout(brightnessTimeout.current);
    brightnessTimeout.current = setTimeout(async () => {
      await fetch(`${API}/device/brightness`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ value: val, ip: selectedDevice }),
      });
    }, 100);
  };

  const handleColorChange = (newColor: HsvaColor) => {
    setColor(newColor);
    clearTimeout(colorTimeout.current);
    colorTimeout.current = setTimeout(async () => {
      await fetch(`${API}/device/color`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ hue: Math.round(newColor.h), saturation: Math.round(newColor.s), ip: selectedDevice }),
      });
    }, 100);
  };

  const handleColorTemp = (val: number) => {
    setColorTemp(val);
    clearTimeout(tempTimeout.current);
    tempTimeout.current = setTimeout(async () => {
      await fetch(`${API}/device/temperature`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ kelvin: val, ip: selectedDevice }),
      });
    }, 100);
  };

  const quickColors = [
    { h: 0, s: 100, bg: '#ef4444' },
    { h: 25, s: 100, bg: '#f97316' },
    { h: 45, s: 100, bg: '#eab308' },
    { h: 120, s: 80, bg: '#22c55e' },
    { h: 180, s: 80, bg: '#06b6d4' },
    { h: 220, s: 90, bg: '#3b82f6' },
    { h: 270, s: 80, bg: '#a855f7' },
    { h: 320, s: 80, bg: '#ec4899' },
  ];

  const applyQuickColor = async (h: number, s: number) => {
    setColor({ ...color, h, s });
    setMode('color');
    await fetch(`${API}/device/color`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ hue: h, saturation: s, ip: selectedDevice }),
    });
  };

  if (loading) {
    return (
      <div className="h-screen flex flex-col bg-[#0a0a12]">
        <TitleBar />
        <div className="flex-1 flex items-center justify-center">
          <Loader2 className="animate-spin text-violet-500" size={28} />
        </div>
      </div>
    );
  }

  // Login screen
  if (!loggedIn) {
    return (
      <div className="h-screen flex flex-col bg-[#0a0a12]">
        <TitleBar />
        <div className="flex-1 flex flex-col items-center justify-center p-8">
          <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-violet-500 to-fuchsia-500 flex items-center justify-center mb-6">
            <Sparkles size={28} className="text-white" />
          </div>
          <h1 className="text-xl font-semibold mb-1">Tapo Controller</h1>
          <p className="text-zinc-500 text-sm mb-6">Sign in with your Tapo account</p>

          <form onSubmit={handleLogin} className="w-full max-w-xs space-y-3">
            <div className="relative">
              <Mail size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
              <input type="email" value={email} onChange={e => setEmail(e.target.value)} placeholder="Tapo Email"
                className="w-full pl-10 pr-3 py-2.5 bg-zinc-900 border border-zinc-800 rounded-lg text-sm focus:outline-none focus:border-violet-500 transition-colors" required />
            </div>
            <div className="relative">
              <Lock size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
              <input type="password" value={password} onChange={e => setPassword(e.target.value)} placeholder="Password"
                className="w-full pl-10 pr-3 py-2.5 bg-zinc-900 border border-zinc-800 rounded-lg text-sm focus:outline-none focus:border-violet-500 transition-colors" required />
            </div>
            {error && <p className="text-red-400 text-xs">{error}</p>}
            <button type="submit" disabled={connecting}
              className="w-full py-2.5 bg-violet-600 hover:bg-violet-500 rounded-lg text-sm font-medium disabled:opacity-50 flex items-center justify-center gap-2 transition-colors">
              {connecting ? <><Loader2 size={16} className="animate-spin" /> Signing in...</> : 'Sign In'}
            </button>
          </form>
        </div>
      </div>
    );
  }

  // Add device screen
  if (showAddDevice) {
    const isFirstDevice = devices.length === 0;
    return (
      <div className="h-screen flex flex-col bg-[#0a0a12]">
        <TitleBar />
        <div className="flex-1 flex flex-col p-4">
          {!isFirstDevice && (
            <button onClick={() => setShowAddDevice(false)} className="flex items-center gap-1 text-zinc-400 hover:text-white mb-4">
              <ChevronLeft size={16} /> Back
            </button>
          )}
          <h2 className="text-lg font-semibold mb-2">{isFirstDevice ? 'Add Your First Device' : 'Add Device'}</h2>
          {isFirstDevice && (
            <p className="text-zinc-500 text-sm mb-4">Scan your network to find Tapo devices</p>
          )}

          <form onSubmit={handleAddDevice} className="space-y-3">
            <div className="relative">
              <Wifi size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
              <input type="text" value={deviceIp} onChange={e => setDeviceIp(e.target.value)} placeholder="Device IP address"
                className="w-full pl-10 pr-10 py-2.5 bg-zinc-900 border border-zinc-800 rounded-lg text-sm focus:outline-none focus:border-violet-500 transition-colors" required />
              <button type="button" onClick={scanNetwork} disabled={scanning}
                className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-zinc-500 hover:text-violet-400 disabled:opacity-50">
                <Search size={16} className={scanning ? 'animate-pulse' : ''} />
              </button>
            </div>

            {scanning && (
              <div className="flex items-center gap-2 text-zinc-400 text-sm">
                <Loader2 size={14} className="animate-spin" />
                Scanning network...
              </div>
            )}

            {discoveredDevices.length > 0 && (
              <div className="space-y-2">
                <div className="flex items-center justify-between">
                  <p className="text-xs text-zinc-500">Found {discoveredDevices.length} potential device(s):</p>
                  <button type="button" onClick={scanNetwork} disabled={scanning}
                    className="text-xs text-zinc-400 hover:text-violet-400 flex items-center gap-1 disabled:opacity-50">
                    <RefreshCw size={12} className={scanning ? 'animate-spin' : ''} /> Refresh
                  </button>
                </div>
                <div className="flex flex-wrap gap-1">
                  {discoveredDevices.map(ip => (
                    <button key={ip} type="button" onClick={() => setDeviceIp(ip)}
                      className={`px-3 py-1.5 text-xs rounded-lg transition-colors ${deviceIp === ip ? 'bg-violet-600 text-white' : 'bg-zinc-800 text-zinc-400 hover:bg-zinc-700'}`}>
                      {ip}
                    </button>
                  ))}
                </div>
              </div>
            )}

            {!scanning && discoveredDevices.length === 0 && (
              <button type="button" onClick={scanNetwork}
                className="w-full py-2 text-sm text-zinc-400 hover:text-violet-400 border border-zinc-800 hover:border-zinc-700 rounded-lg transition-colors flex items-center justify-center gap-2">
                <Search size={14} /> Scan Network
              </button>
            )}

            {error && <p className="text-red-400 text-xs">{error}</p>}
            <button type="submit" disabled={connecting || !deviceIp}
              className="w-full py-2.5 bg-violet-600 hover:bg-violet-500 rounded-lg text-sm font-medium disabled:opacity-50 flex items-center justify-center gap-2">
              {connecting ? <><Loader2 size={16} className="animate-spin" /> Connecting...</> : 'Add Device'}
            </button>
          </form>
        </div>
      </div>
    );
  }

  // Main app with device list
  return (
    <div className="h-screen flex flex-col bg-[#0a0a12]">
      <TitleBar />

      <div className="flex-1 overflow-auto p-3 space-y-3">
        {/* Device List Header */}
        <div className="flex items-center justify-between">
          <span className="text-xs text-zinc-500 uppercase tracking-wider">Devices ({devices.length})</span>
          <div className="flex items-center gap-1">
            <button onClick={() => { setShowAddDevice(true); setDeviceIp(''); setError(''); }}
              className="p-1.5 text-zinc-400 hover:text-violet-400 hover:bg-zinc-800 rounded-lg transition-colors" title="Add device">
              <Plus size={16} />
            </button>
            <button onClick={handleLogout}
              className="p-1.5 text-zinc-400 hover:text-red-400 hover:bg-zinc-800 rounded-lg transition-colors" title="Logout">
              <LogOut size={16} />
            </button>
          </div>
        </div>

        {/* Device Cards */}
        <div className="space-y-2">
          {devices.map(device => (
            <div key={device.ip}
              onClick={() => device.connected && setSelectedDevice(device.ip)}
              className={`p-3 rounded-xl border transition-all cursor-pointer ${
                selectedDevice === device.ip
                  ? 'bg-violet-600/20 border-violet-500'
                  : device.connected
                    ? 'bg-zinc-900/50 border-zinc-800 hover:border-zinc-700'
                    : 'bg-zinc-900/30 border-zinc-800/50 opacity-60'
              }`}>
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <div className={`w-2 h-2 rounded-full ${device.connected ? (device.device_on ? 'bg-emerald-400' : 'bg-zinc-500') : 'bg-red-400'}`} />
                  <div>
                    <div className="text-sm font-medium">{device.nickname || 'LED Device'}</div>
                    <div className="text-xs text-zinc-500">{device.ip}</div>
                  </div>
                </div>
                <div className="flex items-center gap-1">
                  {!device.connected && (
                    <button onClick={(e) => { e.stopPropagation(); handleReconnect(device.ip); }}
                      className="p-1.5 text-zinc-400 hover:text-emerald-400 hover:bg-zinc-800 rounded-lg" title="Reconnect">
                      <RefreshCw size={14} />
                    </button>
                  )}
                  <button onClick={(e) => { e.stopPropagation(); handleRemoveDevice(device.ip); }}
                    className="p-1.5 text-zinc-400 hover:text-red-400 hover:bg-zinc-800 rounded-lg" title="Remove">
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
              {device.connected && device.brightness !== null && (
                <div className="mt-2 flex items-center gap-2">
                  <div className="flex-1 h-1 bg-zinc-800 rounded-full overflow-hidden">
                    <div className="h-full bg-amber-400 transition-all" style={{ width: `${device.brightness}%` }} />
                  </div>
                  <span className="text-xs text-zinc-500">{device.brightness}%</span>
                </div>
              )}
            </div>
          ))}
        </div>

        {/* Controls for selected device */}
        {currentDevice && currentDevice.connected && (
          <>
            <div className="border-t border-zinc-800 pt-3 mt-3">
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-2">
                  <div className={`w-2 h-2 rounded-full ${currentDevice.device_on ? 'bg-emerald-400' : 'bg-zinc-600'}`} />
                  <span className="text-sm font-medium">{currentDevice.nickname}</span>
                </div>
                <button onClick={togglePower}
                  className={`p-2 rounded-lg transition-colors ${currentDevice.device_on ? 'bg-violet-600 text-white' : 'bg-zinc-800 text-zinc-400 hover:bg-zinc-700'}`}>
                  <Power size={16} />
                </button>
              </div>

              {/* Brightness */}
              <div className="bg-zinc-900/50 rounded-xl p-3 mb-3">
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2">
                    <Sun size={14} className="text-amber-400" />
                    <span className="text-xs text-zinc-400">Brightness</span>
                  </div>
                  <span className="text-sm font-medium text-amber-400">{brightness}%</span>
                </div>
                <input type="range" min="1" max="100" value={brightness}
                  onChange={e => handleBrightness(Number(e.target.value))}
                  className="w-full brightness-slider" />
              </div>

              {/* Mode Tabs */}
              <div className="flex gap-1 p-1 bg-zinc-900/50 rounded-lg mb-3">
                <button onClick={() => setMode('color')}
                  className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors ${mode === 'color' ? 'bg-violet-600 text-white' : 'text-zinc-400 hover:text-zinc-200'}`}>
                  Color
                </button>
                <button onClick={() => setMode('white')}
                  className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors ${mode === 'white' ? 'bg-amber-500 text-white' : 'text-zinc-400 hover:text-zinc-200'}`}>
                  White
                </button>
              </div>

              {mode === 'color' ? (
                <>
                  <div className="bg-zinc-900/50 rounded-xl p-3 mb-3">
                    <HsvaColorPicker color={color} onChange={handleColorChange} />
                  </div>
                  <div className="grid grid-cols-8 gap-1.5">
                    {quickColors.map((c, i) => (
                      <button key={i} onClick={() => applyQuickColor(c.h, c.s)}
                        className="aspect-square rounded-lg hover:scale-110 transition-transform"
                        style={{ backgroundColor: c.bg }} />
                    ))}
                  </div>
                </>
              ) : (
                <div className="bg-zinc-900/50 rounded-xl p-3">
                  <div className="flex items-center justify-between mb-2">
                    <div className="flex items-center gap-2">
                      <Thermometer size={14} className="text-orange-400" />
                      <span className="text-xs text-zinc-400">Temperature</span>
                    </div>
                    <span className="text-sm font-medium text-orange-400">{colorTemp}K</span>
                  </div>
                  <input type="range" min="2500" max="6500" value={colorTemp}
                    onChange={e => handleColorTemp(Number(e.target.value))}
                    className="w-full temp-slider" />
                  <div className="flex justify-between mt-1 text-[10px] text-zinc-600">
                    <span>Warm</span>
                    <span>Cool</span>
                  </div>
                </div>
              )}
            </div>
          </>
        )}

        {devices.length === 0 && (
          <div className="text-center py-8">
            <p className="text-zinc-500 text-sm">No devices yet</p>
            <button onClick={() => setShowAddDevice(true)}
              className="mt-2 px-4 py-2 bg-violet-600 hover:bg-violet-500 rounded-lg text-sm font-medium transition-colors">
              Add Device
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
