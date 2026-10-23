// Picks an importer by file extension. The CAD libraries are large, so each
// one loads only when a file of its kind is opened.
import type { GeometryImporter } from './types';

const loaders: Record<string, () => Promise<GeometryImporter>> = {
  '.3dm': async () => {
    const [{ rhinoImporter }, { default: rhino3dm }, { default: wasmUrl }] = await Promise.all([
      import('./rhino'),
      import('rhino3dm/rhino3dm.module.js'),
      import('rhino3dm/rhino3dm.wasm?url'),
    ]);
    return rhinoImporter(() => rhino3dm({ locateFile: () => wasmUrl }));
  },
  '.skp': async () => (await import('./sketchup')).sketchupImporter,
};

const cache = new Map<string, Promise<GeometryImporter>>();

export function importerFor(fileName: string): Promise<GeometryImporter> {
  const ext = fileName.slice(fileName.lastIndexOf('.')).toLowerCase();
  const load = loaders[ext];
  if (!load) return Promise.reject(new Error(`No importer for ${ext || 'files without an extension'}`));
  let p = cache.get(ext);
  if (!p) cache.set(ext, (p = load()));
  return p;
}
