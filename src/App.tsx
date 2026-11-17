import { useEffect, useState, useRef } from 'react';
import { HsvColor, HsvColorPicker } from 'react-colorful';
import { TitleBar } from './components/TitleBar';
import { EffectsPanel } from './components/EffectsPanel';
import { LightingDiagnostics } from './components/LightingDiagnostics';
import { applyEffect, Experiment, runExperiment } from './lib/effects';
import {
  classicTarget, ConfirmedEffect, createEffectController, CustomColors, CustomMode, EffectColor, EffectKind,
  EffectTarget, EffectView, isRainbowAt, RAINBOW_ID, RainbowSpeed, StripSettings, stripSettings, updateStrip, withCustomColor,
} from './lib/effectState';

type Mode = 'color' | 'white' | EffectView;
const isEffectMode = (m: Mode): m is EffectView => m === 'classic' || m === 'segment' || m === 'custom';
/** An error message that belongs to one strip; shown only while that strip is selected. */
type StripError = { ip: string; message: string } | null;
import { Power, Sun, Wifi, Lock, Mail, Loader2, LogOut, Thermometer, Sparkles, Search, Plus, Trash2, RefreshCw, ChevronLeft, Settings } from 'lucide-react';
import { invoke } from '@tauri-apps/api/core';
import { isEnabled, enable, disable } from '@tauri-apps/plugin-autostart';

interface DeviceInfo {
  ip: string;
  nickname: string;
  model: string | null;
  connected: boolean;
  device_on: boolean | null;
  brightness: number | null;
  hue?: number | null;
  saturation?: number | null;
  supports_effects?: boolean | null;
}

