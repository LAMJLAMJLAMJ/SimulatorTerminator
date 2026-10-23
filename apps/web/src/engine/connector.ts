import type { BandSet } from './protocol/BandSet';
import type { Capabilities } from './protocol/Capabilities';
import type { Command } from './protocol/Command';
import type { Estimate } from './protocol/Estimate';
import type { Event } from './protocol/Event';
import type { ImportReport } from './protocol/ImportReport';
import type { Manifest } from './protocol/Manifest';
import type { Material } from './protocol/Material';
import type { RunRequest } from './protocol/RunRequest';
import type { SceneDoc } from './protocol/SceneDoc';
import type { RawScene } from '../importers/types';
import type { Transport } from './transport';
import { TauriTransport, inTauri } from './tauri-transport';
import { WorkerTransport } from './worker-transport';

export interface ProjectInfo {
  manifest: Manifest;
  scene: SceneDoc | null;
  materials: Material[];
}

export interface Progress {
  done: number;
  total: number;
}

/** The one interface the UI talks to, whether a browser worker or the Mac app answers. */
export class EngineConnector {
  constructor(private transport: Transport) {}

  async capabilities(): Promise<Capabilities> {
    const e = await this.call({ type: 'capabilities' });
    return expect(e, 'capabilities');
  }

  async newProject(bands: BandSet): Promise<ProjectInfo> {
    return expect(await this.call({ type: 'new_project', bands }), 'project');
  }

  async openProject(file: Blob): Promise<ProjectInfo> {
    const bytes = await file.arrayBuffer();
    return expect(await this.call({ type: 'open_project' }, [bytes]), 'project');
  }

  async project(): Promise<ProjectInfo> {
    return expect(await this.call({ type: 'get_project' }), 'project');
  }

  /** Replaces the material library; layers whose material is dropped become unassigned. */
  async setMaterials(materials: Material[]): Promise<ProjectInfo> {
    return expect(await this.call({ type: 'set_materials', materials }), 'project');
  }

  /** Acoustic material per layer key; `null` unassigns. Layers not named are left as they are. */
  async assignMaterials(assignments: Record<string, string | null>): Promise<ProjectInfo> {
    return expect(await this.call({ type: 'assign_materials', assignments }), 'project');
  }

  async saveProject(): Promise<Blob> {
    const reply = await this.transport.send({ type: 'save_project' });
    expect(reply.event, 'ok');
    return new Blob(reply.bufs, { type: 'application/zip' });
  }

  async setScene(raw: RawScene): Promise<ImportReport> {
    const bufs = [
      raw.vertices,
      raw.triangles,
      raw.triGroup,
      raw.triFlags ?? new Uint8Array(),
    ].map((a) => a.slice().buffer as ArrayBuffer);
    const cmd: Command = {
      type: 'set_scene',
      meta: {
        origin: raw.origin,
        groups: raw.groups.map((g) => ({
          key: g.key,
          layer_path: g.layerPath,
          material: g.material ?? null,
          label: g.label,
          acoustic_material: null,
        })),
        importer: raw.importer,
      },
      unit_scale_to_metres: raw.unitScaleToMetres,
      up_axis: raw.upAxis,
    };
    return expect(await this.call(cmd, bufs), 'scene_report').report;
  }

  async estimate(run: RunRequest): Promise<Estimate> {
    return expect(await this.call({ type: 'estimate', run }), 'estimate').estimate;
  }

  async run(run: RunRequest, onProgress?: (p: Progress) => void): Promise<Extract<Event, { type: 'run_done' }>> {
    const reply = await this.transport.send({ type: 'run', run }, [], (e) => {
      if (e.type === 'progress') onProgress?.({ done: e.done, total: e.total });
    });
    return expect(reply.event, 'run_done');
  }

  dispose(): void {
    this.transport.dispose();
  }

  private async call(cmd: Command, bufs: ArrayBuffer[] = []): Promise<Event> {
    return (await this.transport.send(cmd, bufs)).event;
  }
}

function expect<T extends Event['type']>(e: Event, type: T): Extract<Event, { type: T }> {
  if (e.type === type) return e as Extract<Event, { type: T }>;
  if (e.type === 'error') throw new Error(e.message);
  throw new Error(`expected ${type}, engine answered ${e.type}`);
}

export function createConnector(): EngineConnector {
  return new EngineConnector(inTauri() ? new TauriTransport() : new WorkerTransport());
}
