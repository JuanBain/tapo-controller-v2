import { useEffect, useRef, useState } from 'react';
import { CheckCircle2, Loader2, AlertCircle } from 'lucide-react';
import { EffectCatalogs, fetchEffectCatalogs } from '../lib/effects';
import {
  ConfirmedEffect, CUSTOM_MODES, CustomColors, CustomMode, EffectColor, effectColorCss, EffectKind, EffectTarget, EffectView,
  RAINBOW_ID, RAINBOW_SPEEDS, RainbowSpeed, StripSettings,
} from '../lib/effectState';
import { CustomEffectsEditor } from './CustomEffectsEditor';

interface EffectsPanelProps {
  deviceIp: string;
  supported: boolean;
  view: EffectView;
  /** Last effect the device accepted this session, of any kind. */
  confirmed: ConfirmedEffect | null;
  /** Last accepted preset of this view's kind, to highlight it. */
  selectedId: string | null;
  busy: boolean;
  settings: StripSettings;
  onViewChange: (view: EffectView) => void;
  /** Each of these sends a command and resolves once the device accepted it. */
  onActivate: (ip: string, kind: EffectKind, id: string) => Promise<void>;
  onRainbowSpeed: (ip: string, speed: RainbowSpeed) => Promise<void>;
  onApplyCustom: (ip: string, mode: CustomMode, colors: CustomColors) => Promise<void>;
  /** Editing never sends anything. */
  onCustomModeChange: (ip: string, mode: CustomMode) => void;
  onCustomColorChange: (ip: string, mode: CustomMode, index: number, color: EffectColor) => void;
}

const VIEW_LABEL: Record<EffectView, string> = { classic: 'Classic', segment: 'Segment', custom: 'Custom' };
const SPEED_LABEL = Object.fromEntries(RAINBOW_SPEEDS.map(s => [s.id, s.label])) as Record<RainbowSpeed, string>;
const MODE_LABEL = Object.fromEntries(CUSTOM_MODES.map(m => [m.id, m.label])) as Record<CustomMode, string>;

