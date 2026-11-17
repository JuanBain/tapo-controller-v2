import { invoke } from '@tauri-apps/api/core';
import type { ConfirmedEffect, CustomColors, CustomMode, EffectKind, EffectTarget, RainbowSpeed } from './effectState';

export type { ConfirmedEffect, EffectKind } from './effectState';

/** Mirrors `EffectCatalogItem` in `src-tauri/src/commands.rs`. */
export interface EffectCatalogItem {
  id: string;
  label: string;
}

export type EffectCatalogs = Record<EffectKind, EffectCatalogItem[]>;

/** Mirrors `EffectResponse` in `src-tauri/src/commands.rs`. */
export interface EffectResult {
  success: boolean;
  kind: EffectKind;
  effect: string;
  brightness: number | null;
  elapsed_ms: number;
  rainbow_speed?: RainbowSpeed;
}

/** Mirrors `CustomEffectResponse` in `src-tauri/src/commands.rs`. */
interface CustomEffectResult {
  success: boolean;
  mode: CustomMode;
  colors: [number, number, number][];
  brightness: number;
  elapsed_ms: number;
}

let catalogs: Promise<EffectCatalogs> | null = null;

/** Loaded once per session; a failed load is retried on the next call. */
export function fetchEffectCatalogs(): Promise<EffectCatalogs> {
  catalogs ??= invoke<{ catalog: EffectCatalogItem[]; segment_catalog: EffectCatalogItem[] }>('available_effects')
    .then(res => ({ classic: res.catalog, segment: res.segment_catalog }))
    .catch(err => {
      catalogs = null;
      throw err;
    });
  return catalogs;
}

/** Mirrors `LightingCapabilities` in `src-tauri/src/capabilities.rs`. */
export interface LightingCapabilities {
  /** Allowlisted fields only. */
  fields: Record<string, unknown>;
  /** Names of the other keys in the device response; their values are withheld. */
  unlisted_keys: string[];
}

/** Read-only; changes nothing on the device. */
export function readLightingCapabilities(ip: string): Promise<LightingCapabilities> {
  return invoke<LightingCapabilities>('device_lighting_capabilities', { ip });
}

/** Mirrors `device::Experiment`: fixed payloads, no caller-supplied values. */
export type Experiment =
  | 'x1_static_red_white_blue'
  | 'x2_sequence_red_white_blue'
  | 'x3_segment_circulating'
  | 'x4a_rainbow_transition_3000'
  | 'x4b_rainbow_transition_750'
  | 'x5a_rainbow_direction_3'
  | 'x5b_rainbow_direction_4';

export async function runExperiment(ip: string, experiment: Experiment): Promise<number> {
  const res = await invoke<{ success: boolean; experiment: Experiment; elapsed_ms: number }>(
    'device_experiment', { req: { ip, experiment } },
  );
  return res.elapsed_ms;
}

/** Only the mode, three colors and brightness are sent; the backend builds the payload. */
async function applyCustomEffect(ip: string, mode: CustomMode, colors: CustomColors, brightness: number) {
  const res = await invoke<CustomEffectResult>('device_custom_effect', {
    req: { ip, mode, colors: colors.map(c => ({ hue: c.h, saturation: c.s, value: c.v })), brightness },
  });
  const confirmed = res.colors.map(([h, s, v]) => ({ h, s, v })) as CustomColors;
  return {
    target: { kind: 'custom', mode: res.mode, colors: confirmed },
    brightness: res.brightness,
    elapsedMs: res.elapsed_ms,
  } satisfies ConfirmedEffect;
}

/** The confirmation is built from the backend response, not from the request. */
export async function applyEffect(ip: string, target: EffectTarget, brightness: number): Promise<ConfirmedEffect> {
  if (target.kind === 'custom') return applyCustomEffect(ip, target.mode, target.colors, brightness);
  const command = target.kind === 'segment' ? 'device_segment_effect' : 'device_effect';
  const rainbowSpeed = target.kind === 'classic' ? target.rainbowSpeed : undefined;
  const res = await invoke<EffectResult>(command, {
    req: { name: target.id, ip, brightness, ...(rainbowSpeed ? { rainbow_speed: rainbowSpeed } : {}) },
  });
  const confirmedTarget: EffectTarget = res.kind === 'segment'
    ? { kind: 'segment', id: res.effect }
    : res.rainbow_speed
      ? { kind: 'classic', id: res.effect, rainbowSpeed: res.rainbow_speed }
      : { kind: 'classic', id: res.effect };
  return { target: confirmedTarget, brightness: res.brightness, elapsedMs: res.elapsed_ms };
}
