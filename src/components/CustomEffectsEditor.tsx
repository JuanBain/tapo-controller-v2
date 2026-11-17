import { useState } from 'react';
import { HsvColorPicker } from 'react-colorful';
import {
  CUSTOM_MODES, CustomColors, CustomMode, EffectColor, effectColorCss, effectColorError, toEffectColor,
} from '../lib/effectState';

interface CustomEffectsEditorProps {
  mode: CustomMode;
  colors: CustomColors;
  disabled: boolean;
  onModeChange: (mode: CustomMode) => void;
  onColorChange: (index: number, color: EffectColor) => void;
  /** The only action that sends a command. */
  onApply: () => Promise<void>;
}

const MODE_NOTE: Record<CustomMode, string> = {
  static: 'Based on the X1 payload. The three colors are shown at once.',
  sequence: 'Based on the X2 payload, with its fixed direction 1 and duration 50. No speed control yet.',
};

/** Editing never sends anything. */
export function CustomEffectsEditor({ mode, colors, disabled, onModeChange, onColorChange, onApply }: CustomEffectsEditorProps) {
  const [slot, setSlot] = useState(0);
  const invalid = colors.map(effectColorError).find(e => e !== null) ?? null;
  const current = colors[slot];

  return (
    <div className="bg-zinc-900/50 rounded-xl p-3 space-y-2">
      <span className="text-xs font-medium text-zinc-300">Custom Effects</span>

      <div className="flex gap-1 p-1 bg-zinc-900 rounded-lg">
        {CUSTOM_MODES.map(m => (
          <button key={m.id} onClick={() => onModeChange(m.id)} disabled={disabled}
            className={`flex-1 py-1 rounded-md text-[11px] font-medium transition-colors disabled:opacity-50 ${
              mode === m.id ? 'bg-violet-600/80 text-white' : 'text-zinc-400 hover:text-zinc-200'
            }`}>
            {m.label}
          </button>
        ))}
      </div>
      <p className="text-[10px] text-zinc-500">{MODE_NOTE[mode]}</p>

      <div className="flex gap-2">
        {colors.map((c, i) => (
          <button key={i} onClick={() => setSlot(i)}
            className={`flex-1 h-8 rounded-lg border-2 transition-colors ${slot === i ? 'border-violet-400' : 'border-zinc-800'}`}
            style={{ backgroundColor: effectColorCss(c) }}
            title={`Color ${i + 1}: H ${c.h} S ${c.s} V ${c.v}`} />
        ))}
      </div>

      <HsvColorPicker color={current} onChange={c => onColorChange(slot, toEffectColor(c))} />
      <p className="text-[10px] text-zinc-500">
        Color {slot + 1}: H {current.h} · S {current.s} · V {current.v}. The three colors are not mapped to fixed zones of the strip.
      </p>
      {invalid && <p className="text-[10px] text-red-400">{invalid}</p>}

      <button onClick={() => { void onApply(); }} disabled={disabled || invalid !== null}
        className="w-full py-1.5 rounded-lg text-xs font-medium bg-violet-600 hover:bg-violet-500 disabled:opacity-50 transition-colors">
        Apply {mode === 'static' ? 'Static' : 'Sequence'} to this strip
      </button>
    </div>
  );
}
