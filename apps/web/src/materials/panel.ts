// The layer-to-material table shown after a model is imported. Each change goes
// straight to the engine, which keeps the mapping in the project.
import type { EngineConnector, ProjectInfo } from '../engine/connector';
import { mergeLibrary, parseMaterialCsv } from './csv';
import { starterMaterials, suggestMaterial } from './starter';

export class MaterialsPanel {
  constructor(
    private engine: EngineConnector,
    private root: HTMLElement,
  ) {
    root.querySelector<HTMLButtonElement>('#suggest')!.onclick = () => this.suggest();
    const input = root.querySelector<HTMLInputElement>('#library')!;
    input.onchange = async () => {
      const file = input.files?.[0];
      input.value = '';
      if (file) await this.importLibrary(file);
    };
  }

  /** Adds the materials in a CSV file to the library; same names replace the old ones. */
  async importLibrary(file: File): Promise<void> {
    const status = this.root.querySelector<HTMLElement>('#library-status')!;
    try {
      const { materials, warnings } = parseMaterialCsv(decode(await file.arrayBuffer()), file.name);
      const project = await this.engine.project();
      this.render(await this.engine.setMaterials(mergeLibrary(project.materials, materials)));
      status.textContent = [`Imported ${materials.length} materials from ${file.name}.`, ...warnings].join('\n');
    } catch (e) {
      status.textContent = `Could not import ${file.name}: ${e instanceof Error ? e.message : e}`;
    }
  }

  /** Shows the scene's layers, loading the starter library into a project that has none. */
  async show(): Promise<void> {
    let project = await this.engine.project();
    if (project.materials.length === 0) project = await this.engine.setMaterials(starterMaterials);
    this.render(project);
  }

  private async suggest() {
    const project = await this.engine.project();
    const assignments: Record<string, string> = {};
    for (const g of project.scene?.groups ?? []) {
      const m = g.acoustic_material ? null : suggestMaterial(g.layer_path, project.materials);
      if (m) assignments[g.key] = m;
    }
    this.render(await this.engine.assignMaterials(assignments));
  }

  private render({ scene, materials }: ProjectInfo) {
    const body = this.root.querySelector('tbody')!;
    body.replaceChildren();
    this.root.hidden = !scene;
    if (!scene) return;
    scene.groups.forEach((g, i) => {
      const select = document.createElement('select');
      select.dataset.key = g.key;
      select.append(new Option('(none)', ''), ...materials.map((m) => option(m.name)));
      select.value = g.acoustic_material ?? '';
      select.onchange = async () => {
        try {
          await this.engine.assignMaterials({ [g.key]: select.value || null });
        } catch (e) {
          alert(e instanceof Error ? e.message : String(e));
          this.render(await this.engine.project());
        }
      };
      const area = scene.report.group_area_m2[i] ?? 0;
      const row = document.createElement('tr');
      row.append(cell(g.layer_path), cell(area.toFixed(1)), cell(select));
      body.append(row);
    });
  }
}

function cell(content: string | Node) {
  const td = document.createElement('td');
  td.append(content);
  return td;
}

/** Library names can be a whole product description: the list shows the start of it. */
function option(name: string): HTMLOptionElement {
  const o = new Option(name.length > 90 ? `${name.slice(0, 88)}…` : name, name);
  o.title = name;
  return o;
}

/** UTF-8, or Windows-1252 for a CSV saved by Excel without "UTF-8" in the format name. */
function decode(bytes: ArrayBuffer): string {
  try {
    return new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  } catch {
    return new TextDecoder('windows-1252').decode(bytes);
  }
}
