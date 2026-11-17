import { useEffect, useState } from 'react';
import { AlertCircle, FlaskConical, Loader2 } from 'lucide-react';
import { Experiment, LightingCapabilities, readLightingCapabilities } from '../lib/effects';

interface LightingDiagnosticsProps {
  deviceIp: string;
  deviceName: string;
  busy: boolean;
  /** Sends one fixed experiment to this device; resolves with the latency in ms. */
  onRun: (ip: string, experiment: Experiment) => Promise<number>;
  /** Restores the known classic Rainbow preset on this device only. */
  onRestore: (ip: string) => Promise<void>;
}

const EXPERIMENTS: { id: Experiment; title: string; note: string; payload: string }[] = [
  {
    id: 'x1_static_red_white_blue',
    title: 'X1 — static red, white and blue',
    note: 'Confirmed on one L930-5: the three colors showed at once (178 ms). This does not establish how many segments the strip has.',
    payload: `type: static (custom)
segments: [0, 1, 2]
sequence: [359, 85, 100] red, [0, 0, 100] white, [236, 72, 100] blue
expansion_strategy: 1, brightness: 100`,
  },
  {
    id: 'x2_sequence_red_white_blue',
    title: 'X2 — sequence red, white and blue',
    note: 'Confirmed on one L930-5: the colors move (139 ms). Direction and duration stay fixed.',
    payload: `type: sequence (custom)
segments: [0, 1, 2]
sequence: [359, 85, 100] red, [0, 0, 100] white, [236, 72, 100] blue
direction: 1, duration: 50, expansion_strategy: 1, brightness: 100`,
  },
  {
    id: 'x3_segment_circulating',
    title: 'X3 — circulating segment effect',
    note: 'Observed on one L930-5: purple expands from the start to the end (190 ms). Rainbow restored and the other strip stayed unchanged. This does not establish the segment count.',
    payload: `type: circulating (custom segment effect)
segments: [49]
states: [267, 56, 100, 0]
brightness: 100`,
  },
  {
    id: 'x4a_rainbow_transition_3000',
    title: 'X4-A — Rainbow, transition 3000',
    note: 'Confirmed on one L930-5: Rainbow runs slower. Only transition changes, from 1500 to 3000. Units remain unknown.',
    payload: `Rainbow preset, every field unchanged except transition: 3000`,
  },
  {
    id: 'x4b_rainbow_transition_750',
    title: 'X4-B — Rainbow, transition 750',
    note: 'Confirmed on one L930-5: Rainbow runs faster. Only transition changes, from 1500 to 750.',
    payload: `Rainbow preset, every field unchanged except transition: 750`,
  },
  {
    id: 'x5a_rainbow_direction_3',
    title: 'X5-A — Rainbow, direction 3',
    note: 'Not validated. Only direction changes, from the preset value 1 to 3. Transition stays 1500. The number has no documented meaning.',
    payload: `Rainbow preset, every field unchanged except direction: 3`,
  },
  {
    id: 'x5b_rainbow_direction_4',
    title: 'X5-B — Rainbow, direction 4',
    note: 'Not validated. Run only after X5-A is stable. Only direction changes, from 1 to 4. Transition stays 1500.',
    payload: `Rainbow preset, every field unchanged except direction: 4`,
  },
];

/** Development builds only (`npm run dev`). Release installers omit this section. */
const EXPERIMENTS_ENABLED = import.meta.env.DEV;

