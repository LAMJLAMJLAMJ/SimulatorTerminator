import type { Command } from './protocol/Command';
import type { Event } from './protocol/Event';
import type { Reply, Transport } from './transport';
import type { Request, Response } from './engine.worker';

/**
 * Browser transport: one coordinator worker hosting the engine. The pool of chunk
 * workers (separate wasm instances, no shared memory) joins it with the ray engine.
 */
export class WorkerTransport implements Transport {
  private worker = new Worker(new URL('./engine.worker.ts', import.meta.url), { type: 'module' });
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: Reply) => void; onEvent?: (e: Event) => void }>();

  constructor() {
    this.worker.onmessage = (msg: MessageEvent<Response>) => {
      const r = msg.data;
      const p = this.pending.get(r.id);
      if (!p) return;
      const event = JSON.parse(r.event) as Event;
      if (r.kind === 'progress') {
        p.onEvent?.(event);
      } else {
        this.pending.delete(r.id);
        p.resolve({ event, bufs: r.bufs });
      }
    };
  }

  send(cmd: Command, bufs: ArrayBuffer[] = [], onEvent?: (e: Event) => void): Promise<Reply> {
    const id = this.nextId++;
    return new Promise((resolve) => {
      this.pending.set(id, { resolve, onEvent });
      this.worker.postMessage({ id, cmd: JSON.stringify(cmd), bufs } satisfies Request, bufs);
    });
  }

  dispose(): void {
    this.worker.terminate();
  }
}