export function EffectsPanel({
  deviceIp, supported, view, confirmed, selectedId, busy, settings,
  onViewChange, onActivate, onRainbowSpeed, onApplyCustom, onCustomModeChange, onCustomColorChange,
}: EffectsPanelProps) {
  const [catalogs, setCatalogs] = useState<EffectCatalogs>({ classic: [], segment: [] });
  const [catalogError, setCatalogError] = useState('');
  const [pending, setPending] = useState<{ key: string; label: string } | null>(null);
  const [error, setError] = useState('');
  const requestIp = useRef(deviceIp);

  useEffect(() => {
    fetchEffectCatalogs()
      .then(setCatalogs)
      .catch(err => setCatalogError(String(err)));
  }, []);

  useEffect(() => {
    requestIp.current = deviceIp;
    setPending(null);
    setError('');
  }, [deviceIp]);

  const labelOf = (k: EffectKind, id: string) => catalogs[k].find(e => e.id === id)?.label ?? id;

  const describe = (target: EffectTarget) => {
    if (target.kind === 'custom') return `Custom ${MODE_LABEL[target.mode]}`;
    const label = labelOf(target.kind, target.id);
    return target.kind === 'classic' && target.rainbowSpeed ? `${label} · ${SPEED_LABEL[target.rainbowSpeed]}` : label;
  };

  const run = async (key: string, label: string, action: (ip: string) => Promise<void>) => {
    if (pending || busy) return;
    const ip = deviceIp;
    setPending({ key, label });
    setError('');
    try {
      await action(ip);
    } catch (err) {
      if (requestIp.current === ip) setError(String(err));
    } finally {
      if (requestIp.current === ip) setPending(null);
    }
  };

  if (!supported) {
    return (
      <div className="bg-zinc-900/50 rounded-xl p-3 text-xs text-zinc-500">
        This device did not report an RGBIC model (L920/L930); effects are disabled.
      </div>
    );
  }

  const kind: EffectKind | null = view === 'custom' ? null : view;
  const busySelected = busy && kind && selectedId ? { key: `${kind}:${selectedId}`, label: labelOf(kind, selectedId) } : null;
  const sending = pending ?? busySelected;
  const locked = pending !== null || busy;
  const views: EffectView[] = ['classic', 'segment', 'custom'];

  return (
    <div className="space-y-2">
      <div className="flex gap-1 p-1 bg-zinc-900/50 rounded-lg">
        {views.map(v => (
          <button key={v} onClick={() => onViewChange(v)} disabled={locked}
            className={`flex-1 py-1 rounded-md text-[11px] font-medium transition-colors disabled:opacity-50 ${
              view === v ? 'bg-fuchsia-600/80 text-white' : 'text-zinc-400 hover:text-zinc-200'
            }`}>
            {VIEW_LABEL[v]}{v !== 'custom' && catalogs[v].length ? ` (${catalogs[v].length})` : ''}
          </button>
        ))}
      </div>

      <div className="bg-zinc-900/50 rounded-xl p-2 text-xs min-h-[2rem] flex items-center gap-2">
        {sending ? (
          <><Loader2 size={12} className="animate-spin text-violet-400" />
            <span className="text-zinc-400">Sending {sending.label}...</span></>
        ) : error ? (
          <><AlertCircle size={12} className="text-red-400 shrink-0" /><span className="text-red-400">{error}</span></>
        ) : confirmed ? (
          <><CheckCircle2 size={12} className="text-emerald-400 shrink-0" />
            <span className="text-zinc-300">{describe(confirmed.target)}</span>
            {confirmed.target.kind === 'custom' && (
              <span className="flex gap-0.5">
                {confirmed.target.colors.map((c, i) => (
                  <span key={i} className="w-2.5 h-2.5 rounded-sm border border-zinc-700" style={{ backgroundColor: effectColorCss(c) }} />
                ))}
              </span>
            )}
            <span className="text-zinc-500">
              last accepted this session ({VIEW_LABEL[confirmed.target.kind]}, {confirmed.elapsedMs} ms)
            </span></>
        ) : (
          <span className="text-zinc-500">No effect confirmed in this session</span>
        )}
      </div>
      {confirmed && !sending && !error && (
        <p className="text-[10px] text-zinc-600">
          The strip cannot report its active effect; this may differ from its physical state.
        </p>
      )}

      {catalogError && <p className="text-red-400 text-xs">{catalogError}</p>}

      {view === 'custom' ? (
        <CustomEffectsEditor
          mode={settings.customMode}
          colors={settings.customColors[settings.customMode]}
          disabled={locked}
          onModeChange={mode => onCustomModeChange(deviceIp, mode)}
          onColorChange={(index, color) => onCustomColorChange(deviceIp, settings.customMode, index, color)}
          onApply={() => {
            const mode = settings.customMode;
            const colors = settings.customColors[mode];
            return run(`custom:${mode}`, `Custom ${MODE_LABEL[mode]}`, ip => onApplyCustom(ip, mode, colors));
          }} />
      ) : (
        <>
          {kind === 'classic' && (
            <div className="bg-zinc-900/50 rounded-xl p-2 space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-[11px] text-zinc-400">Rainbow speed</span>
                <div className="flex gap-1">
                  {RAINBOW_SPEEDS.map(s => (
                    <button key={s.id} disabled={locked}
                      onClick={() => run(`speed:${s.id}`, `Rainbow · ${s.label}`, ip => onRainbowSpeed(ip, s.id))}
                      className={`px-2 py-0.5 rounded-md text-[11px] transition-colors disabled:opacity-50 ${
                        settings.rainbowSpeed === s.id ? 'bg-violet-600/80 text-white' : 'bg-zinc-800 text-zinc-400 hover:text-zinc-200'
                      }`}>
                      {s.label}
                    </button>
                  ))}
                </div>
              </div>
              <p className="text-[10px] text-zinc-600">
                Rainbow only. Sent right away while Rainbow is the active effect; otherwise used the next time Rainbow is chosen.
              </p>
            </div>
          )}

          <div className="grid grid-cols-2 gap-1.5">
            {kind && catalogs[kind].map(effect => {
              const target = confirmed?.target;
              const isConfirmed = target?.kind === kind && target.id === effect.id;
              const isPending = sending?.key === `${kind}:${effect.id}`
                || (kind === 'classic' && effect.id === RAINBOW_ID && sending?.key.startsWith('speed:') === true);
              const isSelected = !isConfirmed && selectedId === effect.id;
              return (
                <button key={effect.id} disabled={locked}
                  onClick={() => run(`${kind}:${effect.id}`, labelOf(kind, effect.id), ip => onActivate(ip, kind, effect.id))}
                  className={`px-2 py-2 rounded-lg text-xs text-left transition-colors flex items-center justify-between gap-1 disabled:cursor-not-allowed ${
                    isConfirmed
                      ? 'bg-violet-600/30 border border-violet-500 text-white'
                      : isPending || isSelected
                        ? 'bg-zinc-800 border border-violet-500/50 text-zinc-200'
                        : 'bg-zinc-900/50 border border-zinc-800 text-zinc-300 hover:border-zinc-700 disabled:opacity-50'
                  }`}>
                  <span className="truncate">{effect.label}</span>
                  {isPending && <Loader2 size={12} className="animate-spin shrink-0" />}
                  {isConfirmed && !isPending && <CheckCircle2 size={12} className="text-emerald-400 shrink-0" />}
                </button>
              );
            })}
          </div>
        </>
      )}
    </div>
  );
}
