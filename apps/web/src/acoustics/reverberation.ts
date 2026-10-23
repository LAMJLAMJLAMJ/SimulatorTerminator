// Sabine and Eyring reverberation times of the imported room, from its volume and
// the materials on its layers. A quick check before, and later beside, the ray tracer.
import type { EngineConnector } from '../engine/connector';
import type { ReverberationReport } from '../engine/protocol/ReverberationReport';

/** Layers listed as the main absorbers, by their share at 1 kHz. */
const TOP_LAYERS = 5;

export class ReverberationPanel {
  /** Only the latest request's answer is shown when changes come quickly. */
  private latest = 0;

  constructor(
    private engine: EngineConnector,
    private root: HTMLElement,
  ) {
    root.querySelector<HTMLButtonElement>('#reverb-run')!.onclick = () => this.update();
  }

  async update(): Promise<void> {
    const status = this.root.querySelector<HTMLElement>('#reverb-status')!;
    const value = (id: string) => Number(this.root.querySelector<HTMLInputElement>(id)!.value);
    status.textContent = 'Working…';
    const request = ++this.latest;
    try {
      const report = await this.engine.reverberation({
        temperature_c: value('#reverb-temperature'),
        humidity_pct: value('#reverb-humidity'),
        pressure_kpa: 101.325,
      });
      if (request !== this.latest) return;
      this.render(report);
      status.textContent = summary(report);
    } catch (e) {
      if (request !== this.latest) return;
      this.root.querySelector('tbody')!.replaceChildren();
      status.textContent = `Could not estimate: ${e instanceof Error ? e.message : e}`;
    }
  }

  private render(r: ReverberationReport) {
    const body = this.root.querySelector('tbody')!;
    body.replaceChildren(
      ...r.bands_hz.map((hz, b) =>
        row(
          hz >= 1000 ? `${hz / 1000} kHz` : `${hz} Hz`,
          seconds(r.sabine_s[b]),
          seconds(r.eyring_s[b]),
          r.absorption_area_m2[b].toFixed(0),
          r.air_absorption_m2[b].toFixed(0),
        ),
      ),
    );
  }
}

/** Volume, area, the layers that absorb most and the warnings, as text. */
export function summary(r: ReverberationReport): string {
  const k = r.bands_hz.indexOf(1000);
  const total = r.absorption_area_m2[k] ?? 0;
  const top = [...r.layers]
    .filter((l) => (l.absorption_area_m2[k] ?? 0) > 0)
    .sort((a, b) => b.absorption_area_m2[k] - a.absorption_area_m2[k])
    .slice(0, TOP_LAYERS)
    .map((l) => `  ${pct(l.absorption_area_m2[k] / total)}  ${l.key} (${l.material}, ${l.area_m2.toFixed(0)} m²)`);
  return [
    `Volume ${r.volume_m3.toFixed(0)} m³ (${r.volume_from}), surface facing the room ${r.area_m2.toFixed(0)} m².`,
    ...(top.length ? ['Most absorption at 1 kHz:', ...top] : []),
    ...r.warnings,
  ].join('\n');
}

const seconds = (t: number) => (Number.isFinite(t) ? t.toFixed(2) : '∞');
const pct = (x: number) => `${(100 * x).toFixed(0).padStart(3)} %`;

function row(...cells: string[]) {
  const tr = document.createElement('tr');
  for (const c of cells) {
    const td = document.createElement('td');
    td.textContent = c;
    tr.append(td);
  }
  return tr;
}