export default function App() {
  const [loggedIn, setLoggedIn] = useState(false);
  const [loading, setLoading] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState('');

  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [rememberMe, setRememberMe] = useState(false);
  const [deviceIp, setDeviceIp] = useState('');
  const [scanning, setScanning] = useState(false);
  const [discoveredDevices, setDiscoveredDevices] = useState<string[]>([]);

  // Multi-device state
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedDevice, setSelectedDevice] = useState<string | null>(null);
  const [showAddDevice, setShowAddDevice] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [autoStart, setAutoStart] = useState(false);

  // Control state for selected device
  const [color, setColor] = useState<HsvColor>({ h: 200, s: 100, v: 100 });
  const [brightness, setBrightness] = useState(100);
  const [colorTemp, setColorTemp] = useState(4000);
  const [mode, setMode] = useState<Mode>('color');
  // Keyed by device IP; only filled after the device accepted the effect.
  const [confirmedEffects, setConfirmedEffects] = useState<Record<string, ConfirmedEffect>>({});
  const [lastColors, setLastColors] = useState<Record<string, { h: number; s: number }>>({});
  const [lastTemps, setLastTemps] = useState<Record<string, number>>({});
  // Per device: last accepted preset of each kind, and which view was last.
  const [lastEffects, setLastEffects] = useState<
    Record<string, Partial<Record<EffectKind, string>> & { latest?: EffectView }>
  >({});
  // Per device: Rainbow speed and custom colors, for this session only.
  const [strips, setStrips] = useState<Record<string, StripSettings>>({});
  const stripsRef = useRef<Record<string, StripSettings>>({});
  // Per device: the effect most recently sent, accepted or still in flight.
  // A delayed brightness resend uses it so it never restores a replaced effect.
  const effects = useRef(createEffectController(applyEffect)).current;
  const brightnessRef = useRef<Record<string, number>>({});
  const modeRef = useRef<Record<string, Mode>>({});
  const lastColorRef = useRef<Record<string, { h: number; s: number }>>({});
  const lastTempRef = useRef<Record<string, number>>({});
  const [brightnessFailure, setBrightnessFailure] = useState<StripError>(null);
  const [controlFailure, setControlFailure] = useState<StripError>(null);
  // The strip selected right now, for async replies that arrive after a switch.
  const selectedRef = useRef<string | null>(null);
  selectedRef.current = selectedDevice;
  const [applyingMode, setApplyingMode] = useState(false);
  const [modeStatus, setModeStatus] = useState('');
  const modeRequest = useRef(0);

  const colorTimeout = useRef<ReturnType<typeof setTimeout>>(undefined);
  // One pending brightness change per strip, so selecting the other strip does not drop it.
  const brightnessTimeouts = useRef<Record<string, ReturnType<typeof setTimeout>>>({});
  const tempTimeout = useRef<ReturnType<typeof setTimeout>>(undefined);
  const cancelBrightness = (ip: string) => {
    clearTimeout(brightnessTimeouts.current[ip]);
    delete brightnessTimeouts.current[ip];
  };

  const loadDevices = async () => {
    try {
      const data: { devices: DeviceInfo[] } = await invoke('list_devices');
      setDevices(data.devices);
      if (data.devices.length > 0 && !selectedDevice) {
        setSelectedDevice(data.devices[0].ip);
      }
    } catch (err) {
      console.error('Failed to load devices:', err);
    }
  };

  useEffect(() => {
    // Load auth status
    invoke<{ authenticated: boolean; connected: boolean }>('auth_status')
      .then(async (statusData) => {
        if (statusData.authenticated) {
          setLoggedIn(true);
          await loadDevices();
        }
      })
      .catch(() => {})
      .finally(() => setLoading(false));

    // Load auto-start setting
    isEnabled().then(setAutoStart).catch(() => {});

    // Load saved credentials
    invoke<{ email: string; password: string; rememberMe: boolean } | null>('get_saved_credentials')
      .then((creds) => {
        if (creds) {
          setEmail(creds.email || '');
          setPassword(creds.password || '');
          setRememberMe(creds.rememberMe || false);
        }
      })
      .catch(() => {});
  }, []);

  // Load device info when selected device changes
  useEffect(() => {
    if (selectedDevice) {
      const device = devices.find(d => d.ip === selectedDevice);
      if (device && device.connected) {
        invoke<{ brightness: number; hue: number | null; saturation: number | null; color_temp: number | null }>('device_info', { ip: selectedDevice })
          .then(info => {
            if (selectedRef.current !== selectedDevice) return;
            const remembered = brightnessRef.current[selectedDevice];
            if (remembered === undefined) brightnessRef.current[selectedDevice] = info.brightness || 100;
            setBrightness(remembered ?? (info.brightness || 100));
            const chosen = lastColorRef.current[selectedDevice];
            if (chosen) {
              setColor(c => ({ ...c, h: chosen.h, s: chosen.s }));
            } else if (info.hue !== null && info.saturation !== null) {
              setColor(c => ({ ...c, h: info.hue!, s: info.saturation! }));
            }
            const temp = lastTempRef.current[selectedDevice];
            if (temp) setColorTemp(temp);
            else if (info.color_temp) setColorTemp(info.color_temp);
          })
          .catch(() => {});
      }
    }
  }, [selectedDevice, devices]);

  // Each strip keeps its own tab. Selecting a strip only changes the view; it sends nothing.
  useEffect(() => {
    modeRequest.current++;
    setApplyingMode(false);
    setModeStatus('');
    if (!selectedDevice) return;
    setMode(modeRef.current[selectedDevice] ?? 'color');
    const remembered = brightnessRef.current[selectedDevice];
    if (remembered !== undefined) setBrightness(remembered);
  }, [selectedDevice]);

  // Reconnects every saved device that is not connected yet, using the session
  // the login just created. Offline devices stay listed with a Reconnect button.
  // Returns how many devices are saved.
  const reconnectSavedDevices = async (): Promise<number> => {
    const data: { devices: DeviceInfo[] } = await invoke('list_devices');
    await Promise.allSettled(
      data.devices
        .filter(d => !d.connected)
        .map(d => invoke('reconnect_device', { ip: d.ip })),
    );
    await loadDevices();
    return data.devices.length;
  };

  const handleLogin = async (e: React.FormEvent) => {
    e.preventDefault();
    setConnecting(true);
    setError('');
    try {
      await invoke('login', { req: { email, password, device_ip: null, remember_me: rememberMe } });
      setPassword('');
      setLoggedIn(true);
      // Saved devices come back without asking again; discovery only runs
      // when there is nothing saved yet (first use).
      const saved = await reconnectSavedDevices();
      if (saved === 0) {
        setShowAddDevice(true);
        scanNetwork();
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setConnecting(false);
    }
  };

  const handleAddDevice = async (e: React.FormEvent) => {
    e.preventDefault();
    setConnecting(true);
    setError('');
    try {
      const res = await invoke<{ device: { ip: string } }>('add_device', { ip: deviceIp });
      setShowAddDevice(false);
      setDeviceIp('');
      await loadDevices();
      setSelectedDevice(res.device.ip);
    } catch (err) {
      setError(String(err));
    } finally {
      setConnecting(false);
    }
  };

  const handleLogout = async () => {
    await invoke('logout');
    setLoggedIn(false);
    setDevices([]);
    setSelectedDevice(null);
  };

  const toggleAutoStart = async () => {
    const newValue = !autoStart;
    setAutoStart(newValue);
    try {
      if (newValue) {
        await enable();
      } else {
        await disable();
      }
    } catch (err) {
      console.error('Failed to toggle autostart:', err);
      setAutoStart(!newValue); // revert
    }
  };

  const handleRemoveDevice = async (ip: string) => {
    await invoke('delete_device', { ip });
    await loadDevices();
    if (selectedDevice === ip) {
      setSelectedDevice(devices.find(d => d.ip !== ip)?.ip || null);
    }
  };

  const handleReconnect = async (ip: string) => {
    try {
      await invoke('reconnect_device', { ip });
      await loadDevices();
    } catch (err) {
      console.error('Reconnect failed:', err);
    }
  };

  const scanNetwork = async () => {
    setScanning(true);
    setDiscoveredDevices([]);
    try {
      const data: { potential_devices: { ip: string }[] } = await invoke('discover_devices_cmd');
      const existingIps = devices.map(d => d.ip);
      setDiscoveredDevices(
        data.potential_devices
          .map((d) => d.ip)
          .filter((ip) => !existingIps.includes(ip))
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
    await invoke('device_power', { req: { on: newState, ip: selectedDevice } });
    await loadDevices();
  };

  const rememberColor = (ip: string, h: number, s: number) => {
    const chosen = { h: Math.round(h), s: Math.min(100, Math.max(1, Math.round(s))) };
    lastColorRef.current = { ...lastColorRef.current, [ip]: chosen };
    setLastColors(lastColorRef.current);
    return chosen;
  };

  const rememberTemp = (ip: string, kelvin: number) => {
    const chosen = Math.min(6500, Math.max(2500, Math.round(kelvin)));
    lastTempRef.current = { ...lastTempRef.current, [ip]: chosen };
    setLastTemps(lastTempRef.current);
    return chosen;
  };

  // `null` after a color, white, plain brightness or experiment command.
  const setConfirmedEffect = (ip: string, effect: ConfirmedEffect | null) => {
    if (effect) {
      const target = effect.target;
      setLastEffects(prev => ({
        ...prev,
        [ip]: target.kind === 'custom'
          ? { ...prev[ip], latest: 'custom' }
          : { ...prev[ip], [target.kind]: target.id, latest: target.kind },
      }));
    } else {
      effects.clear(ip);
    }
    setConfirmedEffects(prev => {
      const next = { ...prev };
      if (effect) next[ip] = effect;
      else delete next[ip];
      return next;
    });
  };

  const changeStrip = (ip: string, change: (s: StripSettings) => StripSettings) => {
    stripsRef.current = updateStrip(stripsRef.current, ip, change);
    setStrips(stripsRef.current);
  };

  const brightnessOf = (ip: string) => brightnessRef.current[ip] ?? brightness;

  const activeEffect = selectedDevice ? confirmedEffects[selectedDevice] ?? null : null;
  const effectBrightness = isEffectMode(mode) && activeEffect !== null;

  // Every effect request goes through here. Commands to one strip are applied
  // in send order (backend `CommandGate`), so the last request sent wins. A
  // reply to a request that was replaced meanwhile is not recorded.
  const sendEffect = async (ip: string, target: EffectTarget, value: number) => {
    cancelBrightness(ip);
    const { confirmed, current } = await effects.send(ip, target, value);
    if (current) setConfirmedEffect(ip, confirmed);
  };

  // Runs a color, white, plain brightness or experiment command. On success the
  // strip no longer shows an effect, unless an effect was sent after this one.
  const sendPlainCommand = async <T,>(ip: string, command: () => Promise<T>): Promise<T> => {
    const before = effects.snapshot(ip);
    const result = await command();
    if (effects.clearIfUnchanged(ip, before)) setConfirmedEffect(ip, null);
    return result;
  };

  const clearFailure = (setter: React.Dispatch<React.SetStateAction<StripError>>, ip: string) =>
    setter(prev => (prev?.ip === ip ? null : prev));
  const failure = (current: StripError) => (current && current.ip === selectedDevice ? current.message : '');

  const activateEffect = async (ip: string, kind: EffectKind, id: string) => {
    clearFailure(setBrightnessFailure, ip);
    const target: EffectTarget = kind === 'classic'
      ? classicTarget(id, stripSettings(stripsRef.current, ip).rainbowSpeed)
      : { kind: 'segment', id };
    await sendEffect(ip, target, brightnessOf(ip));
  };

  // The selector follows what the strip was last asked to show, so a late reply
  // for an older speed never overwrites a newer choice.
  const rememberSpeedIfCurrent = (ip: string, speed: RainbowSpeed) => {
    if (isRainbowAt(effects.current(ip), speed)) changeStrip(ip, s => ({ ...s, rainbowSpeed: speed }));
  };

  // Sent right away only while Rainbow is this strip's latest effect; otherwise
  // the choice is kept for the next time Rainbow is chosen.
  const changeRainbowSpeed = async (ip: string, speed: RainbowSpeed) => {
    const current = effects.current(ip);
    if (current?.kind === 'classic' && current.id === RAINBOW_ID) {
      clearFailure(setBrightnessFailure, ip);
      await sendEffect(ip, classicTarget(RAINBOW_ID, speed), brightnessOf(ip));
      rememberSpeedIfCurrent(ip, speed);
      return;
    }
    changeStrip(ip, s => ({ ...s, rainbowSpeed: speed }));
  };

  const applyCustomEffect = async (ip: string, mode: CustomMode, colors: CustomColors) => {
    clearFailure(setBrightnessFailure, ip);
    await sendEffect(ip, { kind: 'custom', mode, colors }, brightnessOf(ip));
  };

  const restoreRainbow = async (ip: string) => {
    await sendEffect(ip, classicTarget(RAINBOW_ID, 'normal'), brightnessOf(ip));
    rememberSpeedIfCurrent(ip, 'normal');
  };

  // Experiments are not presets and are not saved. Clearing the confirmed
  // effect keeps a later brightness change from re-sending a preset over them.
  const runLightingExperiment = async (ip: string, experiment: Experiment) => {
    cancelBrightness(ip);
    return sendPlainCommand(ip, () => runExperiment(ip, experiment));
  };

  // `tapo` documents that plain brightness/color/temperature requests remove a
  // running effect, so in Effects mode the effect is re-sent with the new
  // brightness instead. Slower debounce: each call resends the whole effect,
  // with the strip's own speed or colors.
  const handleBrightness = (val: number) => {
    setBrightness(val);
    const ip = selectedDevice;
    if (!ip) return;
    clearFailure(setBrightnessFailure, ip);
    brightnessRef.current[ip] = val;
    cancelBrightness(ip);
    const report = (err: unknown) => setBrightnessFailure({ ip, message: String(err) });
    if (isEffectMode(mode) && effects.current(ip)) {
      brightnessTimeouts.current[ip] = setTimeout(async () => {
        delete brightnessTimeouts.current[ip];
        try {
          const res = await effects.resend(ip, val);
          if (res?.current) setConfirmedEffect(ip, res.confirmed);
        } catch (err) {
          report(err);
        }
      }, 400);
      return;
    }
    brightnessTimeouts.current[ip] = setTimeout(async () => {
      delete brightnessTimeouts.current[ip];
      try {
        await sendPlainCommand(ip, () => invoke('device_brightness', { req: { value: val, ip } }));
      } catch (err) {
        report(err);
      }
    }, 100);
  };

  // Color and White errors are shown for their strip; nothing is marked as applied.
  const sendColor = async (ip: string, hue: number, saturation: number) => {
    clearFailure(setControlFailure, ip);
    try {
      await sendPlainCommand(ip, () => invoke('device_color', { req: { hue, saturation, ip } }));
    } catch (err) {
      setControlFailure({ ip, message: String(err) });
    }
  };

  const sendTemperature = async (ip: string, kelvin: number) => {
    clearFailure(setControlFailure, ip);
    try {
      await sendPlainCommand(ip, () => invoke('device_temperature', { req: { kelvin, ip } }));
    } catch (err) {
      setControlFailure({ ip, message: String(err) });
    }
  };

  const handleColorChange = (newColor: HsvColor) => {
    setColor(newColor);
    clearTimeout(colorTimeout.current);
    const ip = selectedDevice;
    if (!ip) return;
    rememberColor(ip, newColor.h, newColor.s);
    const hue = Math.round(newColor.h);
    const saturation = Math.min(100, Math.max(1, Math.round(newColor.s)));
    colorTimeout.current = setTimeout(() => { void sendColor(ip, hue, saturation); }, 100);
  };

  const handleColorTemp = (val: number) => {
    setColorTemp(val);
    clearTimeout(tempTimeout.current);
    const ip = selectedDevice;
    if (!ip) return;
    rememberTemp(ip, val);
    tempTimeout.current = setTimeout(() => { void sendTemperature(ip, val); }, 100);
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

  // Every tab reapplies its own last value, in any switch order. A pending
  // brightness resend must not restore the previous mode afterwards.
  // Opening Custom never sends: its colors are only sent with Apply.
  const switchMode = async (next: Mode) => {
    if (next === mode || applyingMode) return;
    const ip = selectedDevice;
    if (ip) cancelBrightness(ip);
    clearTimeout(colorTimeout.current);
    clearTimeout(tempTimeout.current);
    setMode(next);
    setModeStatus('');
    if (!ip) return;
    modeRef.current[ip] = next;
    if (next === 'custom') return;
    const effectId = isEffectMode(next) ? lastEffects[ip]?.[next] : undefined;
    if (isEffectMode(next) && !effectId) return;

    const request = ++modeRequest.current;
    setApplyingMode(true);
    setModeStatus(isEffectMode(next) ? 'Applying effect…' : next === 'color' ? 'Applying color…' : 'Applying white…');
    try {
      if (isEffectMode(next) && effectId) {
        const target: EffectTarget = next === 'classic'
          ? classicTarget(effectId, stripSettings(stripsRef.current, ip).rainbowSpeed)
          : { kind: 'segment', id: effectId };
        await sendEffect(ip, target, brightnessOf(ip));
        if (modeRequest.current !== request) return;
      } else if (next === 'color') {
        const chosen = lastColors[ip] ?? rememberColor(ip, color.h, color.s);
        setColor(c => ({ ...c, h: chosen.h, s: chosen.s }));
        await sendPlainCommand(ip, () => invoke('device_color', { req: { hue: chosen.h, saturation: chosen.s, ip } }));
        if (modeRequest.current !== request) return;
      } else {
        const kelvin = lastTemps[ip] ?? rememberTemp(ip, colorTemp);
        setColorTemp(kelvin);
        await sendPlainCommand(ip, () => invoke('device_temperature', { req: { kelvin, ip } }));
        if (modeRequest.current !== request) return;
      }
      setModeStatus('');
    } catch (err) {
      if (modeRequest.current === request) setModeStatus(String(err));
    } finally {
      if (modeRequest.current === request) setApplyingMode(false);
    }
  };

  const applyQuickColor = async (h: number, s: number) => {
    setColor({ ...color, h, s });
    setMode('color');
    const ip = selectedDevice;
    if (!ip) return;
    modeRef.current[ip] = 'color';
    rememberColor(ip, h, s);
    await sendColor(ip, h, s);
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
            <label className="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" checked={rememberMe} onChange={e => setRememberMe(e.target.checked)}
                className="w-4 h-4 rounded bg-zinc-900 border-zinc-700 text-violet-600 focus:ring-violet-500 focus:ring-offset-0" />
              <span className="text-sm text-zinc-400">Remember me</span>
            </label>
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

  // Settings screen
  if (showSettings) {
    return (
      <div className="h-screen flex flex-col bg-[#0a0a12]">
        <TitleBar />
        <div className="flex-1 flex flex-col p-4">
          <button onClick={() => setShowSettings(false)} className="flex items-center gap-1 text-zinc-400 hover:text-white mb-4">
            <ChevronLeft size={16} /> Back
          </button>
          <h2 className="text-lg font-semibold mb-4">Settings</h2>

          <div className="space-y-3">
            <div className="flex items-center justify-between p-3 bg-zinc-900/50 rounded-xl">
              <div>
                <p className="text-sm font-medium">Start on login</p>
                <p className="text-xs text-zinc-500">Launch app when Windows starts</p>
              </div>
              <button onClick={toggleAutoStart}
                className={`w-12 h-6 rounded-full transition-colors ${autoStart ? 'bg-violet-600' : 'bg-zinc-700'}`}>
                <div className={`w-5 h-5 rounded-full bg-white shadow transition-transform ${autoStart ? 'translate-x-6' : 'translate-x-0.5'}`} />
              </button>
            </div>
          </div>
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
            <button onClick={() => setShowSettings(true)}
              className="p-1.5 text-zinc-400 hover:text-violet-400 hover:bg-zinc-800 rounded-lg transition-colors" title="Settings">
              <Settings size={16} />
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
                {effectBrightness && (
                  <p className="mt-1 text-[10px] text-zinc-500">Re-applies the active effect with this brightness</p>
                )}
                {failure(brightnessFailure) && <p className="mt-1 text-[10px] text-red-400">{failure(brightnessFailure)}</p>}
              </div>

              {/* Mode Tabs */}
              <div className="flex gap-1 p-1 bg-zinc-900/50 rounded-lg mb-3">
                <button onClick={() => switchMode('color')} disabled={applyingMode}
                  className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors disabled:opacity-50 ${mode === 'color' ? 'bg-violet-600 text-white' : 'text-zinc-400 hover:text-zinc-200'}`}>
                  Color
                </button>
                <button onClick={() => switchMode('white')} disabled={applyingMode}
                  className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors disabled:opacity-50 ${mode === 'white' ? 'bg-amber-500 text-white' : 'text-zinc-400 hover:text-zinc-200'}`}>
                  White
                </button>
                <button onClick={() => switchMode(lastEffects[currentDevice.ip]?.latest ?? 'classic')} disabled={applyingMode}
                  className={`flex-1 py-1.5 rounded-md text-xs font-medium transition-colors disabled:opacity-50 ${isEffectMode(mode) ? 'bg-fuchsia-600 text-white' : 'text-zinc-400 hover:text-zinc-200'}`}>
                  Effects
                </button>
              </div>
              {modeStatus && (
                <p className={`mb-2 text-[10px] ${modeStatus.startsWith('Applying') ? 'text-zinc-400' : 'text-red-400'}`}>
                  {modeStatus}
                </p>
              )}

              {isEffectMode(mode) ? (
                <div className="space-y-2">
                  <EffectsPanel
                    deviceIp={currentDevice.ip}
                    supported={currentDevice.supports_effects === true}
                    view={mode}
                    confirmed={activeEffect}
                    selectedId={mode === 'custom' ? null : lastEffects[currentDevice.ip]?.[mode] ?? null}
                    busy={applyingMode}
                    settings={stripSettings(strips, currentDevice.ip)}
                    onViewChange={switchMode}
                    onActivate={activateEffect}
                    onRainbowSpeed={changeRainbowSpeed}
                    onApplyCustom={applyCustomEffect}
                    onCustomModeChange={(ip, customMode) => changeStrip(ip, s => ({ ...s, customMode }))}
                    onCustomColorChange={(ip, customMode, index, c: EffectColor) =>
                      changeStrip(ip, s => withCustomColor(s, customMode, index, c))} />
                  {currentDevice.supports_effects === true && (
                    <LightingDiagnostics
                      deviceIp={currentDevice.ip}
                      deviceName={currentDevice.nickname || currentDevice.model || 'this strip'}
                      busy={applyingMode}
                      onRun={runLightingExperiment}
                      onRestore={restoreRainbow} />
                  )}
                </div>
              ) : mode === 'color' ? (
                <>
                  <div className="bg-zinc-900/50 rounded-xl p-3 mb-3">
                    <HsvColorPicker color={color} onChange={handleColorChange} />
                  </div>
                  <div className="grid grid-cols-8 gap-1.5">
                    {quickColors.map((c, i) => (
                      <button key={i} onClick={() => applyQuickColor(c.h, c.s)}
                        className="aspect-square rounded-lg hover:scale-110 transition-transform"
                        style={{ backgroundColor: c.bg }} />
                    ))}
                  </div>
                  {failure(controlFailure) && <p className="mt-2 text-[10px] text-red-400">{failure(controlFailure)}</p>}
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
                  {failure(controlFailure) && <p className="mt-2 text-[10px] text-red-400">{failure(controlFailure)}</p>}
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
