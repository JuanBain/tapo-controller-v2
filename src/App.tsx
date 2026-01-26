import { useEffect, useState, useRef } from 'react';
import { HsvaColor, HsvaColorPicker } from 'react-colorful';
import { TitleBar } from './components/TitleBar';
import { Power, Sun, Wifi, Lock, Mail, Loader2, LogOut, Thermometer, Sparkles, Search } from 'lucide-react';

const API = 'http://127.0.0.1:8742';

interface DeviceInfo {
  nickname: string;
  device_on: boolean;
  brightness: number;
  hue: number | null;
  saturation: number | null;
}

export default function App() {
  const [connected, setConnected] = useState(false);
  const [loading, setLoading] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState('');
  const [device, setDevice] = useState<DeviceInfo | null>(null);

  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [deviceIp, setDeviceIp] = useState('');
  const [scanning, setScanning] = useState(false);
  const [discoveredDevices, setDiscoveredDevices] = useState<string[]>([]);

  const [color, setColor] = useState<HsvaColor>({ h: 200, s: 100, v: 100, a: 1 });
  const [brightness, setBrightness] = useState(100);
  const [colorTemp, setColorTemp] = useState(4000);
  const [mode, setMode] = useState<'color' | 'white'>('color');

  const colorTimeout = useRef<NodeJS.Timeout>();
  const brightnessTimeout = useRef<NodeJS.Timeout>();
  const tempTimeout = useRef<NodeJS.Timeout>();

  useEffect(() => {
    Promise.all([
      fetch(`${API}/auth/status`).then(r => r.json()),
      fetch(`${API}/auth/last-ip`).then(r => r.json()),
    ])
      .then(([statusData, lastIpData]) => {
        if (statusData.connected) {
          setConnected(true);
          setDevice(statusData.device);
          setBrightness(statusData.device.brightness);
          if (statusData.device.hue !== null) {
            setColor(c => ({ ...c, h: statusData.device.hue, s: statusData.device.saturation }));
          }
        }
        if (lastIpData.ip) {
          setDeviceIp(lastIpData.ip);
        }
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const handleLogin = async (e: React.FormEvent) => {
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
      setConnected(true);
      setDevice(data.device);
      setBrightness(data.device.brightness);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setConnecting(false);
    }
  };

  const handleLogout = async () => {
    await fetch(`${API}/auth/logout`, { method: 'POST' });
    setConnected(false);
    setDevice(null);
  };

  const scanNetwork = async () => {
    setScanning(true);
    setDiscoveredDevices([]);
    try {
      const res = await fetch(`${API}/discover`);
      const data = await res.json();
      setDiscoveredDevices(data.potential_devices.map((d: { ip: string }) => d.ip));
    } catch (err) {
      console.error('Scan failed:', err);
    } finally {
      setScanning(false);
    }
  };

  const togglePower = async () => {
    if (!device) return;
    const newState = !device.device_on;
    await fetch(`${API}/device/power`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ on: newState }),
    });
    setDevice({ ...device, device_on: newState });
  };

  const handleBrightness = (val: number) => {
    setBrightness(val);
    clearTimeout(brightnessTimeout.current);
    brightnessTimeout.current = setTimeout(async () => {
      await fetch(`${API}/device/brightness`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ value: val }),
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
        body: JSON.stringify({ hue: Math.round(newColor.h), saturation: Math.round(newColor.s) }),
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
        body: JSON.stringify({ kelvin: val }),
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
      body: JSON.stringify({ hue: h, saturation: s }),
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

  return (
    <div className="h-screen flex flex-col bg-[#0a0a12]">
      <TitleBar />

      {!connected ? (
        <div className="flex-1 flex flex-col items-center justify-center p-8">
          <div className="w-14 h-14 rounded-2xl bg-gradient-to-br from-violet-500 to-fuchsia-500 flex items-center justify-center mb-6">
            <Sparkles size={28} className="text-white" />
          </div>
          <h1 className="text-xl font-semibold mb-1">Tapo Controller</h1>
          <p className="text-zinc-500 text-sm mb-6">Connect to your LED strip</p>

          <form onSubmit={handleLogin} className="w-full max-w-xs space-y-3">
            <div className="relative">
              <Mail size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
              <input type="email" value={email} onChange={e => setEmail(e.target.value)} placeholder="Email"
                className="w-full pl-10 pr-3 py-2.5 bg-zinc-900 border border-zinc-800 rounded-lg text-sm focus:outline-none focus:border-violet-500 transition-colors" required />
            </div>
            <div className="relative">
              <Lock size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
              <input type="password" value={password} onChange={e => setPassword(e.target.value)} placeholder="Password"
                className="w-full pl-10 pr-3 py-2.5 bg-zinc-900 border border-zinc-800 rounded-lg text-sm focus:outline-none focus:border-violet-500 transition-colors" required />
            </div>
            <div className="relative">
              <Wifi size={16} className="absolute left-3 top-1/2 -translate-y-1/2 text-zinc-500" />
              <input type="text" value={deviceIp} onChange={e => setDeviceIp(e.target.value)} placeholder="Device IP"
                className="w-full pl-10 pr-10 py-2.5 bg-zinc-900 border border-zinc-800 rounded-lg text-sm focus:outline-none focus:border-violet-500 transition-colors" required />
              <button type="button" onClick={scanNetwork} disabled={scanning}
                className="absolute right-2 top-1/2 -translate-y-1/2 p-1 text-zinc-500 hover:text-violet-400 disabled:opacity-50"
                title="Scan network">
                <Search size={16} className={scanning ? 'animate-pulse' : ''} />
              </button>
            </div>
            {discoveredDevices.length > 0 && (
              <div className="flex flex-wrap gap-1">
                {discoveredDevices.map(ip => (
                  <button key={ip} type="button" onClick={() => setDeviceIp(ip)}
                    className={`px-2 py-1 text-xs rounded-md transition-colors ${deviceIp === ip ? 'bg-violet-600 text-white' : 'bg-zinc-800 text-zinc-400 hover:bg-zinc-700'}`}>
                    {ip}
                  </button>
                ))}
              </div>
            )}
            {error && <p className="text-red-400 text-xs">{error}</p>}
            <button type="submit" disabled={connecting}
              className="w-full py-2.5 bg-violet-600 hover:bg-violet-500 rounded-lg text-sm font-medium disabled:opacity-50 flex items-center justify-center gap-2 transition-colors">
              {connecting ? <><Loader2 size={16} className="animate-spin" /> Connecting...</> : 'Connect'}
            </button>
          </form>
        </div>
      ) : (
        <div className="flex-1 overflow-auto p-3 space-y-3">
          {/* Header */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <div className={`w-2 h-2 rounded-full ${device?.device_on ? 'bg-emerald-400' : 'bg-zinc-600'}`} />
              <span className="text-sm font-medium">{device?.nickname || 'LED Strip'}</span>
              <span className="text-xs text-zinc-500">{device?.device_on ? 'On' : 'Off'}</span>
            </div>
            <div className="flex items-center gap-1">
              <button onClick={togglePower}
                className={`p-2 rounded-lg transition-colors ${device?.device_on ? 'bg-violet-600 text-white' : 'bg-zinc-800 text-zinc-400 hover:bg-zinc-700'}`}>
                <Power size={16} />
              </button>
              <button onClick={handleLogout} className="p-2 text-zinc-500 hover:text-zinc-300 hover:bg-zinc-800 rounded-lg transition-colors">
                <LogOut size={16} />
              </button>
            </div>
          </div>

          {/* Brightness */}
          <div className="bg-zinc-900/50 rounded-xl p-3">
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
          <div className="flex gap-1 p-1 bg-zinc-900/50 rounded-lg">
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
              {/* Color Picker */}
              <div className="bg-zinc-900/50 rounded-xl p-3">
                <HsvaColorPicker color={color} onChange={handleColorChange} />
              </div>

              {/* Quick Colors */}
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
      )}
    </div>
  );
}
