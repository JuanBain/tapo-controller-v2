// Pure effect state: no Tauri imports, so `node --test` can load it directly.

/** `classic` = `set_lighting_effect` preset, `segment` = `set_segment_effect` preset. */
export type EffectKind = 'classic' | 'segment';
/** Sub-views of the Effects tab. */
export type EffectView = EffectKind | 'custom';
/** Mirrors `custom_effects::CustomMode`. */
export type CustomMode = 'static' | 'sequence';
/** Mirrors `custom_effects::RainbowSpeed`. */
export type RainbowSpeed = 'fast' | 'normal' | 'slow';

export interface EffectColor {
  h: number;
  s: number;
  v: number;
}

export type CustomColors = [EffectColor, EffectColor, EffectColor];

export type EffectTarget =
  | { kind: 'classic'; id: string; rainbowSpeed?: RainbowSpeed }
  | { kind: 'segment'; id: string }
  | { kind: 'custom'; mode: CustomMode; colors: CustomColors };

/** An effect the device accepted; the device cannot report it back later. */
export interface ConfirmedEffect {
  target: EffectTarget;
  brightness: number | null;
  elapsedMs: number;
}

export const RAINBOW_ID = 'rainbow';

/** Only the transitions tried in X4 (750, 1500, 3000); no other values. */
export const RAINBOW_SPEEDS: { id: RainbowSpeed; label: string }[] = [
  { id: 'fast', label: 'Fast' },
  { id: 'normal', label: 'Normal' },
  { id: 'slow', label: 'Slow' },
];

export const CUSTOM_MODES: { id: CustomMode; label: string }[] = [
  { id: 'static', label: 'Static' },
  { id: 'sequence', label: 'Sequence' },
];

/** Same limits as `validation::hue`, `effect_saturation` and `effect_value`. */
export const EFFECT_COLOR_LIMITS = {
  h: [0, 360],
  s: [0, 100],
  v: [1, 100],
} as const;

/** The red, white and blue of the X1/X2 payloads. */
export const DEFAULT_CUSTOM_COLORS: CustomColors = [
  { h: 359, s: 85, v: 100 },
  { h: 0, s: 0, v: 100 },
  { h: 236, s: 72, v: 100 },
];

const COMPONENT_NAMES = { h: 'hue', s: 'saturation', v: 'value' } as const;

/** `null` when every component is an integer inside its limits. */
export function effectColorError(color: EffectColor): string | null {
  for (const key of ['h', 's', 'v'] as const) {
    const [min, max] = EFFECT_COLOR_LIMITS[key];
    const value = color[key];
    if (!Number.isInteger(value) || value < min || value > max) {
      return `${COMPONENT_NAMES[key]} must be an integer between ${min} and ${max}`;
    }
  }
  return null;
}

/** Rounds and clamps a picker value into the accepted limits. */
export function toEffectColor(color: EffectColor): EffectColor {
  const fit = (value: number, [min, max]: readonly [number, number]) =>
    Math.min(max, Math.max(min, Math.round(Number.isFinite(value) ? value : min)));
  return {
    h: fit(color.h, EFFECT_COLOR_LIMITS.h),
    s: fit(color.s, EFFECT_COLOR_LIMITS.s),
    v: fit(color.v, EFFECT_COLOR_LIMITS.v),
  };
}

/** Approximate preview color for a swatch; not what the strip renders. */
export function effectColorCss({ h, s, v }: EffectColor): string {
  return `hsl(${h} ${s}% ${Math.max(8, v / 2)}%)`;
}

/** A speed is attached to Rainbow only. */
export function classicTarget(id: string, rainbowSpeed: RainbowSpeed): EffectTarget {
  return id === RAINBOW_ID ? { kind: 'classic', id, rainbowSpeed } : { kind: 'classic', id };
}

/** True when `target` is Rainbow at `speed`, i.e. the strip was last asked for exactly that. */
export function isRainbowAt(target: EffectTarget | undefined, speed: RainbowSpeed): boolean {
  return target?.kind === 'classic' && target.id === RAINBOW_ID && target.rainbowSpeed === speed;
}

export function targetView(target: EffectTarget): EffectView {
  return target.kind;
}

