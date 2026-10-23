/// <reference lib="webworker" />
// Runs a model import off the main thread; large models take tens of seconds.
import { importerFor } from './registry';
import type { ImportOptions, RawScene } from './types';

export interface ImportRequest {
  name: string;
  file: ArrayBuffer;
  opts: ImportOptions;
}

export type ImportResponse = { ok: true; scene: RawScene } | { ok: false; error: string };

self.onmessage = async (msg: MessageEvent<ImportRequest>) => {
  const { name, file, opts } = msg.data;
  try {
    const scene = await (await importerFor(name)).import(file, opts);
    const bufs = [scene.vertices, scene.triangles, scene.triGroup, scene.triFlags]
      .filter((a) => a !== undefined)
      .map((a) => a.buffer as ArrayBuffer);
    self.postMessage({ ok: true, scene } satisfies ImportResponse, bufs);
  } catch (err) {
    self.postMessage({ ok: false, error: err instanceof Error ? err.message : String(err) } satisfies ImportResponse);
  }
};
