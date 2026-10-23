// Sources and receivers: read from point objects in the model, added or moved here.
// Positions are shown in metres in model coordinates; the engine keeps them relative
// to the scene origin, like the vertices.
import type { EngineConnector, ProjectInfo } from '../engine/connector';
import type { Point } from '../engine/protocol/Point';
import type { Setup } from '../engine/protocol/Setup';

type Role = keyof Setup;

export class SetupPanel {
  private origin: [number, number, number] = [0, 0, 0];
  /** Edits run one after another, each on the setup the previous one left. */
  private queue: Promise<void> = Promise.resolve();
  private seed: number[] | null = null;

  constructor(
    private engine: EngineConnector,
    private root: HTMLElement,
  ) {
    root.querySelector<HTMLButtonElement>('#add-source')!.onclick = () => this.edit((s) => this.add(s, 'sources'));
    root.querySelector<HTMLButtonElement>('#add-receiver')!.onclick = () => this.edit((s) => this.add(s, 'receivers'));
  }

  /** Applies `change` to the engine's current setup and saves it. */
  private edit(change: (setup: Setup) => void): Promise<void> {
    const next = this.queue.then(async () => {
      const setup = structuredClone((await this.engine.project()).setup);
      change(setup);
      await this.save(setup);
    });
    this.queue = next.catch(() => {});
    return next;
  }

  async show(): Promise<void> {
    this.render(await this.engine.project());
  }

  private add(setup: Setup, role: Role) {
    const list = setup[role];
    const prefix = role === 'sources' ? 'S' : 'R';
    let n = list.length + 1;
    while (list.some((p) => p.id === `${prefix}${n}`)) n++;
    // 2 m from the last point of its kind, or from the point the leak test found inside
    // the room, which is in the middle of the plan near the floor.
    const inside = this.seed ?? [0, 0, 1.2];
    const last = list.at(-1)?.position ?? (role === 'sources' ? inside : [inside[0] + 4, inside[1], inside[2]]);
    const step = list.length ? 2 : 0;
    list.push({ id: `${prefix}${n}`, position: [last[0] + step, last[1], last[2]], from_model: false });
  }

  private async save(setup: Setup) {
    const status = this.root.querySelector<HTMLElement>('#setup-status')!;
    try {
      this.render(await this.engine.setSetup(setup));
      status.textContent = '';
    } catch (e) {
      status.textContent = e instanceof Error ? e.message : String(e);
      this.render(await this.engine.project());
    }
  }

  private render({ scene, setup }: ProjectInfo) {
    this.origin = scene?.origin ?? [0, 0, 0];
    this.seed = scene?.report.leak?.seed ?? null;
    const body = this.root.querySelector('tbody')!;
    body.replaceChildren();
    for (const role of ['sources', 'receivers'] as const) {
      setup[role].forEach((p, i) => body.append(this.row(role, p, i)));
    }
    const n = setup.sources.length + setup.receivers.length;
    this.root.querySelector<HTMLElement>('#setup-empty')!.hidden = n > 0;
  }

  private row(role: Role, p: Point, i: number) {
    // An edited model point stays a model point: re-importing the model moves it back.
    const name = input('text', p.id, () => this.edit((s) => (s[role][i].id = name.value.trim())));
    const coords = p.position.map((v, k) => {
      const box: HTMLInputElement = input('number', (v + this.origin[k]).toFixed(2), () =>
        this.edit((s) => (s[role][i].position[k] = Number(box.value) - this.origin[k])),
      );
      box.step = '0.1';
      return box;
    });
    const remove = Object.assign(document.createElement('button'), { textContent: 'Remove' });
    remove.onclick = () => this.edit((s) => s[role].splice(i, 1));
    const tr = document.createElement('tr');
    for (const c of [role === 'sources' ? 'Source' : 'Receiver', name, ...coords, p.from_model ? 'model' : 'app', remove]) {
      const td = document.createElement('td');
      td.append(c);
      tr.append(td);
    }
    return tr;
  }
}

function input(type: string, value: string, onchange: () => void) {
  const el = Object.assign(document.createElement('input'), { type, value });
  el.onchange = onchange;
  return el;
}