export function LightingDiagnostics({ deviceIp, deviceName, busy, onRun, onRestore }: LightingDiagnosticsProps) {
  const [report, setReport] = useState<LightingCapabilities | null>(null);
  const [reading, setReading] = useState(false);
  const [armed, setArmed] = useState<Experiment | null>(null);
  const [sending, setSending] = useState<Experiment | 'restore' | null>(null);
  const [status, setStatus] = useState('');
  const [error, setError] = useState('');

  useEffect(() => {
    setReport(null);
    setArmed(null);
    setStatus('');
    setError('');
  }, [deviceIp]);

  const read = async () => {
    setReading(true);
    setError('');
    try {
      setReport(await readLightingCapabilities(deviceIp));
    } catch (err) {
      setError(String(err));
    } finally {
      setReading(false);
    }
  };

  const send = async (what: Experiment | 'restore') => {
    setSending(what);
    setError('');
    setStatus('');
    try {
      if (what === 'restore') {
        await onRestore(deviceIp);
        setStatus('Rainbow restored.');
      } else {
        const ms = await onRun(deviceIp, what);
        setStatus(`${what} accepted by the device (${ms} ms). Restore with Rainbow when done.`);
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setArmed(null);
      setSending(null);
    }
  };

  const disabled = busy || reading || sending !== null;

  return (
    <details className="bg-zinc-900/50 rounded-xl p-2 text-xs">
      <summary className="cursor-pointer text-zinc-400 flex items-center gap-1.5">
        <FlaskConical size={12} /> Diagnostics{EXPERIMENTS_ENABLED ? ' & experiments' : ''}
      </summary>

      <div className="mt-2 space-y-3">
        <section className="space-y-1.5">
          <button onClick={read} disabled={disabled}
            className="px-2 py-1 rounded-md bg-zinc-800 text-zinc-200 hover:bg-zinc-700 disabled:opacity-50">
            {reading ? 'Reading…' : 'Read lighting capabilities (read-only)'}
          </button>
          {report && (
            <div className="space-y-1 text-[11px]">
              {Object.entries(report.fields).map(([key, value]) => (
                <div key={key} className="flex gap-2">
                  <span className="text-zinc-500 shrink-0">{key}</span>
                  <span className="text-zinc-200 break-all">{JSON.stringify(value)}</span>
                </div>
              ))}
              <p className="text-zinc-500">
                Other keys (values withheld): {report.unlisted_keys.join(', ') || 'none'}
              </p>
            </div>
          )}
        </section>

        {EXPERIMENTS_ENABLED && (
          <section className="space-y-3 border-t border-zinc-800 pt-2">
            {EXPERIMENTS.map(experiment => (
              <div key={experiment.id} className="space-y-1.5">
                <p className="text-amber-400 font-medium">{experiment.title}</p>
                <p className="text-zinc-500">{experiment.note}</p>
                <pre className="text-[10px] text-zinc-400 whitespace-pre-wrap">{experiment.payload}</pre>
                {armed !== experiment.id ? (
                  <button onClick={() => setArmed(experiment.id)} disabled={disabled}
                    className="px-2 py-1 rounded-md bg-zinc-800 text-amber-300 hover:bg-zinc-700 disabled:opacity-50">
                    Prepare for {deviceName}
                  </button>
                ) : (
                  <div className="flex gap-1.5">
                    <button onClick={() => send(experiment.id)} disabled={disabled}
                      className="px-2 py-1 rounded-md bg-amber-600 text-white hover:bg-amber-500 disabled:opacity-50">
                      Send to {deviceName} ({deviceIp})
                    </button>
                    <button onClick={() => setArmed(null)} disabled={disabled}
                      className="px-2 py-1 rounded-md bg-zinc-800 text-zinc-300 hover:bg-zinc-700 disabled:opacity-50">
                      Cancel
                    </button>
                  </div>
                )}
              </div>
            ))}
            <button onClick={() => send('restore')} disabled={disabled}
              className="px-2 py-1 rounded-md bg-zinc-800 text-zinc-200 hover:bg-zinc-700 disabled:opacity-50">
              Restore Rainbow
            </button>
          </section>
        )}

        {(sending || status || error) && (
          <p className={`flex items-center gap-1.5 ${error ? 'text-red-400' : 'text-zinc-400'}`}>
            {sending && <Loader2 size={12} className="animate-spin" />}
            {error && <AlertCircle size={12} className="shrink-0" />}
            {error || status || (sending === 'restore' ? 'Restoring Rainbow…' : 'Sending…')}
          </p>
        )}
      </div>
    </details>
  );
}
