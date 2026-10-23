import type { ImportResponse } from './import.worker';
import type { ImportOptions, RawScene } from './types';

/** Kept here rather than read from the registry, so the page does not bundle the CAD libraries. */
export const importExtensions = ['.3dm', '.skp'];

/** Reads a .3dm or .skp file in a worker and returns the RawScene for the engine. */
export async function importModel(file: File, opts: ImportOptions = { skipHidden: true }): Promise<RawScene> {
  const worker = new Worker(new URL('./import.worker.ts', import.meta.url), { type: 'module' });
  try {
    const bytes = await file.arrayBuffer();
    const reply = await new Promise<ImportResponse>((resolve, reject) => {
      worker.onmessage = (e: MessageEvent<ImportResponse>) => resolve(e.data);
      worker.onerror = (e) => reject(new Error(e.message || 'Import worker failed'));
      worker.postMessage({ name: file.name, file: bytes, opts }, [bytes]);
    });
    if (!reply.ok) throw new Error(reply.error);
    return reply.scene;
  } finally {
    worker.terminate();
  }
}
