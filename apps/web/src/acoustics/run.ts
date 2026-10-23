// Runs the ray tracer for every source and receiver and shows the ISO 3382-1
// parameters per receiver.
import type { EngineConnector } from '../engine/connector';
import type { RunResults } from '../engine/protocol/RunResults';

export class RunPanel {
  constructor(
    private engine: EngineConnector,
    private root: HTMLElement,
  ) {
    root.querySelector<HTMLButtonElement>('#run')!.onclick = () => this.run();
  }

  async run(): Promise<RunResults | null> {
    const status = this.root.querySelector<HTMLElement>('#run-status')!;
    const button = this.root.querySelector<HTMLButtonElement>('#run')!;
    const value = (id: string) => Number(document.querySelector<HTMLInputElement>(id)!.value);
    button.disabled = true;
    status.textContent = 'Tracing…';
    try {
      const project = await this.engine.project();
      const t0 = performance.now();
      const results = await this.engine.run(
        {
          solver: 'rt',
          bands: project.manifest.bands,
          max_order: 0,
          n_rays: value('#run-rays'),
          seed: 1,
          air_absorption_per_m: null,
          air: { temperature_c: value('#reverb-temperature'), humidity_pct: value('#reverb-humidity'), pressure_kpa: 101.325 },
          // Empty means 0: the engine picks a radius from the room volume and the ray count.
          receiver_radius_m: value('#run-radius'),
          duration_s: value('#run-length'),
          receivers: [],
          grids: [],
          listening: [],
        },
        (p) => (status.textContent = `Tracing… ${Math.round((100 * p.done) / p.total)} %`),
      );
      const seconds = ((performance.now() - t0) / 1000).toFixed(1);
      this.render(results);
      status.textContent = [
        `Traced in ${seconds} s. Receiver radius ${results.receiver_radius_m.toFixed(2)} m, mean free path ${results.mean_free_path_m.toFixed(1)} m.`,
        results.leak_fraction > 0.001
          ? `${(100 * results.leak_fraction).toFixed(1)} % of rays left the model through gaps: the decay is too short.`
          : '',
        'Mid = mean of 500 Hz and 1 kHz. Empty where the decay is too short or noisy: trace longer or with more rays.',
      ]
        .filter(Boolean)
        .join('\n');
      return results;
    } catch (e) {
      status.textContent = `Could not run: ${e instanceof Error ? e.message : e}`;
      return null;
    } finally {
      button.disabled = false;
    }
  }

  private render(r: RunResults) {
    const [summary, bands] = this.root.querySelectorAll('table');
    const mid = midBands(r.bands_hz);
    summary.querySelector('tbody')!.replaceChildren(
      ...r.receivers.map((rec) =>
        row([
          rec.id,
          fmt(mean(rec.edt_s, mid), 2),
          fmt(mean(rec.t30_s, mid), 2),
          fmt(mean(rec.c80_db, mid), 1),
          fmt(mean(rec.d50, mid), 2),
          fmt(mean(rec.ts_s, mid) * 1000, 0),
        ]),
      ),
    );
    bands.querySelector('thead')!.replaceChildren(row(['T30 s', ...r.bands_hz.map(label)], 'th'));
    bands.querySelector('tbody')!.replaceChildren(...r.receivers.map((rec) => row([rec.id, ...rec.t30_s.map((t) => fmt(t ?? NaN, 2))])));
  }
}

/** Indices of 500 Hz and 1 kHz. */
export function midBands(bandsHz: number[]): number[] {
  return [500, 1000].map((f) => bandsHz.indexOf(f)).filter((i) => i >= 0);
}

/** Mean over the given bands, NaN if any is missing. */
export function mean(values: (number | null)[], at: number[]): number {
  const v = at.map((i) => values[i]);
  return v.some((x) => x == null) ? NaN : (v as number[]).reduce((a, b) => a + b, 0) / v.length;
}

const fmt = (x: number, digits: number) => (Number.isFinite(x) ? x.toFixed(digits) : '');
const label = (hz: number) => (hz >= 1000 ? `${hz / 1000}k` : `${hz}`);

function row(cells: string[], tag: 'td' | 'th' = 'td') {
  const tr = document.createElement('tr');
  for (const c of cells) {
    const td = document.createElement(tag);
    td.textContent = c;
    tr.append(td);
  }
  return tr;
}