export function sameTarget(a: EffectTarget | null | undefined, b: EffectTarget | null | undefined): boolean {
  if (!a || !b || a.kind !== b.kind) return false;
  if (a.kind === 'custom' && b.kind === 'custom') {
    return a.mode === b.mode && a.colors.every((c, i) => {
      const o = b.colors[i];
      return c.h === o.h && c.s === o.s && c.v === o.v;
    });
  }
  if (a.kind === 'classic' && b.kind === 'classic') {
    return a.id === b.id && (a.rainbowSpeed ?? null) === (b.rainbowSpeed ?? null);
  }
  return a.kind === 'segment' && b.kind === 'segment' && a.id === b.id;
}

/** Per strip, kept for the session only. */
export interface StripSettings {
  rainbowSpeed: RainbowSpeed;
  customMode: CustomMode;
  customColors: Record<CustomMode, CustomColors>;
}

export function defaultStripSettings(): StripSettings {
  return {
    rainbowSpeed: 'normal',
    customMode: 'static',
    customColors: {
      static: [...DEFAULT_CUSTOM_COLORS] as CustomColors,
      sequence: [...DEFAULT_CUSTOM_COLORS] as CustomColors,
    },
  };
}

export function stripSettings(all: Record<string, StripSettings>, ip: string): StripSettings {
  return all[ip] ?? defaultStripSettings();
}

export function updateStrip(
  all: Record<string, StripSettings>,
  ip: string,
  change: (settings: StripSettings) => StripSettings,
): Record<string, StripSettings> {
  return { ...all, [ip]: change(stripSettings(all, ip)) };
}

export function withCustomColor(
  settings: StripSettings,
  mode: CustomMode,
  index: number,
  color: EffectColor,
): StripSettings {
  if (!Number.isInteger(index) || index < 0 || index > 2) return settings;
  const colors = [...settings.customColors[mode]] as CustomColors;
  colors[index] = toEffectColor(color);
  return { ...settings, customColors: { ...settings.customColors, [mode]: colors } };
}

export type ApplyEffect = (ip: string, target: EffectTarget, brightness: number) => Promise<ConfirmedEffect>;

export interface EffectSendResult {
  confirmed: ConfirmedEffect;
  /** False when a later request for the same strip replaced this one. */
  current: boolean;
}

/**
 * Tracks, per strip, the effect most recently sent (accepted or in flight).
 * The backend `CommandGate` applies one strip's commands in send order, so the
 * latest request wins; a brightness resend always uses that latest target.
 */
export function createEffectController(apply: ApplyEffect) {
  // Each send gets its own entry, so a resend of the same target still replaces older ones.
  const latest = new Map<string, { target: EffectTarget }>();

  const send = async (ip: string, target: EffectTarget, brightness: number): Promise<EffectSendResult> => {
    const previous = latest.get(ip);
    const entry = { target };
    latest.set(ip, entry);
    try {
      const confirmed = await apply(ip, target, brightness);
      return { confirmed, current: latest.get(ip) === entry };
    } catch (err) {
      if (latest.get(ip) === entry) {
        if (previous) latest.set(ip, previous);
        else latest.delete(ip);
      }
      throw err;
    }
  };

  return {
    current: (ip: string): EffectTarget | undefined => latest.get(ip)?.target,
    /** After a color, white, plain brightness or experiment command. */
    clear: (ip: string) => {
      latest.delete(ip);
    },
    /** Taken when a non-effect command is sent; pass it to `clearIfUnchanged` afterwards. */
    snapshot: (ip: string): object | undefined => latest.get(ip),
    /**
     * Clears the strip's effect only if no effect was sent since `snapshot`.
     * A late color/white reply must not drop an effect requested after it.
     */
    clearIfUnchanged: (ip: string, snapshot: object | undefined): boolean => {
      if (latest.get(ip) !== snapshot) return false;
      latest.delete(ip);
      return true;
    },
    send,
    /** Re-sends the strip's latest target with a new brightness; `null` if there is none. */
    resend: async (ip: string, brightness: number): Promise<EffectSendResult | null> => {
      const target = latest.get(ip)?.target;
      return target ? send(ip, target, brightness) : null;
    },
  };
}

export type EffectController = ReturnType<typeof createEffectController>;
